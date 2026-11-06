# Native compression-window limit

The ordinary hardware H.264 and HEVC encoders on this Mac reject explicit
`MaxFrameDelayCount` values zero and one. Neither limit is adopted by Fincode.
This is a negative capability result, with no latency, quality, bandwidth,
CPU or memory improvement claim.

Exact source: `4391a67750a492aaa0dc97c4f30e4a31225b7076`, custom fork only.
Host: Apple M5 Pro / Mac17,8, macOS 27.2 build26B5101f, Cargo1.99.0,
Xcode-beta macOS27.2 SDK. The experiment uses required hardware, compatible
profile, ordinary real-time rate control, no explicit priority or QP limit,
320×180 at60fps, target500kbps and pipeline depth2. These tiny controls test
capability, not realistic desktop performance or text quality.

| Codec | Untouched delay property | Limit0 | Limit1 |
| --- | --- | --- | --- |
| H.264 | 14/14 frames independently decoded | Setter rejects, -12900 | Setter rejects, -12900 |
| HEVC | 14/14 frames independently decoded | Setter rejects, -12900 | Setter rejects, -12900 |

Each control has two warmup and twelve observed frames, a forced interval of
six frames and verified submission/output ordering. All four experimental
requests fail at property setting, before preparation or frame submission.
The error is `Platform { operation: "maximum frame delay", code: -12900 }`.
Consequently no typed readback, matched-quality or performance comparison is
possible for these requested configurations. Other devices, geometries and
encoder modes remain unqualified; this is not a platform-wide impossibility.
The untouched native delay default was not queried.

The SDK declares this optional integer property as a compression-window limit:
zero can require output before the encode call returns. The new opt-in API
requires hardware, setter success and matching non-floating CFNumber readback
after preparation; unsupported or ignored limits fail. Existing constructors
leave the property untouched. Output queues, reference ordering, callback panic
containment and terminal backpressure behavior keep their existing bounds.
The app's media pin and encoder defaults are unchanged.

Native test compilation, the release probe build and strict library/probe
Clippy pass. Two new tests cover pre-session refusal of software/implicit
fallback, zero and integer boundaries, mismatched/unlimited/float/boolean/null
readback; one existing output-before-wake/first-failure contract also passes.
All21 bounded command groups were independently absent after completion, with
no timeout or forced descendant cleanup. Four native exit1 results are expected
rejections, not passing encodes. The orchestrator's terminal0 means its capability
plan completed; it does not turn those requests into supported configurations.

`receipt.json` retains exact commands, individual exits, deadlines, source
checks and group cleanup. Paths are normalized; logs are compressed with hashes
in `files.json`. Owner-only originals retain31 files including bitstreams and
the exact harness. No image, font, decoded pixel or bitstream is published.
Run `python3 -B media/results/2026-11-06-frame-delay/verify.py` to check retained
hashes, selected test counts, decoded controls and negative results. This checks
the numeric receipt; it does not replay native encoding or independently attest
the historical host.
