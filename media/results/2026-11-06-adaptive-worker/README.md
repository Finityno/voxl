# Native worker feedback observation

Exact component source: `ab12301fad765d46e21fd0874ac2bd603cd75e78`.
Apple M5 Pro, arm64 macOS 27.2; toolchain and platform details are in
`platform.json`. This followup adds synchronous worker observation through the
existing close handle so pacing a frame does not delay the next encode target.
The read rejects unknown, older-than-two-second, retired and closed feedback.

At this exact source, fourteen selected adaptive tests, both legacy H.264/HEVC
UDP loopbacks and strict library/tests/examples Clippy pass. Native component
coverage remains generated 320x180 NV12 with hardware H.264/HEVC and CPU H.264;
the transport access units are synthetic NAL bytes, not independently decoded
native output. A feedback target can retain GCC's configured prior after its
first report; it is not a measured capacity or wire-bitrate guarantee.

The first attempt timed out on a duplicate build lock before compilation
(exit 75). Removing the duplicate outer lock allowed the unchanged source to
pass. Both attempts, exact commands, deadlines and normalized logs are retained;
twelve owned groups were independently absent. Original receipts and logs remain
in the local owner-only archive. Exported receipts preserve the generic runner's
original limits text and clarify its actual selected command scope.

Run `python3 -B verify.py` for offline integrity/count verification. This does
not rerun native code. Complete application coupling, real changing-capacity,
loss/recovery, decoded quality, CPU/RSS/power/bandwidth and physical endpoint
admission remain separate work. Existing constructors and defaults are unchanged.
