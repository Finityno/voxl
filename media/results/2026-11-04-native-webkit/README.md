# Native encoded video into the phone receiver

The fixture replays real VideoToolbox access units through the library's actual
UDP/DTLS/SRTP sender into the production phone WebKit receiver on the owned
iOS 27.1 Simulator. It preserves native frame boundaries and capture timestamps.
A second native video-frame callback counts presented frames; no screenshot or
human visual inspection is involved. Local file/HTTP signaling replaces Connect
only for this fixture, so account authorization and full application flows are
not established by this test.

## Actual interoperability bug and correction

At sender source `0efe585` and phone test source `ee6021d`, hardware H.264 emitted
SPS/profile 42001f. The receiver advertised Constrained Baseline 42e01f and
Constrained High 640c1f, not ordinary Baseline. Its open failed before an answer:
one Swift test failed after 14.959 seconds with WebKit's unreachable asynchronous
result error. The outer xcodebuild watchdog subsequently terminated at 90 seconds,
so no final xcresult was available. This failure is retained, not a passing check.

At source `d612224`, requesting VideoToolbox's actual Constrained Baseline
hardware profile emitted 42c01f, compatible with the receiver's constrained
profile. The unchanged phone test passed, with 60 sent / 59 presented frames,
1280x720, a nominated Direct pair and nonzero byte counters. SDP is derived from
the actual SPS; no constraints are fabricated by editing the SDP or bitstream.
The software encoder then rejected that property with profile status -12902;
its test and HEVC were not run at that source. Source `4f7ac81` therefore retains
the supported software Baseline property, whose measured output is 42e01f.

| Exact media source | Encoder mode | Sent / presented | Native test |
| --- | --- | --- | --- |
| d612224 | H.264 hardware, actual 42c01f | 60 / 59 | 1 passed |
| 4f7ac81 | H.264 software, actual 42e01f | 60 / 60 | 1 passed |
| 4f7ac81 | HEVC hardware, actual Main | 60 / 60 | 1 passed |

All three require decoded/presented frames at 1280x720, the production `frame`
event, completed native sending, and an actual nominated Direct route with
nonzero counters. Each xcresult confirms one passed, zero failed/skipped.
Hardware H.264's implementation is unchanged by the final software-only branch;
it was not repeated without cause. The full source IDs, commands, exits,
watchdogs, sender/encoder logs, receiver capabilities/results and summaries are
in `artifact-pack.json`. Full phone build/test logs remain in its private repo.
Media native example builds and strict library/examples Clippy passed.

The new hardware profile constant exists from macOS 12. It is resolved as
optional SDK data to preserve older embedding deployment targets; a missing
symbol retains the earlier standalone Baseline path and actual-profile report.
Static nm inspection confirms the new constant is not a strong import and dlsym
is imported. Older macOS execution is not covered. The native capture provider
requires ScreenCaptureKit independently.

## Scope and remaining measurements

These are short synthetic 720p30 replay runs, not real capture, physical phone,
LAN/NAT/VPS interoperability, text ROI quality, five-run sustained performance,
CPU/GPU/helper memory, latency percentiles or power evidence. Control and input
are not exercised. The optimistic performance targets are unchanged.

This Mac's VideoToolbox inventory reports hardware H.264/HEVC encoders and AV1
hardware decoding, but no AV1 encoder. This Simulator's WebRTC receiver advertises
H.264/HEVC and no AV1, regardless of the Mac's AV1 hardware-decoder capability.
Those inventories do not establish physical iPhone capabilities or geometry/fps
budgets. AV1 still needs an implemented provider and compatible receiver.

Both application viewers still request H.264 by default. HEVC's successful
receiver test closes an interoperability gap; capability-based production codec
selection and further performance work remain to be implemented.

All runs used clean frozen exact source, owned targets/DerivedData/Simulator,
shared Mac build lock, nice 19, at most two build jobs and external process-group
deadlines. No Windows machine, credentials, live screen or input was used.
Initial Swift missing-import and fixture bare-IP failures are preserved.
