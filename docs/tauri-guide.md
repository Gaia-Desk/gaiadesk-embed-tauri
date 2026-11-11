# Add in-app support to a Tauri app

Give your Tauri app a "Get help" button: your customer clicks it and agrees, and your support team sees your app's window in the GaiaDesk console — and, if you allow it, guides it — without the customer installing anything.

GaiaDesk's Tauri plugin, `tauri-plugin-gaiadesk-support`, wraps the same signed native library as the other GaiaDesk embed SDKs. It is open source (MIT); the library it loads is shipped as a prebuilt binary, unmodified, beside your app.

## What your customer sees

- **One button.** Your app shows "Get help" wherever you put it.
- **A consent dialog in your colours.** It names your company, says what will be shared (this app's window, or the screen if you chose it) and whether your team will be able to guide it. Nothing is shared until the customer agrees.
- **An indicator for as long as anything is shared.** "Sharing with Acme · Stop sharing", drawn by the library on top of your app. Your app cannot hide it. In a guided session it also has **Pause control**.
- **A support code.** Nine digits your customer can read to an agent over the phone.

When the session ends — the customer presses Stop sharing, your app stops it, the session expires, or the app quits — sharing stops at once and nothing is left behind: the library installs nothing, runs no service and writes nothing to disk.

## What your support team sees

Agents open **Support** in the GaiaDesk console, see the waiting customer (with the details your app passed) and join, or type the support code. Only your own account and team can join, and only team members with a support seat.

## 1. Add the plugin

In `src-tauri/`:

```
cargo add tauri-plugin-gaiadesk-support
cargo add --build gaiadesk-embed-fetch
```

In your frontend:

```
npm install @gaiadesk/tauri-plugin-support
```

## 2. Bundle the library

`src-tauri/build.rs` fetches the library for the platform you build for, and refuses it unless its SHA-256 matches the hash pinned in the helper:

```
fn main() {
    gaiadesk_embed_fetch::fetch("gaiadesk").expect("libgaiadesk_embed");
    tauri_build::build();
}
```

Then ship it with your app, one file per platform:

```
// src-tauri/tauri.macos.conf.json
{ "bundle": { "macOS": { "frameworks": ["gaiadesk/libgaiadesk_embed.dylib"] },
              "resources": { "gaiadesk/LICENSE-BINARY": "gaiadesk/LICENSE-BINARY" } } }

// src-tauri/tauri.windows.conf.json
{ "bundle": { "resources": { "gaiadesk/gaiadesk_embed.dll": "gaiadesk/gaiadesk_embed.dll",
                             "gaiadesk/LICENSE-BINARY": "gaiadesk/LICENSE-BINARY" } } }

// src-tauri/tauri.linux.conf.json
{ "bundle": { "resources": { "gaiadesk/libgaiadesk_embed.so": "gaiadesk/libgaiadesk_embed.so",
                             "gaiadesk/LICENSE-BINARY": "gaiadesk/LICENSE-BINARY" } } }
```

On macOS the library goes in `Contents/Frameworks`, so Tauri signs and notarizes it with your app.

## 3. Make a publishable key

On your GaiaDesk account page, open **API keys**, create a **Publishable** key, and register your app's origin: `app://` followed by your Tauri `identifier`, in lowercase — for `com.acme.desk`, `app://com.acme.desk`.

A publishable key can do one thing: create support sessions, at most an hour long, for that app. It is safe to ship inside your app. Sessions created with it show the customer's details as your app's word (unverified) in the console.

Put it in `tauri.conf.json`, with your name and look:

```
"plugins": {
  "gaiadesk-support": {
    "publishableKey": "ak_…",
    "company": "Acme",
    "branding": { "color": "#0f766e", "logo": "icons/logo.png" }
  }
}
```

And allow the plugin in your window's capability (`src-tauri/capabilities/default.json`): add `"gaiadesk-support:default"` to `permissions`.

## 4. Add the button

```
import { mountHelpButton, on } from "@gaiadesk/tauri-plugin-support";

mountHelpButton("#help", { customer: { name: "Ada", plan: "pro" } });
on("started", ({ code }) => showStatus(`Your support code is ${code}`));
```

That is the whole integration. The button asks, creates the session, starts sharing, and becomes "Stop sharing" while the session lasts.

## Sessions from your own backend

If you would rather decide on the server who may get help, and pass verified customer details, create the session there with a secret API key (scope `support`) and hand the app only its embed token:

```
curl -s -X POST https://api.gaiadesk.net/v1/support/sessions \
  -H "Authorization: Bearer $GAIADESK_API_KEY" -H "Content-Type: application/json" \
  -d '{"mode": "cobrowse", "customer": {"name": "Ada", "plan": "pro"}}'
```

```
await startHelp({ embedToken, guided: true });
```

Never ship the secret key in your app.

## Guided sessions

By default your team can only look. To let them click and type, create the session as `cobrowse` (`mode: "cobrowse"`) **and** pass `guided: true` — both ends must want it. Their input is delivered inside your app's own process, only to your app's windows, only while your app is in front, never under a mask, and at most 20 actions a second. Your customer can pause it at any time from the indicator; your app can too (`pause()`). Linux is view-only.

## Keeping things private

- **Your app's window only.** Other apps, notifications and the desktop are never captured, unless you choose `capture: "screen"`.
- **Masks.** Mark anything sensitive — card numbers, balances, passwords — with `data-gaiadesk-mask`. It is painted black before a frame is encoded, from the very first frame, and follows the element as it moves. If the window cannot be located, the whole picture is blacked out.
- **Consent every time.** The library refuses to start without it; there is no setting that skips the dialog or hides the indicator.

## What each platform supports

- **macOS 12.3 or later** (Apple silicon and Intel): macOS asks your customer for **Screen Recording** permission the first time. Guided input needs no Accessibility permission.
- **Windows 10 and 11** (x64): nothing to install; the library needs the Visual C++ runtime most apps already ship.
- **Linux** (x86_64, X11): view-only. There is no native consent dialog, so the plugin asks with your app's own (or the browser-style confirm by default).

## What is and is not permitted

**You may:** ship the prebuilt library unmodified inside your app, with its licence in your app's notices; offer support sessions to your own customers; brand the dialog and indicator with your name, colour and logo.

**You may not:** modify, decompile or re-sign the library separately from your app; hide or imitate the indicator; start a session without your customer's agreement.

See the [plugin's README](https://github.com/Gaia-Desk/gaiadesk-embed-tauri#readme) for every option, event and error code.
