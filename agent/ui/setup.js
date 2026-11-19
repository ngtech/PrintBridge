const get = (id) => document.getElementById(id);
const origin = get('origins');
const token = get('token');
const reveal = get('reveal');
const tauri = window.__TAURI__;
let tokens = {};

function hideCode() {
  token.value = '';
  token.hidden = true;
}

async function refresh() {
  hideCode();
  try {
    const setup = await tauri.tauri.invoke('label_setup');
    get('status').textContent = setup.status;
    get('endpoint').textContent = setup.endpoint;
    get('settings').value = setup.settingsPath;
    tokens = setup.tokens;
    origin.replaceChildren(...Object.keys(tokens).map((value) => new Option(value, value)));
    reveal.disabled = !origin.options.length;
  } catch {
    get('status').textContent = 'Could not read PrintBridge setup.';
  }
}

origin.addEventListener('change', hideCode);
window.addEventListener('blur', hideCode);
reveal.addEventListener('click', () => {
  token.value = tokens[origin.value] ?? '';
  token.hidden = false;
  token.select();
});
void tauri.event.listen('label-setup-changed', refresh);
void refresh();
