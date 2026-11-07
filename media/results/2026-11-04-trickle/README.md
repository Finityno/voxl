# Incremental candidate exchange

At exact `10fd258ff4fcceaca235fd30a89dbad6e9f07db7`, the locked WebRTC
library prebuild, three `trickle_` tests and strict library/examples Clippy
passed in the existing isolated Linux cloud checkout. Raw commands, exit codes,
counts and source verification are in `10fd258/artifact-pack.json`.

The direct fixture holds a real UDP STUN response until synthetic H.264 media
arrives, strips candidates from both wire SDPs, exchanges candidates separately
in both directions, and applies viewer candidates queued before the answer.
After media arrives it releases the STUN response, observes the server-reflexive
candidate and native completion. The other checks cover cumulative queue limits,
malformed input, cancellation and close waking a pending signal consumer.

The earlier exact `1ddd84c94bb215af7216d6ba3c7b9bced35a7925` run is retained
in `1ddd84c/artifact-pack.json`: one trickle test passed and two failed because
the tests assumed asynchronous host/receiver candidate callbacks were already
complete. The later source explicitly synchronizes those events. The initial
run's build, strict Clippy and unchanged full-SDP H.264/HEVC loopbacks passed.
No failed run is counted as passing. No retry loops were used.

This is synthetic transport and lifecycle evidence, not native capture,
decoding, physical LAN/NAT routing, startup latency or CPU/RAM/power evidence.
At exact `367b015b1b198caeaab4e35ee48e14780f8f18df`, the locked
prebuild, three forced-relay trickle fixtures (UDP, TCP and verified TLS), one
legacy forced-UDP regression and strict library/examples Clippy all passed.
Both wire SDPs omit candidates: the native callbacks must exchange candidates
incrementally in both directions. Every peer has two IPv4 bindings and one
unique relay endpoint; selected route/transport and both synthetic payloads
must pass. Coturn independently counted eight allocations, eight permissions
and fourteen channel binds across the four scenarios. All owned child groups
were gone, ports rebindable and private test state deleted after termination.
Raw results are in `367b015/artifact-pack.json`.

The initial relay-fixture source `f32bb33` failed its test prebuild because the
fixture treated the SDK's candidate event as optional. Zero relay tests ran and
coturn was not started; independent production Clippy passed. The correction
only changes the test callback type. The failed evidence remains in `f32bb33/`.

A native Mac codec queue at exact `10fd258` expired after 900.02 seconds waiting
for the shared build lock, exit 75, without compiling or running any probe.
`native-10fd258/queue-results.json` records the bounded queue. No active native
job or new encoder/CPU/quality result is claimed.
