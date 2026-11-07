# Explicit STUN discovery

Custom fork source: `2ab66f7739e71f9d64975b95e7787029eaab8f23`.
Linux cloud thread: `01a1a3f0-1541-7477-8ab6-beaf03e5b067`.
Turn: `01a1a5aa-2e99-713d-a440-845e85a3d347`.

The cloud reported clean source before and after. Locked library test build,
the one selected STUN discovery test and strict library/examples Clippy all
exited zero. The test performed a real loopback UDP STUN Binding exchange with
a synthetic mapped address, asserted its server-reflexive candidate in SDP,
retained host candidates and Automatic routing, and had no relay candidates.

Selected sanitized command/result logs exported by the cloud agent are kept
here. Full step logs remain under `/workspace/voxl-stun-validation-2ab66f7/`.
The STUN fixture is local and needs no service credential or public endpoint.
This is candidate-gathering evidence, not actual NAT traversal, nominated
Internet-direct media, decoding, native capture or a performance measurement.
