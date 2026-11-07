# Native source color signaling

Final runtime/test source `af9cad081eee1581bbd498f383e25c2ebbf1f85e`, custom
fork only. Native compression now receives the actual first input buffer's
primaries, transfer function and YCbCr matrix. It leaves absent tags unspecified,
retains only three immutable string copies, and performs no pixel conversion or
extra frame retention. Later tag or NV12 range changes close the encoder, as do
malformed attachments or rejected native properties. A new encoder generation is
required. No capture color space is hardcoded, no codec/profile/low-latency default
changes, no dependency/features or minimum macOS deployment target is raised.

The unsafe input contract now explicitly includes immutable attachments as well
as pixels. The borrowed CoreVideo getter remains available before macOS 12;
reading takes place while the immutable source buffer is retained. Stored
strings have independent immutable copies. Property application is once per
encoder; later frames compare range and three tags without allocating strings.
This source inspection establishes no measured CPU or memory saving.

Apple documents [source buffer color attachments](https://developer.apple.com/documentation/avfoundation/tagging-media-with-video-color-information)
and [compression color properties](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_colorprimaries).
AVAssetWriter's documented propagation does not prove direct VTCompressionSession
behavior. Likewise output format-description tags alone do not prove bitstream
signaling, so the checks inspect actual Annex B streams through an independent
decoder without overriding encoded input color metadata.

## Regression and final results

The generated probe adds sixteen color/gray bars, video/full NV12 range and
none/BT.709/sRGB transfer requests at source
`0ad2646386e3a1ad7f6996632ad97b5bce7ce14d`. Existing defaults retain the prior
monochrome video-range input without explicit tags. Each of four paths crosses
two ranges and three tag choices: hardware H.264 compatible, hardware H.264
Constrained High, hardware HEVC Main and software H.264 compatible. Twenty
frames per case have exact capture-clock progression and actual IDRs at 0/10,
with codec parameter sets before each keyframe. Independent decoding must
produce all twenty frames at 1280x720 and the correct codec.

| Observation | Before fix (0ad2646) | After fix (af9cad0) |
| --- | ---: | ---: |
| Cases encoded and independently decoded | 24 | 24 |
| Explicitly tagged cases | 16 | 16 |
| Explicit primaries/transfer/matrix present and correct | 0 | 16 |
| Qualified numeric center-region comparisons | 0 | 16 |

The initial untagged results lack primaries, transfer and matrix, including when
input buffers explicitly request them. Their NV12 range is separately observable
for some paths. No color-fidelity scores are assigned to that underspecified
output. Final explicitly tagged output identifies actual 709 primaries/matrix,
709 or IEC 61966-2-1 transfer, and requested video/full range. Untagged inputs
remain unqualified for declared-transfer fidelity.

The rational BT.709 quantizer independently generates reference NV12. FFmpeg
normalizes reference and actual decoded first frames to 709 primaries, sRGB
transfer, full range and RGB24; encoded input tags are never replaced by test
assumptions. For each of sixteen bars, the comparison uses only its center
16x16 pixels. Hardware H.264 mean absolute errors are 0.4375–0.95833 sRGB code
values across cases, with maximum 6. Hardware HEVC and software H.264 have zero
error in those flat regions for this short static workload. These are numeric
observations, not a full-image or overall quality pass: edges, text, temporal
quality, wide gamut/HDR, browser conversion and matched-quality bitrate remain
unmeasured. Neither decoder hardware identity nor any performance budget is
qualified.

A separate FFmpeg RGB-to-NV12 BT.709 conversion crosschecks the rational
reference at the same flat center regions: video range matches exactly; full
range differs by at most one code value. That rounding/filter observation is
recorded separately and does not turn the reference into a text/edge or gamut
qualification. Its successful retry follows the retained lock-queue expiration.

Static comparison of the completed before/after streams finds every compressed
picture NAL identical across all 24 cases. Actual stream length changes are
0–10 bytes per twenty-frame fixture, in headers/metadata. This supports the
narrow signaling correction; it is not a total-wire or matched-quality bandwidth
saving and does not measure runtime resource overhead.

Final release example build, strict default-feature library/test/example Clippy
and selected library test build exit zero. Four native tests pass: stable tags
across independent buffers, transfer change, missing/added tags, NV12 range
change and non-string attachment rejection (the missing/added variants share
one test). A rejected change permanently refuses subsequent encode calls.
Source inputs are never mutated after retaining their immutable wrappers.

## Retained failures and compatibility control

The first control required byte-identical output to the earlier monochrome
run. It fails before any color case executes: two bytes differ in one type-5
unregistered-user-data SEI message. Its UUID/type/length match, while SPS/PPS and
all picture NAL bytes are identical. The SEI's private payload meaning is not
identified. A separate supervisor preserves that failure, then verifies all 210
independently decoded NV12 frame hashes and complete frame/hash clock rows are
identical before continuing. The final source also passes this decoded-frame /
non-SEI compatibility control. Private encoder metadata is not treated as a
pixel regression or falsely claimed byte-identical.

Initial fix source `3bb188652b9c8729baf86a3ec9a408c098d748a0` builds and lints,
but all four new tests fail before encoding because their 64x64 fixture violates
the existing minimum geometry. Zero color cases execute in that attempt. The
correction uses supported 320x180 frames and changes only the test file;
production native source blobs are identical. The failed attempt is retained
separately from final passing tests. A later independent reference crosscheck's
first queue attempt expires at the shared build lock (75); zero reference
commands execute. That missing execution is not a pass or encoder failure.

## Evidence and limits

The packs retain exact source, helper versions/hashes, all bounded commands,
exits/counts, actual profile and acceleration observations, requested versus
encoded color tags, raw numeric ROI results and decoded-frame hash controls.
Every included text hash and command-log hash is independently checked. Binary
streams, RGB/NV12 references and native executables remain local with lengths
and hashes; no user screen, screenshot, input, private app source, credentials
or recordings are published. Large successful Cargo artifact JSON remains
local with its hash and selected compiler-artifact metadata.

All completed native groups are reaped. Runs use a clean detached source, its
own target, the shared build lock, nice 19, two jobs and process-group watchdogs.
All 36 protected manifest/lock/vendor/config inputs remain unchanged. The root
server is not built. This is native Mac component coverage, not Windows/Linux
capture, a physical phone, physical LAN/NAT/VPS, full app lifecycle or measured
codec admission. No optimistic performance target is changed or marked achieved.
