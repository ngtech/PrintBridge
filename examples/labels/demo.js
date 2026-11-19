const byId = (id) => document.getElementById(id);
const bridge = new window.PrintBridge.LabelPrintBridge();
const result = byId('result');
const job = byId('job');
const profile = byId('profile');

function action(id, execute) {
  byId(id).addEventListener('click', async () => {
    try {
      const data = await execute();
      result.textContent = typeof data === 'string' ? data : JSON.stringify(data, null, 2);
    } catch (error) {
      result.textContent = `${error.code ? error.code + ': ' : ''}${error.message}`;
    }
  });
}

async function profiles() {
  const entries = await bridge.listProfiles();
  profile.replaceChildren(...Object.entries(entries).map(([name, settings]) =>
    new Option(`${name} — ${settings.widthMm} × ${settings.heightMm} mm`, name)));
  return entries;
}

action('connect', async () => {
  const token = byId('token');
  const pairing = bridge.connect(token.value.trim());
  token.value = '';
  await pairing;
  await profiles();
  return bridge.agentStatus();
});
action('agent', () => bridge.agentStatus());
action('printers', () => bridge.listPrinters());
action('profiles', profiles);
action('new-id', () => {
  job.value = crypto.randomUUID();
  return 'New print intent ID selected.';
});
action('submit', () => {
  if (!job.value) job.value = crypto.randomUUID();
  const copies = byId('copies').value;
  return bridge.submit(job.value, {
    url: byId('document').value.trim(),
    profile: profile.value,
    ...(copies ? { copies: Number(copies) } : {}),
  });
});
action('status', () => bridge.jobStatus(job.value));
action('cancel', () => bridge.cancelJob(job.value));
