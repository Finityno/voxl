# Optional packet pacing headroom

Exact executed component source: `e3568aa21395996ba5403479f00b8eb06f12160c`.

The new opt-in responsive gain paces packets at2.5x the unchanged media target.
The1x mode remains default. GCC limits, forwarded media feedback and encoder85%
headroom stay separate from packet pacing. Rate changes retain/clamp credit;
reset restores the selected prior. Queue256/history1024, packet/frame500ms,
sixteen-packet producer waits and synchronous close/restart fencing stay fixed.

A new advisory frame budget exposes exact media/pacing bits per second, payload
bytes and estimate age. It uses350ms pacing time and a ten-percent reserve.
Unknown feedback uses the configured prior; held feedback is not measured
capacity. Close/failed transport returns no budget. Stale encoder target
observation still returns none independently.

Twenty-one focused adaptive tests, the existing H.264 loopback and selected live
ICE restart test pass, with strict library/tests/examples Clippy. Coverage
includes1001032-byte RTP packets on virtual time and one103281-byte synthetic
H.264 access unit over UDP/DTLS/SRTP with exact reassembly. At1.5Mbps media, the
responsive schedule drains the virtual burst below300ms while the1x schedule
hits the unchanged500ms bound. These are synthetic bytes, not decoded quality.

The initial exact-source run at`e9c225c55a9ee96d645516237d1ee651f65ae145`
passed20 tests and failed one virtual-clock assertion. The SDK rounds a
sub-nanosecond token deficit to zero; a simulated wake repeatedly reused that
same instant. Advancing virtual time by at least1ns models a real wake. The
retained failure is not relabelled as a passing run.

The choice follows [libwebrtc's separate pacing headroom](https://webrtc.googlesource.com/src/+/refs/heads/main/modules/congestion_controller/goog_cc/goog_cc_network_control.cc).
Higher instantaneous packet departure can increase slow-path queueing; no
capacity/loss, native decode, quality, CPU/memory/power, bandwidth saving or
default admission follows from these component tests. Existing constructors
remain unchanged. Thirteen owned groups are independently absent. Original
logs remain in the local owner-only archive; exported logs/receipts normalize
local home paths and retain original log hashes. Run`python3 -B verify.py` for
offline integrity/count checks without native replay.
