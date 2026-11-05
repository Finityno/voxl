# Low-latency bitrate candidates: availability and short pilots

Measured source `f098e462a62833b73d6a6103e4f50ff810fb7ffb`, Apple M5 Pro,
macOS27.2. The application still consumes its earlier media pin; no application
default, target or codec/profile admission changes. All priority settings are
Default (property neither set nor queried).

The earlier HEVC low-latency 1500-kbps case drops output after one completed
frame. A bounded availability follow-up completes and independently decodes all
270 frames at requested 3000, 6000 and12000 kbps. Ordinary HEVC1500 also completes.
That narrows the observed failure to this configuration; it does not prove the
underlying rate-control/driver cause. Availability alone provides no quality or
performance admission. The failed earlier case remains in the failure pack.

Two separate matched pilots then use one second warmup and eight seconds observed
at1920×1080/30fps, same generated clip and two-second forced keyframe cadence.
Every case independently decodes270 frames. The table compares each candidate
only with its own pilot's ordinary H.264 baseline. Foreground rows are exact
aggregate integer error sums on9792 reference-derived ink/halo pixels.

| Pilot/candidate | Requested kbps | Observed Annex B Mbps | Callback p95 ms | Worse aggregate frames vs H.264 | Worse R/G/B frames |
| --- | ---: | ---: | ---: | ---: | --- |
| First: ordinary H.264 |3000|1.248875|12.097625|0|0/0/0|
| First: low-latency H.264 |6000|0.946516|5.259583|161|110/221/161|
| First: ordinary HEVC |1500|0.772445|14.806750|0|0/0/0|
| First: low-latency HEVC |3000|0.958472|5.577667|31|29/39/29|
| Second: ordinary H.264 |3000|1.248875|11.093666|0|0/0/0|
| Second: low-latency HEVC |6000|0.978799|5.552500|11|9/13/11|
| Second: low-latency HEVC |12000|0.955590|5.549708|0|0/0/0|

The12000-kbps HEVC request is a promising next candidate on this clip: all240
aggregate foreground frames and separate channels are no worse than paired
H.264, with23.48% fewer observed encoded bytes and lower callback p95. This is
not per-pixel/glyph/human legibility nonregression or full profile admission.
The other low-latency candidates still regress this foreground requirement.
Different bitrate/profile settings are candidate tradeoffs, not isolated causal
attribution. In particular, doubling a requested bitrate is not a measured
doubling of output: sizes are not monotonic here. Requested bitrate is neither
a ceiling nor a prediction for other content; a12000-kbps setting can permit
larger bursts and must not be advertised as a1-Mbps cap or billed-cost guarantee.

One eight-second generated coding/motion clip is cycled; these short sessions
do not meet sustained/five-session qualification. No controlled thermal/power
experiment, independent corpus/device result, application1080p60 or physical
phone/LAN/NAT/input result is claimed. Region YUV SSIM, original-RGB aggregate
errors and the uncompressed4:2:0 floor remain separate. Probe CPU/RSS include
I/O/init/warmup and exclude app/viewer baseline, OS helpers, GPU/power and physical
footprint. Encoded bytes exclude packetization/audio/retransmissions/wire/TURN/
provider accounting. No new unit tests execute in these measurement runs.

Receipts retain exact source, tool/executable/fixture hashes, commands, bounded
groups and resource scope. All58 owned command groups are independently absent;
this does not establish shared OS-helper teardown. Matched input hashes are
checked before/after; the earlier availability-only helper uses the supplied
fixture path without a separate per-case input-hash receipt. `index.json`
hashes normalized-path records and deterministic gzip numeric/log files. Payload
sums, p95s, region SSIM means and foreground comparisons are recomputed during
export. Raw totals:1680 encode/payload rows,5040 region SSIM rows and2160
foreground rows including uncompressed controls. No bitstream, font, screen,
decoded image, binary, credential or private application source is published.
