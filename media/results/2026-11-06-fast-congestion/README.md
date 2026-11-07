# Faster congestion response, with retained failed candidates

Exact final component source: `b21fb65eb97fe34ede1ce46f218dfb34a5824d6d`.
This extends the [controlled-network baseline](../2026-11-06-network-control/README.md).
The opt-in `SendControl::with_delay_control(DelayControl::Trendline)` is separate
from pacing gain. `Standard` remains the controller default. Application and
physical performance admission remain separate.

The baseline trace pinpoints slow filtering: raw inter-group delay changes are
25–40 ms while the filtered value stays below the overuse threshold for about
1.8 seconds. The first candidate raises Kalman process noise; it cuts response
to 397 ms but reaches 941 ms residence and fails the new 750 ms gate. A twenty-group
trend candidate has one passing runtime trial, then fails its lint check; the
receiver-interval revision reaches 774 ms and fails the unchanged queue gate.
All attempts, including the first SDK statistics-constructor build failure,
remain exact-source failures rather than being relabelled as passes.

The final candidate uses a fixed ten-group regression of smoothed accumulated
delay. It is an independent composition inspired by
[WebRTC's trend signal](https://webrtc.googlesource.com/src/+/refs/heads/main/modules/congestion_controller/goog_cc/trendline_estimator.cc),
not a port of the complete modern Google controller. Grouping, overuse
hysteresis, AIMD, loss and configured min/max ranges use the existing SDK stages.
Delivery rate uses relative intervals on the receiver clock, so a feedback batch
cannot compress its packets into one rate instant. Old receiver samples are
ignored conservatively. No sender/receiver absolute-clock subtraction or
one-way latency estimate is made.

The regression scratch array is fixed. Invalid/long measurement gaps retire
its valid rows and overuse state. ICE generation retirement resets delay/rate
history. One selected controller allocation is owned per controlled peer and
reused on reset; no per-packet controller allocation, thread or recurring timer
is added. The existing SDK received-rate window still owns its sample storage.
Pacing gain, producer waits, queue/history limits, loss control, media ceilings,
85% encoder headroom, close/epoch authority and 500 ms send deadline stay fixed.

At the same final source, 34 selected adaptive tests and strict
library/tests/examples Clippy pass. Two additional independent fast-transient
runs use the identical binary/environment. Tests cover limits, stale/retired
feedback, idle gaps, history reset, jitter/spike behavior, rate-clock batching,
large keyframes, actual one-packet NACK repair and clock-preserving access-unit
reconstruction. The synthetic bytes are not independently decoded video.

| Controlled 3 Mbps → 500 kbps → 3 Mbps case | First reduction below 1 Mbps | Maximum UDP queue residence |
| --- | --- | --- |
| Standard controller, same final source | 1817373 us | 3115262 us |
| Trend candidate, full serial adaptive run | 298691 us | 654379 us |
| Independent trend repeat one | 300205 us | 658504 us |
| Independent trend repeat two | 297670 us | 658655 us |

All three final trend cases meet the unchanged experimental 500/750 ms transient
gate, with no modeled queue overflow, and recover their media target after
capacity returns. This is a repeatable improvement in this emulated path,
not low-latency/default admission: even 659 ms queueing is substantial. The
optimistic product targets remain unchanged. Native encoding/decoding, real
input-to-photon, physical networks, quality and full resource/power accounting
remain required. UDP payload serialization excludes IP/link overhead and cannot
establish provider-billed bandwidth or codec savings.

Thirty-nine recorded command groups independently ended. Original helper
scripts, failed/pass receipts and raw logs remain in the owner-only archive;
exported receipts normalize local home paths and retain original hashes. The
original generic runner's benchmark limits wording is retained and its narrower
selected-command scope is stated separately. Run `python3 -B verify.py` for
offline source/count/hash/numeric verification without native replay.
