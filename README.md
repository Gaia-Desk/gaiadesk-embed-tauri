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
- **Consent first**: the library's native dialog, in your colour and logo (yours on Linux). Nothing
  is shared until your user agrees.
- Shares **your app's own window** by default. Other apps and the desktop are never in the picture
  unless you opt into `screen`.
- A visible **"Sharing with Acme · Stop sharing"** indicator, drawn by the library, for as long as
  anything is shared.
- **Masks**: mark an element `data-gaiadesk-mask` and it is painted black before a frame is encoded,
  followed as it moves.
- **View-only by default.** Guided input lands only in your app's own windows, and your customer
  can pause it.

Not in a desktop app? See the [web SDK](https://gaiadesk.net/docs/web-in-app-support), the
[website widget](https://gaiadesk.net/docs/website-widget) or
[Electron](https://gaiadesk.net/docs/electron-in-app-support).

## Install

```sh
cargo add tauri-plugin-gaiadesk-support@0.2                # in src-tauri/
cargo add --build gaiadesk-embed-fetch@0.2                 # in src-tauri/: fetches the library at build time
npm install @gaiadesk/tauri-plugin-support@0.2             # your frontend
```

### Versions that go together

| Part | Version |
|---|---|
| `tauri-plugin-gaiadesk-support` (crates.io) | 0.2.0 |
| `gaiadesk-embed-fetch` (crates.io, build helper) | 0.2.0 |
| `libgaiadesk_embed` (the native library it fetches) | 0.2.0 |
| `@gaiadesk/tauri-plugin-support` (npm) | 0.2.0 |
| Tauri | 2.x (the crate needs `tauri` 2.11 or later) |

The plugin has its own 0.x version line, which follows the library's; it is not Tauri's 2.x. Keep
all four GaiaDesk parts on the same version.

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
      "publishableKey": "ak_…",
      "company": "Acme",
      "branding": { "color": "#0f766e", "logo": "icons/logo.png" }
    }
  }
}
```

`src-tauri/capabilities/default.json`: add `"gaiadesk-support:default"` to `permissions`.

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
| `customer` | Who the customer is: at most 16 short fields (publishable-key flow). |
| `mode` | `view` (default) or `cobrowse` (publishable-key flow; your backend decides otherwise). |
| `expiresIn` | Seconds, 60 to 3600 (publishable-key flow). |
| `guided` | Allow guided input (default `false`). Needs a `cobrowse` session too. |
| `capture` | `window` (default): this app's window. `app`: every window of this app (macOS; elsewhere the focused one, followed). `screen`: the display it is on (opt-in). |
| `branding` | `{ name, color, logo }`: see below. Merged over the config's `branding`. |
| `masks` | Rects in CSS pixels, or a selector (default `[data-gaiadesk-mask]`). |
| `fps` | 1 to 30 (default 15). |
| `consentGranted` | `true` only when you asked your user yourself and they agreed. |
| `askConsent` | Your dialog where the OS has none (Linux). Default `window.confirm`. |

Plugin config (`plugins."gaiadesk-support"`, or `Builder` in Rust): `publishableKey`, `company`,
`server` (default `https://gaiadesk.net`), `branding`, `libraryPath`.

```rust
.plugin(tauri_plugin_gaiadesk_support::Builder::new().publishable_key(env!("ACME_GAIADESK_KEY")).company("Acme").build())
```

## White-label

| `branding` | |
|---|---|
| `name` | "Sharing with **Acme**" on the dialog and the indicator. Default: the config's `company`, then your app's product name. |
| `color` | Accent colour, `#rrggbb` or `#rgb`. |
| `logo` | A PNG, at most 256 KiB (262,144 bytes): a path, an `https://` URL, or a `data:image/png;base64,` URL. |

A relative logo path is read from your app's bundled resources directory (Tauri's
`resource_dir()`: `Contents/Resources` in a macOS app), so list it in `bundle.resources` with that
target path (`"logo": "icons/logo.png"` needs `"icons/logo.png": "icons/logo.png"`). An absolute
path is read as is. A logo that cannot be read or downloaded, is not a PNG, or is over 256 KiB
makes `startHelp` reject with `bad_logo`, before the consent dialog.

Colour and logo need `libgaiadesk_embed` 0.2 or later; with 0.1.0 the plugin shows the plain dialog
and indicator with your name, and drops them silently.

## Masks

Anything marked `data-gaiadesk-mask` is painted out from the first frame, and followed while the
session lasts (resizes, scrolling, moving to another display). Or pass `masks: [{x, y, w, h}]` in CSS
pixels of the page, or call `setMasks(rects | selector)` yourself; `[]` clears. The plugin turns
CSS pixels into the library's units (points from the window frame on macOS, `GetWindowRect` pixels
on Windows, X11 pixels on Linux) for the window that called. Masks **fail closed**: if the window
cannot be located, the whole picture is painted out. Page zoom other than 100 % is not accounted for.

## Events

`on(name, handler)` returns a promise of the unlisten function. Tauri event names are
`gaiadesk-support:<name>`.

| Event | Payload | |
|---|---|---|
| `started` | `{code, sessionId, mode, expiresAt}` | The session is up; `code` is the nine-digit support code |
| `joined` | `{agent}` | An agent joined (their email, or `null`) |
| `left` | `{}` | The agent left |
| `paused` | `{paused}` | Guided input paused (by the customer on the indicator, or by you) or resumed |
| `action` | `{action, applied, reason}` | A guided request and its outcome (`masked`, `not_focused`, …) |
| `error` | `{code, message}` | e.g. `capture_failed`, `connect_failed` |
| `ended` | `{reason}` | `stopped` (the customer), `app_stopped`, `disconnected`, `expired`, `error`. Always last |
| `state` | `{state}` | `starting`, `waiting`, `connected`, `ended` |

Commands: `startHelp`, `stop`, `pause`, `resume` (refused, `pause_refused`, when the customer
paused), `setMasks`, `status` (`{state, code, sessionId, mode, agent, paused, endReason}`).
Refusals reject with a `SupportError` whose `code` is the library's (`consent_required`,
`bad_token`, `bad_config`, `bad_rects`, …) or the plugin's: `declined`, `busy`, `no_session`,
`library_missing`, `create_failed` or the server's reason (`too_many_open_sessions`, …),
`bad_logo`, `bad_color`. A branding value the library's dialog refuses rejects with `bad_config`
(and an `error` event), never as `declined`.

Quitting the app ends the session (`app_stopped`).

## Permissions

`gaiadesk-support:default` allows every command: `allow-start-help`, `allow-stop`,
`allow-set-paused`, `allow-set-masks`, `allow-status`. Grant it only to windows that should offer
help; each `allow-*` / `deny-*` can be used on its own.

## macOS

Your app needs the **Screen Recording** permission; macOS asks your user the first time a session
starts, and the app may need to be reopened after it is granted. The library's dialog and indicator
run on the main thread, which the plugin takes care of. Guided input is delivered inside your app's
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
npm ci && npm run build              # guest-js → dist-js/
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
