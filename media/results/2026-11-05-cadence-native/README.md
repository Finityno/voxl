# Native 60fps source and short codec pilots

At exact source `1f94f7618dc3e11abb15ad858112c8f4686a2320`, the offscreen
Swift generator compiles with Swift 6 strict concurrency and warnings denied.
Fresh default 30fps manifest/NV12/original-RGB files match the original hashes
byte for byte. The new 60fps source has 540 frames: 60 prefix and 480 observed,
four two-second phases. Four consecutive motion frames have distinct hashes;
this source does not duplicate 30fps frames. Independent calibration checks
the conversion matrix, static thin-text reference and source counts. Unsupported
120fps and a missing height exit 1 with diagnostics before creating output;
neither causes the former top-level Swift error crash.

Two native 1080p60 batches use one second warmup plus eight observed seconds
per fresh stream on one M5 Pro/macOS 27.2. Release build/strict library-example
Clippy pass. All five streams independently decode all 540 frames with the
expected color tags. Each batch has its own H.264 compatible/3000 control.

| Batch and request | Observed payload Mbps | Encode callback p95 ms | Probe CPU, one core % | Peak probe RSS MiB | Worse foreground aggregate / R,G,B frames |
| --- | ---: | ---: | ---: | ---: | --- |
| Pilot H.264 3000 | 1.545525 | 9.772 | 4.11 | 39.75 | 0 / 0,0,0 |
| Pilot low-latency HEVC 12000 | 1.242474 | 5.581 | 7.04 | 47.19 | 12 / 12,13,12 |
| Follow-up H.264 3000 | 1.545525 | 11.635 | 5.56 | 40.05 | 0 / 0,0,0 |
| Follow-up low-latency HEVC 18000 | 1.232612 | 5.598 | 6.73 | 47.06 | 7 / 2,10,6 |
| Follow-up low-latency HEVC 20000 | 1.222995 | 5.563 | 6.09 | 47.11 | 3 / 0,10,3 |

The first HEVC pilot uses about 19.6% fewer encoded bytes, with faster measured
callbacks and higher whole-probe CPU/memory. Higher bitrate requests reduce
but do not eliminate foreground regressions. They occur shortly after forced
keyframes; the retained row indices show this timing association, not a proven
driver cause. Mean text SSIM improves but scrolling-phase SSIM remains lower.
Rate request and observed bytes are not monotonic or an output ceiling. The
coordinator initially requested 24000, above the existing 20000-kbps tool bound;
that plan was refused before any output/build/native execution. The corrected
18000/20000 plan preserves the bound. No refused plan counts as a passing test.

`results.json` files retain exact commands, deadlines, source checks, all latency
and payload samples, independent decoder metadata and metrics. Indexed gzip
logs, per-region SSIM and foreground JSON retain numeric evidence, including
the uncompressed 4:2:0 floor. Run `python3 -B verify.py` here to recompute the
published hashes, counts, source mapping, latency/payload/SSIM and foreground
comparisons. Original and normalized hashes remain distinct. Normalized helper
snapshots document executed originals and were not themselves run. All 50 owned
command groups across generation and both batches are independently absent.
No rendered image, font, bitstream, credential or private application code is
published. The 13 earlier parser/metric tests retain their earlier exact source;
they are not rerun or counted as new tests here.

These are short component pilots, not five-session sustained 60fps qualification.
Nine-second process CPU includes startup, warmup and file I/O and excludes OS
helpers/GPU/viewer/power; it cannot be compared directly with a baseline-subtracted
application budget. Software decoding does not prove hardware receiver support.
No capture/input, actual presentation cadence, physical endpoint/network/relay
cost or full quality/resource admission is established. Foreground metrics do
not prove human legibility. Keep these profiles experimental; the 3ms stretch
encode target, other optimistic budgets, defaults and frame/rate bounds remain
unchanged.
