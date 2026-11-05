# Five-session native codec component comparison

Measured source `f1f08a424d7e405543f4088c522c19f5c49b22bc`, Apple M5 Pro
(Mac17,8), macOS 27.2 build 26B5101f. The preceding pilot plus four additional
independent encoder processes per codec now provide **five sessions each**:
30 seconds warmup and 300 seconds observed at 1920x1080/30fps. Every session
independently decodes 9,900 frames, including warmup. No application default,
target or codec admission changes.

| Metric | H.264 Constrained Baseline | HEVC Main |
| --- | ---: | ---: |
| Requested kbps | 3000 | 1500 |
| Observed Annex B Mbps, every session | 1.036773 | 0.694557 |
| Median of the five session encode p95s, ms | 12.3797 | 13.7552 |
| Individual session encode p95 range, ms | 12.2749–12.4560 | 13.4911–14.6935 |
| Median whole-probe CPU, percent of one core | 3.6178 | 3.7145 |
| Median peak probe RSS, MiB | 40.5156 | 40.3594 |
| Code-text mean YUV SSIM, every session | 0.999549 | 0.999610 |
| Thin foreground RGB mean absolute error, every session | 13.64889 | 13.19450 |

HEVC uses **33.01% fewer observed encoded bytes in all five pairs**, with slower
encode callbacks in each pair. Encoded sizes and quality metrics reproduce
exactly on this fixed input. The summary retains all five independent p95s;
a median of session p95s is not a pooled p95 or a statistical confidence interval.
Neither deterministic repetition nor a codec name establishes broader production
or provider-billed savings.

HEVC has no worse aggregate original-RGB foreground error, summed across RGB
or separately per channel, in all **45,000 observed frames** against paired
H.264. The mask contains the same 9,792 source-derived ink/halo pixels. This
does not establish per-pixel/glyph or human legibility nonregression. Scrolling
code-text mean YUV SSIM remains lower for HEVC (0.999644 versus 0.999840), so
quality is not uniformly better. The uncompressed 4:2:0 reconstruction control
remains separate and has no invented baseline-comparison result.

## Input, timing and resource limits

This is one generated eight-second coding/motion clip, cycled with continuously
increasing capture timestamps. Cycles within a session are not independent
sessions or a varied corpus. The pilot and repeat batch use the same clean
source, executable hash, immutable NV12/RGB/manifest, geometry/color interpretation
and forced two-second keyframe cadence. Candidate order is recorded and alternates
in the repeat batch. No controlled thermal, battery, frequency or background-load
experiment is claimed. Other bounded native API checks occur between measured
sessions under the same shared lock; they do not overlap a measured encoder run.

Callback latency excludes input read/fill and file output; it includes native
encoding and worker wake. Whole-probe `time -lp` CPU/RSS includes file I/O,
initialization and warmup. It excludes shared OS compression helpers, GPU/power,
viewer and app baseline. RSS is not physical footprint. The five-run duration/
count requirement is met for this component experiment, while 1080p60 application
latency, decoder identity, full resources and physical endpoint qualification
remain unverified. Framework priority is unspecified at this source; it was
neither set nor queried.

Nonoverlapping one-second p95/peak payload rates remain 2.489184/2.489184 Mbps
for H.264 and 1.623088/1.623088 for HEVC. First/observed/largest frame bursts
remain in each session record. Requested bitrate is not a ceiling. These are
Annex B bytes, without packetization, audio, loss/retransmissions, actual ingress/
egress, TURN or provider accounting. There is no new route, transport or hosting
capacity measurement.

## Reproducible numeric evidence

`summary.json` retains each independent session's profile, structural/decoded
checks, latency/resource/quality summaries and explicit comparison scope.
`validation.json` retains commands, exact sources, executable/tool/fixture hashes,
metadata, deadlines and **65 independently absent comparison command groups**
across the pilot and repeat batch. This does not establish teardown of shared
OS helper services. No new unit tests are credited by aggregating sessions.

The existing pilot's uncompressed CSV/SSIM rows remain referenced in
`../2026-11-05-sustained-codecs/`. Forty-four deterministic gzip files retain the
four new pairs' raw rows, using about 2.2 MB instead of duplicating the expanded
text. `index.json` records compressed and uncompressed hashes, byte counts and
row counts. Across all five sessions per codec there are **90,000 encode/payload
rows, 270,000 region SSIM rows and 135,000 foreground rows**, including the
uncompressed control. CSVs include a header; SSIM files do not. Decompress with
standard gzip tools or Python's `gzip` module. Capture frame is 900+observed
index; source frame is 30+index modulo 240. Foreground channel sums divided by
`9792*3` reproduce frame mean error; percentiles use nearest rank.

`exporter.py` is a portable copy of the executed aggregation helper with the
frozen checkout path supplied through `VOXL_FROZEN_CHECKOUT`. Set that variable,
`VOXL_SOURCE_WORKTREE`, `VOXL_PILOT_RESULTS` and `VOXL_REPEATABILITY_RESULTS` to
the owned paths in a worktree before this result directory exists. Regeneration
records the actual exporter's hash; numeric rows do not depend on its path.
`export-correction.json` retains the pre-publication aggregation correction:
uncomputed floor comparisons are omitted instead of represented as zero/passed.
No measured value or native session was changed or rerun by that correction.

No screen, decoded image, font, bitstream, credentials or private application
source is published. AV1/providers, new-profile measured admission, the BT.709
canvas observer gap, physical phone/LAN/NAT/input, complete app/helper/GPU/power,
wire/relay billing and deployment remain open. All optimistic targets and the
full implementation goal remain active.
