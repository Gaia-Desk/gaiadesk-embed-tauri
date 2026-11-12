// The consent step end to end on the JS side: the real <gaiadesk-consent> (@gaiadesk/consent) in
// a DOM (happy-dom), this package's consentBridge, and Tauri's invoke mocked as the plugin and
// library answer. Accept, decline and timeout, and the receipt's round trip: what the library
// makes is what the element resolves and what startHelp hands back to the plugin.
// The Rust half (the real library making and posting the receipt) is tests/consent_e2e.rs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Window } from 'happy-dom';

const w = new Window({ url: 'https://app.example/' });
for (const k of ['window', 'document', 'HTMLElement', 'HTMLInputElement', 'HTMLImageElement', 'customElements', 'CustomEvent', 'Event', 'KeyboardEvent', 'MouseEvent', 'Node', 'location']) {
  Object.defineProperty(globalThis, k, { value: w[k], configurable: true, writable: true });
}
Object.defineProperty(globalThis, 'navigator', { value: { languages: ['en-US'], language: 'en-US', userAgent: 'Mozilla/5.0 (X11; Linux x86_64)' }, configurable: true });

await import('@gaiadesk/consent');
const g = await import('../../dist-js/index.js');

/** Tauri's IPC, answering as the plugin over libgaiadesk_embed 0.4.0 does. */
function mockPlugin() {
  const calls = [];
  window.__TAURI_INTERNALS__ = {
    transformCallback: () => 0,
    invoke: async (cmd, args) => {
      cmd = cmd.replace('plugin:gaiadesk-support|', '');
      calls.push([cmd, args]);
      if (cmd === 'preview') throw { code: 'capture_unavailable', message: 'headless' };
      if (cmd === 'wait_info') return { team_name: 'Acme Support', team_avatar_url: null, estimated_wait_seconds: 90 };
      if (cmd === 'start_help') return { state: 'starting' };
      if (cmd !== 'record_consent') return null;
      const i = args.input;
      const receipt = {
        id: 'cr_01JCDEFGHJKMNPQRSTVWXYZ012', decision: i.decision, at: new Date().toISOString(), surface: 'embedded',
        locale: i.locale, wording_version: i.wording_version, wording_sha256: 'ab'.repeat(32), choices: i.choices,
        os_user: 'jane', os: 'linux', app_name: 'Acme', app_version: '2.4.1', sdk: 'libgaiadesk_embed/0.4.0',
      };
      return { decision: i.decision, choices: i.choices, receipt, ...(i.decision === 'accepted' && { consent_token: 'gdc_' + '0'.repeat(32) }) };
    },
  };
  return calls;
}

/** Mount the element with the bridge, open it, let `answer` act on it; the decision and the IPC calls. */
async function ask(options, answer, session) {
  const calls = mockPlugin();
  const el = document.body.appendChild(document.createElement('gaiadesk-consent'));
  el.bridge = session ? g.createConsentBridge(session) : g.consentBridge;
  const events = [];
  for (const t of ['consent-shown', 'consent-accepted', 'consent-declined']) el.addEventListener(t, (e) => events.push([t, e.detail]));
  try {
    const done = el.open(options);
    await answer(el);
    const d = await done;
    return { d, calls, events };
  } finally {
    el.remove();
    delete window.__TAURI_INTERNALS__;
  }
}

const press = (el, key) => el.shadowRoot.querySelector('[role=dialog]').dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, composed: true, cancelable: true }));
const recorded = (calls) => calls.filter(([c]) => c === 'record_consent').map(([, a]) => a.input);

test('accept: the library-made receipt and token come back, and startHelp hands both to the plugin', async () => {
  const { d, calls, events } = await ask({ company: 'Acme', guided: true }, (el) => {
    assert.equal(el.shadowRoot.querySelector('input[name=guided]').checked, true, 'click-and-type offered, on');
    el.shadowRoot.querySelector('.accept').click();
  });
  const choices = { guided: true, share_logs: false, capture: 'window' };
  assert.equal(d.decision, 'accepted');
  assert.deepEqual(d.choices, choices);
  assert.match(d.consent_token, /^gdc_/);
  const [input] = recorded(calls);
  assert.deepEqual(
    { ...input, wording: undefined },
    { decision: 'accepted', choices, wording: undefined, wording_version: 'gaiadesk-consent-0.1.0-en', locale: 'en', company: 'Acme', capture: 'window', app_version: null },
  );
  assert.equal(input.wording.title, 'Share with Acme support?', 'the strings actually shown');
  assert.deepEqual(d.receipt.choices, choices);
  assert.deepEqual(events.map(([t]) => t), ['consent-shown', 'consent-accepted']);

  // The round trip: the same receipt, untouched, into start.
  const start = mockPlugin();
  try {
    await g.startHelp({ masks: [], guided: true, consentToken: d.consent_token, consentReceipt: d.receipt });
  } finally {
    delete window.__TAURI_INTERNALS__;
  }
  const o = start.find(([c]) => c === 'start_help')[1].options;
  assert.equal(o.consentToken, d.consent_token);
  assert.deepEqual(o.consentReceipt, d.receipt);
});

test('decline (Esc), with the session: the plugin posts it (embed_token + session_id), no token', async () => {
  const session = { embedToken: 'gdemb_' + 'a'.repeat(64), sessionId: 'ss_test' };
  const { d, calls, events } = await ask({ company: 'Acme' }, (el) => press(el, 'Escape'), session);
  assert.equal(d.decision, 'declined');
  assert.equal(d.receipt.decision, 'declined');
  assert.equal(d.consent_token, undefined);
  const [input] = recorded(calls);
  assert.deepEqual([input.decision, input.embed_token, input.session_id], ['declined', session.embedToken, 'ss_test']);
  assert.deepEqual(calls.find(([c]) => c === 'wait_info')[1], session);
  assert.deepEqual(events.at(-1), ['consent-declined', { reason: 'declined' }]);
});

test('timeout (consent_timeout): the countdown answers timeout and the library records it', async () => {
  const { d, calls, events } = await ask({ company: 'Acme', timeout_seconds: 1 }, () => new Promise((r) => setTimeout(r, 1300)));
  assert.equal(d.decision, 'timeout');
  assert.equal(d.receipt.decision, 'timeout');
  assert.equal(d.consent_token, undefined);
  assert.equal(recorded(calls)[0].decision, 'timeout');
  assert.deepEqual(events.at(-1), ['consent-declined', { reason: 'timeout' }]);
});

test('Enter accepts', async () => {
  const { d } = await ask({ company: 'Acme' }, (el) => press(el, 'Enter'));
  assert.equal(d.decision, 'accepted');
});
