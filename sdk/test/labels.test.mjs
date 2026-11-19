import assert from 'node:assert/strict';
import test from 'node:test';
import { LabelPrintBridge } from '../src/labels.ts';

const fakeToken = 'a'.repeat(32);

class MockSocket {
  static OPEN = 1;
  static current;
  readyState = 0;
  sent = [];
  constructor(url) {
    this.url = url;
    MockSocket.current = this;
  }
  send(text) { this.sent.push(JSON.parse(text)); }
  open() {
    this.readyState = MockSocket.OPEN;
    this.onopen?.();
  }
  receive(data) { this.onmessage?.({ data: JSON.stringify(data) }); }
  close() {
    this.readyState = 3;
    this.onclose?.();
  }
}

globalThis.WebSocket = MockSocket;

async function pair(bridge) {
  const connected = bridge.connect(fakeToken);
  const socket = MockSocket.current;
  socket.open();
  socket.receive({ type: 'ready', protocol: 1 });
  await connected;
  return socket;
}

test('requires protocol readiness, and sends the pairing token in the first frame only', async () => {
  const bridge = new LabelPrintBridge();
  const connected = bridge.connect(fakeToken);
  const socket = MockSocket.current;
  socket.open();
  assert.equal(socket.url, 'ws://127.0.0.1:9718/labels');
  assert.deepEqual(socket.sent, [{ type: 'auth', token: fakeToken }]);
  await assert.rejects(bridge.listPrinters(), { code: 'DISCONNECTED' });
  socket.receive({ type: 'ready', protocol: 1 });
  await connected;
  const discover = bridge.listPrinters();
  const request = socket.sent.at(-1);
  assert.equal(request.type, 'printers.list');
  assert.ok(!Object.hasOwn(request, 'token'));
  socket.receive({ type: 'response', requestId: request.requestId, ok: true, result: ['Test Queue'] });
  assert.deepEqual(await discover, ['Test Queue']);
  bridge.disconnect();
});

test('preserves submission ID, handles correlated responses and does not silently retry', async () => {
  const bridge = new LabelPrintBridge();
  const socket = await pair(bridge);
  const payload = { url: 'https://labels.example.com/order/1', profile: 'small', copies: 2 };
  const job = bridge.submit('intent-1', payload);
  assert.deepEqual(socket.sent.at(-1), { type: 'jobs.submit', requestId: 'intent-1', payload });
  await assert.rejects(bridge.submit('intent-1', payload), { code: 'IN_FLIGHT' });
  socket.receive({ type: 'response', requestId: 'other', ok: true, result: null });
  socket.receive({ type: 'response', requestId: 'intent-1', ok: false,
    error: { code: 'BLOCKED_URL', message: 'Not an approved origin' } });
  await assert.rejects(job, { code: 'BLOCKED_URL' });
  assert.equal(socket.sent.length, 2);
  const status = bridge.jobStatus('intent-1');
  assert.deepEqual(socket.sent.at(-1).payload, { jobId: 'intent-1' });
  socket.close();
  await assert.rejects(status, { code: 'DISCONNECTED' });
});

test('reports connection errors without exposing the token', async () => {
  const bridge = new LabelPrintBridge();
  const connected = bridge.connect(fakeToken);
  MockSocket.current.onerror();
  await assert.rejects(connected, (error) =>
    error.code === 'CONNECTION_FAILED' && !error.message.includes(fakeToken));
});
