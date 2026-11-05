# Controlled coding workload and receiver observations

Encoder/data source `2a316411ce07b1c326e0e3e47a80e4ee3a91915a`;
foreground analysis and replay transport source
`cfb756373d0b23842c57e6e81b3da4a793aaa613`;
final generator source `0473a75bb6e4551271d7af3579383fe59fc13ef0`.
Only tools/examples/documentation change. All fourteen tracked media source files
and all 36 protected manifest/lock/vendor/config inputs match the preceding
checkpoint. No production encoder, transport, default or application pin changes.
Measured per-device codec admission remains open.

## Reproducible source and tools

`media/tools/desktop-workload.swift` renders fixed source text with the installed
Menlo font in an offscreen sRGB CoreGraphics context. It captures no screen,
opens no window and sends no input. The 1920x1080 / 30 fps clip has thirty
warmup frames and four sixty-frame phases: still code, typing, scrolling and
window movement. The source is converted to full-range BT.709-matrix NV12 with
sRGB transfer. A manifest records geometry, interpretation, font identity and
file hashes. The original RGB small-text/stroke region is retained before
4:2:0 conversion. No font or image is bundled in the repository.

`codec_probe` accepts bounded geometry/fps/bitrate and tightly packed NV12 file
input. It checks file type/length before creating output, reads active rows into
the exclusive native buffer and excludes read/fill work from timed encoding.
Defaults retain the previous monochrome input, 720p30 and 2500kbps. An independent
210-frame decoded-hash control matches the preceding default output. Short file,
symlink, mixed generated/file patterns and invalid geometry all fail as intended,
creating no output. These are four CLI contract cases, not Rust test counts.

`media/tools/analyze-text-workload.py` validates source hashes and all 270 static
original RGB region frames, then derives foreground ink and a one-pixel halo
from the original alone. Here 3,856 ink pixels expand to 9,792 evaluated pixels
in a 96,320-pixel region. Independent rational RGB-to-NV12 spot checks at 163
positions in source frames 0/269 match exactly. Contrast threshold eight is an
explicit mask definition, not a quality tolerance. Integer error sums compare
candidate and baseline at every observed frame; the decoded output cannot
choose which pixels are evaluated. Truncated decoded data and a modified middle
source frame with internally consistent hashes are both rejected. Those two
negative contract cases create no result file.

## One short native comparison

All three hardware encoder runs preserve 270 access units and capture timestamps,
with actual keyframes/parameter sets at 0/60/120/180/240. Independent ffprobe
reads all frames and verifies actual profile/geometry, full range, 709 primaries
and matrix, and sRGB transfer. FFmpeg compares the 240 observed frames against
the same uncompressed source with explicit matching frame/filter clocks. The
three YUV regions plus original RGB and uncompressed-conversion controls retain
3,120 SSIM rows. Region and channel results stay separate.

| Candidate | Requested kbps | Observed bytes, 8 s | Encode-to-worker p95, ms | Text-region mean YUV SSIM | Foreground RGB mean absolute code error |
| --- | ---: | ---: | ---: | ---: | ---: |
| H.264 Constrained Baseline | 3000 | 1,248,875 | 12.58 | 0.999217 | 13.8491 |
| H.264 Constrained High | 3000 | 1,030,567 | 12.80 | 0.999250 | 13.8124 |
| HEVC Main | 1500 | 772,445 | 15.02 | 0.999431 | 13.3561 |

These are **one run per candidate**, one second warmup and eight seconds observed
on this Mac. Byte counts are observed Annex B output, not full wire traffic,
relay billing or a transport bandwidth ceiling. Equal input/chroma does not by
itself make quality equal. The observed reductions, roughly 17.5% and 38.1%
against Baseline, are not qualified production or matched-quality savings.
Latency includes native processing and worker wake, excludes file reads, and
is not pure hardware or input-to-display latency. This is 1080p30, not the
optimistic wired 1080p60 target.

Constrained High improves the mean foreground metric but is worse than Baseline
in **26 of 240 frames** (8 still, 6 typing, 12 window movement). The decision
uses exact aggregate RGB integer sums, without an average hiding those frames.
HEVC has no worse aggregate foreground error in this clip's 240 frames. That
one metric is not proof every pixel, glyph or channel is never worse, nor is it
statistical evidence across devices/runs.

The separately decoded uncompressed 4:2:0 source has foreground error 12.7946
against original RGB, and full-region RGB SSIM 0.989949. Significant error thus
exists before encoding. This control includes source subsampling and FFmpeg
reconstruction; it is not a theoretical bound. Do not subtract these MAEs as
additive physical error components or credit source conversion loss to a codec.
Whole-region/whole-frame averages also include substantial background. No
human legibility inspection or complete text-quality admission is claimed.

## Production-page replay and color controls

Three separate native WebKit wrappers run the embedding application's exact,
unmodified receiver page over real localhost UDP/DTLS/SRTP. Each decodes all
270 frames/five keyframes at 1920x1080, with one peer/answer/first-frame event,
selected/nominated direct candidates and connected DTLS. Constrained High works
for this exact 1080p stream; the earlier ordinary-High failures remain separate.
There is no capture, Connect authority, control or OS input in these replays.
Decoder implementation/power flags remain unavailable.

After counters reach 270, one offscreen canvas crop verifies actual sRGB canvas
and ImageData formats, opaque pixels and the same 9,792-pixel reference mask.
It records numeric errors only, with no screenshot/frame persistence or visual
inspection. Mean errors are 15.4090 / 15.3962 / 15.2380 for Baseline / Constrained
High / HEVC. The exact presented frame is not identified; this is one sample,
not a per-frame browser quality distribution or a performance measurement.

Three further 20-frame/two-keyframe sRGB bar replays sample sixteen flat centers.
Maximum center errors are four code values for both H.264 profiles and three
for HEVC; the gray centers differ by at most one. A paired Baseline replay of
old untagged-output and corrected tagged-output streams produces **identical recorded**
sRGB readback metrics on this Mac. This does not demonstrate a visible gain
from the earlier signaling correction, or mean metadata can be omitted for
other endpoints/source spaces.

One BT.709-transfer control compares the tagged stream against the independently
color-normalized uncompressed source. Its canvas center error reaches **17**
code values; gray errors are 13/15/7 where the expected sRGB values are
140/79/198. Thus this canvas path cannot currently serve as a universal
color-fidelity oracle. Successful decoding/opaque readback is not a color-accuracy
pass. These observations neither isolate the cause nor prove the separate
video compositor/display path is wrong. No speculative video filter or capture
color-space override is applied. All nine replay cases succeed at their
measurement/decode oracles; the BT.709 accuracy gap remains explicitly open.
The tested offer and answer negotiate MID/RID/TWCC extensions, with no color-space extension. That absence is observed, not established as the cause.
The embedding application's private evidence retains wrappers and raw reports;
only summaries are published here.

## Source attribution, failures and limits

Initial e67 builds but strict Clippy rejects a nested parser condition; no
Swift generation or encoder case runs. Initial d9 passes Rust build/Clippy,
then Swift rejects Double/CGFloat components; no source clip or codec case runs.
Both failures remain. Corrected 2a passes release example build, strict default
library/example Clippy, Swift 6 complete-concurrency/warnings-as-errors build,
generation and all selected cases. At cfb the replay sender build and strict
WebRTC library/examples Clippy also pass. Zero Rust tests execute in this
milestone; untouched production tests are not rerun or counted anew.

Final 0473 gives the bitmap context ownership of its backing storage and flushes
before reading. Its strict Swift build/regeneration reproduces **identical**
NV12, original RGB and manifest bytes. No encoder is rerun or attributed to that
source; all 85 Rust/manifest/config blobs match cfb. The data identity supports
reuse, not a claim of new performance samples.

Every run has exact clean source checks, its own target, shared build lock,
nice 19, two jobs and process-group watchdogs. All completed owned groups are
independently gone. This does not establish OS-helper teardown. Text packs
retain commands/exits, source helpers, actual profile/acceleration, raw latency
and metric rows, negative guards and hashes. Large Cargo artifact JSON,
executables, RGB/NV12 and encoded files stay local with hashes. No user screen,
private application source, credentials or full SDP is published.

Required 30-second warmup/five independent 300-second observations, complete
CPU/RAM/helper/GPU/power and hardware-decoder proof, physical phone/LAN/NAT/VPS,
actual application entry/input/revocation and device/profile admission remain
open. AV1 and other native platform providers remain unimplemented. The codec
policy, dependency pins and every optimistic performance target stay unchanged.
