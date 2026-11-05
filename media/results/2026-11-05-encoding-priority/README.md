# Encoding-priority API and short native experiments

Measured source `53cf75a21adcd3cb4d322a2492a320a767cf0495`, Apple M5 Pro
(Mac17,8), macOS 27.2 build 26B5101f. This is an optional component experiment.
The application still consumes its separately frozen earlier media revision;
this API does not change application defaults, performance targets or admission.

## API and compatibility checks

`VideoEncoder::new_tuned_with_priority` accepts `EncodingPriority::Default`,
`Speed` or `Quality`. Default delegates without querying or setting the property.
Explicit modes require hardware and matching typed native boolean readback after
preparation. Software/preferred-hardware requests fail before session creation.
The getter reports the verified explicit setting; Default leaves actual native
preference unspecified. A readback is not measured speed or quality evidence.

The release probe builds, strict library/example/test Clippy passes, one selected
Rust contract test and five Python parsing/profile tests pass. Four ordinary
H.264/HEVC × Speed/Quality combinations each encode and independently decode 32
generated 320×180 frames. All four explicit-priority/low-latency combinations
fail at property setting with native status -12900; they remain unavailable.
Legacy/current default hardware H.264/HEVC and software H.264 controls decode
to matching hashes (six 32-frame controls). All successful availability/control
checks decode **320 frames total**. Tiny bars do not qualify speed, legibility,
real-time software fallback, native decoding or other hardware.

Four invalid command cases refuse with exit 1 before output creation. The first
outer supervisor receipt fails after its successful child because parent and
child shared a marker path. Its failure is retained. A corrected controller and
worker use separate namespaces, validate the original artifacts without replay,
and add the software control; both exit zero. These distinct results are not
collapsed into an all-passed claim.

## Matched speed-priority pilot

One second warmup plus eight seconds observed, 1920×1080/30fps, same generated
coding/motion clip and two-second explicit keyframe cadence; 270 independently
decoded frames per case. The four-case matrix and all quality oracles complete.

| Candidate | Observed Annex B bytes | Encode callback p95, ms | Thin foreground mean RGB error |
| --- | ---: | ---: | ---: |
| H.264, default priority, requested 3000 kbps | 1,248,875 | 12.231916 | 13.849086 |
| H.264, speed priority, requested 3000 kbps | 1,248,875 | 11.720625 | 13.849086 |
| HEVC, default priority, requested 1500 kbps | 772,445 | 14.613959 | 13.356076 |
| HEVC, speed priority, requested 1500 kbps | 1,002,146 | 10.541584 | 13.647046 |

H.264's byte counts and foreground metrics match; its encoded stream hashes
differ. The small timing difference is not a repeatable speedup claim. HEVC
speed priority is faster in this observation, but uses **29.74% more bytes**
and has worse aggregate foreground error in all 240 frames against its own
default. Each separate RGB channel also worsens in all 240 frames. Against
H.264 it worsens in 20 aggregate frames and R/G/B in 4/174/31 frames. It fails
the quality requirement and is not promoted.

## Separate low-latency pilot: original matrix failed

This pilot leaves priority at Default. H.264 low latency encodes/decodes 270
frames, but HEVC low latency closes during encoding (`Error: Closed`), after
successful creation and first output. The original four-case harness exits 1;
its three earlier completed encode/decode cases lack the final matrix-wide
foreground step. A separately bounded analysis of their retained RGB crops
completes without encoding again. It does not turn the matrix into a pass.

Within this separate pilot, ordinary H.264 has p95 12.506333 ms and 1,248,875
observed bytes. Low-latency H.264 has p95 5.383417 ms and 888,269 bytes, with
worse aggregate foreground error in 178/240 frames (R/G/B: 143/210/95). Its
mean error rises from 13.849086 to 14.072573. It also fails quality admission.
The HEVC failure's native cause is not identified by the existing closed error;
no missing latency/quality result is filled with zero or a passing result.

## Evidence and limits

`summary.json` separates API availability, complete priority pilot, and partial
low-latency analysis. Receipts, failed logs, helper snapshots, executable/tool/
fixture hashes, exact commands, external group deadlines and numeric rows are
retained. All **89 owned command groups**, including expected refusals and the
failed matrix/supervisor, are independently absent; this does not count them as
89 passing commands or establish shared OS-helper teardown. Deterministic gzip
preserves log/CSV/SSIM bytes; original and normalized-path hashes are distinct.
`index.json` hashes published files. No screen, decoded image, font, bitstream,
credentials or private application source is included.

Eight seconds is a pilot, not sustained or five-session qualification. Callback
timing excludes source read/fill and file output. Whole-probe resource values
include initialization/warmup/I/O and exclude shared OS helpers, GPU/power,
application/viewer baseline and physical footprint. No new CPU/memory budget,
native hardware-decoder, 1080p60, physical phone/LAN/NAT/input, wire/relay bill,
codec/profile admission or deployment result is claimed. The earlier five-
session ordinary-codec comparison remains separate. All targets remain active.
