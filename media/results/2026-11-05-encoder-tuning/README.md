# Native encoder tuning experiments

Runtime source `7677d66abe68be42572b838c5460fd8583bc5f34`, custom fork only.
The ordinary encoder constructor retains its compatible profile, real-time mode,
two-second keyframe setting and hardware-first/software fallback. The embedding
application remains pinned to its prior media revision. No new mode/profile is
automatically enabled or admitted by this work.

`new_tuned` supports explicit local profile/rate-control requests. Non-default
requests require hardware and fail rather than substituting another tuning or
software. Optional framework constants are dynamically resolved. H.264 High and
Constrained High can be selected independently of low-latency rate control;
HEVC keeps Main. Low-latency requests omit the ordinary periodic-keyframe
property, with explicit recovery requests owned by the caller. Probe options
bound warmup, observation and keyframe periods, preserve access-unit boundaries
and timestamps, and never overwrite output files.

## Initial failure and acceleration observation

At `ce39fc599fcfd7b349a4096b86c7283f69d249b3`, native release build and strict
Clippy pass. Five ordinary modes encode all 20 frames, independently decode them
and restart a fresh decoder at the later forced keyframe. Four low-latency
attempts successfully create/prepare their sessions, then fail specifically at
the hardware-status query with unsupported-property error -12900. These four
failures are preserved in `initial-ce39fc5/artifact-pack.json`; no low-latency
performance result is inferred from them.

Apple's [required-hardware specification](https://developer.apple.com/documentation/videotoolbox/kvtvideoencoderspecification_requirehardwareacceleratedvideoencoder)
requires creation to fail when hardware cannot be used. The corrected path
accepts only the unsupported-status error for an explicitly hardware-required
low-latency request. Ordinary hardware queries, other errors, missing successful
query values and selection mismatches remain strict. The existing ordinary
software-only path still relies on explicit hardware prohibition when that
query is unavailable.

`EncoderAcceleration` reports `QueriedHardware`, `QueriedSoftware`,
`RequiredHardware` or `DisabledHardware`. The legacy hardware boolean derives
from it. A successful required specification is not labelled as a successful
status query, physical power measurement or hardware-decoder observation.

The [low-latency key](https://developer.apple.com/documentation/videotoolbox/kvtvideoencoderspecification_enablelowlatencyratecontrol)
and installed SDK say “Only High profiles”. This Mac nevertheless accepts an
explicit low-latency request with Constrained Baseline, emits SPS `42c01f`, and
passes independent decode/recovery. Record that disagreement rather than
assuming other OS/device combinations behave alike. The observation is an
accepted creation request and actual bitstream, not a separate internal
rate-control readback. No SPS flags or SDP are rewritten to disguise a profile.

## Native validation

On an Apple M5 Pro / macOS 27.2 build 26B5101f, release compilation and strict
native library/examples Clippy pass at the corrected source. All nine smoke
cases encode/decode/recover: hardware H.264 compatible/High/Constrained High,
their three low-latency variants, hardware HEVC Main in both modes, and explicit
software H.264. Each keeps 20 access units and correct capture timestamps, with
real keyframes/parameter sets at frames 0 and 10. A fresh decoder starting at
frame 10 reads the remaining ten frames. Software H.264 is explicitly selected;
this does not exercise an actual hardware-failure fallback.

The subsequent matrix uses five fresh encoder processes per candidate, **45
successful component runs**. Same-profile pairs alternate regular/low-latency
order, and pair groups rotate/reverse. Each run has 30 warmup frames and 180
observed frames at 1280x720 / 30 fps / requested average 2,500 kbps: one second
warmup and six seconds observed, not a 30/300-second application qualification.
All streams have keyframes exactly at 0/60/120/180, preserved timestamps and
parameter sets. Independent ffprobe decodes all 210 frames; a fresh decoder at
frame 180 reads all 30 remaining frames. ffmpeg compares the 180 observed frames
against a separately generated NV12 reference, retaining each SSIM row. Its
first 60 frames also byte-match the earlier reference generator.

The initial quality logs warn about unequal filter time bases. A separate
decoder-only rerun explicitly sets both filter clocks to 1/30 and frame index
after trimming. All 45 retained streams decode without that warning, and all
8,100 per-frame score rows are numerically unchanged. Original warning logs
remain intact; `quality-alignment.json` records the correction, commands and
hashes. This rerun adds no encoder executions or performance samples.

| Candidate | Median of five per-run p95, ms | Observed Annex B bytes per run | Mean luma SSIM | Acceleration source |
| --- | ---: | ---: | ---: | --- |
| H.264 compatible, regular | 7.143 | 227,567 | 0.999942 | Queried hardware |
| H.264 compatible, low latency | 6.416 | 214,915 | 0.999739 | Required hardware |
| H.264 High, regular | 7.277 | 100,182 | 0.999967 | Queried hardware |
| H.264 High, low latency | 6.388 | 120,550 | 0.999563 | Required hardware |
| H.264 Constrained High, regular | 7.253 | 100,182 | 0.999967 | Queried hardware |
| H.264 Constrained High, low latency | 6.406 | 120,550 | 0.999563 | Required hardware |
| HEVC Main, regular | 8.261 | 199,082 | 0.999846 | Queried hardware |
| HEVC Main, low latency | 6.756 | 139,007 | 0.999740 | Required hardware |
| H.264 compatible, software | 7.553 | 284,561 | 0.999960 | Hardware disabled |

The latency is encode call through native output received by the worker,
including its wake. It is not pure hardware or input-to-display latency. The
table shows the median of five run p95 values; all individual samples and paired
changes are retained. Percentiles use explicit nearest rank. Earlier 60-frame
summaries used conservative upper order statistics, including index 57 for a
p95 label. That measurement correction is not an encoder speedup, and old raw
samples remain unchanged.

In the same-profile pairs, low-latency requests lower measured p95 in every run:
2.7–10.6% for compatible H.264, 11.0–12.5% for High, 11.2–12.9% for Constrained
High, and 16.5–18.5% for HEVC. Luma SSIM is lower in every low-latency variant.
High/Constrained High use 20.3% more bytes with low latency; HEVC uses 30.2% fewer.
These are observed tradeoffs at equal requested bitrate, **not matched-quality
bandwidth savings**. Regular Constrained High uses substantially fewer bytes
than compatible H.264 on this pattern; browser/profile compatibility and the
actual endpoint budgets still need qualification before selecting it.

Whole probe process CPU medians are approximately 5.0–5.3% of one core for
regular hardware modes and 6.0% for low-latency modes, with peak RSS medians
roughly 37 versus 43 MiB. These include warmup, synthetic pixel generation and
file I/O; the latency interval excludes the generation/write work. `/usr/bin/time`
has 0.01-second time precision, and its Darwin RSS unit is bytes as documented
by the installed `getrusage` manual. They exclude native codec/GPU helpers and
baseline subtraction. The software probe's lower process-only CPU does not
establish lower total software CPU/power. RSS is not physical footprint. Raw
resources and their full scope are in `final-7677d66/analysis.json`.

## Evidence and remaining work

`final-7677d66/native-smoke.json`, `native-matrix.json` and `analysis.json`
contain commands, exits, raw latency/SSIM/time logs, checksums, compiler artifact
and exact source. The matrix reuses the verified smoke binary without another
build. Encoded/framed/reference binaries stay outside the repository with
lengths/hashes recorded. These contain only generated pixels, never a captured
desktop, input, screenshot or recording of the user's screen.

At the final source, two selected Linux tuning/observation-policy tests pass,
with default library test build, WebRTC library/examples check, strict Clippy,
diff and protected-source/feature guards. All manifests, locks, vendor code and
Cargo configuration remain byte-identical; AWS-LC stays active without active
Ring. The initial one-test cloud run remains attributed to ce39. Included-file
and transfer hashes were verified; large successful Cargo artifact JSON remains
remote with a size/hash and selected executable metadata included. Cloud runs
provide no native codec/quality/performance evidence and build no root server.

Independent cleanup finds all 237 recorded native/decoder/supervisor groups
gone and the frozen native checkout clean at 7677d66. This says nothing about
persistent system codec/GPU helpers. See `cleanup.json` and `host.json`.
The later alignment rerun independently verifies its additional 46 groups gone.

The input is monochrome edges and a moving bar. Whole-frame/luma SSIM does not
qualify readable text, thin colored text, chroma damage, a realistic coding
workload or perceptually matched quality. No browser/phone decoder, physical
network/VPS, full application, helper/GPU power, thermal, end-to-end latency or
five independent 300-second target is established. The application still needs
measured per-device/profile admission, hardware-decoder identity/budgets and
real hardware-failure fallback. AV1, Windows/Linux native paths, backend
deployment and the remaining full-application work are not completed here.
