# Adaptive transport and native bitrate precursor

This is component implementation and qualification in the custom fork. Fincode's
consuming media pin and application defaults are unchanged. Integration,
changing-capacity/loss tests, actual native codec-to-decoder coupling and resource/
quality admission remain required; an optional constructor alone is not the goal.

Runtime source `0ec781781ba5b3063264f4f6d11dddf3f71c6bc1` passes **13 adaptive
tests and 2 legacy transport tests**, with zero failure/ignore. Strict library,
tests and examples Clippy passes at `66bd7b014e9e43d39455ef21d70d524a2af92083`.
That later commit only handles the new update variant in the unpaced replay
example; the recorded diff verifies all library/test sources, manifest and lock
unchanged. Runtime evidence keeps its original attribution.

## Implemented behavior

- Separate controlled constructors use the pinned SDK's GCC/TWCC estimator and
  token bucket. Existing constructors retain their unpaced behavior.
- Pacing waits for release every 16 original packets and at each frame boundary.
  Driver enqueue is not controlled-send completion. Release notifications are
  coalesced at the requested boundary, rather than waking the producer for every
  packet. This is a verified notification contract, not measured CPU savings.
- A 5 ms RTP burst budget follows the target, floored at one 1500-byte packet and
  capped at 6000 RTP bytes. Held/released RTP is bounded to 256 packets, with a
  500 ms packet/frame deadline. These are safety bounds, not latency admission.
- Overflow/deadline/missing metadata ends the controlled stream. Synchronous
  authority close retires queued RTP before later paced release. A cancelled
  partially written access unit closes either sender mode; it cannot be reused
  as if the frame completed. Already handed-off network data is not recalled.
- The SDK NACK responder stores raw RTP and discards custom attributes. A
  bounded 1024-entry exact SSRC/sequence/timestamp map restores current-epoch
  tickets. Retransmissions consume pacing budget without falsely completing a
  producer send. Recovery retires that map/queue and filters old-path feedback.
  Controlled live ICE restart remains unqualified beyond these component checks.
- Estimates remain unknown without receiver feedback. A GCC target can retain
  its initial prior until enough reports arrive; it is not a measured link
  capacity. Its age is exposed. Encoder-update events coalesce changes and
  reserve 15% for packet overhead/retransmissions, subject to the existing
  250 kbps encoder minimum and the embedder's original ceiling.
- VideoToolbox can retune future submissions on the same session, with exact
  integer readback, including explicit CPU H.264. It cannot exceed its original
  configured ceiling. A partially applied/unverified change is terminal. Prior
  submissions may keep their old target. Initial getter values remain requests,
  not native observations; average bitrate is not a hard wire-rate ceiling.
- Every sending generation additionally requires a completed keyframe boundary
  before deltas. The embedder still authenticates generations, controls capture
  epochs and supplies a fresh post-recovery keyframe.

The pacing state was boxed after Clippy identified a 304-byte enum. No dependency
version/feature, runtime or dedicated thread was added. The original description
of a four-packet burst was corrected to a rate-dependent RTP byte budget; packet
sizes vary. Queue failure is currently terminal, so normal capacity drops still
need full application adaptation/recovery qualification before default admission.

## Exact coverage

| Scope | Passed | What it establishes |
| --- | ---: | --- |
| Pure contracts | 7 | Config bounds/unknown target; finite pacing/deadline/close; overflow; boundary wake coalescing; NACK epoch/completion; old feedback/target coalescing; keyframe boundary |
| Native VideoToolbox | 3 | Hardware H.264, hardware HEVC and explicit CPU H.264 retune 750→350→1000 kbps with typed readback; geometry/clock/hardware selection unchanged; invalid rates leave the session usable |
| Controlled real UDP | 3 | Two synthetic-NAL media scenarios and a partial-send cancellation scenario: complete H.264 payload/90 kHz clock, paced wait, real negotiated TWCC/target; no-feedback receiver stays unknown; cancelled queued RTP does not escape |
| Legacy real UDP | 2 | Existing H.264/HEVC payload, capture clock, feedback, direct route and close checks retain their unpaced behavior |
| Strict Clippy | pass | Library, tests and examples compile with warnings denied at the later example-only fix |

Native tests use generated 320×180 NV12 surfaces, with four outputs per encoder.
They establish successful session/property/output behavior, not independent
native decoding, text quality, encoded bitrate achievement or savings. UDP tests
carry synthetic NAL bytes, not those native outputs. No capture, OS input,
application/phone viewer or physical networking is exercised here. Actual loss/
congestion changes, mixed MTUs, controlled live recovery and resources/power/
quality need stronger tests. No optimistic target or default is admitted.

## Retained failures

1. `22fda7f0d935982ca4a03e5538d261cd7b91563f`: test build exits 101 for an
   incomplete enum pattern and a newly deprecated atomic method. Zero tests run.
   The repair uses a portable compare-exchange loop, not a newer toolchain API.
2. `3a14eb51a9547c6160cc0f801514882acb896cc3`: ten targeted tests pass; strict
   Clippy exits 101 on the oversized pacing enum. This remains an incomplete run.
3. `9ca75abe8ce3671a0c3bf37e02e95d97517a7dce`: eleven pass and one fails.
   The NACK test still assumes immediate release after the burst shrinks. The
   repair advances to the computed release deadline, preserving the smaller
   rate-dependent budget and all safety/performance limits. Clippy does not run.
4. Runtime source above: thirteen adaptive and two legacy tests pass. Clippy
   exits 101 because replay omitted the new update variant. Final source handles
   that variant with an explicit error: unpaced encoded-file replay cannot
   retune an encoder it does not own. The final unchanged-core check and strict
   library/tests/examples Clippy pass; runtime is not re-attributed.

All **28 recorded command groups** are independently absent, with no deadline or
forced cleanup. System-managed codec helpers are not claimed as exclusively
owned/reaped. There are **44 indexed owner-only originals**, including complete
logs, receipts and coordinators. No images, bitstreams, credentials, addresses,
SDP or profiling archive is published. Checkout paths are normalized in public
receipts/helpers; original hashes remain recorded. Generic runner boilerplate
is preserved separately and explicitly clarified to the actual command scope.

The host is Apple M5 Pro/Mac17,8, macOS 27.2 build 26B5101f, arm64, Rust 1.99.0;
`platform.json` records the checked metadata. All jobs use nice 19, two build
jobs, the shared lock, this checkout's own target and process-group deadlines.
The independent verifier checks published hashes, exact source/command exits,
pass/failure history and cleanup attestations without launching media:

```bash
python3 verify.py
```

The API uses Apple's [average bitrate property](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_averagebitrate)
and the pinned RTC estimator/pacing APIs. Successful setting/support is not
bandwidth or quality admission. Windows remains untested while its machines are
stopped; macOS AV1 encoding and the complete cross-device deployment goal remain
separate outstanding work.
