import { mountHelpButton, on, pause, resume, type SupportError } from '@gaiadesk/tauri-plugin-support';

const status = document.getElementById('status')!;
const pauseButton = document.getElementById('pause') as HTMLButtonElement;
let paused = false;

// One click: the plugin asks (its consent dialog, in Acme's colour and logo), creates the session
// with the publishable key from tauri.conf.json, and starts. The red card ([data-gaiadesk-mask])
// is painted out of every frame.
mountHelpButton('#help', {
  customer: { name: 'Ada Lovelace', plan: 'pro' },
  mode: 'cobrowse',
  guided: true,
  onError: (e: SupportError) => (status.textContent = `Could not start: ${e.message} (${e.code})`),
});

void on('started', ({ code }) => {
  status.textContent = `Waiting for Acme support. Your support code is ${code.replace(/(\d{3})(?=\d)/g, '$1 ')}.`;
});
void on('joined', ({ agent }) => {
  status.textContent = `${agent ?? 'An agent'} is viewing this window.`;
  pauseButton.hidden = false;
});
void on('paused', (p) => {
  paused = p.paused;
  pauseButton.textContent = paused ? 'Allow control' : 'Pause control';
});
void on('ended', ({ reason }) => {
  status.textContent = `Sharing ended (${reason}).`;
  pauseButton.hidden = true;
});
pauseButton.addEventListener('click', () => void (paused ? resume() : pause()).catch((e: SupportError) => (status.textContent = e.message)));
document.getElementById('buy')!.addEventListener('click', () => (status.textContent = 'Bought!'));
