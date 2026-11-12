# Changelog

## 0.5.0

For `libgaiadesk_embed` 0.4.0 (the fetch helper fetches it; its pinned hashes are filled in when the
gaiadesk-embed-cpp v0.4.0 archives are published).

- The consent step (GD-31): `consent(options)` (the library's dialog: live preview, your customer's
  choices, who is waiting, localized, themed, timeout) and `consentBridge` /
  `createConsentBridge({ embedToken, sessionId })` for the `@gaiadesk/consent` web component in your
  page; both give a decision with a library-made receipt. Also `preview`, `recordConsent`,
  `waitInfo`, and the `ConsentOptions`, `Decision`, `ConsentReceipt`, `Choices` types (Rust:
  `ConsentOptions`).
- `startHelp` takes `consentToken` and `consentReceipt` (the dialog is skipped; `capture` and
  `guided` follow your customer's choices).
- Events `consent_shown`, `consent_accepted`, `consent_declined`; `state` `waiting` (and `status()`)
  carry `choices`; new `EndReason`s `customer_declined` and `consent_timeout`.
- New commands in `gaiadesk-support:default`: `allow-consent`, `allow-preview`,
  `allow-record-consent`, `allow-wait-info`.
- With a library before 0.4.0 the consent step rejects with `unsupported`; everything else works as
  in 0.4.1.

## 0.4.1

The fetch helper downloads `libgaiadesk_embed` 0.3.1 (the same library as 0.3.0, rebuilt so the
macOS static library no longer carries the build machine's file paths) and pins its archive hashes.
No API change.

## 0.4.0

For `libgaiadesk_embed` 0.3.0 (the fetch helper's pins move to it when it is released; with 0.2.0
the new keys are not sent and everything else works as in 0.3.0).

- The consent dialog is attached to the calling window: the plugin passes the Tauri window's
  `NSWindow` / `HWND` (`parent_window`), so it is a sheet on it (macOS) or owned by it (Windows),
  in front and focused, never behind (seen with remote-URL webviews).
- A modern dialog: larger logo, light and dark with the system, and a checklist of exactly what is
  shared (this window or the screen, guided or view only, masking on, how to stop).
- Cancel while waiting: the indicator offers Cancel until an agent joins; it (or `stop()`) ends the
  session at once as `customer_cancelled` (new `EndReason`), gone from the console's queue, and no
  agent can join it.
- The indicator: always on top, draggable, its place remembered per app
  (`<app data dir>/gaiadesk-support/indicator.json`), clear of your window by default, and the new
  `indicatorPosition` option.
- README: "Your own consent dialog", the minimum wording for `consentGranted: true`.

## 0.3.0

Still fetches `libgaiadesk_embed` 0.2.0 (no library change).

- **Secret keys refused**: an `sk_…` key in `plugins."gaiadesk-support".publishableKey` fails the
  build (the plugin's build script reads the config the Tauri CLI passes it), and `startHelp`
  rejects with `secret_key_in_app` when the configured, `Builder` or passed key is one.
  `pk_live_…`, `pk_test_…` and legacy `ak_…` keys are accepted.
- `getInfo()`: plugin and library versions, where the library was found, platform, guided support,
  Screen Recording state, company. Never throws; `{available: false}` in a build without the plugin.
- macOS Screen Recording: `screenPermission()`, `requestScreenPermission()`,
  `openScreenPermissionSettings()`; `startHelp` checks first and, when it is not granted, emits
  `permission_required` and rejects with `permission_required` before creating any session.
- Masks: password, card (`cc-*`), `one-time-code`, `current-password` and `new-password` fields are
  masked by default; `data-gaiadesk-unmask` opts out; `showMaskOverlay()` / `debugMasks` outline
  what is masked; `maskedElements()`.
- Logos never stop a session: an oversized PNG is downscaled to fit 256 KiB, any other failure is a
  `warning` event and text-only branding; `logo: "$appIcon"` uses the app's window icon.
- Sessions created with a publishable key send `client` (OS, OS version, app version, SDK); the
  `started` event and `status()` carry `estimatedWaitSeconds`.
- Ratings: `ended` carries `canRate`; `rate(rating, comment?)`.
- Diagnostics: `attachDiagnostics({files, text, maxBytes})`, uploaded only after the customer agrees
  ("Also share app logs with …?", an in-page dialog or your `askDiagnostics`).
- Session requests: `listen({listenToken})`, `request` / `request_closed` / `listening` /
  `listen_stopped` events, `acceptRequest(id, options)` (consent still shown), `declineRequest(id)`,
  `stopListening()`.
- New commands and their `allow-*` permissions, all in `gaiadesk-support:default`.

## 0.2.0

The first release; versioned with `libgaiadesk_embed` 0.2.0, which it downloads.

- `tauri-plugin-gaiadesk-support`: a Tauri v2 plugin over `libgaiadesk_embed`, loaded at run time
  (no link-time dependency): commands `start_help`, `stop`, `set_paused`, `set_masks`, `status`;
  events `gaiadesk-support:started`, `joined`, `left`, `paused`, `action`, `error`, `ended`,
  `state`; the `gaiadesk-support:default` permission set. The session ends when the app quits.
- One-click help with a publishable key (the plugin creates the session with
  `Origin: app://<identifier>`), or an embed token from your backend.
- Branding (name, colour, logo) on the consent dialog and the indicator with libraries that support
  it (`gd_embed_show_consent_ex`); older libraries get the plain dialog.
- Masks in CSS pixels or by selector (`[data-gaiadesk-mask]` by default), followed as they move.
- `@gaiadesk/tauri-plugin-support`: the typed TypeScript API and `mountHelpButton()`.
- `gaiadesk-embed-fetch`: a build-script helper that downloads the library for the target from the
  gaiadesk-embed-cpp 0.2.0 release, checks it against pinned SHA-256s, and places it for bundling.
- Example: `examples/tauri-app` ("Acme Desk").
