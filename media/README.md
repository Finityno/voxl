# Embedded desktop media

This crate is maintained in the Fincode custom fork. It has no dependency on
Voxl's process-owning server entry point, web asset build, libx264 or audio stack.
It currently provides a macOS VideoToolbox H.264 encoder, not a complete desktop
stream. Screen capture and native application integration remain separate work. An
optional WebRTC sender is implemented behind the `webrtc` feature; platform
validation and receiver interoperability are still in progress.

Give the encoder immutable NV12 buffers from the capture API. The hardware-first
mode tries a required hardware encoder, then an explicitly software encoder;
the actual hardware property is queried and exposed. Software-only and
hardware-required modes make matched experiments possible. The profile is H.264
Baseline with real-time encoding, no frame reordering and two-second keyframes.
The bitrate is an average target, not a hard transport bandwidth ceiling.

Output is complete Annex B access units with SPS/PPS before each IDR. There are
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
H.264 video and selects AWS-LC crypto explicitly. Register the actual encoder
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
credential service for a working LAN path. Read `route_statistics` for the
nominated path and byte counters: the presence of a TURN URL alone says nothing
about the path used. Returned diagnostics contain no addresses or credentials.
A same-process loopback test proves a direct nominated path; physical two-device
LAN, Internet-direct and forced-relay cases remain required integration checks.

`codec::select_codec` covers AV1, HEVC and H.264. It prefers a hardware pair in
caller preference order, then allows only explicitly permitted, measured
real-time software fallbacks. Unknown capability is never interpreted as
support. This is selection policy, not an AV1/HEVC encoder implementation;
providers must advertise only paths they actually implement and have probed for
the requested geometry, fps, profile and chroma. The initial implemented native
encoder and sender still use H.264.
