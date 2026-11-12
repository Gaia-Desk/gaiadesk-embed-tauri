// The consent step (libgaiadesk_embed 0.4.0): the library's own dialog (`consent`), or the
// `<gaiadesk-consent>` web component in your page with `consentBridge`. Options and decisions use
// the library's snake_case JSON, so one options object works for both.

import { call } from './core.js';

/** What is shared: this window, every window of this app, or the whole screen. */
export type ConsentCapture = 'window' | 'app' | 'screen';

/** A rect in CSS pixels, relative to the top-left of the page. */
export interface ConsentRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ConsentTheme {
  appearance?: 'light' | 'dark' | 'auto';
  /** `#RRGGBB` */
  background?: string;
  surface?: string;
  text?: string;
  accent?: string;
  /** px, 0..24 */
  radius?: number;
  /** A family name; falls back to the system UI font. */
  font_family?: string;
  button_style?: 'filled' | 'outline' | 'pill';
}

/**
 * The consent options: every key optional, the same object `<gaiadesk-consent>` takes. The plugin
 * fills `company`, `brand_color`, `logo_png_base64` (from its config), `parent_window` (the calling
 * window) and `app_version` (your app's) when you leave them out; `masks` are CSS pixels of this page.
 */
export interface ConsentOptions {
  company?: string;
  capture?: ConsentCapture;
  /** You want click-and-type; your customer may still say no (see `choices`). */
  guided?: boolean;
  /** You pass masks: the dialog says private fields stay hidden. */
  masked?: boolean;
  brand_color?: string;
  logo_png_base64?: string;
  parent_window?: number;
  theme?: ConsentTheme;
  /** `"auto"` (the system's languages) or a BCP-47 tag. */
  locale?: string;
  /** Per-key overrides of the default wording (`{company}`, `{team}`, `{minutes}`, `{count}`). */
  wording?: Record<string, string>;
  wording_version?: string;
  choices?: { offer_guided?: boolean; offer_logs?: boolean; offer_capture?: boolean };
  privacy_url?: string;
  terms_url?: string;
  /** 0 = never; else the dialog declines itself, decision `"timeout"`. */
  timeout_seconds?: number;
  /** The live preview of what support would see (default true). */
  preview?: boolean;
  /** With `embed_token` and `session_id` the library shows who is waiting and posts the receipt itself. */
  server?: string;
  embed_token?: string;
  session_id?: string;
  app_version?: string;
  window_id?: number;
  masks?: ConsentRect[];
}

/** What your customer chose. */
export interface Choices {
  guided: boolean;
  share_logs: boolean;
  capture: ConsentCapture;
}

/** The record of the answer: when, which words (version and hash), what was chosen, on which machine account and app. */
export interface ConsentReceipt {
  id: string;
  decision: 'accepted' | 'declined' | 'timeout';
  at: string;
  surface: 'native_dialog' | 'embedded';
  locale: string;
  wording_version: string;
  wording_sha256: string;
  choices: Choices;
  os_user: string | null;
  os: string | null;
  app_name: string | null;
  app_version: string | null;
  sdk: string;
}

/**
 * The decision. `unavailable`: no dialog on this system (show `<gaiadesk-consent>` instead).
 * Accepted: pass `consent_token` and `receipt` to `startHelp` (`consentToken`, `consentReceipt`).
 */
export interface Decision {
  decision: 'accepted' | 'declined' | 'timeout' | 'unavailable';
  choices?: Choices;
  consent_token?: string;
  receipt?: ConsentReceipt;
}

/** What `<gaiadesk-consent>` hands its bridge on a decision. */
export interface RecordConsentInput {
  decision: 'accepted' | 'declined' | 'timeout';
  choices: Choices;
  /** The strings actually shown. */
  wording: Record<string, string>;
  wording_version: string;
  locale: string;
  company: string;
  capture: ConsentCapture;
  app_version: string | null;
}

export interface PreviewRequest {
  capture: ConsentCapture;
  parent_window?: number;
  window_id?: number;
  /** CSS pixels of this page. */
  masks?: ConsentRect[];
}

export interface PreviewFrame {
  png_base64: string;
  width: number;
  height: number;
  masked_count: number;
}

export interface WaitInfo {
  team_name: string;
  team_avatar_url: string | null;
  estimated_wait_seconds: number | null;
}

/** `@gaiadesk/consent`'s bridge (structurally: no dependency on that package). */
export interface ConsentBridge {
  preview?(opts: PreviewRequest): Promise<PreviewFrame | null>;
  waitInfo?(): Promise<WaitInfo | null>;
  recordConsent(input: RecordConsentInput): Promise<Decision>;
}

/**
 * The library's consent dialog (modal, attached to this window): what support will see, a live
 * preview, your customer's choices, who is waiting. Resolves with the decision (its receipt made by
 * the library); events `consent_shown`, `consent_accepted`, `consent_declined`. Rejects with
 * `unsupported` before libgaiadesk_embed 0.4.0.
 */
export function consent(options: ConsentOptions = {}): Promise<Decision> {
  return call('consent', { options });
}

/** One frame of exactly what support would see, masks filled black (rejects `capture_unavailable`). */
export function preview(options: PreviewRequest = { capture: 'window' }): Promise<PreviewFrame> {
  return call('preview', { options });
}

/**
 * Your own (or the embedded) consent UI's answer, made a receipt by the library (`surface:
 * "embedded"`). With `embed_token` and `session_id` it posts the receipt too (a decline or timeout
 * then ends the waiting request).
 */
export function recordConsent(input: RecordConsentInput & { embed_token?: string; session_id?: string }): Promise<Decision> {
  return call('record_consent', { input });
}

/** Who is waiting for the session: the support team and its typical wait. */
export function waitInfo(session: { embedToken: string; sessionId: string }): Promise<WaitInfo> {
  return call('wait_info', { embedToken: session.embedToken, sessionId: session.sessionId });
}

/**
 * A bridge for `<gaiadesk-consent>`. With the session (`embedToken`, `sessionId`, from your
 * backend's create) it also says who is waiting and has the library post every receipt, so a
 * decline ends the request at once; without, an accepted receipt is posted by `startHelp`.
 */
export function createConsentBridge(session?: { embedToken: string; sessionId: string }): ConsentBridge {
  const bridge: ConsentBridge = {
    preview,
    recordConsent: (input) => recordConsent(session ? { ...input, embed_token: session.embedToken, session_id: session.sessionId } : input),
  };
  if (session) bridge.waitInfo = () => waitInfo(session);
  return bridge;
}

/** `el.bridge = consentBridge`: the preview and a library-made receipt (see {@link createConsentBridge}). */
export const consentBridge: ConsentBridge = createConsentBridge();
