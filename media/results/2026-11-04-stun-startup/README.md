# STUN-first discovery can yield to authenticated relay startup

Exact clean source: `c7eb4dc61e29fac1a37de52bb25136fc497756c2`.
Linux cloud, Cargo 1.99.0, owned checkout/target, two jobs and explicit deadlines.
Scoped library test compilation and strict library/examples Clippy exit 0.
Ten exact tests pass, zero failed/ignored; no requested scope is missing.
The follow-up at `ae1a9896d9c3a9d206936314de13f22ca85eeedf` adds only a
cancellation test: its scoped build, one test and strict Clippy also pass.
All three production files are byte-identical to c7eb4dc, verified by hashes.
This is ten tests at c7 plus one at ae1, not eleven executions at one source.

The startup-only fallback API now accepts an initially STUN-only or server-free
Automatic trickle peer. It still requires its initial answer and an unconnected
view, preserves format/bindings/policy/DTLS identity and performs only one
upgrade. The driver cancels old STUN discovery before the wrapper advances the
candidate/answer generation. This removes the old requirement to wait for that
discovery to complete. It performs no HTTP or authorization of its own.

The fork-only SDK barrier uses one bounded interrupt channel during gathering
and an acknowledged command in the ordinary driver FIFO. Interrupting the
resolver future lets the driver reach that command even if DNS never completes.
The driver discards old STUN clients and queued discovery writes/events, then
dispatches already queued core callbacks before acknowledging. Only then may
the wrapper reset its queues and create the new offer. A late old response
cannot become a new-generation candidate. Cancellation of the wrapper closes
the peer/adapters through its existing authority-bound handle. SDK callers
must serialize negotiation and close on cancellation. This barrier is limited
to STUN-only configuration; it does not tear down established TURN allocations.

Gathering flags and the deferred TURN deadline are consolidated into the
driver's discovery enum. The existing connected-before-allocation behavior is
retained. There is no new timer, OS thread or steady media-loop listener; the
interrupt receiver is selected only while gathering. Additional channel/state
resource costs and end-user startup effects are not measured here.

| Exact scope | Result |
| --- | --- |
| Pending UDP STUN, stop retries, inject late old response, restart and actual direct payload | 1 passed |
| Resolver future deliberately held pending, cancellation observed before restart, actual direct payload | 1 passed |
| Existing server-free late configuration, fresh ICE/stable DTLS and stale signaling fences | 1 passed |
| Close while restart answer is outstanding; no detached retries after teardown | 1 passed |
| Existing H.264 payload/capture-clock/keyframe feedback | 1 passed |
| Existing HEVC payload/both IDR types/capture-clock | 1 passed |
| Initially configured one-sided actual TURN UDP baseline | 1 passed |
| Actual TURN UDP after unanswered STUN | 1 passed |
| Actual TURN TCP after unanswered STUN | 1 passed |
| Actual verified-TLS TURN after unanswered STUN | 1 passed |
| Drop startup while the STUN barrier is still outstanding (ae1a989) | 1 passed |

The additional cancellation case deliberately polls startup once while its
resolver is still held, requires the barrier to remain outstanding, then drops
the future. The authority-bound handle must end the view synchronously, old
signaling must fail and peer teardown must drop the pending resolver. It then
requires the ended update; this is distinct from closing after a restart offer
has already returned.

The DNS fixture intercepts only its reserved test hostname and never resolves
it externally. The UDP fixture binds a real loopback socket, receives an actual
binding request without answering, checks that old gathering is still pending,
requires the restart within its one-second functional deadline and observes no
old retry for another 300 ms after draining earlier queued datagrams. It then
injects the old transaction's response; that mapped address must never appear
among the new generation's candidates. These deadlines are test assertions,
not a latency benchmark or proof that a performance target is qualified.

Four actual coturn cases run in an owned rootless user/network namespace with
fresh REST credentials, CA/certificate and private SQLite state. Each delivers
the expected synthetic payload on its nominated native relay transport with
exactly one server allocation, two permissions, four channel bindings and 36
actual host UDP drops. The receiver has no TURN allocation. No external packet,
host firewall rule or Docker container is used. Process-group watchdogs remain;
cleanup proves all groups gone, private state/xtables lock removed and fixture
ports rebindable. Coturn termination is recorded as signal 15, not exit zero.

`artifact-pack.json` contains 22 safe files: exact commands, source/cleanliness
checks, test counts, logs, fixture scripts and cleanup/counter summaries. Its
34,052 bytes have SHA-256
`0b49abbf1962073060eaf87554cc8dac144d2fb66c851ebe35167b5e158bf840`.
Transfer lengths/hashes and all internal file hashes were independently checked
after export. Private configuration, credentials, keys/HMAC values, raw SDP/ICE,
unsanitized coturn logs, binaries and pixels are excluded.
`cancellation-artifact-pack.json` independently retains the additional source
comparison, build, one cancellation test and Clippy. Its 5,096 bytes have
SHA-256 `7002efbc8c291226bc0659ffad93cc4ccff9ecaca0b481ab920fce0d3f37d7d3`;
its transfer and internal hashes were checked separately.

This is synthetic transport evidence, not capture, native decoding, input,
physical phone/LAN/NAT/VPS, Windows or endpoint resource qualification. The
earlier first-SRTP-packet failure/fix remains a separate dated result pack.
Fincode still pins 5229dac and uses its two-second credential lookup and eager
viewer allocation. Backend STUN discovery metadata, authenticated view/generation
coordination in the host/phone/desktop viewer, lazy credential issuance and
general renewal/handover remain required. No cost, bandwidth, CPU/RAM, quality,
power or whole-application startup improvement is claimed by these tests.
