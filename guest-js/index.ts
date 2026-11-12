// @gaiadesk/tauri-plugin-support: a "Get help" button for your Tauri app.
// Your customer clicks it and agrees; your app's window is shared with your
// support team in the GaiaDesk console, who can, if you allow it, guide it.
//
//   import { startHelp, on } from "@gaiadesk/tauri-plugin-support";
//   await startHelp({ customer: { name: "Ada" }, mode: "cobrowse", guided: true,
//                     branding: { name: "Acme", color: "#0f766e" } });
//   on("started", ({ code }) => …);

import { invoke } from '@tauri-apps/api/core';
import { listen as listenEvent, type UnlistenFn } from '@tauri-apps/api/event';

/** What the support team may do (asked for when the plugin creates the session). */
export type SupportMode = 'view' | 'cobrowse';
/** What is shared: this app's window (default), every window of this app, or the display it is on. */
export type Capture = 'window' | 'app' | 'screen';
/** Who the customer is: at most 16 short fields. Shown to your own agents only. */
export type Customer = Record<string, string | number | boolean | null>;

/** Your company's look on the consent dialog and the "Sharing with …" indicator. */
export interface Branding {
  /** "Sharing with <name>". Default: the plugin config's `company`, then your app's product name. */
  name?: string;
  /** Accent colour, `#rrggbb` or `#rgb`. */
  color?: string;
  /**
   * A PNG: a path (relative ones resolve in your app's bundled resources, `resource_dir()`; under
   * `tauri dev` that is `target/debug/`), an `https://` URL, a `data:image/png;base64,` URL, or
   * `"$appIcon"` (your app's window icon). Over 256 KiB it is downscaled. A logo that cannot be used
   * is a `warning` event and the session goes on with text-only branding.
   */
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
  /**
   * A publishable key (`pk_live_…`, `pk_test_…`, or an older `ak_…`) registered for
   * `app://<your identifier>`. Usually set once in the plugin config instead. A secret key (`sk_…`)
   * is refused (`secret_key_in_app`).
   */
  publishableKey?: string;
  /** An embed token (`gdemb_…`) your backend got from `POST /v1/support/sessions`. Wins over a publishable key. */
  embedToken?: string;
  /** A session request heard by {@link listen} (`sr_…`); prefer {@link acceptRequest}. */
  requestId?: string;
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
  /**
   * `true` only when you asked your user yourself and they agreed: the native dialog is skipped.
   * Your dialog must say who will see what, whether they may guide, and how to stop (README:
   * "Your own consent dialog"). The "Sharing with …" indicator is shown either way.
   */
  consentGranted?: boolean;
  /**
   * Where the "Sharing with …" indicator starts. Default: top right of the screen, clear of your
   * window. Your customer can drag it; where they put it is remembered for your app.
   */
  indicatorPosition?: IndicatorPosition;
  /** Your own consent dialog, used where the OS has none (Linux). Default: `window.confirm`. */
  askConsent?: (question: string) => boolean | Promise<boolean>;
  /**
   * Your own "Also share app logs?" question, asked when {@link attachDiagnostics} has something
   * waiting. Default: a dialog in this page.
   */
  askDiagnostics?: (question: string) => boolean | Promise<boolean>;
  /** Outline every masked region in this page while the session runs (development only). */
  debugMasks?: boolean;
}

/** The indicator's corner. */
export type IndicatorPosition = 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right';

/** Where a session is. */
export type State = 'idle' | 'consent' | 'starting' | 'waiting' | 'connected' | 'ended';
/**
 * Why a session ended. `customer_cancelled`: Cancel on the indicator, or {@link stop}, while still
 * waiting (no agent had joined): the request left your team's queue at once.
 */
export type EndReason = 'stopped' | 'app_stopped' | 'customer_cancelled' | 'disconnected' | 'expired' | 'error';

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
  /** Your team's typical wait for an agent, in seconds, when GaiaDesk has an estimate. */
  estimatedWaitSeconds: number | null;
}

/** A session request: an agent asks your customer to share (see {@link listen}). */
export interface SupportRequest {
  id: string;
  /** Your company's name as the plugin shows it (branding name, config `company`, product name). */
  company: string;
  mode: SupportMode;
  /** The agent's note to the customer, if any. */
  message: string | null;
  /** Unix seconds; the request expires then (10 minutes after it was made). */
  expiresAt: number | null;
}

export interface EventMap {
  /** The session is up: `code` is what your customer can read to an agent. */
  started: { code: string; sessionId: string; mode: SupportMode; expiresAt: number | null; estimatedWaitSeconds: number | null };
  joined: { agent: string | null };
  left: Record<string, never>;
  /** Guided input paused (by the customer on the indicator, or by you) or resumed. */
  paused: { paused: boolean };
  /** A guided request and whether it was applied (`reason`: `masked`, `not_focused`, …). */
  action: { action: string; applied: boolean; reason: string | null };
  error: { code: string; message: string };
  /** Always last, exactly once. `canRate`: {@link rate} may be called (within 24 hours). */
  ended: { reason: EndReason; canRate: boolean };
  state: { state: Exclude<State, 'idle' | 'consent'> };
  /** Something went wrong that does not stop the session (`bad_logo`, `diagnostics_skipped`, an upload's refusal). */
  warning: { code: string; message: string };
  /** macOS Screen Recording is not allowed: `startHelp` refused before creating any session. */
  permission_required: { permission: 'screen'; restartRequired: boolean };
  /** Diagnostics were uploaded after your customer agreed. */
  diagnostics_sent: { attachments: Attachment[] };
  /** {@link listen} is connected. */
  listening: Record<string, never>;
  /** An agent asks your customer to share. Answer with {@link acceptRequest} or {@link declineRequest}. */
  request: SupportRequest;
  /** A request is no longer pending (`accepted`, `declined`, `expired`): close its prompt. */
  request_closed: { id: string; state: string };
  /** Listening stopped for good: the listen token was revoked, expired or refused. */
  listen_stopped: { reason: string; message: string };
}
export type EventName = keyof EventMap;

/** An uploaded attachment, as GaiaDesk answers it. */
export interface Attachment {
  id: string;
  object: 'support_attachment';
  session_id: string;
  filename: string;
  content_type: string;
  size: number;
  created_at: number;
}

/** A refusal: `code` is `declined`, `busy`, `consent_required`, `bad_token`, `library_missing`, `secret_key_in_app`, `permission_required`, … */
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
/**
 * Always masked, whatever selector you pass: password fields, card fields and one-time codes.
 * Put `data-gaiadesk-unmask` on an element (or an ancestor) to show it anyway.
 */
export const SECRET_MASK_SELECTOR = [
  'input[type=password]',
  '[autocomplete^="cc-"]',
  '[autocomplete*=" cc-"]',
  '[autocomplete~="one-time-code"]',
  '[autocomplete~="current-password"]',
  '[autocomplete~="new-password"]',
].join(', ');
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
  return listenEvent<EventMap[K]>(`gaiadesk-support:${name}`, (e) => fn(e.payload));
}

/**
 * The elements to mask: `selector`'s and every secret field ({@link SECRET_MASK_SELECTOR}), less
 * those inside `[data-gaiadesk-unmask]` (unless they carry `data-gaiadesk-mask` themselves).
 */
export function maskedElements(selector: string = DEFAULT_MASK_SELECTOR, root: ParentNode = document): Element[] {
  return Array.from(root.querySelectorAll(`${selector}, ${SECRET_MASK_SELECTOR}`)).filter(
    (el) => el.hasAttribute('data-gaiadesk-mask') || !el.closest('[data-gaiadesk-unmask]'),
  );
}

/** The CSS-pixel rects of the elements to mask (see {@link maskedElements}), visible ones only. */
export function rectsOf(selector: string = DEFAULT_MASK_SELECTOR, root: ParentNode = document): Rect[] {
  return maskedElements(selector, root)
    .map((el) => el.getBoundingClientRect())
    .filter((r) => r.width > 0 && r.height > 0)
    .map((r) => ({ x: r.left, y: r.top, w: r.width, h: r.height }));
}

let follow: ReturnType<typeof setInterval> | undefined;
let endedHooked = false;
let maskSelector = DEFAULT_MASK_SELECTOR;
let overlay: { box: HTMLElement; timer: ReturnType<typeof setInterval> } | undefined;
let overlayForSession = false;

/**
 * Development aid: outline every region that would be masked in this page (red, dashed), kept up to
 * date twice a second. The outlines are in the page, so a session in progress shows them too.
 */
export function showMaskOverlay(show: boolean, selector: string = maskSelector): void {
  if (overlay) {
    clearInterval(overlay.timer);
    overlay.box.remove();
    overlay = undefined;
  }
  if (!show) return;
  const box = document.createElement('div');
  box.setAttribute('data-gaiadesk-overlay', '');
  box.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647';
  const draw = () => {
    box.replaceChildren(
      ...rectsOf(selector).map((r) => {
        const d = document.createElement('div');
        d.style.cssText = `position:absolute;left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px;outline:2px dashed #dc2626;background:rgba(220,38,38,.15)`;
        return d;
      }),
    );
  };
  draw();
  document.body.appendChild(box);
  overlay = { box, timer: setInterval(draw, 500) };
}

/** Keep the masks on `selector`'s elements as they move, until the session ends. */
function followMasks(selector: string): void {
  clearInterval(follow);
  maskSelector = selector;
  if (!endedHooked) {
    endedHooked = true;
    void on('ended', () => {
      clearInterval(follow);
      if (overlayForSession) showMaskOverlay(false);
      overlayForSession = false;
    });
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
  const { askConsent, askDiagnostics, debugMasks, masks = DEFAULT_MASK_SELECTOR, ...rest } = options;
  const selector = typeof masks === 'string' ? masks : null;
  const send = { ...rest, masks: selector === null ? masks : rectsOf(selector), masking: selector !== null || masks.length > 0 };
  let status: Status;
  try {
    status = await call<Status>('start_help', { options: send });
  } catch (e) {
    if (!(e instanceof SupportError) || e.code !== 'no_native_dialog') throw e;
    const yes = await (askConsent ?? ((q: string) => window.confirm(q)))(question(options));
    if (!yes) {
      if (options.requestId) await declineRequest(options.requestId).catch(() => undefined);
      throw new SupportError('declined', 'your user declined');
    }
    status = await call<Status>('start_help', { options: { ...send, consentGranted: true } });
  }
  if (selector !== null) followMasks(selector);
  if (debugMasks) {
    overlayForSession = true;
    showMaskOverlay(true, selector ?? DEFAULT_MASK_SELECTOR);
  }
  askDiagnosticsWith = askDiagnostics;
  companyName = options.branding?.name;
  // Never fails the session: a refused upload is a `warning` event.
  await offerDiagnostics().catch(() => undefined);
  return status;
}

/** What this app build can do. Never throws: `{ available: false }` when the plugin is not registered (an older build). */
export type Info =
  | { available: false }
  | {
      available: true;
      pluginVersion: string;
      libraryVersion: string | null;
      libraryFound: boolean;
      /** Where the library was loaded from. */
      libraryPath: string | null;
      /** Why it could not be loaded (`library_missing`, `library_unusable`). */
      libraryError: { code: string; message: string } | null;
      platform: 'macos' | 'windows' | 'linux' | string;
      /** Guided input works here (not on Linux: view-only). */
      guidedSupported: boolean;
      screenPermission: ScreenPermission;
      /** The company name the plugin shows. */
      company: string;
    };

export async function getInfo(): Promise<Info> {
  try {
    return await invoke<Info>(PLUGIN + 'get_info');
  } catch {
    return { available: false };
  }
}

/**
 * macOS Screen Recording for this app: `granted` or `denied` (macOS cannot tell "never asked" from
 * "refused", so both read `denied`). Always `granted` on Windows and Linux.
 */
export type ScreenPermission = 'granted' | 'denied';

export function screenPermission(): Promise<ScreenPermission> {
  return call('screen_permission');
}

/** Show macOS's Screen Recording prompt (first time only; afterwards it does nothing and you open Settings). A grant counts after the app relaunches. */
export function requestScreenPermission(): Promise<ScreenPermission> {
  return call('request_screen_permission');
}

/** Open System Settings → Privacy & Security → Screen Recording (macOS; nothing elsewhere). */
export function openScreenPermissionSettings(): Promise<void> {
  return call('open_screen_permission_settings');
}

/**
 * Your customer's rating of the session that just ended, 1 to 5, with an optional comment (at most
 * 500 characters). Once per session, within 24 hours of its end (`ended` says `canRate`).
 */
export function rate(rating: number, comment?: string): Promise<{ rating: number; comment: string | null; rated_at: number }> {
  return call('rate', { rating, comment: comment ?? null });
}

export interface Diagnostics {
  /** Files to send (each its last `maxBytes`), e.g. your app's log files. */
  files?: string[];
  /** Text to send as `diagnostics.txt`. */
  text?: string;
  /** Per file, at most 10 MiB (the default). Up to 10 attachments and 25 MiB per session. */
  maxBytes?: number;
}

let diagnosticsWaiting = false;
let askDiagnosticsWith: HelpOptions['askDiagnostics'];
let companyName: string | undefined;

/**
 * Offer your support team diagnostics (logs). Nothing is sent until your customer agrees: they are
 * asked "Also share app logs with <Company>?" right after they agree to share (or at once, when a
 * session is already up), and only a yes uploads them. A later call replaces what is waiting.
 */
export async function attachDiagnostics(d: Diagnostics, ask?: HelpOptions['askDiagnostics']): Promise<void> {
  const active = await call<boolean>('attach_diagnostics', { diagnostics: d });
  diagnosticsWaiting = true;
  if (ask) askDiagnosticsWith = ask;
  if (active) await offerDiagnostics();
}

async function offerDiagnostics(): Promise<void> {
  if (!diagnosticsWaiting) return;
  diagnosticsWaiting = false;
  const info = await getInfo();
  const company = companyName ?? (info.available ? info.company : 'our');
  const q = `Also share app logs with ${company} support? They help find what went wrong.`;
  const yes = await (askDiagnosticsWith ?? confirmInPage)(q);
  await call(yes ? 'send_diagnostics' : 'discard_diagnostics');
}

/** A small yes/no dialog in this page (macOS's webview has no `window.confirm`). */
export function confirmInPage(question: string, yes = 'Share logs', no = 'Not now'): Promise<boolean> {
  return new Promise((resolve) => {
    const shade = document.createElement('div');
    shade.setAttribute('role', 'dialog');
    shade.setAttribute('aria-modal', 'true');
    shade.style.cssText = 'position:fixed;inset:0;z-index:2147483646;display:flex;align-items:center;justify-content:center;background:rgba(0,0,0,.35);font:14px/1.4 system-ui,-apple-system,"Segoe UI",sans-serif';
    const card = document.createElement('div');
    card.style.cssText = 'background:Canvas;color:CanvasText;max-width:360px;padding:20px;border-radius:12px;box-shadow:0 10px 30px rgba(0,0,0,.3)';
    const p = document.createElement('p');
    p.textContent = question;
    p.style.margin = '0 0 16px';
    const row = document.createElement('div');
    row.style.cssText = 'display:flex;gap:8px;justify-content:flex-end';
    const button = (label: string, answer: boolean) => {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = label;
      b.addEventListener('click', () => {
        shade.remove();
        resolve(answer);
      });
      return b;
    };
    const noButton = button(no, false);
    row.append(noButton, button(yes, true));
    card.append(p, row);
    shade.append(card);
    document.body.appendChild(shade);
    noButton.focus();
  });
}

/**
 * Listen for session requests: an agent asks your customer to share from the console ("Ask to
 * share"). `listenToken` (`gdlis_…`) comes from your backend (`POST /v1/support/listeners` with a
 * secret key, for this signed-in user). Reconnects with backoff; stops for good (`listen_stopped`)
 * when the token is revoked or expires. Events: `listening`, `request`, `request_closed`.
 */
export function listen(options: { listenToken: string }): Promise<void> {
  return call('listen', { listenToken: options.listenToken });
}

export function stopListening(): Promise<void> {
  return call('stop_listening');
}

/** Your customer allowed request `id`: ask their consent (as `startHelp` does), accept it, and start sharing. */
export function acceptRequest(id: string, options: Omit<HelpOptions, 'requestId' | 'embedToken' | 'publishableKey'> = {}): Promise<Status> {
  return startHelp({ ...options, requestId: id });
}

export function declineRequest(id: string): Promise<void> {
  return call('decline_request', { id });
}

/**
 * End the session (the `ended` event follows: `app_stopped`, or `customer_cancelled` while your
 * customer is still waiting — the request then leaves your team's queue at once).
 */
export function stop(): Promise<Status> {
  clearInterval(follow);
  if (overlayForSession) showMaskOverlay(false);
  overlayForSession = false;
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
      // macOS shows its Screen Recording prompt the first time; the app needs a relaunch after.
      if (e.code === 'permission_required') void requestScreenPermission().catch(() => undefined);
      if (e.code !== 'declined') onError(e);
    });
  });
  host.appendChild(button);
  return button;
}
