import type { PrintBridgeOptions, ClientMessage, AgentMessage } from './types';

type MessageHandler = (message: AgentMessage) => void;
type EventHandler = (...args: unknown[]) => void;

/**
 * WebSocket connection manager
 */
export class Connection {
  private ws: WebSocket | null = null;
  private options: Required<PrintBridgeOptions>;
  private messageHandler: MessageHandler;
  private eventHandlers: Map<string, Set<EventHandler>> = new Map();
  private reconnectAttempts = 0;
  private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
  private isConnecting = false;
  private shouldReconnect = true;

  constructor(options: PrintBridgeOptions, messageHandler: MessageHandler) {
    this.options = {
      host: options.host ?? 'localhost',
      port: options.port ?? 9718,
      reconnect: options.reconnect ?? true,
      reconnectInterval: options.reconnectInterval ?? 3000,
      maxReconnectAttempts: options.maxReconnectAttempts ?? 10,
      authToken: options.authToken ?? '',
    };
    this.messageHandler = messageHandler;
  }

  /**
   * Connect to the PrintBridge agent
   */
  async connect(): Promise<void> {
    if (this.ws?.readyState === WebSocket.OPEN) {
      return;
    }

    if (this.isConnecting) {
      // Wait for existing connection attempt
      return new Promise((resolve, reject) => {
        const checkConnection = () => {
          if (this.ws?.readyState === WebSocket.OPEN) {
            resolve();
          } else if (!this.isConnecting) {
            reject(new Error('Connection failed'));
          } else {
            setTimeout(checkConnection, 100);
          }
        };
        checkConnection();
      });
    }

    this.isConnecting = true;
    this.shouldReconnect = true;

    return new Promise((resolve, reject) => {
      try {
        let url = `ws://${this.options.host}:${this.options.port}`;
        if (this.options.authToken) {
          url += `?token=${encodeURIComponent(this.options.authToken)}`;
        }

        this.ws = new WebSocket(url);

        this.ws.onopen = () => {
          this.isConnecting = false;
          this.reconnectAttempts = 0;
          this.emit('connected');
          resolve();
        };

        this.ws.onclose = (event) => {
          this.isConnecting = false;
          const wasConnected = this.ws !== null;
          this.ws = null;

          if (wasConnected) {
            this.emit('disconnected', event.reason || 'Connection closed');
          }

          if (this.shouldReconnect && this.options.reconnect) {
            this.scheduleReconnect();
          }
        };

        this.ws.onerror = (error) => {
          this.isConnecting = false;
          this.emit('error', error);
          if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
            reject(new Error('Failed to connect to PrintBridge agent'));
          }
        };

        this.ws.onmessage = (event) => {
          try {
            const message = JSON.parse(event.data) as AgentMessage;
            this.messageHandler(message);
          } catch (e) {
            console.error('Failed to parse message:', e);
          }
        };
      } catch (error) {
        this.isConnecting = false;
        reject(error);
      }
    });
  }

  /**
   * Disconnect from the agent
   */
  disconnect(): void {
    this.shouldReconnect = false;
    if (this.reconnectTimeout) {
      clearTimeout(this.reconnectTimeout);
      this.reconnectTimeout = null;
    }
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Send a message to the agent
   */
  send(message: ClientMessage): void {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      throw new Error('Not connected to PrintBridge agent');
    }
    this.ws.send(JSON.stringify(message));
  }

  /**
   * Add event listener
   */
  on(event: string, handler: EventHandler): void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
  }

  /**
   * Remove event listener
   */
  off(event: string, handler: EventHandler): void {
    this.eventHandlers.get(event)?.delete(handler);
  }

  /**
   * Emit event
   */
  private emit(event: string, ...args: unknown[]): void {
    this.eventHandlers.get(event)?.forEach(handler => {
      try {
        handler(...args);
      } catch (e) {
        console.error('Event handler error:', e);
      }
    });
  }

  /**
   * Schedule reconnection attempt
   */
  private scheduleReconnect(): void {
    if (this.reconnectTimeout) {
      return;
    }

    if (
      this.options.maxReconnectAttempts > 0 &&
      this.reconnectAttempts >= this.options.maxReconnectAttempts
    ) {
      this.emit('error', new Error('Max reconnect attempts reached'));
      return;
    }

    this.reconnectAttempts++;
    this.emit('reconnecting', this.reconnectAttempts);

    this.reconnectTimeout = setTimeout(() => {
      this.reconnectTimeout = null;
      this.connect().catch(() => {
        // Reconnect failed, will retry
      });
    }, this.options.reconnectInterval);
  }
}
