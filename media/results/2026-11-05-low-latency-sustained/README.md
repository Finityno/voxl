# Sustained low-latency HEVC comparison

Five fresh sessions per codec at source `f098e462a62833b73d6a6103e4f50ff810fb7ffb`,
on one M5 Pro running macOS 27.2: 1080p30, 30 seconds warmup and 300 observed
seconds per session. Each session cycles the same generated eight-second coding
workload. All ten streams independently decode 9,900 frames, including warmup.
All 55 owned command groups terminated and were independently absent.

| Measurement | H.264 compatible, requested 3,000 kbps | HEVC low-latency compatible, requested 12,000 kbps |
| --- | ---: | ---: |
| Observed Annex B payload, Mbps | 1.036773 | 0.864336 |
| Individual-session encode p95, median (range), ms | 12.266 (12.065–12.408) | 5.581 (5.535–5.599) |
| Probe CPU, median, percent of one core | 3.70 | 3.39 |
| Peak probe RSS, median, MiB | 40.59 | 47.64 |
| Peak probe physical footprint, median, MiB | 13.70 | 19.55 |
| One-second payload p95 and peak, Mbps | 2.489184 | 2.106776 |
| First startup frame, bytes | 46,229 | 73,369 |
| Text-region mean YUV SSIM | 0.999549 | 0.999642 |
| Scrolling-phase text-region mean YUV SSIM | 0.999840 | 0.999687 |
| Thin foreground mean absolute RGB code error | 13.64889 | 13.15306 |

HEVC uses **16.632% fewer observed payload bytes in each pair**. It is faster
at the measured encoder callback boundary, uses more peak process memory, and
has a larger first frame. Requested bitrate is not a ceiling or a forecast for
other content. This comparison changes both codec and tuning/rate request; it
does not isolate a codec-only saving. Default priority is unchanged and its
property is neither requested nor queried.

HEVC foreground aggregate and separate R/G/B error sums are no worse than the
paired H.264 baseline in all 45,000 observed paired frames. This is a metric on
reference-derived ink and halo, not a per-pixel, glyph or human-legibility
guarantee. Scrolling text SSIM is lower with HEVC. The uncompressed 4:2:0 floor
has no baseline comparison; no comparison is invented for it.

`summary.json` retains individual sessions and distributions. `validation.json`
retains exact commands, deadlines, source checks, decoder metadata, fixture and
compiler hashes. `index.json` hashes 65 gzip records: 90,000 encode/payload rows,
270,000 region SSIM rows, 135,000 foreground rows including the floor, and ten
resource-log projections. Resource logs omit only the long latency-sample line,
whose exact observed values are retained in CSV; they preserve all `time -lp`
fields. Original and normalized/projection hashes are distinct. Physical
footprint was extracted afterward from those existing logs, with no native rerun.
Missing or zero footprint observations remain unknown.

Run `python3 -B verify.py` here to independently recompute retained row counts,
clocks, payload totals/windows, latency percentiles, channel/phase SSIM,
foreground comparisons, resource scalars and distributions. The normalized
exporter snapshot documents the earlier export; it was not itself executed.
Owned absolute paths are represented by `$RESULTS`, `$CHECKOUT`, `$FIXTURE`,
`$EXPORT_CHECKOUT` and `$USER`. No image, font, bitstream, credential or private
application source is published.

CPU and memory cover the probe process, including initialization, file I/O and
warmup. RSS and physical footprint are different counters. These are not
baseline-subtracted application measurements and exclude shared OS helpers,
GPU, viewer and power. The repeated clip provides no independent content/device
population or confidence interval. Software decode success does not establish
native receiver hardware decoding. Capture, control, actual network/relay
traffic, packet loss, physical devices, application resource budgets and
1080p60 qualification remain open. Targets, codec defaults and production
admission are unchanged.
