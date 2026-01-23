import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { PrintBridge, PrintJobHandle, PrintBatchHandle } from './client';

// Mock WebSocket with message simulation
class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  onopen: (() => void) | null = null;
  onclose: ((event: { reason: string }) => void) | null = null;
  onerror: ((error: Error) => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;

  private sentMessages: object[] = [];

  constructor(_url: string) {
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN;
      this.onopen?.();
      // Send welcome message
      this.simulateMessage({ type: 'welcome', version: '1.0.0' });
    }, 10);
  }

  send(data: string) {
    const message = JSON.parse(data);
    this.sentMessages.push(message);

    // Auto-respond to certain messages
    setTimeout(() => {
      this.autoRespond(message);
    }, 5);
  }

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ reason: 'closed' });
  }

  private autoRespond(message: { type: string; id: string; [key: string]: unknown }) {
    switch (message.type) {
      case 'getStatus':
        this.simulateMessage({
          type: 'status',
          id: message.id,
          status: {
            version: '1.0.0',
            platform: 'windows',
            printerCount: 2,
            queueLength: 0,
            uptime: 12345,
          },
        });
        break;
      case 'getPrinters':
        this.simulateMessage({
          type: 'printers',
          id: message.id,
          printers: [
            {
              name: 'Test Printer 1',
              status: 'ready',
              isDefault: true,
              capabilities: { color: true, duplex: false, paperSizes: ['letter'] },
            },
            {
              name: 'Test Printer 2',
              status: 'offline',
              isDefault: false,
              capabilities: { color: false, duplex: true, paperSizes: ['a4'] },
            },
          ],
        });
        break;
      case 'print':
      case 'printRaw':
        this.simulateMessage({
          type: 'jobAccepted',
          id: message.id,
          job: {
            id: 'job-' + Date.now(),
            status: 'pending',
            printer: (message.request as { printer?: string })?.printer || 'Default',
            options: {},
            createdAt: new Date().toISOString(),
            updatedAt: new Date().toISOString(),
          },
        });
        break;
      case 'ping':
        this.simulateMessage({ type: 'pong', id: message.id });
        break;
    }
  }

  simulateMessage(data: object) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }

  getSentMessages() {
    return this.sentMessages;
  }
}

const originalWebSocket = globalThis.WebSocket;

describe('PrintBridge', () => {
  beforeEach(() => {
    // @ts-expect-error - replacing WebSocket for testing
    globalThis.WebSocket = MockWebSocket;
  });

  afterEach(() => {
    globalThis.WebSocket = originalWebSocket;
  });

  describe('constructor', () => {
    it('should create instance with default options', () => {
      const bridge = new PrintBridge();
      expect(bridge).toBeDefined();
    });

    it('should create instance with custom options', () => {
      const bridge = new PrintBridge({
        host: 'custom.host',
        port: 8080,
        authToken: 'secret',
      });
      expect(bridge).toBeDefined();
    });
  });

  describe('connect', () => {
    it('should connect successfully', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();
      expect(bridge.isConnected()).toBe(true);
    });

    it('should emit connected event', async () => {
      const bridge = new PrintBridge();
      const onConnected = vi.fn();
      bridge.on('connected', onConnected);

      await bridge.connect();

      expect(onConnected).toHaveBeenCalled();
    });
  });

  describe('disconnect', () => {
    it('should disconnect', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();
      bridge.disconnect();
      expect(bridge.isConnected()).toBe(false);
    });

    it('should emit disconnected event', async () => {
      const bridge = new PrintBridge();
      const onDisconnected = vi.fn();
      bridge.on('disconnected', onDisconnected);

      await bridge.connect();
      bridge.disconnect();

      expect(onDisconnected).toHaveBeenCalled();
    });
  });

  describe('getStatus', () => {
    it('should return agent status', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const status = await bridge.getStatus();

      expect(status.version).toBe('1.0.0');
      expect(status.platform).toBe('windows');
      expect(status.printerCount).toBe(2);
    });
  });

  describe('getPrinters', () => {
    it('should return list of printers', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const printers = await bridge.getPrinters();

      expect(printers).toHaveLength(2);
      expect(printers[0].name).toBe('Test Printer 1');
      expect(printers[0].isDefault).toBe(true);
      expect(printers[1].name).toBe('Test Printer 2');
      expect(printers[1].status).toBe('offline');
    });
  });

  describe('print', () => {
    it('should submit print job and return handle', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const job = await bridge.print({
        url: 'https://example.com/doc.pdf',
        printer: 'Test Printer',
      });

      expect(job).toBeInstanceOf(PrintJobHandle);
      expect(job.id).toMatch(/^job-/);
      expect(job.status).toBe('pending');
    });

    it('should submit print job with options', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const job = await bridge.print({
        url: 'https://example.com/doc.pdf',
        printer: 'Test Printer',
        options: {
          copies: 3,
          colorMode: 'monochrome',
        },
      });

      expect(job).toBeInstanceOf(PrintJobHandle);
    });

    it('should submit print job with headers', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const job = await bridge.print({
        url: 'https://api.example.com/doc.pdf',
        headers: {
          Authorization: 'Bearer token123',
        },
      });

      expect(job).toBeInstanceOf(PrintJobHandle);
    });
  });

  describe('printRaw', () => {
    it('should submit raw print job with string data', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const job = await bridge.printRaw({
        data: '^XA^FO50,50^ADN,36,20^FDTest^FS^XZ',
        printer: 'Zebra',
        contentType: 'application/x-zpl',
      });

      expect(job).toBeInstanceOf(PrintJobHandle);
    });

    it('should submit raw print job with Uint8Array', async () => {
      const bridge = new PrintBridge();
      await bridge.connect();

      const escpos = new Uint8Array([0x1b, 0x40, 0x48, 0x65, 0x6c, 0x6c, 0x6f]);
      const job = await bridge.printRaw({
        data: escpos,
        printer: 'Epson',
        contentType: 'application/x-escpos',
      });

      expect(job).toBeInstanceOf(PrintJobHandle);
    });
  });

  describe('events', () => {
    it('should handle multiple event types', async () => {
      const bridge = new PrintBridge();
      const onConnected = vi.fn();
      const onError = vi.fn();
      const onReconnecting = vi.fn();

      bridge.on('connected', onConnected);
      bridge.on('error', onError);
      bridge.on('reconnecting', onReconnecting);

      await bridge.connect();

      expect(onConnected).toHaveBeenCalled();
    });

    it('should remove event handlers with off', async () => {
      const bridge = new PrintBridge();
      const handler = vi.fn();

      bridge.on('connected', handler);
      bridge.off('connected', handler);

      await bridge.connect();

      expect(handler).not.toHaveBeenCalled();
    });
  });
});

describe('PrintJobHandle', () => {
  it('should track job properties', () => {
    const handle = new PrintJobHandle({
      id: 'job-123',
      status: 'pending',
      printer: 'Test',
      options: {},
      createdAt: '',
      updatedAt: '',
    });

    expect(handle.id).toBe('job-123');
    expect(handle.status).toBe('pending');
  });

  it('should emit status events', () => {
    const handle = new PrintJobHandle({
      id: 'job-123',
      status: 'pending',
      printer: 'Test',
      options: {},
      createdAt: '',
      updatedAt: '',
    });

    const onStatus = vi.fn();
    handle.on('status', onStatus);

    // Simulate status update
    handle._update({
      id: 'job-123',
      status: 'downloading',
      printer: 'Test',
      options: {},
      createdAt: '',
      updatedAt: '',
    });

    expect(onStatus).toHaveBeenCalledWith('downloading');
  });

  it('should emit completed event', () => {
    const handle = new PrintJobHandle({
      id: 'job-123',
      status: 'printing',
      printer: 'Test',
      options: {},
      createdAt: '',
      updatedAt: '',
    });

    const onCompleted = vi.fn();
    handle.on('completed', onCompleted);

    handle._update({
      id: 'job-123',
      status: 'completed',
      printer: 'Test',
      options: {},
      createdAt: '',
      updatedAt: '',
    });

    expect(onCompleted).toHaveBeenCalled();
  });

  it('should emit failed event with error', () => {
    const handle = new PrintJobHandle({
      id: 'job-123',
      status: 'printing',
      printer: 'Test',
      options: {},
      createdAt: '',
      updatedAt: '',
    });

    const onFailed = vi.fn();
    handle.on('failed', onFailed);

    handle._update({
      id: 'job-123',
      status: 'failed',
      printer: 'Test',
      options: {},
      error: 'Printer offline',
      createdAt: '',
      updatedAt: '',
    });

    expect(onFailed).toHaveBeenCalled();
    expect(onFailed.mock.calls[0][0].message).toBe('Printer offline');
  });
});

describe('PrintBatchHandle', () => {
  it('should track batch properties', () => {
    const handle = new PrintBatchHandle('batch-123', 10);
    expect(handle.id).toBe('batch-123');
  });

  it('should emit progress events', () => {
    const handle = new PrintBatchHandle('batch-123', 10);
    const onProgress = vi.fn();

    handle.on('progress', onProgress);
    handle._onProgress({
      batchId: 'batch-123',
      completed: 5,
      failed: 1,
      total: 10,
    });

    expect(onProgress).toHaveBeenCalledWith(
      expect.objectContaining({
        completed: 5,
        failed: 1,
        total: 10,
      })
    );
  });

  it('should resolve wait() on completion', async () => {
    const handle = new PrintBatchHandle('batch-123', 10);

    const waitPromise = handle.wait();

    handle._onComplete({
      batchId: 'batch-123',
      completed: 8,
      failed: 2,
      cancelled: 0,
      total: 10,
      jobs: [],
    });

    const result = await waitPromise;
    expect(result.completed).toBe(8);
    expect(result.failed).toBe(2);
  });

  it('should emit completed event', () => {
    const handle = new PrintBatchHandle('batch-123', 10);
    const onCompleted = vi.fn();

    handle.on('completed', onCompleted);
    handle._onComplete({
      batchId: 'batch-123',
      completed: 10,
      failed: 0,
      cancelled: 0,
      total: 10,
      jobs: [],
    });

    expect(onCompleted).toHaveBeenCalled();
  });
});
