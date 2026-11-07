# Quality-priority HEVC bitrate pilots

At exact custom-fork source `8a4a52bf66ec46da5a2bb323c6b56f92e0bd1b9b`, eight
short native streams encode/decode all **2,160** frames. Each has one second
warmup and eight observed seconds at generated 1920×1080/30fps, with the same
still/typing/scroll/window-movement clip, NV12/color interpretation and forced
two-second keyframes. The HEVC variants request verified quality priority.
Two batches each have their own unchanged H.264 compatible/3000 control.
Host: Apple M5 Pro (Mac17,8), macOS 27.2 build 26B5101f, Cargo 1.99.0.

| Batch / request, kbps | Actual payload Mbps | Encode callback p95 ms | Whole-probe CPU, one core % | Peak probe RSS MiB | Text-SSIM frames below control |
| --- | ---: | ---: | ---: | ---: | ---: |
| Initial H.264 3000 | 1.248875 | 12.431 | 3.79 | 39.92 | 0 |
| HEVC quality 1500 | 0.772445 | 14.858 | 2.87 | 39.77 | 105 |
| HEVC quality 2500 | 0.777661 | 15.239 | 4.20 | 39.69 | 105 |
| HEVC quality 4000 | 0.809041 | 14.614 | 3.54 | 39.78 | 73 |
| Higher H.264 3000 | 1.248875 | 12.540 | 3.09 | 39.84 | 0 |
| HEVC quality 8000 | 1.056907 | 15.169 | 3.87 | 39.94 | 0 |
| HEVC quality 12000 | 1.134930 | 15.123 | 4.09 | 40.12 | 0 |
| HEVC quality 20000 | 1.334991 | 15.090 | 3.97 | 40.09 | 0 |

All six HEVC candidates have no worse original-RGB foreground aggregate/channel
errors in their 240 observed frames. The lower requests nevertheless reduce
scrolling text SSIM; increasing to 8000 closes that metric gap on this clip.
At 8000 the candidate uses **15.37% fewer encoded payload bytes** while all
240 text-SSIM observations improve against its paired control. Its callbacks
are slower, and nine-second whole-probe CPU is higher. At 20000 the candidate
uses **6.90% more bytes** than H.264. A rate request is neither actual bitrate
nor an output ceiling. Better aggregate metrics do not prove every pixel,
human legibility, chroma/text quality on other content or full codec admission.

This is one fresh session per HEVC variant, not the required five-session
sustained qualification. CPU includes startup, warmup and file I/O and excludes
helpers/GPU/power, a viewer and baseline subtraction. RSS is not incremental
physical footprint. No capture, presentation/input-to-picture latency, actual
wire traffic, TURN billing, hardware-decoder or physical-device result follows.
The roughly 15ms encode p95 does not meet the optimistic 3ms 1080p60 target;
this experiment is 30fps. No application pin, default, admission or target is
changed. The 8000 setting remains a candidate for sustained broader testing.

Release build and strict library/example Clippy pass in the initial batch.
The higher batch verifies and reuses that exact executable before/after, rather
than rebuilding. Shared lock, nice 19, two build jobs and external process-group
deadlines remain in place; every recorded group is independently absent.
The immutable generated source is hash/matrix checked. Independent FFmpeg
decodes every frame, measures three SSIM regions and normalizes foreground RGB.
No application or browser is launched, no OS input/screen capture occurs, and
no images, font or bitstreams are published.

[summary.json](summary.json) and the two compressed observations retain the
actual frame/region numeric evidence. `python3 -B verify.py` recomputes payload,
latency, process CPU and foreground summaries plus the per-frame text comparison
and verifies all indexed file hashes. Published command paths are normalized;
the index records original evidence hashes and an owner-only archive reference.
This is experiment evidence, not a change to the production codec policy.
