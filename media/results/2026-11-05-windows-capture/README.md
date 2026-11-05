# Windows capture and GPU conversion: cross-check evidence

Custom media runtime: `c976515bc3b71789c472a614ae20f301ab5a334b`.
Application pin: `2e4d126037caf90de71efede8baade59a6b996f2`.
This adds native implementation and a synthetic numeric fixture. **No capture,
conversion, encoding or Windows-native execution has been performed here.** The
Windows app host provider is still unfinished and no host capability is enabled.

## Implemented path

Windows Graphics Capture uses the selected monitor's adapter, system cursor and
required border, two raw frame buffers, native event wakeups and QPC timestamps.
Monitor proof, active desktop, geometry/color and closure are checked; errors
close instead of selecting another display or crossing the secure desktop.
Discovery starts no capture or consent UI. The embedding host still owns local
sharing, OS capability and authenticated view admission. Native minimum update
interval is requested when supported and exposed separately from measured fps.

Conversion checks progressive zero-history GPU BGRA -> limited BT.709 NV12,
with scaling and automatic enhancement off. Compatible input bindings feed it
directly; others use one explicit GPU BGRA copy. The device encoder path has
no CPU pixel readback; the explicit CPU-codec path reads converted NV12. Three
output surfaces bound retention, and every encoder sample lease must release
before rewriting a texture. A GPU completion fence finishes source reads before
WGC checks a frame back in. Driver behavior and internal copies are unqualified.

For recovery on a still desktop, the same live capture may repeat its last
converted pixels as a forced keyframe. Duplicate timestamps advance by elapsed
monotonic time in the capture clock domain and are not newly captured frames.
Every overlapping lease is retained; synthetic conversion clears this state.
SDR is explicit; HDR, input injection, app provider wiring and measured codec
admission remain unfinished.

## Executed checks

All commands used the authorized Mac, Cargo 1.99.0, shared build lock, nice 19,
two jobs, an exact clean frozen checkout, its own target and external child
process-group deadlines/cleanup. The Windows target is x86_64-pc-windows-msvc,
**default features**. It supplies no Windows linking, WebRTC/AWS-LC build,
COM/WinRT/D3D/WGC execution, native color/clock/lifetime or performance evidence.

| Source | Check | Exit / result |
| --- | --- | --- |
| e89a274 | Windows-target library/examples | 101: six errors from two native capability-query signatures |
| 16fb710 | Windows-target library/examples | 0 |
| 16fb710 | strict Windows-target library/examples/tests Clippy | 101: four condition-style diagnostics |
| 83ebba6 | same check / strict Clippy | both 0 |
| 31ac597 | same check / strict Clippy after idle recovery | both 0 |
| 6ff1fac | same check / strict Clippy after source-binding handling | both 0 |
| c976515 | final same check / strict Clippy | both 0, 0.435s / 0.506s |
| 2e4d126 | Mac `desktop_media --lib` check | 0, 3.967s |
| 2e4d126 | strict Mac `desktop_media --lib` Clippy | 0, 4.128s |
| 2e4d126 | dependency guard | 0, 0.291s |

**Zero tests executed** in this checkpoint. The prepared conversion oracle has
not run. Earlier Mac framing/transport tests retain their own source and scope;
they are not tests of these Windows additions.

`comparison.json` proves the Mac/transport/SDK/core trees and media lock are
identical to the preceding encoder runtime. Only Windows modules, target-only
Windows API features and a synthetic example change. `pin-comparison.json`
proves only three custom-fork Git source IDs change in the app lock: all 1321
package versions, dependencies and selected RTC/SDK features are preserved.
No new dependency or C++ translation unit is introduced.

Seven runner packs preserve **61 hashed text exports**, complete commands/logs,
failed attempts, clean-source checks, deadlines and cleanup. `cleanup.json`
independently confirms **40 recorded process groups absent**. No app/Simulator,
recording, screen pixels, input, formatter, screenshot/visual verification,
Windows/cloud/VPS job, deployment, secret change or merge ran. Last cloud
preflight was offline. Scoped Mac work used the user's local authorization.

## Required native/application work

[Native plan](native-plan.md) is unexecuted: synthetic direct/copied-input
color/size/readback and independent raw-NV12 decode, native MFT input/release,
actual WGC/lock/close/hotplug, selected physical GPU/driver support and retained
frame behavior. The prepared oracle compares eight independent matrix vectors
within three Y/U/V codes; it is not evidence that this bound was achieved.

Next application integration must connect this worker to the same authenticated
view lifecycle, demand pacing, generation/keyframe receipt fences, synchronous
media close and held-input release used by the existing provider. It must not
advertise Windows hosting before those pieces and native checks are complete.
Windows input/UIPI, viewer qualification, Linux providers, AV1 admission,
physical devices/routes and sustained matched quality/resources/bandwidth/input
latency remain required by the full goal. No performance target is relaxed or
claimed achieved by typechecking.

Documentation previously said capture/conversion were entirely outside the
library; it now distinguishes optional low-level Windows helpers from the
encoder and from still-unfinished app authority/provider integration.
