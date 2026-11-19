# Windows setup

This development build is for a single signed-in Windows operator per workstation. The tray app uses Tauri/WebView2 and listens only at `ws://127.0.0.1:9718/labels`. No firewall exception, Acrobat or default-printer change is required. Do not use an unsupervised production queue until the [manual acceptance procedure](CALIBRATION.md) passes.

## Build or install

On Windows 10/11 x64, install the WebView2 Runtime, VS Build Tools with the Desktop development with C++ workload and Windows SDK, and current stable Rust with the MSVC host. Use Node.js 22.18+ for the SDK/tests.

```powershell
cd agent
cargo test --lib
cargo build
cargo run
```

For an unsigned NSIS installer from the same source:

```powershell
cargo install tauri-cli --version "^1" --locked
cargo tauri build --bundles nsis
```

The installer is under `agent/target/release/bundle/nsis` from the repository root. The label-validation workflow builds and uploads an installer artifact, not a release. See [Tauri v1 prerequisites](https://v1.tauri.app/v1/guides/getting-started/prerequisites/) and [Windows installers](https://v1.tauri.app/v1/guides/building/windows/). Production signing and deployment are separate work.

Build and test the SDK from the repository root:

```powershell
cd sdk
npm ci
npm test
npm run test:labels
npm run build
```

## Install and configure the print path

1. Install official [SumatraPDF 3.6.1](https://www.sumatrapdfreader.org/download-free-pdf-viewer). The agent checks the executable's Windows file version (3.6.1.0); it looks beside PrintBridge and in standard Program Files and per-user `%LOCALAPPDATA%\SumatraPDF` directories. A portable executable can be placed beside PrintBridge by the workstation administrator. Nothing is fetched or bundled automatically. Neither PATH nor the browser determines the executable.
2. Verify the Epson model suffix, driver version, actual queue names, and label stock. Use the vendor driver, not a generic replacement. In the operator's **Printing Preferences**, register the correct User Defined Media Layout, select detection/gap/cut settings for the loaded stock, and select the layout. Epson documents this under “Registering User Defined Media” in its [TM-C3400 Technical Reference Guide](https://files.support.epson.com/pdf/pos/bulk/tmc3400_trg_revi.pdf). Actual names, selectable dimensions, and queue-specific persistence vary by model and driver. Disable preview, scaling, fit, N-up, and any driver rotation or mirroring; choose a quality mode with equal X/Y DPI.
3. Run the tray app once. Open **Label setup and pairing** to find `%LOCALAPPDATA%\PrintBridge\label-settings.json`. Edit this local file as described in [printer profiles](PRINTING_PROFILES.md) and restart the agent. Set only known origins and queues. Driver preferences are local to the Windows account, so run the agent and configure queues as the same operator.
4. Run the [browser demo](STOCKMAN_INTEGRATION.md#browser-demo), pair it from the tray, and verify agent status and queue discovery. Then run the [physical acceptance test](CALIBRATION.md) with calibration PDFs and representative Stockman-generated samples. If a queue or media check fails, fix its configuration; do not override the PDF dimensions.

## What preflight and printing prove

The backend uses Windows printer enumeration, not WMIC. Before dispatch, it merges the chosen media ID and copies into the queue's DEVMODE via `DocumentPropertiesW`, verifies the result, and compares the physical page reported by the GDI device context against the profile. It allows 0.30 mm for driver rounding, and rejects unequal horizontal/vertical DPI because this Sumatra version derives one print zoom from both axes. It does not verify the loaded roll or all vendor-private driver options. Change stock and profile together; don't change queue preferences during jobs.

The agent invokes Sumatra directly using `-silent -appdata <job directory> -print-to <queue> -print-settings noscale,portrait,paperkind=<id>,<N>x,disable-auto-rotation <pdf>`. The last rotation token is omitted only on opt-in. Here `portrait` selects **content orientation**, not the Windows media orientation. Pinning `paperkind` avoids automatic first-page paper inference in the [3.6.1 print implementation](https://github.com/sumatrapdfreader/sumatrapdf/blob/3.6.1rel/src/Print.cpp); see also [Sumatra's CLI reference](https://www.sumatrapdfreader.org/docs/Command-line-arguments) (which also lists newer options). No arbitrary printer/output/process arguments are accepted from a webpage. Standard file/prompt ports are rejected; use only a physical queue whose driver can print without dialogs.

A zero exit code is reported as `sumatraAccepted`. It does **not** prove the spooler delivered a label, that every page rendered, or that the printer physically printed. Check the Windows queue and device for timeouts, missing labels or paper errors before initiating a deliberate reprint. Do not quit the tray app mid-job; abandoned jobs become `interrupted` on restart and are not replayed. The agent deletes per-job files after dispatch/failure, and attempts crash-leftover cleanup at startup under `%LOCALAPPDATA%\PrintBridge\print-tmp`. Review that folder if an interrupted renderer still has files open. Do not delete the separate SQLite job ledger as part of cleanup.

## Validation and current limits

- The branch includes origin pairing, persistent profiles, Windows discovery and media checks, PDF preflight, a single dispatch worker, persistent job IDs, status/cancel, the SDK/demo, and a calibration generator.
- On the development Mac, five SDK/calibration tests passed; TypeScript declarations and ESM/IIFE bundles built with cached compiler/bundler versions. The locked npm install/upstream Vitest suite and Rust checks were not runnable on that host. The workflow runs normal installs, the Windows Rust tests and installer build when enabled in GitHub Actions.
- Still verify the Windows build/launch, WebView2 tray pairing, browser permissions, driver layout and copies for the exact Epson variant, and measured scale/registration. Live spooler monitoring, cancellation after dispatch, driver-private offsets, and signed production installers are not implemented.
