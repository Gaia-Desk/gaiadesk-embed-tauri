// @gaiadesk/tauri-plugin-support: a "Get help" button for your Tauri app.
// Your customer clicks it and agrees; your app's window is shared with your
// support team in the GaiaDesk console, who can, if you allow it, guide it.
//
//   import { startHelp, on } from "@gaiadesk/tauri-plugin-support";
//   await startHelp({ customer: { name: "Ada" }, mode: "cobrowse", guided: true,
//                     branding: { name: "Acme", color: "#0f766e" } });
//   on("started", ({ code }) => …);

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

/** What the support team may do (asked for when the plugin creates the session). */
export type SupportMode = 'view' | 'cobrowse';
/** What is shared: this app's window (default), or the display it is on. */
export type Capture = 'window' | 'screen';
/** Who the customer is: at most 16 short fields. Shown to your own agents only. */
export type Customer = Record<string, string | number | boolean | null>;

/** Your company's look on the consent dialog and the "Sharing with …" indicator. */
export interface Branding {
  /** "Sharing with <name>". Default: the plugin config's `company`, then your app's product name. */
  name?: string;
  /** Accent colour, `#rrggbb` or `#rgb`. */
  color?: string;
  /** A PNG (at most 256 KiB): a path (relative ones are in your app's resources), an `https://` URL or a `data:image/png;base64,` URL. */
  logo?: string;
}

/** A rect in CSS pixels, relative to the top-left of the page. */
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface HelpOptions {
  /** A publishable key (`ak_…`) registered for `app://<your identifier>`. Usually set once in the plugin config instead. */
  publishableKey?: string;
  /** An embed token (`gdemb_…`) your backend got from `POST /v1/support/sessions`. Wins over a publishable key. */
  embedToken?: string;
  customer?: Customer;
  /** Publishable-key flow: `view` (default) or `cobrowse`. With an embed token your backend chose. */
  mode?: SupportMode;
  /** Publishable-key flow: seconds the session lives (60 to 3600). */
  expiresIn?: number;
  /** Allow guided input: clicks and typing inside your app only (needs a `cobrowse` session too). Default false. */
  guided?: boolean;
  capture?: Capture;
  branding?: Branding;
  /** Frames per second, 1 to 30 (default 15). */
  fps?: number;
  /**
   * What to paint out of every frame: rects, or a CSS selector whose elements are masked and
   * followed as they move. Default `"[data-gaiadesk-mask]"`.
   */
  masks?: Rect[] | string;
  /** `true` only when you asked your user yourself and they agreed: the native dialog is skipped. */
  consentGranted?: boolean;
  /** Your own consent dialog, used where the OS has none (Linux). Default: `window.confirm`. */
  askConsent?: (question: string) => boolean | Promise<boolean>;
}

/** Where a session is. */
export type State = 'idle' | 'consent' | 'starting' | 'waiting' | 'connected' | 'ended';
/** Why a session ended. */
export type EndReason = 'stopped' | 'app_stopped' | 'disconnected' | 'expired' | 'error';

export interface Status {
  state: State;
  /** The nine-digit support code, once the session is up. */
  code: string | null;
  sessionId: string | null;
  mode: SupportMode | null;
  /** The agent who joined (their email), while one is connected. */
  agent: string | null;
  paused: boolean;
  endReason: EndReason | null;
}

export interface EventMap {
  /** The session is up: `code` is what your customer can read to an agent. */
  started: { code: string; sessionId: string; mode: SupportMode; expiresAt: number | null };
  joined: { agent: string | null };
  left: Record<string, never>;
  /** Guided input paused (by the customer on the indicator, or by you) or resumed. */
  paused: { paused: boolean };
  /** A guided request and whether it was applied (`reason`: `masked`, `not_focused`, …). */
  action: { action: string; applied: boolean; reason: string | null };
  error: { code: string; message: string };
  /** Always last, exactly once. */
  ended: { reason: EndReason };
  state: { state: Exclude<State, 'idle' | 'consent'> };
}
export type EventName = keyof EventMap;

/** A refusal: `code` is `declined`, `busy`, `consent_required`, `bad_token`, `library_missing`, … */
export class SupportError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
    this.name = 'SupportError';
  }
}

export const DEFAULT_MASK_SELECTOR = '[data-gaiadesk-mask]';
const PLUGIN = 'plugin:gaiadesk-support|';

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(PLUGIN + cmd, args);
  } catch (e) {
    const o = (e ?? {}) as { code?: unknown; message?: unknown };
    throw new SupportError(typeof o.code === 'string' ? o.code : 'internal', typeof o.message === 'string' ? o.message : String(e));
  }
}

/** Listen to a session event. Returns the function that stops listening. */
export function on<K extends EventName>(name: K, fn: (detail: EventMap[K]) => void): Promise<UnlistenFn> {
  return listen<EventMap[K]>(`gaiadesk-support:${name}`, (e) => fn(e.payload));
}

/** The CSS-pixel rects of the elements matching `selector`, visible ones only. */
export function rectsOf(selector: string = DEFAULT_MASK_SELECTOR): Rect[] {
  return Array.from(document.querySelectorAll(selector))
    .map((el) => el.getBoundingClientRect())
    .filter((r) => r.width > 0 && r.height > 0)
    .map((r) => ({ x: r.left, y: r.top, w: r.width, h: r.height }));
}

let follow: ReturnType<typeof setInterval> | undefined;
let endedHooked = false;

/** Keep the masks on `selector`'s elements as they move, until the session ends. */
function followMasks(selector: string): void {
  clearInterval(follow);
  if (!endedHooked) {
    endedHooked = true;
    void on('ended', () => clearInterval(follow));
  }
  // shortcut: polls twice a second; switch to observers if a page needs tighter tracking.
  let last = JSON.stringify([rectsOf(selector), window.devicePixelRatio]);
  follow = setInterval(() => {
    const rects = rectsOf(selector);
    const key = JSON.stringify([rects, window.devicePixelRatio]);
    if (key === last) return;
    last = key;
    setMasks(rects).catch(() => clearInterval(follow));
  }, 500);
}

function question(o: HelpOptions): string {
  const what = o.capture === 'screen' ? 'your screen' : "this app's window";
  return `Share ${what} with ${o.branding?.name ?? 'our'} support team?` + (o.guided ? ' They will be able to guide it (click and type in this app).' : '');
}

/**
 * Ask your user, create the session if needed, and start sharing. Resolves once sharing has
 * started; the `started` event carries the support code. Rejects with a `SupportError`
 * (`declined` when your user said no).
 */
export async function startHelp(options: HelpOptions = {}): Promise<Status> {
  const { askConsent, masks = DEFAULT_MASK_SELECTOR, ...rest } = options;
  const selector = typeof masks === 'string' ? masks : null;
  const send = { ...rest, masks: selector === null ? masks : rectsOf(selector) };
  let status: Status;
  try {
    status = await call<Status>('start_help', { options: send });
  } catch (e) {
    if (!(e instanceof SupportError) || e.code !== 'no_native_dialog') throw e;
    const yes = await (askConsent ?? ((q: string) => window.confirm(q)))(question(options));
    if (!yes) throw new SupportError('declined', 'your user declined');
    status = await call<Status>('start_help', { options: { ...send, consentGranted: true } });
  }
  if (selector !== null) followMasks(selector);
  return status;
}

/** End the session (the `ended` event follows, reason `app_stopped`). */
export function stop(): Promise<Status> {
  clearInterval(follow);
  return call('stop');
}

/** Pause guided input. */
export function pause(): Promise<Status> {
  return call('set_paused', { paused: true });
}

/** Resume guided input (refused, `pause_refused`, when your customer paused it). */
export function resume(): Promise<Status> {
  return call('set_paused', { paused: false });
}

/** Replace the masks: rects in CSS pixels, or a selector's elements now. `[]` clears. */
export function setMasks(masks: Rect[] | string): Promise<void> {
  return call('set_masks', { masks: typeof masks === 'string' ? rectsOf(masks) : masks });
}

/** Where the session is. */
export function status(): Promise<Status> {
  return call('status');
}

export interface HelpButtonOptions extends HelpOptions {
  /** Default "Get help". */
  label?: string;
  /** Default "Stop sharing". */
  stopLabel?: string;
  /** Called with refusals (default: `console.error`). `declined` is not reported. */
  onError?: (e: SupportError) => void;
}

/**
 * One-click help: a button in `target` (an element or a selector) that starts help, and stops
 * it while a session is up. Returns the button.
 */
export function mountHelpButton(target: HTMLElement | string, options: HelpButtonOptions = {}): HTMLButtonElement {
  const { label = 'Get help', stopLabel = 'Stop sharing', onError = (e) => console.error(e), ...help } = options;
  const host = typeof target === 'string' ? document.querySelector<HTMLElement>(target) : target;
  if (!host) throw new SupportError('bad_options', `no element matches ${String(target)}`);
  const button = document.createElement('button');
  button.type = 'button';
  button.className = 'gaiadesk-help-button';
  button.textContent = label;
  let active = false;
  const show = (a: boolean) => {
    active = a;
    button.textContent = a ? stopLabel : label;
    button.disabled = false;
  };
  void on('ended', () => show(false));
  button.addEventListener('click', () => {
    button.disabled = true;
    (active ? stop().then(() => show(false)) : startHelp(help).then(() => show(true))).catch((e: SupportError) => {
      show(false);
      if (e.code !== 'declined') onError(e);
    });
  });
  host.appendChild(button);
  return button;
}
