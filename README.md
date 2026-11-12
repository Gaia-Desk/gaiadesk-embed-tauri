# gaiadesk-embed-tauri

[![CI](https://github.com/Gaia-Desk/gaiadesk-embed-tauri/actions/workflows/ci.yml/badge.svg)](https://github.com/Gaia-Desk/gaiadesk-embed-tauri/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/tauri-plugin-gaiadesk-support)](https://crates.io/crates/tauri-plugin-gaiadesk-support)
[![npm](https://img.shields.io/npm/v/@gaiadesk/tauri-plugin-support)](https://www.npmjs.com/package/@gaiadesk/tauri-plugin-support)
[![License: MIT + binary licence](https://img.shields.io/badge/license-MIT%20%2B%20binary%20licence-blue)](#licence)

A **"Get help" button for your Tauri app**: in-app remote support for Tauri v2 desktop apps on
macOS, Windows and Linux, for customer support with optional guided control. Your customer clicks
it and agrees; **your app's window** is shared with **your** support team, who see it in the
GaiaDesk console's Support page and, if you allow it, guide it: click and type **inside your app
only**, while it is in front.

- A Tauri v2 plugin (`tauri-plugin-gaiadesk-support`) and its typed TypeScript API
  (`@gaiadesk/tauri-plugin-support`), over GaiaDesk's prebuilt native library `libgaiadesk_embed`.
- **One click**: with a publishable key in your config, `startHelp()` asks, creates the session and
  starts. Or hand it an embed token from your own backend.
- **Consent first**: the library's native dialog, attached to the window that asked (a sheet on
  macOS, an owned window on Windows), light or dark with the system, in your colour and logo, with a
  checklist of exactly what is shared — or [your own dialog](#your-own-consent-dialog). Nothing is
  shared until your user agrees.
- Shares **your app's own window** by default. Other apps and the desktop are never in the picture
  unless you opt into `screen`.
- A visible **"Sharing with Acme"** indicator, drawn by the library, for as long as anything is
  shared: always on top, draggable (its place is remembered for your app), **Cancel** while your
  customer waits (the request leaves your team's queue at once), **Stop sharing** once an agent
  joined. It can never be hidden.
- **Masks**: password, card and one-time-code fields are masked by default; mark any other element
  `data-gaiadesk-mask` and it is painted black before a frame is encoded, followed as it moves.
- **View-only by default.** Guided input lands only in your app's own windows, and your customer
  can pause it.

Not in a desktop app? See the [web SDK](https://gaiadesk.net/docs/web-in-app-support), the
[website widget](https://gaiadesk.net/docs/website-widget) or
[Electron](https://gaiadesk.net/docs/electron-in-app-support).

## Install

```sh
cargo add tauri-plugin-gaiadesk-support@0.3                # in src-tauri/
cargo add --build gaiadesk-embed-fetch@0.3                 # in src-tauri/: fetches the library at build time
npm install @gaiadesk/tauri-plugin-support@0.3             # your frontend
```

### Versions that go together

| Part | Version |
|---|---|
| `tauri-plugin-gaiadesk-support` (crates.io) | 0.4.0 |
| `gaiadesk-embed-fetch` (crates.io, build helper) | 0.4.0 |
| `libgaiadesk_embed` (the native library it fetches) | 0.3.0 |
| `@gaiadesk/tauri-plugin-support` (npm) | 0.4.0 |
| Tauri | 2.x (the crate needs `tauri` 2.11 or later) |

The plugin has its own 0.x version line; it is not Tauri's 2.x. Keep the crate, the fetch helper
and the npm package on the same version: the fetch helper pins the library version that goes with
them (0.4.0 fetches library 0.3.0). With an older library (0.2.0) the plugin still works: the
dialog is not attached to your window, the indicator ignores `indicatorPosition` and its place is
not remembered, and a cancel while waiting ends as `app_stopped`.

## Quick start

### 1. Bundle the library

`src-tauri/build.rs`:

```rust
fn main() {
    // Downloads libgaiadesk_embed for the target into src-tauri/gaiadesk/ (once), after
    // checking its SHA-256 against the hash pinned in gaiadesk-embed-fetch.
    gaiadesk_embed_fetch::fetch("gaiadesk").expect("libgaiadesk_embed");
    tauri_build::build();
}
```

Then tell Tauri to ship it, one file per platform. Tauri merges `tauri.<os>.conf.json` over
`tauri.conf.json` as a [JSON Merge Patch (RFC 7396)](https://datatracker.ietf.org/doc/html/rfc7396):
**objects merge key by key, lists are replaced whole**. So keep `bundle.resources` a *map*
(`"source": "target"`) in `tauri.conf.json` too: the platform file's entries are then added to yours.
A resources *list* in `tauri.conf.json` would be replaced by the platform file's map, and its
entries lost on that platform. Likewise, if you already list `bundle.macOS.frameworks`, repeat them
in `tauri.macos.conf.json`. The platform files hold only the GaiaDesk entries:

`src-tauri/tauri.macos.conf.json`

```json
{
  "bundle": {
    "macOS": { "frameworks": ["gaiadesk/libgaiadesk_embed.dylib"] },
    "resources": { "gaiadesk/LICENSE-BINARY": "gaiadesk/LICENSE-BINARY" }
  }
}
```

`src-tauri/tauri.windows.conf.json`

```json
{
  "bundle": {
    "resources": {
      "gaiadesk/gaiadesk_embed.dll": "gaiadesk/gaiadesk_embed.dll",
      "gaiadesk/LICENSE-BINARY": "gaiadesk/LICENSE-BINARY"
    }
  }
}
```

`src-tauri/tauri.linux.conf.json`

```json
{
  "bundle": {
    "resources": {
      "gaiadesk/libgaiadesk_embed.so": "gaiadesk/libgaiadesk_embed.so",
      "gaiadesk/LICENSE-BINARY": "gaiadesk/LICENSE-BINARY"
    }
  }
}
```

and your own resources stay in `tauri.conf.json`, as a map; on Windows the bundle then gets
`icons/logo.png`, `gaiadesk/gaiadesk_embed.dll` and `gaiadesk/LICENSE-BINARY`:

```json
{ "bundle": { "resources": { "icons/logo.png": "icons/logo.png" } } }
```

On macOS the library goes in `Contents/Frameworks`, where Tauri signs it with your app (a dylib in
`Resources` is not signed and fails notarization). Add `src-tauri/gaiadesk/` to your `.gitignore`.

### 2. Register the plugin

`src-tauri/src/lib.rs` (or `main.rs`):

```rust
tauri::Builder::default()
    .plugin(tauri_plugin_gaiadesk_support::init())
```

`src-tauri/tauri.conf.json`:

```json
{
  "plugins": {
    "gaiadesk-support": {
      "publishableKey": "pk_live_…",
      "company": "Acme",
      "branding": { "color": "#0f766e", "logo": "icons/logo.png" }
    }
  }
}
```

`src-tauri/capabilities/default.json`: add `"gaiadesk-support:default"` to `permissions`.

**Never a secret key.** `publishableKey` takes a publishable key: `pk_live_…`, `pk_test_…` for test
mode, or an older `ak_…` one. A secret key (`sk_live_…`, `sk_test_…`) can do anything on your
account, and anything in `tauri.conf.json` ships inside your app, so the plugin refuses it twice:
its build script fails the build when the config the Tauri CLI passes it (`tauri.conf.json`, the
platform files and `--config`, merged) holds an `sk_` key, and `startHelp` rejects with
`secret_key_in_app` (before any dialog or request) when the configured key, a `Builder` key or a
`publishableKey` option starts with `sk_`. A plain `cargo build` without the Tauri CLI skips the
build-time check; the run-time one always applies.

### 3. Add the button

```ts
import { mountHelpButton, on } from "@gaiadesk/tauri-plugin-support";

mountHelpButton("#help", { customer: { name: "Ada", plan: "pro" }, mode: "cobrowse", guided: true });
on("started", ({ code }) => showStatus(`Your support code is ${code}`));
on("joined", ({ agent }) => showStatus(`${agent ?? "An agent"} is viewing`));
```

Or wire your own button to `startHelp(options)` and `stop()`. Your support team opens **Support**
in the GaiaDesk console, sees the waiting customer and joins (or types the nine-digit code).

## Two ways to create the session

**A publishable key** (the plugin creates the session itself). Mint a key on your GaiaDesk account
page (API keys → *Publishable*) and register the origin **`app://<your identifier>`**, lowercased:
for `"identifier": "com.acme.desk"`, that is `app://com.acme.desk`. The plugin's Rust side sends
`POST https://api.gaiadesk.net/v1/support/sessions` with that `Origin`. A publishable key can only
create support sessions, at most an hour long, and the customer details are your app's word
(`customer_verified: false` in the console). The request comes from Rust, not from your webview,
so its `Origin` is `app://<identifier>` whatever the webview has loaded.

**An embed token from your backend** (the session is created with your secret key, scope
`support`, and you choose its mode and verified customer details):

```sh
curl -s -X POST https://api.gaiadesk.net/v1/support/sessions \
  -H "Authorization: Bearer $GAIADESK_API_KEY" -H "Content-Type: application/json" \
  -d '{"mode": "cobrowse", "customer": {"name": "Ada", "plan": "pro"}, "origin": "app://com.acme.desk"}'
```

`origin` is optional; if you pin one, make it `app://<identifier>`. The library connects without
an `Origin`, which only an `app://` pin admits (an `https://` pin is refused, `origin_not_allowed`).
The answer, `201 Created`:

```json
{
  "id": "ss_0123456789abcdef",
  "state": "waiting",
  "mode": "cobrowse",
  "customer": { "name": "Ada", "plan": "pro" },
  "customer_present": false,
  "customer_verified": true,
  "join_code": "123456789",
  "join_url": "https://gaiadesk.net/app/support.html#session=ss_0123456789abcdef",
  "desk_id": null,
  "origin": "app://com.acme.desk",
  "owner": "you@acme.com",
  "created_at": 1767225600,
  "expires_at": 1767229200,
  "joined_at": null,
  "joined_by": null,
  "ended_at": null,
  "end_reason": null,
  "embed_token": "gdemb_…"
}
```

`created_at`, `expires_at`, `joined_at` and `ended_at` are Unix seconds (numbers); `expires_at` is
`created_at` + `expires_in` (default 3600). `embed_token` (`gdemb_` + 64 hex) is shown only here.
Every field: [support API reference](https://gaiadesk.net/docs/ref-support).

```ts
const { embedToken } = await myBackend.post("/support-session");
await startHelp({ embedToken, guided: true });
```

Never ship the secret key in the app: the embed token is all it needs.

### Remote-URL apps

If your window loads your site (`"url": "https://yourapp.com"`) instead of bundled files:

- The publishable key still needs **`app://<identifier>`**: the plugin creates the session from
  Rust with that `Origin`, not from your page. The key may also list `https://yourapp.com` if the
  same key serves the web SDK in browsers; the plugin does not need it.
- Bundle `@gaiadesk/tauri-plugin-support` into your site's own JavaScript.
- Grant the permission to the remote URL with the capability's `remote.urls`
  ([URLPattern](https://urlpattern.spec.whatwg.org/) strings: `https://yourapp.com` matches every
  path on that host, `https://*.yourapp.com` its subdomains). Any page there can then start and
  stop a session in that window, so list only origins you control.

`src-tauri/capabilities/remote-help.json`:

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "remote-help",
  "description": "Lets yourapp.com, loaded in the main window, start GaiaDesk support.",
  "windows": ["main"],
  "remote": { "urls": ["https://yourapp.com"] },
  "permissions": ["gaiadesk-support:default"]
}
```

## Options

`startHelp(options)` (and `mountHelpButton(target, options)`):

| Option | |
|---|---|
| `publishableKey` | Overrides the config's. |
| `embedToken` | `gdemb_…` from your backend. Wins over a publishable key. |
| `requestId` | A session request (`sr_…`) heard by `listen`; use `acceptRequest(id, options)`. |
| `customer` | Who the customer is: at most 16 short fields (publishable-key flow). |
| `mode` | `view` (default) or `cobrowse` (publishable-key flow; your backend decides otherwise). |
| `expiresIn` | Seconds, 60 to 3600 (publishable-key flow). |
| `guided` | Allow guided input (default `false`). Needs a `cobrowse` session too. |
| `capture` | `window` (default): this app's window. `app`: every window of this app (macOS; elsewhere the focused one, followed). `screen`: the display it is on (opt-in). |
| `branding` | `{ name, color, logo }`: see below. Merged over the config's `branding`. |
| `masks` | Rects in CSS pixels, or a selector (default `[data-gaiadesk-mask]`; secret fields are always added, see Masks). |
| `debugMasks` | Outline every masked region in the page while the session runs (development). |
| `fps` | 1 to 30 (default 15). |
| `consentGranted` | `true` only when you asked your user yourself and they agreed: see [Your own consent dialog](#your-own-consent-dialog). |
| `indicatorPosition` | `top-left`, `top-right`, `bottom-left` or `bottom-right`. Default: top right of the screen, moved to the first corner clear of your window (bottom right when your window fills the screen). Where your customer drags it wins, remembered in `<app data dir>/gaiadesk-support/indicator.json`. |
| `askConsent` | Your dialog where the OS has none (Linux). Default `window.confirm`. |
| `askDiagnostics` | Your "Also share app logs?" dialog (see Diagnostics). Default: a small dialog in the page. |

Plugin config (`plugins."gaiadesk-support"`, or `Builder` in Rust): `publishableKey`, `company`,
`server` (default `https://gaiadesk.net`), `branding`, `libraryPath`.

```rust
.plugin(tauri_plugin_gaiadesk_support::Builder::new().publishable_key(env!("ACME_GAIADESK_KEY")).company("Acme").build())
```

## Your own consent dialog

The library's dialog is the default. To ask in your own UI instead (your design system, your words),
ask first and pass `consentGranted: true` — a first-class path, not a workaround. The rules:

- Ask **every time**, right before `startHelp`, and only start on an explicit yes. Never pass
  `consentGranted: true` without asking.
- Say **who** will see **what** (`capture`: this window, your app's windows, or the whole screen),
  **whether they may guide** (`guided`), that **private fields stay hidden** (masks), and **how to
  stop**.
- The "Sharing with …" indicator is shown either way, for the whole session, and cannot be hidden:
  that is how your customer stops.

The minimum wording (adapt the style, keep every point):

> **Share this window with Acme support?**
>
> Acme's support team will see this window until you stop sharing. Nothing else on your computer is
> shared.
>
> - They see this window only *(capture `app`: "this app's windows"; `screen`: "your whole screen,
>   including other apps")*
> - View only: they cannot click or type *(guided: "They may click and type in this app; you can
>   pause that any time")*
> - Passwords and private fields stay hidden
> - Stop any time with "Stop sharing" on the Acme bar
>
> [Don't Share] [**Share**]

```ts
import { startHelp } from '@gaiadesk/tauri-plugin-support';

async function getHelp() {
  const yes = await myDialog.confirm({
    title: 'Share this window with Acme support?',
    body: "Acme's support team will see this window until you stop sharing. Nothing else on your computer is shared.",
    checklist: [
      'They see this window only',
      'View only: they cannot click or type',
      'Passwords and private fields stay hidden',
      'Stop any time with "Stop sharing" on the Acme bar',
    ],
    confirm: 'Share',
    cancel: "Don't Share",
  });
  if (yes) await startHelp({ consentGranted: true });
}
```

Keep what you say and what you pass in step: a dialog that said "view only" must not start with
`guided: true`, nor one that said "this window" with `capture: 'screen'`.

## White-label

| `branding` | |
|---|---|
| `name` | "Sharing with **Acme**" on the dialog and the indicator. Default: the config's `company`, then your app's product name. |
| `color` | Accent colour, `#rrggbb` or `#rgb`. |
| `logo` | A PNG: a path, an `https://` URL, a `data:image/png;base64,` URL, or `"$appIcon"` (your app's own window icon). |

**Where a relative path resolves:** in your app's bundled resources directory (Tauri's
`resource_dir()`: `Contents/Resources` in a macOS app, the install directory on Windows,
`/usr/lib/<app>` for a Linux package; under `tauri dev`, `target/debug/`), so list the file in
`bundle.resources` with that target path (`"logo": "icons/logo.png"` needs
`"icons/logo.png": "icons/logo.png"`). An absolute path is read as is. `"$appIcon"` uses Tauri's
default window icon (`bundle.icon`), so nothing needs bundling.

**A logo never stops a session.** One over 256 KiB (the library's limit, 262,144 bytes) is
downscaled until it fits (sources up to 8 MiB). One that cannot be read, downloaded or decoded, or
is not a PNG, is a `warning` event (`{code: "bad_logo", message}`) and the session goes on with
your name and colour only. A bad `color` still rejects with `bad_color`.

Colour and logo need `libgaiadesk_embed` 0.2 or later; with 0.1.0 the plugin shows the plain dialog
and indicator with your name, and drops them silently.

## Masks

Secrets are masked by default: `input[type=password]` and fields whose `autocomplete` is a card field
(`cc-*`), `one-time-code`, `current-password` or `new-password`, whatever selector you pass. Put
`data-gaiadesk-unmask` on an element (or an ancestor) to show it anyway; an element that carries
`data-gaiadesk-mask` itself is masked even there. Anything marked `data-gaiadesk-mask` is painted
out from the first frame too, and every masked element is followed while the session lasts
(resizes, scrolling, moving to another display). Or pass `masks: [{x, y, w, h}]` in CSS
pixels of the page, or call `setMasks(rects | selector)` yourself; `[]` clears. The plugin turns
CSS pixels into the library's units (points from the window frame on macOS, `GetWindowRect` pixels
on Windows, X11 pixels on Linux) for the window that called. Masks **fail closed**: if the window
cannot be located, the whole picture is painted out. Page zoom other than 100 % is not accounted for.
Rects you pass yourself are used as they are (no secret scan).

To see what is masked while you develop, `showMaskOverlay(true)` (or `startHelp({ debugMasks: true })`)
outlines every masked region of the page in red, kept up to date; `showMaskOverlay(false)` removes
it. The outlines are part of the page, so an agent in a session sees them too: development only.
`maskedElements(selector?)` and `rectsOf(selector?)` return what would be masked.

## Events

`on(name, handler)` returns a promise of the unlisten function. Tauri event names are
`gaiadesk-support:<name>`.

| Event | Payload | |
|---|---|---|
| `started` | `{code, sessionId, mode, expiresAt, estimatedWaitSeconds}` | The session is up; `code` is the nine-digit support code; `estimatedWaitSeconds` your team's typical wait for an agent when GaiaDesk has an estimate (publishable-key and request sessions), else `null` |
| `joined` | `{agent}` | An agent joined (their email, or `null`) |
| `left` | `{}` | The agent left |
| `paused` | `{paused}` | Guided input paused (by the customer on the indicator, or by you) or resumed |
| `action` | `{action, applied, reason}` | A guided request and its outcome (`masked`, `not_focused`, …) |
| `error` | `{code, message}` | e.g. `capture_failed`, `connect_failed` |
| `ended` | `{reason, canRate}` | `stopped` (the customer), `app_stopped`, `customer_cancelled` (Cancel on the indicator, or `stop()`, while still waiting: no agent had joined; gone from your team's queue at once, and no agent can join it), `disconnected`, `expired`, `error`. Always last. `canRate`: `rate()` may be called |
| `state` | `{state}` | `starting`, `waiting`, `connected`, `ended` |
| `warning` | `{code, message}` | Something that does not stop the session: `bad_logo`, `diagnostics_skipped`, an upload's refusal |
| `permission_required` | `{permission, restartRequired}` | macOS Screen Recording is not allowed; `startHelp` refused before creating a session |
| `diagnostics_sent` | `{attachments}` | Diagnostics uploaded after the customer agreed |
| `listening` | `{}` | `listen` is connected |
| `request` | `{id, company, mode, message, expiresAt}` | An agent asks your customer to share |
| `request_closed` | `{id, state}` | That request is no longer pending (`accepted`, `declined`, `expired`) |
| `listen_stopped` | `{reason, message}` | The listen token was revoked, expired or refused; listening stopped |

Commands: `startHelp`, `stop`, `pause`, `resume` (refused, `pause_refused`, when the customer
paused), `setMasks`, `status` (`{state, code, sessionId, mode, agent, paused, endReason,
estimatedWaitSeconds}`), `getInfo`, `screenPermission`, `requestScreenPermission`,
`openScreenPermissionSettings`, `rate`, `attachDiagnostics`, `listen`, `stopListening`,
`acceptRequest`, `declineRequest`, `showMaskOverlay`.
Refusals reject with a `SupportError` whose `code` is the library's (`consent_required`,
`bad_token`, `bad_config`, `bad_rects`, …) or the plugin's: `declined`, `busy`, `no_session`,
`library_missing`, `secret_key_in_app`, `permission_required`, `not_listening`, `create_failed`
or the server's reason (`too_many_open_sessions`, `request_expired`, …), `bad_color`,
`bad_options`. A branding value the library's dialog refuses rejects with `bad_config` (and an
`error` event), never as `declined`.

## Feature detection

`getInfo()` never throws. In a build without the plugin (an older version of your app, or the
plugin not registered) it answers `{ available: false }`, so a shared frontend can hide its help
button without try/catch:

```ts
const info = await getInfo();
// { available: true, pluginVersion: "0.4.0", libraryVersion: "0.3.0", libraryFound: true,
//   libraryPath: "…/Frameworks/libgaiadesk_embed.dylib", libraryError: null, platform: "macos",
//   guidedSupported: true, screenPermission: "granted", company: "Acme" }
if (!info.available || !info.libraryFound) hideHelpButton();
```

`guidedSupported` is `false` on Linux (view-only). Loading the library to answer is the same load
`startHelp` would do.

## Ratings

When a session ends, `ended` says `canRate: true` if the plugin holds the session's credentials
(every session this app run started). Ask your customer however you like, then:

```ts
on("ended", async ({ canRate }) => {
  if (canRate) await rate(5, "Quick fix, thanks!");    // 1 to 5; comment optional, ≤ 500 characters
});
```

The rating goes to `POST /v1/support/sessions/{id}/csat` with the session's own embed token, which
the plugin keeps in memory (never on disk) until the session is rated, 24 hours after its end, or
the next session. It shows on the session in the console and in the `support.session.rated`
webhook. Server refusals: `already_rated`, `rating_window_closed`, `bad_rating`, `bad_comment`;
nothing to rate: `no_session`.

## Diagnostics

Your app can offer your support team its logs; your customer decides:

```ts
await attachDiagnostics({ files: [await appLogPath()], text: JSON.stringify(state), maxBytes: 2_000_000 });
await startHelp();   // after "Share this window?": "Also share app logs with Acme support?"
```

Nothing is sent until your customer says yes. The question comes right after they agree to share
(or at once, when `attachDiagnostics` is called while a session is up). The library's consent
dialog takes no extra line, and macOS's webview has no `window.confirm`, so it is a second, small
dialog drawn in your page (`confirmInPage`); pass `askDiagnostics(question)` to `startHelp` or
`attachDiagnostics` to ask with your own UI. A no discards what was attached.

Each file is sent as its last `maxBytes` (at most and by default 10 MiB; logs grow at the end),
`text` as `diagnostics.txt`; at most 10 attachments and 25 MiB per session. They are uploaded with
the session's embed token to `POST /v1/support/sessions/{id}/attachments`, appear on the session in
the console (download only), are kept 30 days, and fire `support.session.attachment_added`. A file
that cannot be read is skipped with a `warning`; a refused upload is a `warning` and stops the
rest. Files are read by the plugin's Rust side, so any path the app can read can be attached: only
attach what you mean to.

## Session requests

An agent can ask a customer to share from the console ("Ask to share"). Your backend vouches for the
signed-in user by minting a listen token (`POST /v1/support/listeners` with your **secret** key and
your own `external_id` for that user) and hands it to their app:

```ts
await listen({ listenToken });                       // gdlis_…, from your backend
on("request", async (r) => {
  if (await myPrompt(`${r.company} support would like to see this app. ${r.message ?? ""}`)) {
    await acceptRequest(r.id, { guided: true });     // the consent dialog still shows
  } else {
    await declineRequest(r.id);
  }
});
on("request_closed", ({ id }) => closeMyPrompt(id));  // expired, or answered elsewhere
```

The plugin holds the stream (`GET /v1/support/requests/stream`), reconnects with backoff after a
network error (and every two minutes, to notice a connection that died silently), tells you each
request once, and stops for good (`listen_stopped`) when the token is revoked, expires or is
refused. `acceptRequest` shows the consent dialog first; a no declines the request. On a yes it
accepts the request (`POST …/accept`, which creates the session and sends your app's `client`
info) and starts sharing, exactly as `startHelp` does. `stopListening()` at sign-out.

Quitting the app ends the session (`app_stopped`).

## Permissions

`gaiadesk-support:default` allows every command: `allow-start-help`, `allow-stop`,
`allow-set-paused`, `allow-set-masks`, `allow-status`, `allow-get-info`,
`allow-screen-permission`, `allow-request-screen-permission`,
`allow-open-screen-permission-settings`, `allow-rate`, `allow-attach-diagnostics`,
`allow-send-diagnostics`, `allow-discard-diagnostics`, `allow-listen`, `allow-stop-listening`,
`allow-decline-request`. Grant it only to windows that should offer help; each `allow-*` /
`deny-*` can be used on its own (e.g. `deny-attach-diagnostics` to forbid diagnostics).

## macOS

Your app needs the **Screen Recording** permission, for every `capture` mode. `startHelp` checks it
first (`CGPreflightScreenCaptureAccess`): when it is not granted, it emits `permission_required
{permission: "screen", restartRequired: true}` and rejects with `permission_required` before any
dialog or session, so no agent ever joins a session that cannot share. Then:

```ts
if ((await screenPermission()) !== "granted") {
  await requestScreenPermission();        // macOS's own prompt, the first time only
  // already refused once? send them to the switch:
  await openScreenPermissionSettings();   // System Settings → Privacy & Security → Screen Recording
}
```

macOS's public API cannot tell "never asked" from "refused", so `screenPermission()` answers
`granted` or `denied`; after a grant your app must be relaunched before it counts. `mountHelpButton`
calls `requestScreenPermission()` for you on `permission_required`. Windows and Linux always
answer `granted`. The library's dialog and indicator run on the main thread, which the plugin takes
care of. Guided input is delivered inside your app's
process, so it needs no Accessibility permission.

## Platforms

| | Library | Capture | Guided input | Consent dialog |
|---|---|---|---|---|
| macOS 12.3+ (universal: Apple silicon and Intel) | `libgaiadesk_embed.dylib` | window, screen | yes | native, branded |
| Windows x64 | `gaiadesk_embed.dll` (needs `VCRUNTIME140.dll`) | window, screen | yes | native, branded |
| Linux x86_64 (X11, glibc) | `libgaiadesk_embed.so` (needs `libgbm1`, `libwayland-client0`, `libxcb1`) | window, screen | **view-only** | yours (`askConsent`) |

Other targets (Windows on Arm, Linux arm64) build, and `startHelp` rejects with `library_missing`.

## How the library is found

At run time, the first that exists of: `GAIADESK_EMBED_LIBRARY` (the full path of the library
file), the config's `libraryPath`, `Contents/Frameworks/` (macOS), `<resources>/gaiadesk/`, and
`gaiadesk/` beside the executable (where `gaiadesk_embed_fetch::fetch` also copies it, so
`tauri dev` works).

At build time `fetch`:

- downloads `gaiadesk_embed-macos-universal.tar.gz`, `gaiadesk_embed-win-x64.zip` or
  `gaiadesk_embed-linux-x64.tar.gz` from the
  [gaiadesk-embed-cpp v0.2.0 release](https://github.com/Gaia-Desk/gaiadesk-embed-cpp/releases/tag/v0.2.0),
  with `curl` and `tar` (macOS, Linux and Windows 10+ have both);
- refuses it unless its SHA-256 matches the hash pinned in `PINNED` (`fetch/src/lib.rs`): nothing is
  unpacked and the build fails;
- places the library and `LICENSE-BINARY` in `src-tauri/gaiadesk/` and in `target/<profile>/gaiadesk/`;
- caches in `src-tauri/gaiadesk/`: it stamps the pinned hash in `.sha256` there and does not
  download again while the library and stamp are present (per project; `cargo clean` leaves it;
  delete the folder to fetch again);
- on a target with no prebuilt library, warns and goes on (`startHelp` rejects `library_missing`).

### Offline builds and mirrors

| Variable | |
|---|---|
| `GAIADESK_EMBED_ROOT` | An **unpacked** release archive (`lib/libgaiadesk_embed.dylib`, `bin/gaiadesk_embed.dll` or `lib/libgaiadesk_embed.so`, plus `LICENSE-BINARY`). Nothing is downloaded; the files are copied as they are and **not** hash-checked, so unpack an archive you verified. |
| `GAIADESK_EMBED_BASE_URL` | Where the archives are instead of the GitHub release (your mirror, or `file://`). Same file names; still checked against the pinned hashes. |

Changing either reruns the build script, but a `src-tauri/gaiadesk/` already holding the pinned
version from an earlier download is used as it is: delete it for the variable to take effect.

## Example

[`examples/tauri-app`](examples/tauri-app): "Acme Desk", a Get help button, a masked card, a Buy
button for guided mode and Pause control.

```sh
cd examples/tauri-app && npm install && npx tauri dev
```

Put your publishable key in `src-tauri/tauri.conf.json` first, registered for `app://com.acme.desk`.

## Development

```sh
npm ci && npm test                   # guest-js → dist-js/, then its DOM tests (masks, overlay, dialog)
cargo test --workspace               # unit tests + live tests against the real library
cargo clippy --workspace --all-targets -- -D warnings
```

The live tests (`tests/live_library.rs`) download the library for this platform (checked against
the pinned hash), load it, and check its build info and the refusals it makes before touching the
network or the screen.

## Links

- Guide: [Add in-app support to a Tauri app](docs/tauri-guide.md) · [Embedding GaiaDesk](https://gaiadesk.net/docs/embedding-gaiadesk) on gaiadesk.net
- Embed SDKs: [Web](https://github.com/Gaia-Desk/gaiadesk-embed) · [Electron](https://github.com/Gaia-Desk/gaiadesk-embed-electron) · [Tauri](https://github.com/Gaia-Desk/gaiadesk-embed-tauri) · [macOS (Swift)](https://github.com/Gaia-Desk/gaiadesk-embed-swift) · [C/C++ and Qt](https://github.com/Gaia-Desk/gaiadesk-embed-cpp) · [.NET](https://github.com/Gaia-Desk/gaiadesk-embed-dotnet) · [iOS](https://github.com/Gaia-Desk/gaiadesk-embed-ios) · [Android](https://github.com/Gaia-Desk/gaiadesk-embed-android) · [React Native](https://github.com/Gaia-Desk/gaiadesk-embed-react-native) · [Flutter](https://github.com/Gaia-Desk/gaiadesk-embed-flutter)
- [Changelog](CHANGELOG.md) · [Security policy](https://github.com/Gaia-Desk/.github/blob/main/SECURITY.md)

## Licence

The plugin, its TypeScript, the fetch helper and the example are MIT ([LICENSE](LICENSE)). The
prebuilt `libgaiadesk_embed` binary is licensed under the GaiaDesk Native Binary Licence
([LICENSE-BINARY](LICENSE-BINARY)): free to use in your apps, shipped beside them, unmodified, with
that licence included in your app's notices (the bundle lines above ship it).
