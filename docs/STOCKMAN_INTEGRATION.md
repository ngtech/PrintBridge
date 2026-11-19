# Stockman integration

The production application is not modified. Use `LabelPrintBridge` from the local SDK build, not the upstream generic `PrintBridge` client. The label protocol intentionally does not expose raw, image, upload, local-file, batch, arbitrary-header, or driver-configuration routes.

## Pair and connect

- Serve the Stockman UI over HTTPS. Configure its exact origin in `trustedOrigins` (scheme, host and non-default port, no path or wildcard). HTTP is accepted only for explicitly configured loopback dev origins.
- Open the tray's **Label setup and pairing**, choose the matching origin, and paste its session-only code into the web app. Open the socket from a user action. Codes live only in RAM, per origin, and are replaced when the tray restarts. Don't put codes in URLs, logs, cookies, browser storage, analytics, or support screenshots. Clear the input and clipboard after pairing.
- Chromium can prompt for **Loopback network** or **Local network** permission; grant it only to the trusted site. Use a top-level secure context and, if setting CSP, allow `connect-src ws://127.0.0.1:9718`. Older browsers, embedded iframes, enterprise policies or mixed-content policies may block the connection. Diagnose using the browser's permissions and console; don't disable its protection. See [Chrome local network access guidance](https://developer.chrome.com/docs/modern-web-guidance/guides/security/local-network-access).
- Treat the local Windows user and installed agent as trusted. Origin validation prevents unrelated sites from pairing; it does not protect against malicious code already executing under a trusted page origin or Windows account. Serve the pairing UI without third-party scripts.

## Submit and track

Use a short-lived, scoped HTTPS URL for an immutable PDF. Stockman should authorize access before issuing it. Don't give PrintBridge a Stockman API token or cookies. Each destination and redirect must match a DNS origin in `documentOrigins`. The agent resolves and connects only to checked public addresses, rejects any blocked answer, ignores system proxies, and validates TLS. It allows at most 3 redirects, 30 seconds total, 16 MiB, successful HTTP 200 with `application/pdf`, and up to 100 PDF pages. Intranet-only document hosts are deliberately unsupported. Generate each TCPDF document with the exact MediaBox, no PDF rotation, no differing CropBox, and standard user units; use separate PDFs/jobs for mixed stock.

```ts
import { LabelPrintBridge } from 'printbridge';

const bridge = new LabelPrintBridge();
await bridge.connect(operatorPairingCode);
const profiles = await bridge.listProfiles();
const profile = 'meadows-small';
if (!profiles[profile]) throw new Error('Workstation profile is missing');
const job = await bridge.submit(existingPrintIntentId, { url: shortLivedPdfUrl, profile, copies: 1 });
const current = await bridge.jobStatus(job.jobId);
```

Persist the job ID in Stockman as the identity of the operator's print intent before submitting. Do not persist signed URLs or pairing codes in logs. The same authenticated origin, request ID, and **identical payload** return the existing state; changing the URL, profile or copies gives `REQUEST_ID_CONFLICT`. After a connection error or expired signed URL, first query `jobs.status`. Retry with the same ID and payload if not yet recorded. If the expired URL makes the original job unusable, confirm its state, then deliberately create a new print intent (new ID and URL). Do not auto-retry with a new ID. Idempotency is per workstation and while its SQLite ledger exists; don't delete the ledger or silently fail over to a different agent. The ledger stores the payload fingerprint and job state, not the URL or token.

## Wire protocol

Connect to `ws://127.0.0.1:9718/labels` without query parameters. The upgrade requires exact loopback Host and allowed browser Origin. Within five seconds send `{"type":"auth","token":"<local pairing code>"}`. Wait for `{"type":"ready","protocol":1}`; an invalid upgrade or pairing is rejected. Idle connections expire after 15 minutes. Requests use `{"type":"jobs.submit","requestId":"intent-123","payload":{"url":"https://labels.example.com/label.pdf","profile":"meadows-small","copies":1}}`. Responses use `{"type":"response","requestId":"intent-123","ok":true,"result":{...}}` or `ok:false` with `error:{code,message}`. Use unique request IDs for lookup messages so responses correlate correctly.

| Operation | Payload | Result |
| --- | --- | --- |
| `agent.status` | None | Protocol/platform/version and Sumatra availability/version requirement |
| `printers.list` | None | Windows queue-name strings |
| `profiles.list` | None | Map of name to profile |
| `profiles.get` | `{name}` | Profile |
| `jobs.submit` | `{url,profile,copies?}` | Job; `jobId` is the submission's `requestId` |
| `jobs.status` | `{jobId}` | Latest persisted job; only the submitting origin can read it |
| `jobs.cancel` | `{jobId}` | Cancelled job, or `TOO_LATE_TO_CANCEL` after dispatch or another terminal state |

Poll for `queued`, `preparing`, `printing`, `sumatraAccepted`, `failed`, `cancelled` or `interrupted`. `printing` means the agent crossed the dispatch gate, not that paper is moving. `mayHavePrinted` identifies results where dispatch occurred or was possible. A failed/unknown process result, cancellation after dispatch, restart, or client timeout can't safely be treated as “nothing printed.” Check the Windows queue/device before retrying. The agent never automatically resubmits to SumatraPDF.

## Browser demo

After building the SDK, run this from the repository root in PowerShell. The server root contains only the demo and its browser bundle.

~~~powershell
$demoRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("PrintBridgeDemo-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path "$demoRoot\examples\labels","$demoRoot\sdk\dist" -ErrorAction Stop | Out-Null
Copy-Item -LiteralPath examples\labels\index.html,examples\labels\demo.js -Destination "$demoRoot\examples\labels" -ErrorAction Stop
Copy-Item -LiteralPath sdk\dist\printbridge.js -Destination "$demoRoot\sdk\dist" -ErrorAction Stop
py -m http.server 8080 --bind 127.0.0.1 --directory "$demoRoot"
~~~

Use `python` in place of `py` if Python is installed without the Windows launcher. Open `http://127.0.0.1:8080/examples/labels/` and add `http://127.0.0.1:8080` to `trustedOrigins` in the local settings. Restart the tray agent, pair from **Label setup and pairing**, and check **Agent status**, **List queues**, and **Load profiles**. The browser uses an authenticated loopback WebSocket (`ws://`) from a secure-context page. Loading the demo doesn't send a print request. To print, use a pilot queue and a PDF from an approved HTTPS origin. Remove the demo origin when finished.
