/**
 * PrintBridge SDK Types
 */

// ============================================================================
// Configuration
// ============================================================================

export interface PrintBridgeOptions {
  /** Host to connect to (default: 'localhost') */
  host?: string;
  /** Port to connect to (default: 9718) */
  port?: number;
  /** Auto-reconnect on disconnect (default: true) */
  reconnect?: boolean;
  /** Reconnect interval in ms (default: 3000) */
  reconnectInterval?: number;
  /** Max reconnect attempts (default: 10, 0 for infinite) */
  maxReconnectAttempts?: number;
  /** Optional auth token */
  authToken?: string;
}

// ============================================================================
// Print Requests
// ============================================================================

export interface PrintRequest {
  /** URL to download document from */
  url?: string;
  /** Headers to send with URL request (e.g., Authorization) */
  headers?: Record<string, string>;
  /** Blob data to print (for client-side files) */
  blob?: Blob;
  /** Target printer name (default: system default) */
  printer?: string;
  /** Print options */
  options?: PrintOptions;
}

export interface PrintOptions {
  /** Scaling mode */
  scaling?: 'fit' | 'shrink' | 'none';
  /** Color mode */
  colorMode?: 'color' | 'monochrome' | 'grayscale';
  /** Number of copies */
  copies?: number;
  /** Paper size (e.g., 'letter', 'a4') */
  paperSize?: string;
  /** Orientation */
  orientation?: 'portrait' | 'landscape';
  /** Duplex mode */
  duplex?: 'none' | 'long' | 'short';
}

export interface RawPrintRequest {
  /** Raw data to send */
  data?: Uint8Array | string;
  /** URL to fetch raw data from */
  url?: string;
  /** Headers for URL request */
  headers?: Record<string, string>;
  /** Target printer name (required) */
  printer: string;
  /** Content type */
  contentType: 'application/x-escpos' | 'application/x-zpl' | 'application/x-epl' | 'application/octet-stream';
}

// ============================================================================
// Batch Printing
// ============================================================================

export interface BatchOptions {
  /** Error handling mode */
  onError?: 'stop' | 'continue' | 'retry';
  /** Max retries for 'retry' mode (default: 3) */
  maxRetries?: number;
  /** Retry delay in ms (default: 1000) */
  retryDelay?: number;
  /** Progress callback */
  onProgress?: (progress: BatchProgress) => void;
}

export interface BatchProgress {
  /** Number of completed jobs */
  completed: number;
  /** Number of failed jobs */
  failed: number;
  /** Total number of jobs */
  total: number;
  /** Current job being processed */
  currentJob?: PrintJob;
}

export interface BatchResult {
  /** Number of completed jobs */
  completed: number;
  /** Number of failed jobs */
  failed: number;
  /** Number of cancelled jobs */
  cancelled: number;
  /** Total number of jobs */
  total: number;
  /** All jobs with their final status */
  jobs: PrintJob[];
}

// ============================================================================
// Jobs
// ============================================================================

export type JobStatus = 'pending' | 'downloading' | 'printing' | 'completed' | 'failed' | 'cancelled';

export interface PrintJob {
  /** Unique job ID */
  id: string;
  /** Current status */
  status: JobStatus;
  /** Target printer */
  printer: string;
  /** Content type */
  contentType?: string;
  /** Print options */
  options: PrintOptions;
  /** Error message if failed */
  error?: string;
  /** Creation timestamp */
  createdAt: string;
  /** Last update timestamp */
  updatedAt: string;
}

// ============================================================================
// Printers
// ============================================================================

export type PrinterStatus = 'ready' | 'offline' | 'error' | 'busy';

export interface PrinterInfo {
  /** Printer name */
  name: string;
  /** Current status */
  status: PrinterStatus;
  /** Whether this is the default printer */
  isDefault: boolean;
  /** Printer capabilities */
  capabilities: PrinterCapabilities;
}

export interface PrinterCapabilities {
  /** Supports color printing */
  color: boolean;
  /** Supports duplex printing */
  duplex: boolean;
  /** Supported paper sizes */
  paperSizes: string[];
}

// ============================================================================
// Agent Status
// ============================================================================

export interface AgentStatus {
  /** Agent version */
  version: string;
  /** Platform (windows, macos, linux) */
  platform: string;
  /** Number of available printers */
  printerCount: number;
  /** Number of jobs in queue */
  queueLength: number;
  /** Uptime in milliseconds */
  uptime: number;
}

// ============================================================================
// Events
// ============================================================================

export type PrintBridgeEvent =
  | 'connected'
  | 'disconnected'
  | 'reconnecting'
  | 'error';

export type PrintJobEvent =
  | 'status'
  | 'completed'
  | 'failed';

export type PrintBatchEvent =
  | 'progress'
  | 'completed';

// ============================================================================
// Protocol Messages (Internal)
// ============================================================================

export interface ClientMessage {
  type: string;
  id: string;
  [key: string]: unknown;
}

export interface AgentMessage {
  type: string;
  id?: string;
  [key: string]: unknown;
}
