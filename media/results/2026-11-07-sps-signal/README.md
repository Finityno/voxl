# Literal H.264 SPS signal qualification

`h264::sps_signal` reads one bounded raw SPS NAL prefix through explicit VUI
video signal and chroma-location fields. It preserves absent fields and raw
unspecified/reserved colour codes, and does not infer defaults or validate the
remaining VUI/HRD/trailing syntax. The local quality decoder stores the parsed
scalar descriptor once for its fixed parameter chain and exposes it only while
active. This changes no production encoder, colour tag, transport or default.

At exact source `bc480baf27b0a518ae74526c8381a5a04131bfce`, six parser tests and
one native software H.264 encode/decode test pass. The latter submits four
320 × 180 synthetic, full-range NV12 images tagged with BT.709 primaries,
sRGB transfer and BT.709 matrix. Actual emitted SPS fields assert video format
5, full range true and literal colour codes [1, 13, 1]. The descriptor is absent
before decoding and after chain retirement. This uses native VideoToolbox,
without screen capture, OS input, a network peer or persisted pixels.

Parser tests cover missing VUI/video/colour fields, raw unspecified codes,
high-profile scaling syntax, picture-order branches, cropping, extended aspect,
chroma bounds, truncation, emulation prevention and the 16-KiB cap. Strict
library/test Clippy, the selected test build and both test commands exit zero;
[the ledger](evidence.json) records exact commands, deadlines and log hashes.
Every command runs at low process priority with one Cargo job, a shared build
lock and external process-group containment. All command groups retire without
cleanup escalation. The initial fixed-chunk lint failure is preserved, then
corrected at the tested source; it is not counted as passing coverage.

The scalar result distinguishes encoded metadata from decoded pixel-buffer or
receiver metadata. It does not prove rendered colour, readable text, peer-frame
identity, hardware behaviour, sustained CPU/RAM/power, bitrate savings or codec
admission. Full stream and endpoint qualification remain required.
