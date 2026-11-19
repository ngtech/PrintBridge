# Upstream audit

Scope: [CenterfireDigital/PrintBridge](https://github.com/CenterfireDigital/PrintBridge), `main` at `6bc26da47975007fce87c40aba5cd75abea0d513` (before fork changes). Inspected the Rust agent, tray HTML, TypeScript SDK, examples, tests and both workflows. Implementation findings below are source observations, not hardware results.

| Area | What the code actually does |
| --- | --- |
| Desktop | Tauri 1 starts a hidden tray window and a background server. The status page mostly shows static values; startup can say “started” before the listener binds. The factory rejects macOS/Linux, so building an executable there would not make it a functioning print agent. |
| Authentication | Host and port are configurable; loopback is just the default. The default origin policy is a wildcard; absent/invalid Origin headers are not required to pass validation. A static token is optional and carried in the upgrade URL. Debug logging can record request contents. |
| Downloads | Reqwest downloads URLs to a cache, with optional caller-supplied headers and a configurable initial HTTPS check. It has request/redirect time limits but does not apply a host allowlist, DNS/address checks or a streaming size limit. The job path also accepts local file references and chunked uploads. Cache cleanup is not guaranteed on early failures. |
| Discovery | Windows discovery calls WMIC and parses CSV; PowerShell is only reached after a successful WMIC call with no usable rows. WMIC missing/nonzero exits early. Capabilities and several status values are defaults, not authoritative driver information. |
| PDF printing | Windows spawns SumatraPDF using the named queue, scaling/color tokens and silent mode. It searches beside the executable, standard directories and PATH. Copies add a second settings argument; the exit error omits the numeric code. Upstream does not ship a Sumatra executable. |
| Other formats | Image submission takes a separate Paint path; raw data uses Windows spooler APIs. Chunk-upload logic exists; binary WebSocket frames are not implemented. These modes were not run on hardware. |
| Queue, retries and status | Single jobs spawn detached tasks, batches loop and may retry, and state is in memory. There is no durable idempotency key or worker-wide queue. Cancellation marks pending state without a dispatch gate; batch cancellation returns not implemented. Agent responses are sent, but no job/batch event broadcaster drives the SDK's progress callbacks. No spooler or physical completion tracking. |
| Dimensions | No persistent named profiles, PDF page box inspection, explicit custom-media selection, printable-area checks, or calibration. The generic scaling option alone cannot verify dimensional accuracy. |
| Packaging | Tag workflow runs `cargo build --release` and uploads binaries, not installers. Tauri has bundle configuration, but no workflow invokes its bundler; Windows signing is not configured. |

## Baseline checks

- Checked GitHub access with the authenticated connector: login `ngtech`, upstream readable; target initially returned 404. Shell Git could not resolve GitHub and `gh` was absent. Imported a shallow local checkout of the original commit and verified all blobs, the tree and commit hashes. `git fsck --full` passed. Later created the proper `ngtech/PrintBridge` fork in signed-in Safari and confirmed write access and matching `main` via the connector.
- `cargo test --manifest-path agent/Cargo.toml`: blocked because Cargo is not installed. Agent compile, Windows launch, installer and printer behavior were not verified.
- `cd sdk && npm ci --offline`: blocked by missing cached tarballs (first reported: `word-wrap@1.2.5`). Upstream Vitest and esbuild checks could not run on this host.

## Recommendation

Retain Tauri, Rust, TypeScript, loopback WebSocket and the external Sumatra renderer. Make the PDF label protocol the only network listener for this build; do not expose the legacy upload, raw, image, arbitrary-header or local-file routes. Use exact origin plus local pairing, a bounded download/dispatch path, named queue/media validation, and durable request IDs. Call zero exit **Sumatra accepted**, not printed. Validate installation, driver behavior and registration on a Windows workstation before rollout.
