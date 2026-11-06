# Bounded asynchronous encoder experiment

Allowing two ordered submissions to overlap improves this Mac's HEVC completion
cadence on generated 3840×2160/60 input. It adds memory and more CPU per wall
second. H.264 overlap increases receipt latency, so that variant is not selected
for the consuming application's opt-in path. No production default or
performance target is changed by these measurements.

Host: Apple M5 Pro (Mac17,8), macOS 27.2 build 26B5101f, Rust/Cargo 1.99.0.
Every case requests hardware, compatible/default tuning, 20,000 kbps,
full-range NV12 with sRGB/BT.709 tags, no reordering and forced two-second
keyframes. The immutable generated source cycles still/typing/scrolling/window
movement; its SHA-256 is
`1353a6c216af0e3415b3748bc54626828d144a7dd10b574902f17fd948411431`.
This is one clip on one device, not independent content or endpoint diversity.

## Results

Two short pairs per codec alternate depth order. Each stream has 60 warmup and
480 observed frames. These are observed ordered output receipts, not nominal
PTS, native callback timestamps, application display FPS or end-to-end latency.

| Codec / pair | Serial completion fps | Two-frame fps | Serial receipt p95 ms | Two-frame receipt p95 ms |
| --- | ---: | ---: | ---: | ---: |
| H.264 / 1 | 41.18 | 50.76 | 27.43 | 41.13 |
| H.264 / 2 | 40.71 | 50.85 | 27.52 | 40.99 |
| HEVC / 1 | 47.32 | 59.93 | 22.76 | 22.50 |
| HEVC / 2 | 48.10 | 59.96 | 22.80 | 22.41 |

The follow-up has two new HEVC pairs, each with 600 warmup and 3,600 observed
frames. Slow serial controls finish the same frame count in more wall time;
the explicit native watchdog is twice nominal duration plus 20 seconds. That
watchdog does not admit missed cadence. Every per-frame output deadline and
quality criterion stays unchanged.

| HEVC pair / depth | Completion fps | Receipt p95 ms | Peak probe RSS MiB | Whole-probe CPU, one core % |
| --- | ---: | ---: | ---: | ---: |
| 1 / serial | 45.29 | 22.90 | 59.39 | 13.70 |
| 1 / two | 60.00 | 22.62 | 83.02 | 18.10 |
| 2 / serial | 52.12 | 22.69 | 60.22 | 11.59 |
| 2 / two | 60.00 | 22.56 | 83.20 | 16.89 |

The two-frame path adds about 23–24 MiB peak process RSS and 4–5 percentage
points of one core in these longer runs. CPU time for the whole fixed clip is
12.55/12.69 seconds in pair one and 9.23/11.84 in pair two, serial/two respectively.
It is not evidence of lower CPU work per encoded frame. CPU/RSS include file
I/O, initialization and warmup and exclude native helpers, GPU activity, power,
viewer, transport and app baseline. RSS is not incremental physical footprint.

Within every pair, observed payload byte counts are equal. Short H.264/HEVC
payload rates are respectively 2.353044/2.482875 Mbps; the longer HEVC rate is
1.962733 Mbps. These use nominal source duration, not actual wire traffic or a
VPS invoice. There is no pipeline bandwidth saving or codec-only comparison.

## Correctness and provenance

- The short experiment is exact source
  `cbb5848d71202123fc738a96e5b13f3e7dc58b0f`; its four pure ledger, two
  parser/geometry and one per-frame/channel comparator tests pass. Native
  example build and strict library/example Clippy pass.
- The longer helper is exact `b24300de85439d6a93333eb2b65ff415818a8f3f`.
  Its change only gives the slow serial control an explicit bounded watchdog;
  the Rust probe/library are unchanged. Both pairs finish all encode/decode
  counts, dimensions, color tags, forced keyframes and FIFO clocks.
- The twelve paired streams independently decode 21,120 frames. Every pair
  has the same normalized thin-text RGB digest, no worse full/text/thin SSIM
  frame/channel, and no worse reference-derived foreground aggregate/channel.
  These metrics are not complete glyph/pixel/human-legibility admission.
- Independent full-frame decode fingerprints additionally match all 540 frames
  of each short serial/two-frame pair. All 540 frames of the retained old
  `8a4a52bf66ec46da5a2bb323c6b56f92e0bd1b9b` serial executable match the new
  serial HEVC control. Including that old control, 21,660 frames decode.
  Encoded stream hashes differ; bitstream identity is not claimed. Full-frame
  fingerprints were not computed for the longer streams.
- Optional worker output wakes are exact
  `7e9faae2e976c5554ea28f3c7dc9b2bc5d92e75a`: three wake and three existing
  failure/callback tests pass, along with native test compilation and strict
  library/example Clippy. This API does not change encoder defaults or add a
  thread; the embedding worker owns delivery capacity and its wait lock.

All 100 recorded groups are independently absent; none timed out or needed
forced descendant cleanup. Builds/native encodes use the shared build lock,
nice 19, two compiler jobs and external process-group deadlines. An owner-only
archive retains 204 original numeric/log/bitstream/helper files and a hardlinked
immutable source fixture. Large observation copies and removed RGB crops can be
regenerated from that source and the retained bitstreams.

`python3 -B verify.py` verifies indexed hashes and independently recomputes
payload, cadence, receipt p95, CPU/RSS, per-frame/channel comparisons and
full-frame fingerprint equivalence. Only generated numeric records are public;
no pixels, font or bitstreams are published. Native application capture/decode,
recovery and resource results belong to the consuming application's separately
recorded source. Windows, physical LAN/phone, relay cost, complete endpoint
CPU/memory/GPU/power and optimistic performance admission remain unqualified.
