import { describe, it, expect } from 'vitest';
import type {
  PrintBridgeOptions,
  PrintRequest,
  RawPrintRequest,
  PrintOptions,
  BatchOptions,
  PrintJob,
  PrinterInfo,
  AgentStatus,
} from './types';

describe('Types', () => {
  describe('PrintBridgeOptions', () => {
    it('should allow minimal options', () => {
      const options: PrintBridgeOptions = {};
      expect(options).toBeDefined();
    });

    it('should allow full options', () => {
      const options: PrintBridgeOptions = {
        host: 'localhost',
        port: 9718,
        reconnect: true,
        reconnectInterval: 3000,
        maxReconnectAttempts: 10,
        authToken: 'secret',
      };
      expect(options.host).toBe('localhost');
      expect(options.port).toBe(9718);
      expect(options.authToken).toBe('secret');
    });
  });

  describe('PrintRequest', () => {
    it('should allow URL-based request', () => {
      const request: PrintRequest = {
        url: 'https://example.com/doc.pdf',
        printer: 'HP LaserJet',
      };
      expect(request.url).toBe('https://example.com/doc.pdf');
    });

    it('should allow request with headers', () => {
      const request: PrintRequest = {
        url: 'https://api.example.com/doc.pdf',
        headers: {
          Authorization: 'Bearer token123',
          'X-Custom': 'value',
        },
      };
      expect(request.headers?.Authorization).toBe('Bearer token123');
    });

    it('should allow request with all options', () => {
      const request: PrintRequest = {
        url: 'https://example.com/doc.pdf',
        printer: 'My Printer',
        options: {
          scaling: 'fit',
          colorMode: 'monochrome',
          copies: 3,
          paperSize: 'a4',
          orientation: 'landscape',
          duplex: 'long',
        },
      };
      expect(request.options?.copies).toBe(3);
      expect(request.options?.duplex).toBe('long');
    });
  });

  describe('RawPrintRequest', () => {
    it('should allow string data', () => {
      const request: RawPrintRequest = {
        data: '^XA^FO50,50^ADN,36,20^FDHello^FS^XZ',
        printer: 'Zebra ZT410',
        contentType: 'application/x-zpl',
      };
      expect(request.contentType).toBe('application/x-zpl');
    });

    it('should allow Uint8Array data', () => {
      const escposCommands = new Uint8Array([0x1b, 0x40, 0x1b, 0x61, 0x01]);
      const request: RawPrintRequest = {
        data: escposCommands,
        printer: 'Epson TM-T88',
        contentType: 'application/x-escpos',
      };
      expect(request.data).toBeInstanceOf(Uint8Array);
    });

    it('should allow URL-based raw request', () => {
      const request: RawPrintRequest = {
        url: 'https://api.example.com/labels/123.zpl',
        headers: { Authorization: 'Bearer token' },
        printer: 'Zebra',
        contentType: 'application/x-zpl',
      };
      expect(request.url).toBeDefined();
    });
  });

  describe('PrintOptions', () => {
    it('should allow scaling modes', () => {
      const options1: PrintOptions = { scaling: 'fit' };
      const options2: PrintOptions = { scaling: 'shrink' };
      const options3: PrintOptions = { scaling: 'none' };
      expect(options1.scaling).toBe('fit');
      expect(options2.scaling).toBe('shrink');
      expect(options3.scaling).toBe('none');
    });

    it('should allow color modes', () => {
      const options1: PrintOptions = { colorMode: 'color' };
      const options2: PrintOptions = { colorMode: 'monochrome' };
      const options3: PrintOptions = { colorMode: 'grayscale' };
      expect(options1.colorMode).toBe('color');
      expect(options2.colorMode).toBe('monochrome');
      expect(options3.colorMode).toBe('grayscale');
    });

    it('should allow duplex modes', () => {
      const options1: PrintOptions = { duplex: 'none' };
      const options2: PrintOptions = { duplex: 'long' };
      const options3: PrintOptions = { duplex: 'short' };
      expect(options1.duplex).toBe('none');
      expect(options2.duplex).toBe('long');
      expect(options3.duplex).toBe('short');
    });
  });

  describe('BatchOptions', () => {
    it('should allow error modes', () => {
      const stop: BatchOptions = { onError: 'stop' };
      const cont: BatchOptions = { onError: 'continue' };
      const retry: BatchOptions = { onError: 'retry', maxRetries: 5, retryDelay: 2000 };

      expect(stop.onError).toBe('stop');
      expect(cont.onError).toBe('continue');
      expect(retry.maxRetries).toBe(5);
    });

    it('should allow progress callback', () => {
      let progressCalled = false;
      const options: BatchOptions = {
        onProgress: (progress) => {
          progressCalled = true;
          expect(progress.total).toBeGreaterThanOrEqual(0);
        },
      };

      // Simulate calling the callback
      options.onProgress?.({ completed: 5, failed: 1, total: 10 });
      expect(progressCalled).toBe(true);
    });
  });

  describe('PrintJob', () => {
    it('should have all required fields', () => {
      const job: PrintJob = {
        id: 'job-123',
        status: 'pending',
        printer: 'HP LaserJet',
        options: {},
        createdAt: '2024-01-15T10:00:00Z',
        updatedAt: '2024-01-15T10:00:00Z',
      };
      expect(job.id).toBe('job-123');
      expect(job.status).toBe('pending');
    });

    it('should allow all status values', () => {
      const statuses: PrintJob['status'][] = [
        'pending',
        'downloading',
        'printing',
        'completed',
        'failed',
        'cancelled',
      ];
      statuses.forEach((status) => {
        const job: PrintJob = {
          id: 'test',
          status,
          printer: 'Test',
          options: {},
          createdAt: '',
          updatedAt: '',
        };
        expect(job.status).toBe(status);
      });
    });

    it('should allow optional error field', () => {
      const job: PrintJob = {
        id: 'job-failed',
        status: 'failed',
        printer: 'HP LaserJet',
        options: {},
        error: 'Printer offline',
        createdAt: '',
        updatedAt: '',
      };
      expect(job.error).toBe('Printer offline');
    });
  });

  describe('PrinterInfo', () => {
    it('should have all required fields', () => {
      const printer: PrinterInfo = {
        name: 'HP LaserJet Pro',
        status: 'ready',
        isDefault: true,
        capabilities: {
          color: true,
          duplex: true,
          paperSizes: ['letter', 'a4', 'legal'],
        },
      };
      expect(printer.name).toBe('HP LaserJet Pro');
      expect(printer.isDefault).toBe(true);
      expect(printer.capabilities.paperSizes).toContain('a4');
    });

    it('should allow all status values', () => {
      const statuses: PrinterInfo['status'][] = ['ready', 'offline', 'error', 'busy'];
      statuses.forEach((status) => {
        const printer: PrinterInfo = {
          name: 'Test',
          status,
          isDefault: false,
          capabilities: { color: false, duplex: false, paperSizes: [] },
        };
        expect(printer.status).toBe(status);
      });
    });
  });

  describe('AgentStatus', () => {
    it('should have all required fields', () => {
      const status: AgentStatus = {
        version: '1.0.0',
        platform: 'windows',
        printerCount: 3,
        queueLength: 2,
        uptime: 123456,
      };
      expect(status.version).toBe('1.0.0');
      expect(status.printerCount).toBe(3);
      expect(status.uptime).toBe(123456);
    });
  });
});
