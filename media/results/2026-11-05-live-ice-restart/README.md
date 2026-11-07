# Live ICE restart transport

Final tested source: `af6478e446c0c05a98645ff54df088f826947945` in the custom fork. The media/SDK application pin remains `af9cad081eee1581bbd498f383e25c2ebbf1f85e`; the new API is not yet adopted by the host, desktop viewer or phone.

`begin_ice_restart` serializes a live trickle negotiation on the existing peer. Codec, DTLS certificate, SSRC, sequence and capture-clock continuity stay attached to that peer. Old signaling handles/answers cannot enter the next generation. Invalid preflight changes preserve the old peer; preparation failure/cancellation closes it. The owner must release control, show reconnecting, authenticate signaling, revalidate authority and enforce the recovery deadline.

The driver retires discovery behind an acknowledged FIFO barrier, drains accepted media under its existing relay mapping, and sends old allocation releases before adapters change. Trickle ICE discards stale candidates and rebinds ephemeral sockets. This allows a different TURN username to create a fresh allocation. Subsequent requests for an existing allocation must retain its original username under [RFC 8656 section 6](https://www.rfc-editor.org/rfc/rfc8656.html#section-6); the new restart bypasses the imported raw SDK's in-place credential-update path, which remains unqualified. TCP/TLS listeners, the IPC allowlist and allocation observations are replaced. Adapter ownership fences reject late callbacks even when a relay address is reused.

## Exact-source results

| Final check | Result | Seconds |
| --- | --- | ---: |
| Mac native library test build | exit 0 | 2.127 |
| Mac direct media through three live generations | 1 passed | 0.797 |
| Mac retired-allocation callback fence | 1 passed | 0.218 |
| Mac IPC allowlist replacement | 1 passed | 0.226 |
| Mac existing pre-connection startup compatibility | 1 passed | 0.676 |
| Mac terminal failure/reachability fence | 1 passed | 0.222 |
| Mac strict library/test/example Clippy | exit 0 | 0.736 |
| Linux library test build | exit 0 | 2.078 |
| Linux strict library/test/example Clippy | exit 0 | 0.870 |
| Linux actual coturn UDP rotation | 1 passed | 2.069 |
| Linux actual coturn TCP rotation | 1 passed | 2.068 |
| Linux actual coturn verified TLS rotation | 1 passed | 2.119 |

Mac: Darwin arm64, macOS 27.2, Rust/Cargo 1.99.0; post-run read-only host/toolchain metadata is included. Native commands run under the shared build lock, nice 19, two jobs, an owned frozen-worktree target and process-group watchdogs. Linux runs in the existing cloud machine with Rust/Cargo 1.99.0 after sourcing its configured environment; its source and target are separate from the application. All final checks use locked manifests. No formatter or visual verification runs.

Eight selected Rust executions pass at the final source: five Mac and three Linux cases. Each forced-relay case selects `RelayOnly` on one endpoint, verifies its nominated relay transport, alternates two valid REST usernames over three completed restarts, and receives all four synthetic H.264 RTP payloads. DTLS fingerprint, peer identity, SSRC, timestamps and sequence progression remain consistent; one receiver track stays active. TCP/TLS observations require one current adapter/allocation after each restart. These are one-sided loopback relay cases, with no namespace or physical-network claim.

All ten final native validation groups and fifteen final cloud command/server groups are independently gone. Private disposable coturn/CA keys, credentials and configuration are removed. Port rebinding after fixture-server cleanup proves harness cleanup, not release timing or capacity. Cloud background launchers are terminal and defunct pending reaping by their external parent; they have no running workload. This distinction is explicit in the cloud packs.

## Stronger readiness oracle

Earlier source `11002743df9b4f0ec10a9c5d2bcfeee3e4a6506d` passes the original packet/identity tests, including actual UDP/TCP/TLS rotation, but retains old `ice_connected()` observations during the new negotiation. The stronger identical test at `bfed51b65110ad0f51c881e759fc7fc34961247a` fails before the new answer: old reachability cannot admit the new generation. Final source changes only seven production lines, leaving the regression blob identical, and passes on Mac and all three actual relay transports. `readiness-comparison.json` retains that source comparison. Old connection/ICE observations now retire only after old callbacks cross the barrier; fresh callbacks establish readiness. Sending remains gated until the new answer and connection readiness. `Disconnected` is observable and nonterminal; Failed/Closed and explicit close remain terminal.

## Preserved attempts and limits

The initial `404bf4c` test build fails because a test's collection type cannot be inferred; zero tests execute. `69654a7` fixes that annotation and passes five selected Mac cases. `57793c6` adds forced-relay tests, and `1100274` drains accepted writes before allocation retirement; each passes the five Mac cases at its own source. The initial cloud supervisor rejects the valid custom origin URL because it lacks a `.git` suffix; it runs no command/test. The next supervisor fetches the exact source but lacks Cargo in its inherited PATH and stops before compilation. The corrected login-shell/environment setup runs the three relay cases. These attempts, the later readiness failure and all source identities remain separately attributed; none is silently promoted to a final-source pass.

`summary.json` indexes attempts and limits. Six native packs retain commands, complete short logs and helpers; local worktree parents are redacted and original/supplied text hashes distinguished. Cloud kernel build ordinals are normalized for the repository reference policy, with original transfer/text hashes retained. `cloud-before-readiness.json` preserves the weaker earlier run and setup failures; `cloud-final.json` retains the final exact-source run. Cargo's larger raw JSON build output is retained remotely by size/hash and its selected executable identity. All **111 embedded text hashes** are verified; `index.json` covers the published files.

This establishes neither native decoding/capture/input, color/text quality, actual credential expiry/HTTP issuance, full application authority/control release, automatic renewal, interface replacement, physical LAN/NAT/mobile/VPS behavior nor latency/bandwidth/CPU/RAM/GPU/power/cost budgets. No new Windows or phone tests run. Existing application pins, optimistic targets and product-description documents are unchanged. Host/desktop/phone integration and physical qualification remain required before release; keep draft.
