# Nominate proven desktop connectivity without another grace period

Final runtime: `2ae3179905ebbf1a343b66fb3db596dadc79d6d6`.
The media and vendored SDK manifests, external versions and features are
unchanged. No dependency or periodic task is added.

ICE callback state is independent of DTLS readiness. `ice_connected()` is
synchronous and reads bounded local state; it permits optional startup fallback
to stop without enabling media/input. Terminal connection state uses an enum
and stable atomic compare/exchange so late callbacks cannot revive closure.
The startup-restart barrier checks ICE both before and after quiescing discovery.

The SDK's default srflx/prflx nomination waits (500/1000 ms) can exceed an
embedder's direct grace and cause unnecessary relay work on a working route.
These proven direct candidates now nominate immediately. Relay nomination also
becomes immediate when TURN is installed only after an embedder-owned direct
grace or in forced-relay tests. Legacy eager TURN keeps its relay preference.

Six exact Linux cases pass, zero failed/ignored: ICE-before-DTLS and terminal
callbacks; H.264 UDP/DTLS/SRTP timestamps/keyframe feedback; HEVC payloads; real
STUN; cancelled signaling/close; and close while awaiting a restart answer.
Production library/examples check, scoped test build, strict Clippy, locked
metadata, crypto tree and diff check exit zero. AWS-LC is selected without an
active Ring provider. All owned groups are gone and source remains clean.

Initial `fc15fa153bd27f350b300e656f62ccf2f4f650e1` check and Clippy exit 101:
Rust 1.99 rejects deprecated `AtomicU8::fetch_update` under warnings-deny.
Its test build and six executions do not run. The initial pack preserves the
diagnostic and missing coverage; the final pack is separate.

Both JSON files contain only this public fork's portion of verified cloud
packs, with every included file hash checked. Large raw build/metadata logs
are excluded; selected artifacts and diagnostics remain. Parent pack hashes
record provenance. These use synthetic H.264/HEVC NAL data and loopback sockets,
not native codec quality, capture/input, physical-device/network or sustained
CPU/RAM/GPU/power/bandwidth qualification. No performance target is lowered or
declared achieved. Native companion evidence remains with the embedder.
