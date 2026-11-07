# Prepared Windows qualification (not executed)

Use the custom fork only, detached clean at
`0b365da584c7a24db01e41beb45f4da75651651c`. Use a standalone scratch checkout
outside the Fincode Cargo workspace, an owned target, two jobs and BelowNormal
priority. Preserve toolchain/OS/adapter/driver metadata and exact clean source
before/after. Native runs need the matching Rust toolchain, not a Mac cross
check. Never modify global caches/configuration or launch/kill the installed app.

On Adrian all commands must run through `C:\fincode-host\adrian-run.ps1` with
`-Commit b2811a6c041555eb2fc4d42b81b253b98ea9a830`, `-WaitSeconds 120` and explicit
`-TimeoutSeconds`. The child must run Cargo from its owned standalone fork
checkout, not the parent Fincode workspace. This avoids the workspace-discovery
failure recorded by earlier validation. Use Windows Job process-tree termination
and terminal process-handle confirmation on Finiy too. No detached test process.

1. Preflight Cargo/rustc versions, free disk/commit, adapter/driver and wrapper
   lane. Verify fork SHA and clean status. Do not invent fallback devices.
2. Build only this library and these examples:
   `cargo check --locked --manifest-path media/Cargo.toml --features webrtc --lib --examples -j2 -q --message-format=short`
   under a 600-second deadline. This checks the actual Windows AWS-LC graph.
3. Build the test library first, separately, 600 seconds:
   `cargo test --locked --manifest-path media/Cargo.toml --features webrtc --lib --no-run -j2 -q --message-format=short`.
   Execute only `windows::bitstream` (2 tests) under a 60-second Job deadline,
   and the selected actual UDP/DTLS/SRTP loopback if transport coverage is needed.
   Keep features/env unchanged between build and execution.
4. Strict library/examples/tests Clippy, 600 seconds:
   `cargo clippy --locked --manifest-path media/Cargo.toml --features webrtc --lib --examples --tests -j2 -q --message-format=short -- -D warnings`.
5. Build `windows_codec_probe` first, default features, 600 seconds. Run it under
   separate 60-second Job deadlines with unique new output files. Start with
   `software h264`, then hardware-memory H.264/HEVC/AV1 and hardware-surface
   H.264/HEVC/AV1. Stop/report the first failure in each independent mode; an
   unsupported codec/driver is missing capability, never a pass or permission to
   relabel the stream. The probe is synthetic and unpaced, with 30fps capture
   timestamps. It supplies no sustained-resource or input-latency qualification.
6. Independently decode successful raw H.264/HEVC or IVF AV1 with a known decoder,
   require all 60 frames and correct 1280x720 geometry, inspect actual profile,
   color/range and frames zero/thirty as random access. Preserve hashes and
   ffprobe/framemd5 evidence. If the tool is unavailable, transfer only these
   synthetic fixture files for Mac decode; do not claim an independent pass
   from the encoder's own keyframe/codec metadata.
7. Record failures, counts, exact commands, output hashes and terminal groups.
   Do not treat Windows-target compilation, an enumerated hardware class or a
   created D3D device as proof of active GPU work or a performance improvement.

Separately, the app's Windows receiver is still at the pin source above. Once
its library dependencies compile natively, run scoped `desktop_media`/`browser`
library check and strict Clippy through the wrapper, then the prepared hidden
`desktop_windows_surface` lifecycle fixture under an external deadline. Read its
source and previous viewer evidence before choosing assertions. This needs
native WebView2; pure state tests or Mac IPC do not validate that surface. This plan performs
no visual verification or user desktop input.

Windows capture/conversion/host-provider work and sustained matched-workload
measurements are subsequent implementation/qualification, not covered here.
