# Embedded desktop media

This crate is maintained in the Fincode custom fork. It has no dependency on
Voxl's process-owning server entry point, web asset build, libx264 or audio stack.
It currently provides a macOS VideoToolbox H.264 encoder, not a complete desktop
stream. Screen capture, WebRTC transport and native application integration
remain separate work.

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
