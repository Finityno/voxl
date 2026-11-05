# Prepared native checks (not executed)

Freeze the custom fork at `c976515bc3b71789c472a614ae20f301ab5a334b`, in a clean
standalone checkout outside the Fincode workspace with its own target. Use two
jobs, BelowNormal priority, exact source/toolchain/OS/GPU/driver metadata, and
Windows Job process-tree deadlines with terminal process-handle confirmation.
Never change shared caches/configuration or launch/kill the installed app.

On Adrian use `C:\fincode-host\adrian-run.ps1` for every command, with
`-Commit 2e4d126037caf90de71efede8baade59a6b996f2`, `-WaitSeconds 120` and explicit
`-TimeoutSeconds`. The child must verify and execute inside the owned standalone
fork checkout so parent Cargo workspace discovery cannot select another source.
Finiy also needs a bounded Job wrapper. No detached or unbounded test/build.

1. Run Cargo/rustc preflight and verify a clean exact fork SHA before/after.
   Under a 600-second build deadline, check native Windows plus WebRTC:
   `cargo check --locked --manifest-path media/Cargo.toml --features webrtc --lib --examples -j2 -q --message-format=short`.
   Run matching strict library/examples/tests Clippy with the same deadline.
   This is the Windows feature/link environment, not Mac cross-check evidence.
2. Build default-feature `windows_conversion_probe` first, 600 seconds:
   `cargo build --locked --manifest-path media/Cargo.toml --example windows_conversion_probe -j2 -q --message-format=short`.
   Run the built executable separately under 60-second Job deadlines:
   `<unique-direct.nv12>` and `<unique-copied.nv12> --shader-only`.
   Each requires 8 bars, 640x360/345600 bytes, max Y/U/V error <=3 and the proper
   explicit source-copy observation. Preserve every actual result/failure; an
   unsupported adapter/driver is missing capability, never a passing case.
3. Independently process successful synthetic output bytes. For example use a
   known ffmpeg build, an external deadline and explicit NV12/640x360/limited
   BT.709 input interpretation to emit one RGB24 frame/framemd5. Check original
   bar interiors and record decoder/version/hash, rather than trusting the
   probe's summary. The three-code bound is a test threshold, not achieved
   accuracy until both numeric and independent byte checks pass. Transfer only
   these synthetic bytes for Mac processing if Windows lacks a decoder.
4. Build/run the native encoder modes from the earlier Windows encoder plan at
   this new source: H.264 software, hardware-memory H.264/HEVC/AV1 and
   hardware-surface H.264/HEVC/AV1, each with separate external deadlines and
   independent frame/profile/keyframe/color verification. Do not combine cases
   or turn unsupported/unrun cases into passes. The default synthetic GPU can
   differ from the monitor adapter on hybrid systems; record that limitation.
5. Actual capture is subsequent app-host validation, after current grants,
   sharing/OS capability, cancellation, generation and input cleanup are wired.
   Cover the selected adapter/display, static/moving desktop, cursor, border,
   direct/copied source bindings, codec/CPU fallback, delayed sample release,
   forced keyframe while still, active-desktop/lock changes, unplug/resize,
   permission withdrawal, close while waiting/converting and native resource
   teardown. Capture no recording by default and perform no visual verification
   without the user's explicit request. Numeric/received-frame assertions can
   cover transport without publishing screen pixels.

Native app check source is `2e4d126037caf90de71efede8baade59a6b996f2`. Windows
provider wiring is not part of this pin; receiver qualification and future host
integration need their own native result. Keep native execution, independent
color/decoder evidence, full path behavior and matched CPU/RSS/GPU/power/network
performance separate. No latency, bandwidth-saving or capacity pass is implied
by a library check or an enumerated hardware encoder.
