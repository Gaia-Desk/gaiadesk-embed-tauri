# Changelog

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
