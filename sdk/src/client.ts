import { Connection } from './connection';
import type {
  PrintBridgeOptions,
  PrintRequest,
  RawPrintRequest,
  BatchOptions,
  PrintJob,
  PrinterInfo,
  AgentStatus,
  AgentMessage,
  BatchProgress,
  BatchResult,
} from './types';

type EventHandler = (...args: unknown[]) => void;

let messageIdCounter = 0;
function generateId(): string {
  return `msg_${Date.now()}_${++messageIdCounter}`;
}

/**
 * Print job handle for tracking individual jobs
 */
export class PrintJobHandle {
  private eventHandlers: Map<string, Set<EventHandler>> = new Map();
  private _job: PrintJob;

  constructor(job: PrintJob) {
    this._job = job;
  }

  get id(): string {
    return this._job.id;
  }

  get status(): string {
    return this._job.status;
  }

  get job(): PrintJob {
    return this._job;
  }

  /** @internal */
  _update(job: PrintJob): void {
    this._job = job;
    this.emit('status', job.status);
    if (job.status === 'completed') {
      this.emit('completed');
    } else if (job.status === 'failed') {
      this.emit('failed', new Error(job.error || 'Print failed'));
    }
  }

  on(event: 'status', handler: (status: string) => void): this;
  on(event: 'completed', handler: () => void): this;
  on(event: 'failed', handler: (error: Error) => void): this;
  on(event: string, handler: EventHandler): this {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
    return this;
  }

  off(event: string, handler: EventHandler): this {
    this.eventHandlers.get(event)?.delete(handler);
    return this;
  }

  private emit(event: string, ...args: unknown[]): void {
    this.eventHandlers.get(event)?.forEach(handler => {
      try {
        handler(...args);
      } catch (e) {
        console.error('Event handler error:', e);
      }
    });
  }
}

/**
 * Batch print handle for tracking batch jobs
 */
export class PrintBatchHandle {
  private eventHandlers: Map<string, Set<EventHandler>> = new Map();
  private resolveWait: ((result: BatchResult) => void) | null = null;
  private rejectWait: ((error: Error) => void) | null = null;
  private _result: BatchResult | null = null;

  readonly id: string;
  readonly jobs: PrintJobHandle[] = [];

  constructor(id: string, total: number) {
    this.id = id;
    // Jobs will be added as they're processed
  }

  /** @internal */
  _onProgress(progress: BatchProgress): void {
    this.emit('progress', progress);
  }

  /** @internal */
  _onComplete(result: BatchResult): void {
    this._result = result;
    this.emit('completed', result);
    if (this.resolveWait) {
      this.resolveWait(result);
    }
  }

  /** @internal */
  _onError(error: Error): void {
    if (this.rejectWait) {
      this.rejectWait(error);
    }
  }

  /**
   * Wait for batch to complete
   */
  wait(): Promise<BatchResult> {
    if (this._result) {
      return Promise.resolve(this._result);
    }
    return new Promise((resolve, reject) => {
      this.resolveWait = resolve;
      this.rejectWait = reject;
    });
  }

  on(event: 'progress', handler: (progress: BatchProgress) => void): this;
  on(event: 'completed', handler: (result: BatchResult) => void): this;
  on(event: string, handler: EventHandler): this {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
    return this;
  }

  off(event: string, handler: EventHandler): this {
    this.eventHandlers.get(event)?.delete(handler);
    return this;
  }

  private emit(event: string, ...args: unknown[]): void {
    this.eventHandlers.get(event)?.forEach(handler => {
      try {
        handler(...args);
      } catch (e) {
        console.error('Event handler error:', e);
      }
    });
  }
}

/**
 * PrintBridge client
 */
export class PrintBridge {
  private connection: Connection;
  private pendingRequests: Map<string, {
    resolve: (value: unknown) => void;
    reject: (error: Error) => void;
  }> = new Map();
  private jobHandles: Map<string, PrintJobHandle> = new Map();
  private batchHandles: Map<string, PrintBatchHandle> = new Map();
  private eventHandlers: Map<string, Set<EventHandler>> = new Map();

  constructor(options: PrintBridgeOptions = {}) {
    this.connection = new Connection(options, this.handleMessage.bind(this));

    // Forward connection events
    this.connection.on('connected', () => this.emit('connected'));
    this.connection.on('disconnected', (reason) => this.emit('disconnected', reason));
    this.connection.on('reconnecting', (attempt) => this.emit('reconnecting', attempt));
    this.connection.on('error', (error) => this.emit('error', error));
  }

  /**
   * Connect to the PrintBridge agent
   */
  async connect(): Promise<void> {
    await this.connection.connect();
  }

  /**
   * Disconnect from the agent
   */
  disconnect(): void {
    this.connection.disconnect();
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connection.isConnected();
  }

  /**
   * Get agent status
   */
  async getStatus(): Promise<AgentStatus> {
    const id = generateId();
    return this.sendRequest({ type: 'getStatus', id }) as Promise<AgentStatus>;
  }

  /**
   * Get available printers
   */
  async getPrinters(): Promise<PrinterInfo[]> {
    const id = generateId();
    return this.sendRequest({ type: 'getPrinters', id }) as Promise<PrinterInfo[]>;
  }

  /**
   * Print a document
   */
  async print(request: PrintRequest): Promise<PrintJobHandle> {
    const id = generateId();

    // Handle blob upload
    if (request.blob) {
      const uploadId = await this.uploadBlob(request.blob);
      // The upload complete handler will trigger the print
      return this.sendRequest({
        type: 'uploadComplete',
        id,
        uploadId,
        request: {
          printer: request.printer,
          options: request.options,
        },
      }) as Promise<PrintJobHandle>;
    }

    return this.sendRequest({
      type: 'print',
      id,
      request: {
        url: request.url,
        headers: request.headers,
        printer: request.printer,
        options: request.options,
      },
    }) as Promise<PrintJobHandle>;
  }

  /**
   * Print raw data (ESC/POS, ZPL, EPL)
   */
  async printRaw(request: RawPrintRequest): Promise<PrintJobHandle> {
    const id = generateId();

    let data: string | undefined;
    if (request.data) {
      if (typeof request.data === 'string') {
        // Encode string as base64
        data = btoa(request.data);
      } else {
        // Encode Uint8Array as base64
        data = btoa(String.fromCharCode(...request.data));
      }
    }

    return this.sendRequest({
      type: 'printRaw',
      id,
      request: {
        url: request.url,
        headers: request.headers,
        data,
        printer: request.printer,
        contentType: request.contentType,
      },
    }) as Promise<PrintJobHandle>;
  }

  /**
   * Print a batch of documents
   */
  async printBatch(
    requests: PrintRequest[],
    options: BatchOptions = {}
  ): Promise<PrintBatchHandle> {
    const id = generateId();
    const batchId = `batch_${Date.now()}`;

    const handle = new PrintBatchHandle(batchId, requests.length);
    this.batchHandles.set(batchId, handle);

    // Set up progress callback
    if (options.onProgress) {
      handle.on('progress', options.onProgress);
    }

    this.connection.send({
      type: 'printBatch',
      id,
      request: {
        id: batchId,
        jobs: requests.map(r => ({
          url: r.url,
          headers: r.headers,
          printer: r.printer,
          options: r.options,
        })),
        onError: options.onError || 'stop',
        maxRetries: options.maxRetries || 3,
        retryDelay: options.retryDelay || 1000,
      },
    });

    return handle;
  }

  /**
   * Cancel a print job
   */
  async cancelJob(jobId: string): Promise<void> {
    const id = generateId();
    await this.sendRequest({ type: 'cancelJob', id, jobId });
  }

  /**
   * Cancel a batch
   */
  async cancelBatch(batchId: string): Promise<void> {
    const id = generateId();
    await this.sendRequest({ type: 'cancelBatch', id, batchId });
  }

  // ============================================================================
  // Events
  // ============================================================================

  on(event: 'connected', handler: () => void): this;
  on(event: 'disconnected', handler: (reason: string) => void): this;
  on(event: 'reconnecting', handler: (attempt: number) => void): this;
  on(event: 'error', handler: (error: Error) => void): this;
  on(event: string, handler: EventHandler): this {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, new Set());
    }
    this.eventHandlers.get(event)!.add(handler);
    return this;
  }

  off(event: string, handler: EventHandler): this {
    this.eventHandlers.get(event)?.delete(handler);
    return this;
  }

  private emit(event: string, ...args: unknown[]): void {
    this.eventHandlers.get(event)?.forEach(handler => {
      try {
        handler(...args);
      } catch (e) {
        console.error('Event handler error:', e);
      }
    });
  }

  // ============================================================================
  // Internal
  // ============================================================================

  private async sendRequest(message: { type: string; id: string; [key: string]: unknown }): Promise<unknown> {
    return new Promise((resolve, reject) => {
      this.pendingRequests.set(message.id, { resolve, reject });
      try {
        this.connection.send(message);
      } catch (error) {
        this.pendingRequests.delete(message.id);
        reject(error);
      }
    });
  }

  private handleMessage(message: AgentMessage): void {
    switch (message.type) {
      case 'welcome':
        // Connection established
        break;

      case 'status':
        this.resolveRequest(message.id!, (message as { status: AgentStatus }).status);
        break;

      case 'printers':
        this.resolveRequest(message.id!, (message as { printers: PrinterInfo[] }).printers);
        break;

      case 'jobAccepted': {
        const job = (message as { job: PrintJob }).job;
        const handle = new PrintJobHandle(job);
        this.jobHandles.set(job.id, handle);
        this.resolveRequest(message.id!, handle);
        break;
      }

      case 'jobStatus': {
        const job = (message as { job: PrintJob }).job;
        const handle = this.jobHandles.get(job.id);
        if (handle) {
          handle._update(job);
        }
        break;
      }

      case 'jobCompleted': {
        const job = (message as { job: PrintJob }).job;
        const handle = this.jobHandles.get(job.id);
        if (handle) {
          handle._update(job);
        }
        break;
      }

      case 'jobFailed': {
        const { job, error } = message as { job: PrintJob; error: string };
        const handle = this.jobHandles.get(job.id);
        if (handle) {
          handle._update({ ...job, status: 'failed', error });
        }
        break;
      }

      case 'batchAccepted': {
        // Batch was accepted, handle is already created
        break;
      }

      case 'batchProgress': {
        const progress = (message as { progress: BatchProgress }).progress;
        const handle = this.batchHandles.get(progress.batchId);
        if (handle) {
          handle._onProgress(progress);
        }
        break;
      }

      case 'batchCompleted': {
        const result = (message as { result: BatchResult }).result;
        const handle = this.batchHandles.get(result.batchId);
        if (handle) {
          handle._onComplete(result);
          this.batchHandles.delete(result.batchId);
        }
        break;
      }

      case 'uploadProgress':
        // Could emit progress events for blob uploads
        break;

      case 'uploadReady':
        this.resolveRequest(message.id!, message.uploadId);
        break;

      case 'pong':
        this.resolveRequest(message.id!, undefined);
        break;

      case 'error': {
        const { code, message: errorMessage } = message as { code: string; message: string };
        this.rejectRequest(message.id!, new Error(`${code}: ${errorMessage}`));
        break;
      }
    }
  }

  private resolveRequest(id: string, value: unknown): void {
    const pending = this.pendingRequests.get(id);
    if (pending) {
      this.pendingRequests.delete(id);
      pending.resolve(value);
    }
  }

  private rejectRequest(id: string, error: Error): void {
    const pending = this.pendingRequests.get(id);
    if (pending) {
      this.pendingRequests.delete(id);
      pending.reject(error);
    }
  }

  private async uploadBlob(blob: Blob): Promise<string> {
    const uploadId = `upload_${Date.now()}`;
    const chunkSize = 64 * 1024; // 64KB chunks
    const totalChunks = Math.ceil(blob.size / chunkSize);

    // Start upload
    const startId = generateId();
    await this.sendRequest({
      type: 'uploadStart',
      id: startId,
      uploadId,
      filename: (blob as File).name || 'upload',
      totalSize: blob.size,
    });

    // Send chunks
    for (let i = 0; i < totalChunks; i++) {
      const start = i * chunkSize;
      const end = Math.min(start + chunkSize, blob.size);
      const chunk = blob.slice(start, end);

      const buffer = await chunk.arrayBuffer();
      const data = btoa(String.fromCharCode(...new Uint8Array(buffer)));

      const chunkId = generateId();
      await this.sendRequest({
        type: 'uploadChunk',
        id: chunkId,
        uploadId,
        chunkIndex: i,
        data,
      });
    }

    return uploadId;
  }
}
