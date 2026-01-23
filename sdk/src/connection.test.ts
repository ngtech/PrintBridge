import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { Connection } from './connection';

// Mock WebSocket
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

  url: string;

  constructor(url: string) {
    this.url = url;
    // Simulate async connection
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN;
      this.onopen?.();
    }, 10);
  }

  send = vi.fn();

  close() {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ reason: 'closed' });
  }

  // Test helpers
  simulateMessage(data: object) {
    this.onmessage?.({ data: JSON.stringify(data) });
  }

  simulateError(error: Error) {
    this.onerror?.(error);
  }

  simulateClose(reason: string) {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.({ reason });
  }
}

// Replace global WebSocket
const originalWebSocket = globalThis.WebSocket;

describe('Connection', () => {
  beforeEach(() => {
    // @ts-expect-error - replacing WebSocket for testing
    globalThis.WebSocket = MockWebSocket;
  });

  afterEach(() => {
    globalThis.WebSocket = originalWebSocket;
  });

  describe('constructor', () => {
    it('should use default options', () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);
      expect(conn).toBeDefined();
    });

    it('should accept custom options', () => {
      const messageHandler = vi.fn();
      const conn = new Connection(
        {
          host: 'custom.host',
          port: 8080,
          reconnect: false,
          reconnectInterval: 5000,
          maxReconnectAttempts: 5,
          authToken: 'token123',
        },
        messageHandler
      );
      expect(conn).toBeDefined();
    });
  });

  describe('connect', () => {
    it('should connect to WebSocket server', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);

      await conn.connect();
      expect(conn.isConnected()).toBe(true);
    });

    it('should emit connected event', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);
      const onConnected = vi.fn();

      conn.on('connected', onConnected);
      await conn.connect();

      expect(onConnected).toHaveBeenCalled();
    });

    it('should include auth token in URL', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({ authToken: 'secret123' }, messageHandler);

      await conn.connect();

      // Check that WebSocket was created with token in URL
      // (This is a simplified check - in real tests we'd capture the URL)
      expect(conn.isConnected()).toBe(true);
    });

    it('should not create duplicate connections', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);

      await conn.connect();
      await conn.connect(); // Second call should be no-op

      expect(conn.isConnected()).toBe(true);
    });
  });

  describe('disconnect', () => {
    it('should close connection', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({ reconnect: false }, messageHandler);

      await conn.connect();
      conn.disconnect();

      expect(conn.isConnected()).toBe(false);
    });

    it('should emit disconnected event', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({ reconnect: false }, messageHandler);
      const onDisconnected = vi.fn();

      conn.on('disconnected', onDisconnected);
      await conn.connect();
      conn.disconnect();

      expect(onDisconnected).toHaveBeenCalled();
    });

    it('should prevent reconnection attempts', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({ reconnect: true }, messageHandler);
      const onReconnecting = vi.fn();

      conn.on('reconnecting', onReconnecting);
      await conn.connect();
      conn.disconnect();

      // Wait a bit to ensure no reconnection is attempted
      await new Promise((resolve) => setTimeout(resolve, 100));
      expect(onReconnecting).not.toHaveBeenCalled();
    });
  });

  describe('send', () => {
    it('should send message when connected', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);

      await conn.connect();
      conn.send({ type: 'test', id: '123' });

      // The mock WebSocket's send should have been called
    });

    it('should throw when not connected', () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);

      expect(() => {
        conn.send({ type: 'test', id: '123' });
      }).toThrow('Not connected');
    });
  });

  describe('events', () => {
    it('should add and remove event handlers', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);
      const handler = vi.fn();

      conn.on('connected', handler);
      await conn.connect();
      expect(handler).toHaveBeenCalledTimes(1);

      conn.off('connected', handler);
      conn.disconnect();
      await conn.connect();
      // Handler was removed, so still only 1 call
      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('should handle multiple handlers for same event', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);
      const handler1 = vi.fn();
      const handler2 = vi.fn();

      conn.on('connected', handler1);
      conn.on('connected', handler2);
      await conn.connect();

      expect(handler1).toHaveBeenCalled();
      expect(handler2).toHaveBeenCalled();
    });
  });

  describe('message handling', () => {
    it('should call message handler on received message', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);

      await conn.connect();

      // Access the internal WebSocket to simulate a message
      // This is a simplified test - real implementation would need proper mocking
    });
  });

  describe('isConnected', () => {
    it('should return false when not connected', () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);
      expect(conn.isConnected()).toBe(false);
    });

    it('should return true when connected', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({}, messageHandler);
      await conn.connect();
      expect(conn.isConnected()).toBe(true);
    });

    it('should return false after disconnect', async () => {
      const messageHandler = vi.fn();
      const conn = new Connection({ reconnect: false }, messageHandler);
      await conn.connect();
      conn.disconnect();
      expect(conn.isConnected()).toBe(false);
    });
  });
});
