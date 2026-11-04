# Embedded desktop media

This crate is maintained in the Fincode custom fork. It has no dependency on
Voxl's process-owning server entry point, web asset build, libx264 or audio stack.
It currently provides a macOS VideoToolbox H.264/HEVC encoder, not a complete desktop
stream. Screen capture and native application integration remain separate work. An
optional WebRTC sender is implemented behind the `webrtc` feature; platform
validation and receiver interoperability are still in progress.

Give the encoder immutable NV12 buffers from the capture API. The hardware-first
mode tries a required hardware encoder, then an explicitly software encoder;
the actual hardware property is queried and exposed. Software-only and
hardware-required modes make matched experiments possible. `VideoEncoder` selects H.264 Baseline or HEVC Main (8-bit 4:2:0), with real-time
encoding, no frame reordering and two-second keyframes. AV1 encoding returns
UnsupportedCodec; it is never silently passed through the H.264 parser.
The bitrate is an average target, not a hard transport bandwidth ceiling.

Output is tagged with its actual codec: complete Annex B access units with
SPS/PPS (H.264) or VPS/SPS/PPS (HEVC) before keyframes. HEVC rejects truncated
or multilayer NAL headers and recognizes IRAP frames. There are
at most three outstanding native encode submissions and two queued output
frames. A lagging consumer closes the encoder rather than losing reference
frames and then delivering corrupt deltas. Recreate the stream and send a fresh
IDR after that failure. Drop invalidates the native session before destroying its
callback context. Run native session creation/destruction off the UI executor.

The embedder supplies a synchronous allocator idle hook, called after every
foreign callback including failures; use a no-op only when its allocator needs
no idle announcement. This crate creates no long-lived Rust thread or runtime.
No callback panic is allowed to unwind across the framework's C boundary.

`cargo run --release --example codec_probe -- hardware /tmp/probe-hardware.h264`
and the corresponding `software` command generate the same 60-frame, 1280×720,
30 fps synthetic workload. They capture no screen. Record the exact commit,
external process deadline, CPU/RSS and independent decode/quality results as
well as the printed encode latency. These numbers are synthetic codec evidence,
not desktop capture, network or physical-phone power measurements.


With `--features webrtc`, the sender uses the caller's Tokio runtime, sends only
video (H.264 or HEVC Main) and selects AWS-LC crypto explicitly. Register the actual encoder
SPS profile/level, exchange its gathered offer/answer inside authenticated
Connect, and keep the native view tied to that session. RTP timestamps follow
capture presentation time even when unchanged frames are skipped. PLI/FIR
requests are coalesced and rate-limited before the encoder forces an IDR.
The bounded loopback test uses real UDP/DTLS/SRTP and synthetic NAL payloads;
it does not test a decoder, display capture or video quality.

macOS cross-compilation of the encoder library and synthetic probe passed on
Finiy at `a0c00038324d3c3c6b31af9cee9cfb5d12e74743` (Rust 1.93.1, default
features, aarch64-apple-darwin). It checks types only, without framework linking
or native execution. Native Windows compilation of the prior WebRTC head found
an RTP module import mismatch; it is corrected in this checkpoint. AWS-LC
compiled on Windows and the active dependency graph contained no ring backend.


Automatic routing keeps host and server-reflexive candidates eligible; relay-only
is an explicit diagnostic policy. `WebRtcConfig::direct_first` needs no TURN
service; `add_ice_server` supplies bounded, authenticated fallback while keeping
automatic direct candidates eligible. It rejects malformed URLs and oversized
credentials before constructing the peer. Read `route_statistics` for the
nominated path and byte counters: the presence of a TURN URL alone says nothing
about the path used. Returned diagnostics contain no addresses or credentials.
The pinned native SDK gathers server-reflexive candidates only from an explicit
plain UDP `stun:` URL. Configure one alongside TURN UDP/TLS when Internet-direct
discovery is wanted; a TURN URL alone does not enable that gatherer. STUN adds
no relay allocation and does not remove host candidates. The discovery test
uses a real loopback UDP exchange with a synthetic mapped address; physical
NAT traversal remains separate required evidence.
`WebRtcVideo::next_update` waits for first connection, coalesced keyframe feedback
and close without a periodic poll. Feedback still has its 250 ms rate limit;
only a pending request arms that deadline. A revision counter preserves wakeups
across cancelled waits, and the synchronous authority close handle wakes the
listener as well as an in-flight sender. This changes scheduling, not the codec
or the meaning of a direct-route result; endpoint CPU/latency need native runs.
The same-process loopback test requires a proven direct nominated path; physical two-device
LAN, Internet-direct and forced-relay cases remain required integration checks.

The pinned SDK's TURN relayer supports UDP. The media wrapper adapts TURN/TCP
and TURN/TLS through endpoint-local loopback sockets, one stream per SDK socket.
Direct ICE and TURN/UDP bypass the adapter. TLS uses AWS-LC, normal hostname/IP
certificate validation and Mozilla roots; remote ICE configuration cannot add
trust roots. Stream framing follows [RFC 8656](https://www.rfc-editor.org/rfc/rfc8656.html).
Queues hold at most 32 datagrams of 2000 bytes, matching the SDK receive limit;
overload closes that stream rather than silently reopening another allocation.
An allocated selected stream failing ends media; an unused fallback failing
does not end healthy direct video. `route_statistics.local_relay_transport`
joins the actual nominated allocation to its transport instead of reporting
the loopback adapter as network UDP. It describes only the local relay leg.

`vendor/webrtc/PROVENANCE.md` records the verified package and two driver
ordering changes: flush relay writes produced by core media before waiting,
and install remote tracks discovered during TURN decapsulation before
delivering their first RTP packet. The first-payload relay fixture failed
before these changes and passes with both present. No upstream PR is opened.

At `32569b3f6816353f85e5d7a453099dd1a565996b`, locked build, strict library/
examples Clippy and seven selected tests passed: framing/bounds, nominated
UDP/TCP/TLS relay media, rejection of an untrusted TLS certificate, and healthy
direct video after an unused TCP relay fails. The same fixtures also require
selected stream loss to wake and end media. Raw sanitized commands, test output
and server/cleanup counts are in `results/2026-11-04-turn-streams/`.
These use isolated loopback coturn with fresh short-lived REST credentials and
a private test CA. They prove synthetic encrypted transport, not native
decoding, physical LAN/VPS routing, hardware performance or endpoint memory.
Credential renewal/ICE restart, trickle gathering and limiting relay allocation
fanout across interfaces remain required work before release.

`codec::select_codec` covers AV1, HEVC and H.264. It prefers a hardware pair in
caller preference order, then allows only explicitly permitted, measured
real-time software fallbacks. Unknown capability is never interpreted as
support. This is selection policy, not an AV1/HEVC encoder implementation;
providers must advertise only paths they actually implement and have probed for
the requested geometry, fps, profile and chroma. The native encoder and WebRTC sender implement H.264 and HEVC Main.
`hevc_main_profile` reads the actual SPS profile/tier/level after removing
emulation-prevention bytes; `VideoFormat::HevcMain` carries those parameters
into RFC 7798 SDP and selects its own RTP payloader. The sender rejects a frame
tagged with a codec other than the negotiated format. Native HEVC execution
and browser/receiver interoperability remain required before advertising HEVC
in the application.


The initial route assertion failed because the statistics report's selected-pair
link was insufficient to establish the route. The sender now reads the actual
ICE transport's selected candidate pair and joins byte counters by its candidate
IDs. The unchanged direct-route and nonzero-counter assertions passed on Linux
at `6980a8dcc22f5528a213e704e58dbfef0bd3f676`, as did strict library/examples
clippy. Unknown remains the result when no selected transport can be obtained.
This is localhost transport evidence, not a physical LAN result.

The first native Mac H.264 result and raw command logs are in
`results/2026-11-04-macos-h264/`. Hardware encoding and independent decoding
passed; the software attempt failed with an unsupported native property.
Operation-specific errors have been added to identify that failure. Do not
advertise a validated CPU fallback before the native software probe succeeds.

Run the same native synthetic workload with `codec_probe hardware /tmp/probe.hevc hevc`
for HEVC. Its native validation and any matched-quality savings are pending.

The HEVC transport loopback separately requires intact type-19 and type-20 IDR
payloads, capture timestamps, keyframe feedback, direct selected candidates,
byte counters and bounded close. Like the H.264 loopback, it uses synthetic NAL
bytes and proves neither valid native decoding nor browser compatibility.
Profile signaling follows [RFC 7798](https://www.rfc-editor.org/rfc/rfc7798.html).
