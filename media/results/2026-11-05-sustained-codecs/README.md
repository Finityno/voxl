# Sustained native H.264 and HEVC component comparison

Exact clean source `f1f08a424d7e405543f4088c522c19f5c49b22bc`, Apple M5 Pro
(Mac17,8), macOS 27.2 build 26B5101f. This is **one independent session per
codec**, each with 30 seconds warmup and 300 seconds observation at 1080p30.
Five independent sessions, the physical endpoint matrix and complete resources
remain required. No runtime, codec policy, default, application pin or target
changes; media source/vendor/manifest/config inputs match runtime
`f81e553ba5c1da15223c3374e57d44531c64ca8c`.

## Workload and structural checks

The existing offscreen CoreText fixture has fixed generated code, still text,
typing, scrolling and window movement. No screen, window or OS input is captured
or manipulated. The source's thirty-frame prefix repeats during warmup; its
240 observed frames repeat during observation. Capture timestamps increase
continuously. Cycling avoids duplicating roughly 30.8 GB of raw input and does
not create independent runs or a wider content corpus. Each codec sees the
same pixels, forced two-second keyframe cadence, full-range BT.709 matrix and
primaries, sRGB transfer and 8-bit 4:2:0 geometry. Source NV12/RGB/manifest hashes
match the earlier fixture and ownership-safe generator reproduction.

Each session emits 9,900 access units, 165 keyframes and all parameter sets at
the required keyframes. Independent FFmpeg/ffprobe decoding reads all frames,
verifies the actual Constrained Baseline/Main profiles and color metadata, and
compares 9,000 observed frames to explicitly clocked source cycles. Both native
encoders report queried hardware selection. There is no receiver, capture,
transport, account/control authority or application profile admission here.

## Measured tradeoffs

| Metric | H.264 Constrained Baseline | HEVC Main |
| --- | ---: | ---: |
| Requested bitrate, kbps | 3000 | 1500 |
| Observed Annex B bytes, 300 s | 38,878,986 | 26,045,878 |
| Observed mean Annex B Mbps | 1.036773 | 0.694557 |
| p95 / peak nonoverlapping one-second Mbps | 2.489184 / 2.489184 | 1.623088 / 1.623088 |
| Startup frame bytes | 46,229 | 44,185 |
| First observed frame / largest observed frame bytes | 103,012 / 104,288 | 86,010 / 92,651 |
| Encode-call-to-callback p95, ms | 12.3797 | 14.6935 |
| Probe process CPU, percent of one core | 3.6198 | 3.6721 |
| Probe process peak RSS, MiB | 40.4844 | 40.3750 |
| Code-text region mean / minimum YUV SSIM | 0.999549 / 0.999276 | 0.999610 / 0.999309 |
| Thin-region foreground RGB mean absolute code error | 13.64889 | 13.19450 |

HEVC produces **33.01% fewer observed Annex B bytes** in this run, with about
2.31 ms higher p95 encode callback latency. Neither byte difference nor codec
name establishes matched-quality production savings. The paired short diagnostic
(one-second warmup/eight seconds observed) instead gives 38.15% fewer bytes;
its H.264/HEVC outputs reproduce the preceding short-workload byte counts. Do
not extrapolate that short ratio to a sustained workload or relay bill.

The original-RGB-derived ink/one-pixel-halo mask evaluates 9,792 pixels per frame.
HEVC's exact aggregate foreground errors are no worse than H.264 in all 9,000
observed frames, both summed across RGB and separately per channel. This does
not prove every pixel or glyph is no worse, human legibility, or all color spaces.
The uncompressed 4:2:0 reconstruction control's mean error is 12.79456; do not
subtract that MAE as an additive codec-loss component. HEVC's scrolling code-text
mean YUV SSIM is **lower** (0.999644 versus 0.999840), despite its higher overall
mean and better measured thin-foreground metric. There is no uniform quality win.

Phase means remain separate. Still/typing expose 2,280 frames each and
scrolling/window movement 2,220 each because 300 seconds ends halfway through
the final eight-second cycle. Both candidates have identical exposure. Requested
bitrates are not output ceilings; measured first-frame and one-second bursts
remain explicit. Probe wall intervals are 329.989/329.981 seconds versus a
329.967-second nominal first-to-last frame interval; nominal PTS do not establish
application presentation cadence or end-to-end latency.

CPU and peak RSS are macOS `time -lp` whole-probe measurements including I/O,
initialization and warmup. They exclude external VideoToolbox helpers, GPU/power,
the viewer and application baseline; RSS is not physical footprint. Encode
latency starts after input read/fill and ends when the callback output is received.
Neither these resources nor this 1080p30 test passes the separate optimistic
1080p60 application targets. Annex B is not packetized video, actual ingress/
egress, TURN overhead, audio, retransmissions or provider-billed bandwidth. The
0.46655/0.31255 decimal GB per hour figures in the JSON are byte arithmetic only.

## Tool changes, checks and evidence

`codec_probe` adds bounded cyclic file input and two-read packed NV12 filling;
padded native buffers retain row reads. The analyzer accepts longer matching
observations, reports aggregate channel regressions and optionally uses NumPy
with exact widened integer differences. The runner validates decoding, quality
and foreground attribution before marking a case complete. Private RGB crops
are removed only after successful analysis and can be regenerated from retained
hashed inputs. AV1 has no native implementation in this Mac probe.

At the final source, release build and strict library/example Clippy pass.
Three selected Rust cycle tests, four Python metric tests and four evidence-parser
tests pass: **11 tests, zero failed/skipped**. Two native tiny-file reader cases
prove packed 320-byte and padded 384-byte strides at widths 320/322; independently
decoded luma order is exactly 16,16,110,235,110,235 in both. Those are CLI checks,
not additional unit tests or general color accuracy. Three invalid-cycle CLI
cases exit nonzero and create no output. Initial source 1bf86f6 has three passed
and one failed parser test: the new test expected H.264's parameter-set size for
HEVC. That fixture expectation is corrected; no native reader or long session
ran at the failing source. Its diagnostics remain in `validation.json`.

`summary.json` contains derived metrics and source attribution. Two sample CSVs
retain all 18,000 observed payload/encode timings; six SSIM files retain 54,000
region rows; three foreground CSVs retain 27,000 rows including the uncompressed
control. RGB channel sums plus the fixed 9,792-pixel mask reproduce the exact
foreground MAEs. Percentiles use nearest rank. `validation.json` retains exact
commands, source checks, tool versions, logs/hashes, deadlines and **64 independently
absent process groups**, including both contract queues. Path placeholders name
private inputs without publishing device identities, source images, font,
bitstreams, credentials, SDP or private application source. `index.json` hashes
published files. Large private build/source/codec files remain local with hashes.
`export-integrity.json` retains a separate bounded post-export check and its
absent group: all CSV/SSIM counts, exact derived foreground means, encode p95s,
payload sums and reduction reproduce from the published numeric rows.

`exporter.py` reproduces the data exports from retained terminal results in a
worktree at the measured source, before this results directory exists. Set
`VOXL_SOURCE_WORKTREE`, `VOXL_FROZEN_CHECKOUT`,
`VOXL_FIXTURE`, `VOXL_SUSTAINED_RESULTS`, `VOXL_DIAGNOSTIC_RESULTS`,
`VOXL_CONTRACT_RESULTS` and `VOXL_INITIAL_CONTRACT_RESULTS` to the owned paths.
It refuses missing/failed terminal comparisons or a surviving owned group.
It is an evidence exporter and does not launch another native measurement.

The README formerly required exactly 240 analyzer frames; it now describes the
bounded variable observation count and exact channel checks actually implemented.
Five-run repeatability, complete app/helper/GPU/power, active hardware decoder
proof, physical phone/LAN/NAT/VPS, AV1/provider admission, actual wire savings and
the existing BT.709 canvas observer gap remain open. No performance target is
relaxed and the full implementation goal remains active.
