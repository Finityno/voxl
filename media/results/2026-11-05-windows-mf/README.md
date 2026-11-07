# Windows Media Foundation encoder component

Runtime/custom fork: `0b365da584c7a24db01e41beb45f4da75651651c`.
Consuming Fincode pin: `b2811a6c041555eb2fc4d42b81b253b98ea9a830`.
This is an encoder primitive and qualification fixture, not a Windows desktop
host provider or a native Windows execution/performance result.

The component requests hardware H.264 Baseline, HEVC Main or AV1 Main, accepts
system-memory NV12 or a protected same-device D3D11 NV12 texture, uses native
input/output events and bounds input/output retention. Tracked sample-release
leases govern GPU texture reuse. The explicit software path is H.264 at most
1280x720/30fps with two requested workers. GPU-device construction does not
silently read back or substitute a software encoder. See the Windows section
of the media README for exact contracts and the probe's limits.

## Executed coverage

All executions below were on the user's authorized Apple Silicon Mac, Cargo
1.99.0, under the shared build lock, nice 19, two jobs and a frozen worktree's
own target. Every child has an external process-group deadline and terminal
cleanup. Windows-target checks use **default features**; they do not compile
Windows AWS-LC/WebRTC, link a Windows binary or execute a transform.

| Source | Scope | Exit / result |
| --- | --- | --- |
| 187d979 | initial Windows-target check | 101: checkout-selected stable toolchain lacked Windows std; no source coverage |
| 187d979 | check after installing matching target | 101: deprecated atomic method denied as warning |
| 0e6c9d3 | Windows-target library/examples check | 101: ignored must-use previous-protection BOOL in probe |
| 2fe58b6 | Windows-target library/examples check | 0 |
| 2fe58b6 | strict Windows-target library/examples/tests Clippy | 101: collapsible condition and range style |
| c0e899f | Windows-target check and strict Clippy | both 0 |
| c0e899f | Mac library test build | 0 |
| c0e899f | Mac `windows_bitstream` filter | 0: **2 passed, 0 failed/ignored**, 13 filtered out |
| c0e899f | strict Mac library/tests Clippy | 0 |
| 0b365da | final Windows-target library/examples check | 0, 0.331s |
| 0b365da | final strict Windows-target library/examples/tests Clippy | 0, 0.390s |
| b2811a6 | Mac application `desktop_media --lib` check | 0, 29.200s |
| b2811a6 | strict Mac application `desktop_media --lib` Clippy | 0, 4.068s |
| b2811a6 | workspace dependency guard | 0, 0.284s |

`comparison.json` proves the framing/common inputs tested at c0e899f are byte
identical at final runtime. Only the Windows event-wait idle fast path changed
after those tests; final target check/strict Clippy were rerun, the unchanged
parser tests were not. The wait consumes queued events before announcing idle.
The initial ownership defects were corrected before the first frozen check:
failed setup closes callback/native references and success does not leak an
activation. These are source/ownership reviews, not runtime lifecycle proof.

`pin-comparison.json` proves only three custom-fork Git source identities change
in the application lock. The two Windows dependencies already exist at the
same versions in the app; no package/version is added or bumped. The selected
RTC/SDK features retain AWS-LC and Tokio. The independent media lock adds the
11 Windows API/support packages listed in `lock-diff.json`; all prior package
versions remain unchanged. No new C++ translation unit is introduced here.

## Evidence and limits

Seven runner packs preserve **60 hashed text exports**, commands, exact-source
clean checks, complete logs, failed attempts and watchdog receipts. The target
installation has its own receipt. `cleanup.json` independently verifies all
**39 recorded process groups absent** after completion. No Windows job,
application/Simulator, capture/input fixture, formatter, screenshot, visual
verification, deployment or merge ran in this checkpoint. Last cloud preflight
was offline; it provides no coverage. This Mac work used the user's explicit
local authorization.

[Prepared native checks](native-plan.md) remain unexecuted. Missing: actual
Windows linking/COM/MFT driver behavior, forced-keyframe timing, independent
decode/profile/color checks, hardware input/release/cancellation, encode speed,
CPU/RSS/GPU/power, capture/BGRA conversion and app host integration. The native
Windows viewer also remains unqualified. No codec default, quality/resource
admission or optimistic performance target is changed or declared achieved.

The media README's previous macOS-only encoder description is updated to name
this experimental Windows component. Earlier AV1 encoder refusal and callback
queue statements describe the macOS implementation, not the new Windows API.
