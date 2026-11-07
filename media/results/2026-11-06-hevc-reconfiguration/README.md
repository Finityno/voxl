# HEVC replacement profile guard

Exact executed source: `cd1bd368d67a40d208f78e30dd25f05a41aead0e`.

`HevcProfile::permits_reconfiguration` accepts an in-band Main profile only when
tier, compatibility flags and constraint bytes match the advertised profile and
the actual level does not increase. The type still comes from the bounded SPS
parser; callers cannot construct a different unsupported profile through public
fields. No SDP, codec choice, transport, authentication or controller default changes.

Two selected profile tests pass: bounded SPS parsing/actual level and replacement
constraints. The replacement test covers identity, lower/higher levels, a changed
tier, changed compatibility flags and changed constraints. Test compilation and
strict library/tests/examples Clippy pass with WebRTC enabled, locked dependencies,
two jobs, nice 19, the shared build lock and external process-group deadlines.
All six recorded groups were independently absent afterward.

This is metadata/API evidence only. It does not implement native replacement,
prove decoder continuity, qualify changed dimensions or establish codec quality,
CPU/RAM, GPU use or bandwidth savings. Native host admission and decoding need
separate actual-frame validation. The offline verifier checks tracked artifact
hashes, command receipts and result counts; it launches no encoder or app.
