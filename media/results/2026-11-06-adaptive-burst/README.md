# Adaptive encoded burst budgets

Exact final component source: `edc88ef013488ee045b2a89380ef05421b69a969`.
Seventeen selected adaptive tests, both legacy H.264/HEVC UDP loopbacks and strict
library/tests/examples Clippy pass. See `comparison.json` and compressed receipts
for exact commands, sources, deadlines and failures; eighteen owned groups were
independently absent. The original logs remain in the local owner-only archive.

A native CPU H.264 application trial had produced a 102927-byte keyframe under
a1500 kbps pacing target, requiring more than549ms before RTP overhead and
exceeding the existing500ms send bound. That application observation is separate
from the component tests here; no successful application fix is claimed here.

`set_streaming_bitrate_target` verifies both average bitrate and a hard native
encoded-byte budget before using it. Hardware uses400ms; CPU H.264 rejects
fractional seconds on this machine and uses a conservative1000ms native window.
Both allow400ms worth of the requested average target as bytes. Consequently
CPU's sustained hard ceiling is lower than its average request. This is an
explicit qualification tradeoff requiring decoded quality/resource measurement,
not a general optimization or default admission.

The first tests rejected hardware's exact floating-point byte readback and
found the CPU's fractional-window error. A numeric diagnostic rerun retained
those failures. The fix accepts exact, finite numeric byte equality (never
booleans or rounded/coerced values) and verifies the actual supported window.
The final three new native tests encode generated320x180 NV12 with hardware
H.264/HEVC and CPU H.264, preserving keyframes and checking their observed size.
They do not establish decoded text quality or full-size frame bounds.

Apple describes average bitrate separately from optional hard byte/time limits:
[DataRateLimits](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_dataratelimits?language=objc).
Support and actual output still need endpoint-specific observation. No changing
link capacity/loss, physical LAN/device, CPU/RSS/power, total wire savings or
application reliability admission is claimed. Existing constructors are unchanged.

Run `python3 -B verify.py` for offline integrity and recorded-count validation;
this does not replay native code.
