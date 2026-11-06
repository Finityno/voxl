# Pinned sans-I/O WebRTC core

Library sources are imported from crates.io `rtc` 0.21.1. Packaged VCS metadata
records commit `2a732df55ca4bcf26b80914fa0f570cfaf74cfcb` from
https://github.com/webrtc-rs/rtc. `IMPORT.json` records the downloaded archive
checksum, every retained file's original/import hash and numeric comment-reference
sanitization and import-only documentation whitespace cleanup. The archive
checksum is checked against the existing media lockfile
before import. MIT/Apache licenses, original manifest and VCS metadata are retained
verbatim. The normalized manifest omits non-vendored example/integration targets
and development-only dependencies; production dependency tables/features remain.

The configuration patch makes `RTCPeerConnection::set_configuration` replace
the ICE-server list even when it is empty, after validating any supplied servers.
An empty list removes old STUN/TURN discovery configuration; it is not a request
to keep old endpoints or credentials. Public configuration and the async driver's
next gathering phase therefore agree. Other validation, transport/codec/crypto
logic and the existing acknowledged live-restart barrier are unchanged.

An additional explicit `RtpPaddingOnly` interceptor attribute carries a local
padding-only packet through pacing into SRTP encryption. Its zero payload is
included in the final RFC 3550 padding count (maximum 255 octets); ordinary RTP
serialization is unchanged. Congestion/pacing size accounting therefore retains
the same serialized length, while the receiver obtains no codec payload. This
supports bounded on-demand quiet-resume feedback probes, not periodic video or
fake decoder progress. Parent tests cover wire validation, actual encrypted
delivery/feedback, retransmission tagging and generation/close boundaries.

The sibling vendored async SDK and media wrapper both use this exact path/core
version, with defaults disabled and explicit AWS-LC features. This avoids distinct
RTC type universes. There is no global Cargo patch or user cache modification.
The parent regression records actual stale Allocate requests before the patch,
then requires an empty public list and no datagram at the old endpoint while
direct UDP/DTLS/SRTP video continues on the same peer across removal/re-addition.
Synthetic payloads and loopback timing do not qualify physical handover, native
decoding, allocation release, performance or resource savings.

Updates must verify the new archive/checksum and licenses, compare production
dependency/features, preserve the empty-list patch and rerun the parent's scoped
configuration, media and forced-relay checks. No upstream submission is made.
