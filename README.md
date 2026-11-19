# PrintBridge

**Stockman label development branch:** Use the paired PDF label API and `LabelPrintBridge` SDK described in [Windows setup](docs/WINDOWS_SETUP.md), [printer profiles](docs/PRINTING_PROFILES.md), [calibration](docs/CALIBRATION.md) and [integration](docs/STOCKMAN_INTEGRATION.md). The upstream README below describes the original project; its legacy browser protocol, non-PDF routes and zero-configuration claims do not apply to this agent. See the [upstream audit](docs/UPSTREAM_AUDIT.md).

**Open-source bulk printing for web applications.**

PrintBridge enables web applications to print documents directly to local printers without browser dialogs, certificate management, or memory-bloating base64 transfers. Print 100 invoices with one click. Send shipping labels directly to thermal printers. Automate overnight print jobs.

---

## Why PrintBridge?

Web applications need to print documents programmatically - shipping labels, invoices, receipts, reports - but browsers have severe limitations:

| Approach | Problems |
|----------|----------|
| **window.print()** | Shows dialog every time, no printer selection, no batch support, blocks UI |
| **JSPM** | Base64 encoding bloats memory (100MB PDF = 133MB in browser), limited error feedback |
| **QZ Tray** | Requires certificate management, firewall configuration, complex setup |

**PrintBridge provides:**

- **Batch printing** - Print hundreds of documents with a single API call
- **Zero configuration** - Install the agent and go. No certificates, no firewall rules
- **Memory efficient** - URL passthrough means the browser never loads document content
- **Real status feedback** - Know exactly what printed, what failed, and why
- **Raw printing** - Send ESC/POS, ZPL, EPL directly to receipt and label printers
- **Configurable error handling** - Stop on error, continue anyway, or retry failed jobs
- **Open source** - MIT licensed, no vendor lock-in

---

## How It Works

```
┌─────────────────┐     WebSocket      ┌─────────────────┐
│   Your Web App  │◀──────────────────▶│ PrintBridge     │───▶ Printer
│   + JS SDK      │   localhost:9718   │ Agent           │
└─────────────────┘                    └─────────────────┘
```

1. User installs the lightweight PrintBridge agent (runs in system tray)
2. Your web app includes the JavaScript SDK (~8KB gzipped)
3. SDK connects to agent via localhost WebSocket (port 9718)
4. Send print jobs with document URLs - agent downloads and prints
5. Receive real-time status updates

---

## Quick Start

### 1. Install the Agent

**Windows:**
```powershell
# Download installer from GitHub Releases
# https://github.com/mattpilott/PrintBridge/releases
```

### 2. Add the SDK

```bash
npm install printbridge
```

Or via CDN:
```html
<script src="https://unpkg.com/printbridge@latest/dist/printbridge.min.js"></script>
```

### 3. Print!

```javascript
import { PrintBridge } from 'printbridge';

const bridge = new PrintBridge();
await bridge.connect();

// Print a single document
const job = await bridge.print({
  url: 'https://api.example.com/invoices/12345.pdf',
  headers: { 'Authorization': 'Bearer your-token' },
  printer: 'HP LaserJet Pro'
});

job.on('completed', () => console.log('Printed!'));
job.on('failed', (err) => console.error('Failed:', err));
```

### Batch Printing

```javascript
// Print 100 invoices with progress tracking
const batch = await bridge.printBatch(
  invoiceIds.map(id => ({
    url: `https://api.example.com/invoices/${id}.pdf`,
    headers: { 'Authorization': 'Bearer your-token' },
    printer: 'HP LaserJet Pro'
  })),
  {
    onError: 'continue',  // Keep printing even if some fail
    onProgress: ({ completed, failed, total }) => {
      console.log(`Progress: ${completed}/${total}, Failed: ${failed}`);
    }
  }
);

const results = await batch.wait();
console.log(`Completed: ${results.completed}, Failed: ${results.failed}`);
```

---

## Features

### URL Passthrough (Zero Browser Memory)

```javascript
// Browser just sends the URL - agent downloads directly
// No base64 bloat, no memory spikes
await bridge.print({
  url: 'https://api.example.com/large-document.pdf',
  headers: { 'Authorization': 'Bearer xxx' }
});
```

### Raw Printing (ESC/POS, ZPL, EPL)

```javascript
// Send raw commands to receipt/label printers
await bridge.printRaw({
  data: zplLabelCommands,
  printer: 'Zebra ZT410',
  contentType: 'application/x-zpl'
});
```

### Printer Discovery

```javascript
const printers = await bridge.getPrinters();
printers.forEach(p => {
  console.log(`${p.name} - ${p.status}`);
  console.log(`  Color: ${p.capabilities.color}`);
});
```

### Error Handling Modes

```javascript
const batch = await bridge.printBatch(documents, {
  onError: 'stop',      // Stop on first error (default)
  onError: 'continue',  // Skip failures, continue with rest
  onError: 'retry',     // Retry failed jobs
  maxRetries: 3
});
```

---

## Supported Platforms

| Platform | Status |
|----------|--------|
| **Windows 10/11** | v1.0 (current focus) |
| **macOS** | Planned (v1.1) |
| **Linux** | Planned (v1.2) |

## Supported Document Types

**Rendered Documents:**
- PDF (via SumatraPDF)
- Images (PNG, JPG, GIF, BMP, TIFF)

**Raw Protocols:**
- ESC/POS (receipt printers)
- ZPL (Zebra label printers)
- EPL (Eltron label printers)

---

## Architecture

PrintBridge consists of two components:

### Agent (Desktop Application)
- Built with [Tauri](https://tauri.app/) (Rust + WebView)
- Runs in system tray, minimal resource usage (~30MB)
- Handles downloads, caching, print queue
- Interfaces with OS print spooler

### SDK (JavaScript Library)
- Lightweight (~8KB gzipped)
- TypeScript with full type definitions
- Promise-based API with event emitters
- Works in all modern browsers

---

## Comparison

| Feature | PrintBridge | JSPM | QZ Tray |
|---------|-------------|------|---------|
| Setup complexity | Install & go | NPM + config | Certificates + ports |
| Memory efficiency | URL passthrough | Base64 bloat | File-based |
| Batch printing | Native | Manual loop | Supported |
| Raw printing | ESC/POS, ZPL, EPL | Limited | Full |
| Open source | MIT | Proprietary | Partial |
| External ports | No | Yes | Yes |
| Certificate management | No | No | Required |

---

## Development

### Prerequisites

- Rust 1.70+
- Node.js 18+
- Windows 10/11 with Visual Studio Build Tools

### Building

```bash
# Clone repository
git clone https://github.com/mattpilott/PrintBridge.git
cd PrintBridge

# Build agent
cd agent
cargo build --release

# Build SDK
cd ../sdk
npm install
npm run build
```

### Testing

```bash
# Agent tests
cd agent && cargo test

# SDK tests
cd sdk && npm test
```

---

## Roadmap

### v1.0 (In Progress)
- [ ] Core agent with WebSocket server
- [ ] JavaScript SDK
- [ ] PDF and image printing
- [ ] Raw printing (ESC/POS, ZPL, EPL)
- [ ] Batch printing with error handling
- [ ] Windows support

### v1.1
- [ ] macOS support
- [ ] Print spooler status monitoring
- [ ] React hooks package

### v1.2
- [ ] Linux support
- [ ] Plugin system

---

## Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

- **Bug reports:** Open an issue with reproduction steps
- **Feature requests:** Describe the use case
- **Code:** Fork, branch, PR

---

## License

MIT License - see [LICENSE](LICENSE) for details.

---

## Support

- [GitHub Issues](https://github.com/mattpilott/PrintBridge/issues) - Bug reports and feature requests
- [Discussions](https://github.com/mattpilott/PrintBridge/discussions) - Questions and community chat
