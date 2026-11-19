export interface LabelProfile {
  printer: string;
  widthMm: number;
  heightMm: number;
  scale: 'none';
  autoRotate: boolean;
  copies: number;
  paperKind: number | null;
}

export interface LabelSubmission {
  url: string;
  profile: string;
  copies?: number;
}

export type LabelJobState = 'queued' | 'preparing' | 'printing' | 'sumatraAccepted'
  | 'failed' | 'cancelled' | 'interrupted';

export interface LabelJob {
  jobId: string;
  profile: string;
  status: LabelJobState;
  errorCode: string | null;
  error: string | null;
  exitCode: number | null;
  mayHavePrinted: boolean;
}

export interface LabelAgentStatus {
  version: string;
  protocol: 1;
  platform: string;
  sumatra: { available: boolean; expectedVersion: string; error: string | null };
}

export class LabelApiError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = 'LabelApiError';
    this.code = code;
  }
}

interface Pending {
  resolve: (result: unknown) => void;
  reject: (error: Error) => void;
  timer: ReturnType<typeof setTimeout>;
}

const ENDPOINT = 'ws://127.0.0.1:9718/labels';
const CONNECT_ERROR = 'Cannot pair with PrintBridge. Check the tray agent, trusted origin, pairing code, and the browser’s loopback/local-network permission.';
const REQUEST_ID = /^[A-Za-z0-9_-]{1,64}$/;

export class LabelPrintBridge {
  private socket?: WebSocket;
  private ready = false;
  private pending = new Map<string, Pending>();

  async connect(token: string): Promise<void> {
    if (this.socket) throw new Error('PrintBridge already connected or connecting');
    if (!/^[0-9a-f]{32}$/.test(token)) throw new Error('Enter the pairing code from the local PrintBridge setup window');
    return new Promise((resolve, reject) => {
      let socket: WebSocket;
      try {
        socket = new WebSocket(ENDPOINT);
      } catch {
        reject(new LabelApiError('CONNECTION_FAILED', CONNECT_ERROR));
        return;
      }
      this.socket = socket;
      const timer = setTimeout(() => fail(), 12_000);
      const fail = () => {
        clearTimeout(timer);
        if (this.socket === socket) {
          this.socket = undefined;
          this.ready = false;
          for (const [requestId, pending] of this.pending) {
            clearTimeout(pending.timer);
            pending.reject(new LabelApiError('DISCONNECTED', 'PrintBridge disconnected; check the existing job ID before reprinting'));
            this.pending.delete(requestId);
          }
        }
        socket.onopen = null;
        socket.onmessage = null;
        socket.onclose = null;
        socket.onerror = null;
        socket.close();
        reject(new LabelApiError('CONNECTION_FAILED', CONNECT_ERROR));
      };
      socket.onopen = () => {
        socket.onopen = null;
        try {
          socket.send(JSON.stringify({ type: 'auth', token }));
        } catch {
          fail();
        }
      };
      socket.onerror = fail;
      socket.onclose = fail;
      socket.onmessage = (event) => {
        if (typeof event.data !== 'string') return;
        let message;
        try {
          message = JSON.parse(event.data);
        } catch {
          return;
        }
        if (!message || typeof message !== 'object') return;
        if (!this.ready && message.type === 'ready' && message.protocol === 1) {
          this.ready = true;
          clearTimeout(timer);
          resolve();
          return;
        }
        if (!this.ready || message.type !== 'response' || typeof message.requestId !== 'string') return;
        const pending = this.pending.get(message.requestId);
        if (!pending) return;
        this.pending.delete(message.requestId);
        clearTimeout(pending.timer);
        if (message.ok === true) {
          pending.resolve(message.result);
        } else {
          pending.reject(new LabelApiError(message.error?.code ?? 'INVALID_RESPONSE',
            message.error?.message ?? 'Invalid PrintBridge response'));
        }
      };
    });
  }

  disconnect(): void {
    this.ready = false;
    this.socket?.close();
  }

  agentStatus(): Promise<LabelAgentStatus> {
    return this.request('agent.status');
  }

  listPrinters(): Promise<string[]> {
    return this.request('printers.list');
  }

  listProfiles(): Promise<Record<string, LabelProfile>> {
    return this.request('profiles.list');
  }

  getProfile(name: string): Promise<LabelProfile> {
    return this.request('profiles.get', { name });
  }

  submit(requestId: string, label: LabelSubmission): Promise<LabelJob> {
    return this.request('jobs.submit', label, requestId);
  }

  jobStatus(jobId: string): Promise<LabelJob> {
    return this.request('jobs.status', { jobId });
  }

  cancelJob(jobId: string): Promise<LabelJob> {
    return this.request('jobs.cancel', { jobId });
  }

  private async request<Result>(type: string, payload?: object, requestId: string = crypto.randomUUID()): Promise<Result> {
    const socket = this.socket;
    if (!this.ready || !socket || socket.readyState !== WebSocket.OPEN) {
      throw new LabelApiError('DISCONNECTED', 'Pair with the local PrintBridge agent first');
    }
    if (!REQUEST_ID.test(requestId)) throw new LabelApiError('INVALID_REQUEST_ID', 'Enter a stable request ID (1–64 ASCII letters, digits, underscores or hyphens)');
    if (this.pending.has(requestId)) throw new LabelApiError('IN_FLIGHT', 'A request with this ID is already in flight');
    return new Promise<Result>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(requestId);
        reject(new LabelApiError('TIMEOUT', 'No PrintBridge response. Check the existing job ID or retry the same payload with the same ID.'));
      }, 10_000);
      this.pending.set(requestId, { resolve: (result) => resolve(result as Result), reject, timer });
      try {
        socket.send(JSON.stringify({ type, requestId, payload }));
      } catch {
        this.pending.delete(requestId);
        clearTimeout(timer);
        reject(new LabelApiError('DISCONNECTED', 'Could not send request; check the existing job ID before reprinting'));
      }
    });
  }
}
