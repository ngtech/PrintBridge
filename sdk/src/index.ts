/**
 * PrintBridge SDK
 *
 * Bulk printing for web applications.
 *
 * @example
 * ```typescript
 * import { PrintBridge } from 'printbridge';
 *
 * const bridge = new PrintBridge();
 * await bridge.connect();
 *
 * // Print a document
 * const job = await bridge.print({
 *   url: 'https://example.com/document.pdf',
 *   printer: 'HP LaserJet'
 * });
 *
 * job.on('completed', () => console.log('Done!'));
 * ```
 */

export { PrintBridge, PrintJobHandle, PrintBatchHandle } from './client';
export { LabelPrintBridge, LabelApiError } from './labels';
export type { LabelAgentStatus, LabelJob, LabelJobState, LabelProfile, LabelSubmission } from './labels';

export type {
  PrintBridgeOptions,
  PrintRequest,
  RawPrintRequest,
  PrintOptions,
  BatchOptions,
  BatchProgress,
  BatchResult,
  PrintJob,
  JobStatus,
  PrinterInfo,
  PrinterStatus,
  PrinterCapabilities,
  AgentStatus,
} from './types';
