# Finite desktop keyframe policy

Final executed source `56a4b318c576b0ac7c6da1ac5616a5fcbe90d896` passes four
selected native Mac tests, test compilation and strict library/tests/examples
Clippy. CPU H.264, hardware H.264 and hardware HEVC each preserve their actual
provider and codec, verify 900 frames/30 seconds by typed native readback,
produce keyframes at initial frame 0 and explicitly forced frame 90 across 120
static submissions, and produce another automatic keyframe at presentation time
35 s. The sparse timestamp proves the time limit independently of frame count;
there is no 35-second wall-clock sleep. Range/late-change guards and time-property
number/boolean/non-finite/inexact tests are included. No provider was skipped.

The initial execution already passed static/forced output and strict Clippy;
the second adds the sparse-time assertion. Both exact receipts/logs are retained,
with 14 ended supervised groups. `verify.py` checks this offline and proves current
production media bytes still match the final tested source.

`set_keyframe_interval` is an opt-in pre-first-submission API. Existing constructors
retain their behavior. Both finite limits are required; a frame count alone does
not bound variable cadence. Partial native setting failure is terminal. Readback
is configuration, not proof that every future input has a particular GOP:
natural keyframes may occur earlier. Forced IDRs remain available.

The [frame](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_maxkeyframeinterval)
and [time](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_maxkeyframeintervalduration)
properties are documented by Apple; the installed SDK header confirms both
limits apply, whichever comes first. The 60-second API upper guard is local policy,
not an Apple limit. These component tests do not qualify actual decoder recovery,
controlled-network latency, text quality, CPU/memory/power or production defaults.
