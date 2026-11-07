# Maximum frame-QP experiment

Exact source `ef7ed1071728a84ec439f7d0e78c4d42d5e7673f` adds optional Mac
`EncodingOptions` with `MaxFrameQp::new(N)`, N=1..51. Non-default options require
explicit hardware and matching typed native readback after preparation. Missing,
unsupported, floating or mismatched readback fails. Existing constructors leave
QP untouched and retain default CPU fallback. The probe exposes `--max-frame-qp=N`;
matched cases append `:qpN`, optionally after explicit speed/quality priority.

Apple's [property documentation](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_maxallowedframeqp)
and [API discussion](https://developer.apple.com/videos/play/wwdc2021/10158/)
describe a quality/bitrate tradeoff that can include dropped frames. Typed
readback is not an observation of actual per-frame QP. Our dropped-output
callback remains terminal; neither frames nor requested settings are silently
substituted to make an experiment pass.

Release build and strict library/example/test Clippy pass. Two new Rust range/
hardware/integer-readback contracts, one existing priority contract and two
selected Python profile-grammar tests pass. All eight tiny combinations
(H.264/HEVC, ordinary/low-latency, QP28/QP36) accept exact native readback and
independently decode32 frames each at320×180. Three old/current decoded default
pairs match, hardware H.264/HEVC and software H.264:192 decoded frames total.
Old control source is `f098e462a62833b73d6a6103e4f50ff810fb7ffb`.

The generated 1080p60 text workload uses60 warmup and480 observed frames. Five
completed streams independently decode all540 frames each; QP28 fails separately.

| Request | Observed payload Mbps | Callback p95 ms | Worse foreground aggregate / R,G,B frames |
| --- | ---: | ---: | --- |
| H.264 compatible3000 | 1.545525 | 11.741 | 0 / 0,0,0 |
| HEVC low-latency12000, no QP option | 1.242474 | 5.537 | 12 / 12,13,12 |
| HEVC low-latency12000, QP36 | 1.242474 | 5.572 | 12 / 12,13,12 |
| HEVC low-latency12000, QP32 | 1.242474 | 5.612 | 12 / 12,13,12 |
| HEVC low-latency12000, QP34 | 1.242474 | 5.617 | 12 / 12,13,12 |

The four completed HEVC streams have identical normalized decoded thin-region
hashes, payload counts and foreground metrics. QP32/34/36 therefore do not fix
the measured keyframe-adjacent regressions on this clip. At1080p60, QP28 accepts
integer readback but reports native dropped-output flag2 after just two decoded
warmup frames; the observed phase never starts. Small-input availability does
not establish support at the intended resolution/workload. No setting is promoted.

The first full matrix remains exit1. Its three earlier complete encode/decode/
SSIM streams are analyzed in a separate receipt without native replay. QP32/34
are fresh candidate processes against the retained exact-source H.264 baseline,
not another baseline execution. Individual timing/CPU samples include startup,
warmup and file I/O; they are not sustained or statistical paired-speed results.

Two earlier supervisor failures are retained: missing harness FFmpeg metadata
stops before compilation; an availability postprocessor then incorrectly asks
the32-frame60fps probe for a complete one-second bitrate window. The corrected
continuation decodes that retained stream without repeating the encoder or
already-passed tests. Availability-only parsing makes no one-second bandwidth
claim; the matched parser's window and frame/rate bounds remain unchanged.

All79 owned command groups across the retained attempts and analyses are
independently absent. `index.json` hashes published records. Exact commands,
source checks, deadlines, exits, numeric samples, SSIM/foreground rows, compiler
records and failures remain alongside passing results. `python3 -B verify.py`
recomputes these retained numeric contracts without native replay. Original and
normalized hashes are distinct; normalized helper/exporter snapshots were not
executed. No image/font/bitstream/credential/private application source is published.

The consuming application stays pinned to f098 and requests no QP experiment.
No default, optimistic target, admitted profile or CPU fallback changes. Physical
devices/LAN, hardware decoder identity, actual network/billed cost, Linux/Windows,
full application/helper/GPU/power resources, human legibility and sustained quality
qualification remain open. This experiment offers no measured bandwidth/quality
improvement over its default HEVC control.
