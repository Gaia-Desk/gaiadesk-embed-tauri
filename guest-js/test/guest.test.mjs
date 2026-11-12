// The guest JS against a DOM (linkedom): what is masked, the dev overlay, the in-page dialog, and
// getInfo outside Tauri. Run with `npm test` (builds dist-js first).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseHTML } from 'linkedom';

const { document, window } = parseHTML(`<!doctype html><html><body>
  <input id="pw" type="password">
  <input id="email" type="email" autocomplete="email">
  <input id="cc" autocomplete="cc-number">
  <input id="ship" autocomplete="shipping cc-exp">
  <input id="otp" autocomplete="one-time-code">
  <input id="cur" autocomplete="username current-password">
  <input id="new" autocomplete="new-password">
  <div id="card" data-gaiadesk-mask></div>
  <section data-gaiadesk-unmask>
    <input id="demo-pw" type="password">
    <div id="forced" data-gaiadesk-mask></div>
  </section>
  <p id="note" class="private">x</p>
</body></html>`);
globalThis.document = document;
globalThis.window = window;
// linkedom does no layout: every element gets a 10×20 box at (1, 2), `#email` none.
window.HTMLElement.prototype.getBoundingClientRect = function () {
  const zero = this.id === 'email';
  return { left: 1, top: 2, width: zero ? 0 : 10, height: zero ? 0 : 20 };
};

const g = await import('../../dist-js/index.js');
const ids = (els) => els.map((e) => e.id).sort();

test('secret fields are masked by default, unmask opts out, an explicit mask wins', () => {
  assert.deepEqual(ids(g.maskedElements()), ['card', 'cc', 'cur', 'forced', 'new', 'otp', 'pw', 'ship']);
});

test('a custom selector still masks secrets', () => {
  assert.deepEqual(ids(g.maskedElements('.private')), ['cc', 'cur', 'new', 'note', 'otp', 'pw', 'ship']);
});

test('rects are CSS pixels of visible elements', () => {
  const r = g.rectsOf('#email, #card');
  assert.equal(r.length, 7, 'the zero-size #email is dropped; the 6 secrets and #card stay');
  assert.deepEqual(r[0], { x: 1, y: 2, w: 10, h: 20 });
});

test('the mask overlay outlines every masked region and goes away', () => {
  g.showMaskOverlay(true);
  const box = document.querySelector('[data-gaiadesk-overlay]');
  assert.ok(box, 'overlay shown');
  assert.equal(box.children.length, 8);
  g.showMaskOverlay(false);
  assert.equal(document.querySelector('[data-gaiadesk-overlay]'), null);
});

test('the in-page dialog answers with the button pressed', async () => {
  const answer = g.confirmInPage('Also share app logs?');
  const buttons = document.querySelectorAll('[role=dialog] button');
  assert.equal(buttons.length, 2);
  buttons[1].dispatchEvent(new window.Event('click'));
  assert.equal(await answer, true);
  assert.equal(document.querySelector('[role=dialog]'), null);
  const no = g.confirmInPage('?');
  document.querySelectorAll('[role=dialog] button')[0].dispatchEvent(new window.Event('click'));
  assert.equal(await no, false);
});

test('getInfo never throws: outside Tauri (or without the plugin) it is unavailable', async () => {
  assert.deepEqual(await g.getInfo(), { available: false });
});

test('startHelp tells the plugin whether masking is on, and the indicator corner', async () => {
  const calls = [];
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => (calls.push([cmd, args]), cmd.endsWith('start_help') ? { state: 'starting' } : null),
    transformCallback: () => 0,
  };
  try {
    await g.startHelp({ masks: [], indicatorPosition: 'bottom-left' });
    await g.startHelp({ masks: [{ x: 0, y: 0, w: 1, h: 1 }] });
  } finally {
    delete window.__TAURI_INTERNALS__;
  }
  const starts = calls.filter(([c]) => c.endsWith('start_help')).map(([, a]) => a.options);
  assert.equal(starts.length, 2);
  assert.deepEqual([starts[0].masking, starts[0].indicatorPosition], [false, 'bottom-left']);
  assert.equal(starts[1].masking, true);
});

function mockTauri(answer) {
  const calls = [];
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      calls.push([cmd.replace('plugin:gaiadesk-support|', ''), args]);
      return answer(cmd.replace('plugin:gaiadesk-support|', ''), args);
    },
    transformCallback: () => 0,
  };
  return calls;
}

const choices = { guided: false, share_logs: false, capture: 'window' };
const input = { decision: 'declined', choices, wording: { title: 'Share?' }, wording_version: 'v', locale: 'en', company: 'Acme', capture: 'window', app_version: null };

test('consentBridge: preview and a library-made receipt, no waitInfo without a session', async () => {
  const calls = mockTauri((cmd) => (cmd === 'preview' ? { png_base64: 'x', width: 1, height: 1, masked_count: 0 } : { decision: 'declined', choices }));
  try {
    assert.equal(g.consentBridge.waitInfo, undefined);
    assert.equal((await g.consentBridge.preview({ capture: 'window', masks: [{ x: 0, y: 0, w: 1, h: 1 }] })).png_base64, 'x');
    assert.equal((await g.consentBridge.recordConsent(input)).decision, 'declined');
  } finally {
    delete window.__TAURI_INTERNALS__;
  }
  assert.deepEqual(calls, [
    ['preview', { options: { capture: 'window', masks: [{ x: 0, y: 0, w: 1, h: 1 }] } }],
    ['record_consent', { input }],
  ]);
});

test('a bridge with the session says who is waiting and has the library post the receipt', async () => {
  const calls = mockTauri((cmd) => (cmd === 'wait_info' ? { team_name: 'Acme Support', team_avatar_url: null, estimated_wait_seconds: 60 } : { decision: 'declined' }));
  const b = g.createConsentBridge({ embedToken: 'gdemb_1', sessionId: 'ss_1' });
  try {
    assert.equal((await b.waitInfo()).team_name, 'Acme Support');
    await b.recordConsent(input);
  } finally {
    delete window.__TAURI_INTERNALS__;
  }
  assert.deepEqual(calls[0], ['wait_info', { embedToken: 'gdemb_1', sessionId: 'ss_1' }]);
  assert.deepEqual(calls[1], ['record_consent', { input: { ...input, embed_token: 'gdemb_1', session_id: 'ss_1' } }]);
});

test('consent() runs the dialog command; refusals keep their code', async () => {
  const calls = mockTauri((cmd) => {
    if (cmd === 'consent') return { decision: 'accepted', choices, consent_token: 'gdc_1' };
    throw { code: 'capture_unavailable', message: 'no permission' };
  });
  try {
    assert.equal((await g.consent({ company: 'Acme', guided: true })).consent_token, 'gdc_1');
    await assert.rejects(g.preview(), (e) => e instanceof g.SupportError && e.code === 'capture_unavailable');
  } finally {
    delete window.__TAURI_INTERNALS__;
  }
  assert.deepEqual(calls[0], ['consent', { options: { company: 'Acme', guided: true } }]);
});

test('startHelp hands the decision to the plugin', async () => {
  const receipt = { id: 'cr_1', decision: 'accepted', choices };
  const calls = mockTauri((cmd) => (cmd === 'start_help' ? { state: 'starting' } : null));
  try {
    await g.startHelp({ masks: [], consentToken: 'gdc_1', consentReceipt: receipt });
  } finally {
    delete window.__TAURI_INTERNALS__;
  }
  const o = calls.find(([c]) => c === 'start_help')[1].options;
  assert.deepEqual([o.consentToken, o.consentReceipt], ['gdc_1', receipt]);
});
