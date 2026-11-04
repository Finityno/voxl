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
The new forced-relay trickle UDP/TCP/TLS fixtures are pending validation.
