# Embedded desktop media

This crate is maintained in the Fincode custom fork. It has no dependency on
Voxl's process-owning server entry point, web asset build, libx264 or audio stack.
It provides a macOS VideoToolbox H.264/HEVC encoder and an experimental Windows
Media Foundation encoder with explicit Windows capture/conversion helpers.
Application authority, display selection and host integration belong to the embedder. An
optional WebRTC sender is implemented behind the `webrtc` feature; platform
validation and receiver interoperability are still in progress.

Native Apple Silicon builds enable AES 0.8's runtime ARM detector through the
repository's target-specific Cargo setting, retaining its software fallback.
Embedders must carry that setting in their own root config because dependency
configs do not propagate. The isolated `tools/aes-probe` workspace and
`results/2026-11-04-arm-aes/` retain exact-source alternating component comparisons,
independent known vectors and generated-instruction evidence. Component CPU
savings do not establish whole-SRTP, codec, endpoint or bandwidth improvements;
full native interoperability with the setting must be validated separately.

### macOS encoder

The macOS encoder accepts immutable NV12 buffers from the capture API. Its hardware-first
mode tries a required hardware encoder, then an explicitly software encoder;
the actual hardware property is queried and exposed when supported. Explicitly
disabling hardware guarantees software encoding; in ordinary real-time mode only that path accepts the
documented unsupported-property error from the hardware-status query. Other
errors, missing successful query values and mismatched selection still fail.
The opt-in low-latency case below also accepts only the unsupported-property
status when hardware was required at creation. `acceleration_selection`
distinguishes queried hardware/software from required/disabled specifications;
`using_hardware` retains the corresponding boolean. Ordinary hardware paths
remain strict. Successful required-hardware creation guarantees no software
substitution, but is not reported as a successful hardware-status query.
Software-only and
hardware-required modes make matched experiments possible. Hardware H.264 now
requests Constrained Baseline when the optional macOS 12+ constant is available;
software retains the supported Baseline property and reports its actual SPS.
The constant is resolved dynamically, preserving older embedding deployment
targets. The observed software output is Constrained Baseline; the SDP always
uses actual encoded flags. HEVC selects Main (8-bit 4:2:0), with real-time
encoding, no frame reordering and two-second keyframes. macOS AV1 encoding returns
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

The macOS embedder supplies a synchronous allocator idle hook, called after every
foreign callback including failures; use a no-op only when its allocator needs
no idle announcement. This crate creates no long-lived Rust thread or runtime.
No callback panic is allowed to unwind across the framework's C boundary.

`cargo run --release --example codec_probe -- hardware /tmp/probe-hardware.h264`
and the corresponding `software` command generate the same 60-frame, 1280×720,
30 fps synthetic workload. They capture no screen. Record the exact commit,
external process deadline, CPU/RSS and independent decode/quality results as
well as the printed encode latency. These numbers are synthetic codec evidence,
not desktop capture, network or physical-phone power measurements. An optional final
`framed-output` argument preserves native access units for the bounded
`encoded_peer_probe` WebRTC fixture. It is test IPC, not a recording API.

`VideoEncoder::new_tuned(config, tuning, idle)` adds explicit local experiments.
The WebRTC sender accepts the actual Baseline, High (`6400`) or Constrained High
(`640c`) SPS profile bytes without relabelling SDP. Other new H.264 profiles
remain rejected; receiver compatibility and measured admission are separate.
`new` still supplies the default tuning and preserves the ordinary compatible
profile, real-time mode, two-second keyframe interval and hardware/CPU fallback.
Non-default tuning requires `RequireHardware`; creation fails rather than
silently substituting software or another tuning. H.264 High and Constrained
High can be requested separately from low-latency rate control, so a comparison
does not have to confound a profile change with the mode change. HEVC retains
Main. Optional newer constants are resolved dynamically; missing symbols fail
the explicit request without raising the default deployment target.

Apple's [low-latency encoder specification](https://developer.apple.com/documentation/videotoolbox/kvtvideoencoderspecification_enablelowlatencyratecontrol)
describes profile restrictions and an infinite GOP. The installed SDK repeats
those restrictions. They are not a compatibility guarantee for an endpoint.
Low-latency tuning omits the default periodic-keyframe property; its caller
must drive explicit recovery requests. Always inspect the actual SPS and
independent decode behavior. A successfully created session is not measured
quality/resource/latency admission, hardware-decoder proof or permission to
advertise a profile a receiver does not support. The embedding app continues
to use its existing encoder entry point.

For example, add `--tuning=low-latency-constrained-high --warmup-frames=30
--frames=180 --force-keyframe-period=60` to a hardware H.264 probe. Tuning names
are `compatible`, `high`, `constrained-high`, `low-latency-compatible`,
`low-latency-high`, and `low-latency-constrained-high`; HEVC can use the compatible
profile in either mode. The probe never overwrites an existing output file.
It excludes warmup from its raw latency list, byte total and percentile summary,
while the output streams retain warmup for independent decoding. Explicit
keyframe requests must produce actual keyframes. Boundaries and timestamps stay
available in the optional framed stream, allowing a decoder to start fresh at
a later forced keyframe. Sample counts are bounded (warmup at most 1800 frames,
observation 1..18000, forced interval 1..1800); run with an external deadline.

`macos::VideoEncoder::new_tuned_with_priority` additionally accepts the local
`EncodingPriority::Speed` or `Quality` experiment. Both require explicitly
selected hardware, successful setting and matching typed CFBoolean readback
after native preparation. An unsupported/ignored property fails; it cannot
silently choose software or another preference. Existing constructors retain
`Default`, which neither sets nor queries this property. That value is
unspecified, not a measured observation of an encoder's internal choice.
`codec_probe --priority=default|speed|quality` exposes the same request and logs
whether readback was verified. The [Apple property reference](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_prioritizeencodingspeedoverquality)
describes a speed/quality tradeoff, so support alone establishes no speed,
bandwidth, text-quality or power improvement. No application default changes.

The [native priority experiment](results/2026-11-05-encoding-priority/README.md)
retains four supported ordinary priority combinations, four unavailable
low-latency/priority combinations, matching default decoded controls and two
short matched pilots. Speed-priority HEVC and low-latency H.264 regress measured
foreground quality, and low-latency HEVC closes during encoding. None is
promoted. The separate [five-session ordinary-codec comparison](results/2026-11-05-codec-repeatability/README.md)
reproduces 33.01% fewer HEVC encoded bytes on one fixed clip, with slower encode
callbacks. These are component observations with explicit resource/network
limits, not production codec admission or provider-billed savings.

`macos::VideoEncoder::new_tuned_with_options` additionally accepts
`EncodingOptions { priority, max_frame_qp, max_frame_delay }`. Optional `MaxFrameQp::new(N)`
accepts 1 through 51 and requires explicit hardware and exact native integer
readback after preparation;
unsupported/ignored values fail. Existing constructors leave this property
untouched. The limit does not observe actual frame QP or guarantee image quality.
Apple's [maximum-QP discussion](https://developer.apple.com/videos/play/wwdc2021/10158/)
warns that the rate controller may drop frames to satisfy quality and bitrate;
our dropped-output callback remains terminal. The probe accepts
`--max-frame-qp=N`, and matched cases append `:qpN` after an optional
`:speed`/`:quality` request. This is an opt-in experiment, not application codec
admission; native support, decoded quality, exact frame counts, resources and
timing still need qualification.
The [maximum-QP experiment](results/2026-11-05-maximum-frame-qp/README.md)
passes three Rust/two Python contracts, eight tiny native combinations and
three old/current decoded default pairs. At1080p60, QP32/34/36 do not improve
the measured payload/foreground results; QP28 drops output after two warmup
frames. Failed receipts and analysis-only continuations remain explicit. No
new limit is admitted, and the consuming application stays on its earlier pin.

Optional `max_frame_delay: Some(N)` requests the native `MaxFrameDelayCount`
compression-window limit, with explicit hardware selection and exact integer
readback after preparation. Unsupported or ignored requests fail. Zero can make
the native encode call synchronous; one permits one delayed frame. The library's
two-output queue bound and terminal backpressure behavior stay the same. None
leaves this property untouched and observes no encoder default. The probe accepts
`--max-frame-delay=N` and logs verified requests. This experimental control is
not an application default or a latency, quality or resource improvement claim.
The [native frame-delay receipt](results/2026-11-06-frame-delay/README.md) retains
four rejected ordinary hardware requests on one Mac and two decoded controls.
No delay limit is admitted; unavailable settings receive no performance claim.

The [failure/wake follow-up](results/2026-11-05-encoder-failures/README.md)
retains the first terminal encoder cause and wakes waiting workers through the
bounded output channel when a native callback fails. Later encode/output calls
still return Closed; `failure_cause()` exposes the original diagnostic. The
probe now identifies the low-latency HEVC failure as dropped output, without
admitting that mode or attributing its underlying driver/rate-control cause.

The [bitrate follow-up](results/2026-11-05-low-latency-budgets/README.md)
completes low-latency HEVC at higher requested rates. In one short matched pilot,
the 12000-kbps request yields 0.955590 Mbps of observed encoded payload and 5.55 ms
callback p95 with no worse aggregate foreground frames/channels than paired
H.264. Lower requests still regress foreground quality. This is a candidate
for sustained qualification; requested bitrate is no ceiling, and no default,
profile admission or billed-cost claim follows from this clip.

The [five-session low-latency comparison](results/2026-11-05-low-latency-sustained/README.md)
now completes 30s warmup and 300s observed per fresh session at 1080p30.
HEVC reproduces 16.63% fewer payload bytes and a median individual-session
encode p95 of 5.58ms versus H.264's 12.27ms. Peak process memory and the first
frame are larger; scrolling text SSIM is lower, despite no worse foreground
aggregate/channel sums in all 45,000 paired frames. Exact numeric records and
an independent verifier retain those tradeoffs. This does not admit 1080p60,
full application resources, hardware decoding or network/billed-cost savings.

The [native 60fps pilot](results/2026-11-05-cadence-native/README.md) verifies
fresh 60fps source generation and unchanged default30 bytes, then independently
decodes five short streams. Low-latency HEVC encodes faster and uses fewer
payload bytes, but all three tested rate requests regress some foreground
frames/channels near keyframes and use more whole-probe CPU. These short
experiments do not promote a profile or establish sustained 60fps budgets.

A consuming application draft now pins the measured failure-wakeup source
`f098e462a62833b73d6a6103e4f50ff810fb7ffb`. Its Mac host library checks,
strict library Clippy and four selected consent/cancellation unit contracts
pass at the consuming draft's recorded source. Integrated viewer/application
compile checks and one updated-source native Mac/owned Simulator scenario also
pass, including capture/Connect/H.264 decode, lease expiry and signed revocation.
Physical endpoints and full application performance qualification remain open.
The app requests none of the new priority/tuning modes and keeps its codec
defaults; these component packs do not establish application performance
qualification.

The [quality-priority bitrate pilots](results/2026-11-06-hevc-quality/README.md)
independently decode eight short 1080p30 streams. A quality-priority HEVC
8000-kbps request uses 15.37% fewer actual payload bytes than its paired H.264
control, with no lower per-frame text SSIM or foreground aggregate/channel
metrics on this clip. Its roughly 15ms callback p95 is slower, and whole-probe
CPU is higher. Lower requests retain scrolling-text regressions; 20000 uses
more bytes than H.264. This is an experimental candidate for sustained broader
testing, not application admission, a bitrate ceiling or a codec-default change.

The [sustained quality follow-up](results/2026-11-06-sustained-quality/README.md)
now decodes 9,900 frames per codec after 30s warmup and 300s observation.
HEVC quality/8000 retains improved measured text/foreground metrics in all
9,000 observed frames, with 12.67% fewer payload bytes and slower callback p95
(14.99ms versus 12.24ms). This is one sustained pair, not the required repeated
broader/60fps admission or a full-app CPU/memory result. The same pack adds a
strictly compiled native `media/tools/macos-codec-capabilities.swift` inventory:
this M5 Pro reports hardware decoding for H.264/HEVC/AV1 but no registered
VideoToolbox AV1 encoder. Missing encoder hardware flags stay unknown; codec
metadata does not establish exact-profile support, available resources or the
receiver engine's actual hardware use. No new codec is advertised.

`codec_probe` also accepts `--width=N --height=N --fps=N --bitrate-kbps=N`
within the encoder's normal bounds. Defaults remain 1280x720, 30fps, 2500kbps.
`--input-nv12=PATH` reads a regular, non-symlink, tightly packed NV12 file directly
into each native buffer's active planes, before the timed encode interval.
Without `--input-cycle-prefix=N`, its exact byte length must match
`(warmup + observed) * width * height * 3 / 2` before output files are created.
With that option, the source instead contains 1..19800 whole frames and a
prefix shorter than the source; warmup and observed pixels cycle separately
without resetting timestamps. It cannot combine with a generated color-bar
pattern. Supply the actual file's range/color tags explicitly; the probe does
not infer them from a filename. Partial reads fail and release the native buffer.
Input read/generation costs are not production capture or endpoint CPU evidence.

`codec_probe --pipeline-depth=1|2` adds a bounded local asynchronous experiment.
Mac embedders that wait for capture and delivery capacity as well as encoder
output can install an `OutputWake` once, before the first submission. Output or
the first terminal error is queued before signaling; the hook must return
promptly without re-entering the encoder. Hook panics close the encoder and
cannot unwind across the native boundary. Existing constructors retain their
channel wait with no hook, thread or queue-depth change.

The default is one; two requires explicit hardware. At most two submissions,
including queued outputs, can be pending. The producer drains available output,
waits on the callback until the next submission/output deadline, and drains the
final frames. It checks every output's FIFO capture timestamp and forced
keyframe, never drops an encoded delta, and fails on an expired two-second
submission deadline. No library/application default, queue size or codec option
changes. Printed latency remains encode-call to producer receipt, including
drain scheduling; it is not a native callback timestamp or presentation latency.

`media/tools/run-pipeline-probe.py` compares depths one and two on the same
generated motion/text pixels, codec, bitrate and native settings. For example:

```sh
python3 media/tools/run-pipeline-probe.py --revision "$(git rev-parse HEAD)" \
  --output /tmp/voxl-pipeline-pilot --resolution 2160p --fps 60 --runs 2
```

It uses the checkout's own target, shared Mac lock, nice 19, two jobs, process
group deadlines, compiler hashes, actual access-unit clocks/keyframes and
independent decoding. Pair order alternates. Geometry choices are exact
1920×1080 and 3840×2160; a 2160p/60 source needs several GiB of private generated
pixels, checked before running. Foreground/channel errors and full/text/thin
per-frame SSIM regressions remain explicit; no gate is loosened for throughput.
Completion FPS is measured from producer receipts rather than nominal PTS, and
source-read/pacing lateness, latency, bytes and probe CPU/RSS remain separate.
The native watchdog allows twice nominal duration plus 20 seconds so a slow
control can complete the same frames; cadence remains a measured result.
These are component experiments, excluding OS helpers/GPU/power and full
application baselines, not permission to promote a default or claim 60fps
playback, text/codec admission, bandwidth savings or physical-device performance.

### Windows encoder component

`windows::VideoEncoder` is a thread-owned Media Foundation component, separate
from the standalone server and the application's host provider. It can request
hardware H.264 Baseline, HEVC Main and AV1 Main system encoders. Availability,
native execution, driver compatibility and performance are not established by
Windows-target checks from a Mac. This component supplies no account or host
authority. The consuming draft application now wires a Windows provider into
its shared admitted-view lifecycle; native Windows application qualification
remains required. See `results/2026-11-05-native-provider/README.md`.

Hardware registrations are enumerated explicitly. `EncoderObservation` separates
the registration class from D3D11 device input; neither proves active GPU work,
driver-internal copies, hardware decoding, power or measured encode latency.
`PreferHardware` tries hardware and then synchronous system-memory software
H.264, bounded to 1280x720 at 30fps and two requested worker threads. Software
HEVC/AV1 are not offered by this component. A supplied D3D11 device requires a
hardware path; GPU readback or changing resolution for CPU fallback belongs to
the embedding worker and is never hidden in encoder creation.

The API takes explicit NV12 colorimetry, sets output before input, requires low
latency/no B frames/CBR and two-second GOP requests, and checks the accepted
format. It closes on output format changes, incomplete output, clock changes or
invalid framing. Each keyframe must contain complete parameter sets; out-of-band
headers may supply an entirely missing set, never complete a partial update with
stale parameters. The framed output, not the requested media type, determines
the codec's actual profile for transport admission. Four-byte NAL lengths and
Annex B are supported; other native framing fails rather than being guessed.
AV1 uses the separately bounded low-overhead OBU path, not the NAL parser.

There is one submitted frame awaiting output and at most three retained GPU
surfaces. Asynchronous input/output credits and kernel events avoid a polling
loop. `wait_ready` and `wait_output` each cap a call at 200ms; an external
process watchdog is still required. A lagging or incompatible transform must
close and start a fresh generation rather than queue reference frames.

`with_device` checks D3D11 multithread protection and binds a DXGI device manager.
`encode_surface` requires an immutable NV12 texture on that device and completed
or synchronized producer work. Its unsafe contract requires keeping pixels
immutable until the returned `SurfaceLease` reports actual native sample
release, including after close. Output arrival alone permits no texture reuse.
The embedding worker must provide a bounded surface pool and handle submission
errors as documented. Device identity/format/size/subresource are checked.
The encoder alone provides no capture or conversion. Optional helpers described
below add those operations after explicit embedding admission. HDR, input
injection and a zero-copy driver claim remain outside this API. The synchronous idle hook runs immediately
before a genuine event wait, after queued notifications have been consumed.
Native callbacks contain no capture, application code or frame allocation.

`windows_codec_probe software|hardware-memory|hardware-surface h264|hevc|av1
<new-output>` generates the same 60-frame synthetic 1280x720 NV12 sequence with nominal
30fps timestamps and 2500kbps target,
forces keyframes at frames zero and thirty, checks capture timestamps and emits
per-frame bytes/submit-to-output time. It is unpaced. AV1 output is IVF; H.264/HEVC are Annex B.
The surface mode allocates/uploads a fresh immutable texture per frame: its
cost is fixture overhead, not production capture or a matched performance win.
It never overwrites a file, captures a screen or sends input. Build first and
run each native case under a process-tree deadline, then independently decode
all 60 frames and inspect actual profile, keyframes, color and dimensions.
Record CPU/RSS/GPU/adapter/driver separately before making performance claims.
See `results/2026-11-05-windows-mf/` for exact-source coverage and remaining gaps.

`windows::capture` adds Windows Graphics Capture monitor discovery and capture
on the monitor's own hardware adapter. Discovery starts no capture or prompt.
`DesktopCapture::open` must be called only after the embedder's current sharing
and authenticated view admission. It retains the system cursor and capture
border, uses two frame-pool buffers and preserves native QPC timestamps. Display
identity/geometry/color and the active input desktop are checked; resize, item
closure, lock/secure desktop or permission/device errors end capture rather
than retargeting it. HDR is rejected pending a qualified tone-mapping path.
This has no Windows input provider or authority of its own.

The thread-owned session uses native frame events and bounded waits, coalesces
up to two raw frames, and explicitly returns native buffers. A close handle may
wake it from another thread; native teardown stays on its owner. `CaptureStop`
also coalesces demand/recovery wake notifications without closing capture,
including notifications before the next native wait. Queued raw frames are
consumed before a pending control wake. Where the OS
supports a minimum update interval, the requested rate and accepted interval
are distinguished from measured cadence. Older systems leave the accepted
interval unknown; the embedding worker must still pace its consumer.
`set_frame_rate` reapplies the selected encoder rate, including bounded CPU
fallback. Display proofs include the exact opaque monitor identity, adapter,
name, size and SDR format; the handle is only queried through the OS. Raw-frame
observations expose actual coalesced count and optional QPC capture age. An
unavailable/inconsistent clock stays unknown instead of claiming zero latency
or a duplicated frame.

`windows::convert::Nv12Converter` checks progressive, zero-history GPU conversion
from SDR BGRA to limited-range BT.709 NV12 and scales to the requested output.
Automatic video enhancement is disabled. Eligible source bindings feed the
video processor directly; incompatible bindings use one explicit private BGRA
GPU copy. There is no CPU pixel readback on the device-encoder path. At most
three converted surfaces are retained, with all tracked encoder leases held
before any texture can be rewritten. A D3D11.4 completion fence finishes source
reads before a WGC frame is returned. Driver-internal copies and native resource
costs still need measurement. An explicit CPU-codec path reads back converted
NV12, without hiding RGB conversion or changing encoder geometry/selection.

Idle-desktop keyframe recovery can repeat only the last converted pixels while
checking the same live capture. Duplicate-frame timestamps advance by elapsed
monotonic time in the captured clock domain; they are not represented as newly
captured frames. This does not continuously encode stills. Multiple samples of
the same texture retain every release lease, and synthetic conversion clears
the captured-frame recovery state.

`windows_conversion_probe <new-nv12-output> [--shader-only]` is a prepared numeric
oracle using eight exact synthetic color bars, 1280x720 input and 640x360 output.
It selects/captures no display and sends no input. The second case requires the
explicit source-copy path; each checks interior Y/U/V against an independent
BT.709-limited matrix reference (three-code bound), records tightly packed NV12
and requires no visual inspection. Neither case has run natively yet. The
Windows-target check/Clippy evidence and unexecuted native plan are in
`results/2026-11-05-windows-capture/`. Cross-checking establishes no native
capture, color accuracy, encoder integration, performance or app-host pass.

API contracts: [Windows capture frame ownership](https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture),
[native capture interval](https://learn.microsoft.com/en-us/uwp/api/windows.graphics.capture.graphicscapturesession.minupdateinterval),
[video processor input bindings](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11videodevice-createvideoprocessorinputview).

`media/tools/desktop-workload.swift` generates an offscreen coding fixture at
30fps (default) or 60fps: a one-second prefix and eight observed seconds, split
into two-second still-text, typing, scrolling and window-movement phases. Its
arguments are `<new-output-directory> [width height [30|60]]`. Motion and typing
use wall time; the higher cadence renders intermediate motion positions rather
than repeating 30fps frames. It uses the installed Menlo font, fixed source text and
sRGB pixels, converted to full-range BT.709-matrix NV12. Its manifest records
geometry, color interpretation, font identity, source hashes and explicit text
regions. A separate original RGB crop retains small colored text/one-pixel
strokes before 4:2:0 conversion, so analysis can distinguish source subsampling
loss from codec damage. It captures no screen, opens no window and sends no input.
Use a new output directory and an external watchdog; never run unbounded probes.
The fixture and component metrics alone do not qualify codec/device admission.

`media/tools/analyze-text-workload.py` consumes independently normalized RGB
crops. It verifies all source hashes, a static original RGB region and rational
RGB-to-NV12 spot checks, then measures only source-derived ink plus a one-pixel
halo. Per-frame integer error sums determine aggregate and per-channel
regressions against the declared baseline; these do not prove every individual
pixel is no worse. The uncompressed 4:2:0-to-RGB control remains separate, so
source chroma loss cannot be credited to a codec. RGB crops must contain exactly
the requested observed frame count (default one source cycle: 240 or 480) with matching geometry/color
interpretation. Optional NumPy acceleration preserves exact integer metrics.
Run decoding and analysis under external process-group deadlines and retain
their commands.

`media/tools/run-matched-codecs.py` runs a bounded native Mac comparison at a
full clean source revision, with an owned target, shared build lock, nice 19,
two build jobs and process-group watchdogs. It builds the probe and runs strict
library/example Clippy, validates generated source hashes, then independently
decodes every frame and measures full/text/thin-region YUV SSIM and original
RGB foreground errors. Private bitstreams and raw numeric rows remain in the
new output directory; large RGB intermediates are removed only after analysis
and can be regenerated from the hashed inputs. For example:

```bash
python3 -B media/tools/run-matched-codecs.py --revision <full-clean-sha> \
  --output /tmp/unique-codec-comparison --analysis-engine numpy
```

Default H.264 Baseline 3000kbps and HEVC Main 1500kbps requests are comparison
hypotheses, not equal-quality guarantees or bandwidth caps. The default plan is
five independent runs, each with 30 seconds warmup and 300 seconds observation;
`--runs=1 --warmup-seconds=1 --observation-seconds=8` is a diagnostic only.
`--fps=60` selects the 1080p60 component profile; its fixture must contain 540
frames, rather than the default 270 at 30fps. A mismatched fixture is rejected;
there is no silent resampling. The 18,000-observed-frame/1,800-warmup-frame bounds
stay in place, limiting 60fps plans to 300 observed/30 warmup seconds. SSIM phases,
one-second payload windows, forced two-second keyframes and normalized timebases
follow the selected cadence. `--fixture` reuses an immutable matching fixture.
`codec_probe` cycles its one-second prefix during warmup and eight-second source cycle during observation,
while capture timestamps keep increasing. This avoids duplicating a large raw
input; repeated cycles within one session are not independent runs or a wider
content corpus. Packed NV12 planes use whole-plane reads; padded native planes
retain row reads. Timing excludes file read/fill from encode callback latency.
Optional case syntax `codec:tuning:kbps:speed` or `:quality` requests verified
hardware priority. Omit the fourth field for the unchanged default; an explicit
`default` alias is rejected. Compatible HEVC Main supports ordinary or
low-latency tuning requests. Profile/priority support is checked natively and
does not replace independent decoded quality and resource comparisons.

The [cadence contracts](results/2026-11-05-cadence-contracts/README.md) cover
parser/metric clocks and refused plans only. Swift generator byte compatibility
at 30fps and actual 60fps generation/encoding are pending; these contracts do not
qualify a device, codec profile or any performance target. Earlier result packs
retain their exact source and 30fps workload attribution.

Results distinguish Annex B payload, nominal frame clocks, encode-to-callback
latency and whole-probe CPU/RSS. They do not measure wire/relay billing,
input-to-display latency, app baseline, OS helpers, GPU utilization, power,
hardware decoding or physical endpoints. AV1 has no implementation in this Mac
probe. No target, production default, codec policy or application pin changes.

`results/2026-11-05-coding-workload/` records the first controlled 1080p30
coding comparisons, foreground metrics and native receiver replays. Single
short-clip byte/quality differences do not admit a device/profile. The sRGB
controls have unchanged before/after readback metrics; a BT.709-transfer canvas
control differs from its independent reference, so that observer cannot yet
qualify every source color space or diagnose the separate video compositor.

The encoder initializes its compression color properties from the first input
buffer's primaries, transfer function and matrix attachments. Absent tags remain
unspecified; source colorimetry is not replaced with fixed display assumptions.
Only copied strings are retained, with no extra source-frame or pixel copy.
Subsequent changes to any of those tags or the NV12 range close the encoder:
the caller must create a new generation rather than continue with stale signaling.
Unsupported native color properties fail explicitly. These session settings alone
do not prove the emitted bitstream's tags or receiver color conversion.

Exact-source before/after color results and all retained failed attempts are in
`results/2026-11-05-source-color/`. Initial explicit input attachments alone
were absent from all sixteen tagged output cases; setting native compression
properties preserves them in all sixteen final cases. Twenty-frame color bars,
four transition tests and flat-region numeric scores do not qualify text,
wide-gamut/HDR, receiver conversion or sustained performance admission.

`--pattern=color-bars --pixel-range=video|full --color-tags=none|bt709|srgb`
adds controlled color/range inputs. Sixteen bars include primaries, mixed colors
and gray levels, generated with the BT.709 YCbCr matrix in the requested 420v
or 420f range. Explicit tags use BT.709 primaries/matrix and either BT.709 or
sRGB transfer, attached before retaining the immutable input. The printed tags
are requests; inspect the actual encoded stream and independent decoded values
before claiming preservation. These options do not change the library or the
application's capture configuration. Defaults remain the earlier monochrome,
video-range, not-explicitly-tagged input. Color bars do not qualify readable
text, colored thin edges, wide-gamut/HDR, browser conversion or performance.

Summaries now state the nearest-rank percentile definition and nominal video
duration used for bitrate. The older 60-frame summaries selected conservative
upper order statistics (including index 57 for the p95 field); do not present
that instrumentation correction as an encoder speedup. Raw historical samples
remain available for reanalysis. The monochrome edge/bar workload is not a
readable-text/color-region or full-desktop quality qualification. No low-latency
mode or changed profile is automatically enabled in the application.


With `--features webrtc`, the sender uses the caller's Tokio runtime, sends only
video (H.264, HEVC Main or bounded AV1 Main temporal units) and selects AWS-LC crypto explicitly. Register the actual encoder
SPS profile/level, exchange offer/answer inside authenticated Connect, and keep
native media tied to that session. `open` retains the full gathered SDP path.
Opt-in `open_trickle` returns without waiting for gathering: exchange its
`VideoSignaling` candidates through the same authenticated channel, apply the
answer to the frame writer, and keep driving `next_update`. Each direction
accepts at most 64 bounded candidates. Closing wakes pending signal consumers. RTP timestamps follow
capture presentation time even when unchanged frames are skipped. PLI/FIR
requests are coalesced and rate-limited before the encoder forces an IDR.
Automatic trickle peers with TURN configured give a direct connection 250 ms
before starting native relay allocation. A connection that wins cancels that
allocation; an unconnected peer starts TURN at the deadline on the existing
driver timer. Full-SDP and Relay-only paths remain immediate. The SDK hook is
opt-in and defaults to zero. This provisional latency/cost tradeoff still needs
physical-device measurement. `results/2026-11-04-lazy-turn/` records zero native
LAN allocations and actual UDP/TCP/TLS fallback with direct packet paths blocked.
Browser viewers and credential lookup have separate remaining allocation/startup
work; these tests establish no measured CPU/RSS saving.
Live Automatic restart now preserves that 250 ms grace when introducing TURN on
a retained peer. Replacement updates the driver's delay together with adapter
addresses; RelayOnly and the distinct one-time startup fallback stay immediate.
`results/2026-11-05-lazy-restart/` reproduces three unused allocations before the
fix and zero after, plus forced-relay rotation and close-cancellation checks.
The companion actual Mac-to-phone case drops from two allocations to one with
real HTTP issuance, video recovery and revocation. This does not establish video
egress/CPU/RSS savings or physical handover. The later a355a95 pin vendors the
same-version core to fix empty-list replacement at the setter itself. The
getter becomes empty and the next gather stops contacting retired endpoints;
`results/2026-11-05-clear-ice/` retains the actual stale-Allocate baseline and
passing remove/re-add media regression, import audit and native integration.
Late incremental candidates explicitly wake the SDK driver so ICE can replace
its previous parked deadline. The identical controlled test fails without this
wake and passes with it while old waits stay held and new ICE timers may run;
see `results/2026-11-04-idle-candidates/`. Passive-TCP event ordering is retained.
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
TURN gathering now separates relay interfaces from host/STUN gathering. For
real UDP servers it prefers the OS-selected source only when that source is
actually bound; otherwise explicit alternate routes are preserved. TCP/TLS
adapters need one local IPC binding per address family. This avoids allocating
once per VPN/VM/interface in the usual topology while keeping all direct host
candidates. The preference is recalculated from actual sockets after a rebind,
adds no DNS query or packet, and does no work per media frame. The two-binding
coturn assertions passed at `a5e0f7cfe3906a3185a869ce70e8154693c3fc61`,
including nominated UDP/TCP/TLS media and preserved direct host candidates.
Raw results are in `results/2026-11-04-relay-fanout/`. Native resource
measurements remain required; no measured CPU/RSS saving is claimed.
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
The later interface-allocation checks below retain these earlier fixtures.
Opt-in trickle gathering is covered separately in `results/2026-11-04-trickle/`;
at exact `367b015b1b198caeaab4e35ee48e14780f8f18df`, all three forced-relay
trickle paths (UDP/TCP/TLS), a legacy forced-UDP regression, locked prebuild and
strict Clippy passed. Both wire SDPs omit candidates, requiring incremental
exchange in both directions. Each peer retains one relay allocation across two
bindings. Physical NAT traversal and credential renewal/ICE restart remain
required work before release.

`codec::select_codec` covers AV1, HEVC and H.264. It prefers a hardware pair in
caller preference order, then allows only explicitly permitted, measured
real-time software fallbacks. Unknown capability is never interpreted as
support. This is selection policy, not an AV1/HEVC encoder implementation;
providers must advertise only paths they actually implement and have probed for
the requested geometry, fps, profile and chroma. The native encoder and WebRTC sender implement H.264 and HEVC Main.
`hevc_main_profile` reads the actual SPS profile/tier/level after removing
emulation-prevention bytes; `VideoFormat::HevcMain` carries those parameters
into RFC 7798 SDP and selects its own RTP payloader. The sender rejects a frame
tagged with a codec other than the negotiated format. Native synthetic HEVC
encoding and independent decoding passed. The production phone WebKit receiver
on the owned iOS Simulator also presents real hardware H.264/HEVC and software
H.264 video; see `results/2026-11-04-native-webkit/`. Physical-device capability
negotiation and full application runtime remain required before broad HEVC use.
Embedding viewers now discover local receiver formats and HEVC hardware
availability, with H.264 compatibility and one bounded startup retry. This is
currently an availability-based candidate policy; measured per-device latency,
quality and resource-budget admission remains unimplemented and required before
release. See the native result notes for the distinction.


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
Operation-specific errors identified the hardware-status query. The narrowly
scoped software-only correction now passes actual software H.264/HEVC encoding
and independent decoding on this Mac; all failed and passing logs remain in
`results/2026-11-04-macos-codec-matrix/`. Sustained CPU fallback performance on
other devices remains required. The probe now uses the callback wait and measures
arrival before stream/profile output; its earlier polling samples retain their
original scope and are not a controlled latency-saving comparison.

Run the same native synthetic workload with `codec_probe hardware /tmp/probe.hevc hevc`
for HEVC. Hardware H.264/HEVC now pass native synthetic encoding and independent
decoding; see `results/2026-11-04-macos-codec-matrix/` for raw results and the
initial software status-query failure and successful correction. Matched-quality
savings remain unmeasured.

The HEVC transport loopback separately requires intact type-19 and type-20 IDR
payloads, capture timestamps, keyframe feedback, direct selected candidates,
byte counters and bounded close. Like the H.264 loopback, it uses synthetic NAL
bytes and proves neither valid native decoding nor browser compatibility.
Profile signaling follows [RFC 7798](https://www.rfc-editor.org/rfc/rfc7798.html).


Late startup credentials can now use `WebRtcVideo::begin_relay_fallback` after
an initially STUN-only/server-free Automatic trickle peer accepts its first answer but
remains unconnected. The embedder owns grace, credential fetch, authenticated
view/generation envelopes and signaling-task replacement. The API performs one
bounded ICE restart with stable DTLS identity; superseded candidate handles and
answers cannot feed it. The driver acknowledges cancellation of old STUN
DNS/retries and drains old callbacks before advancing the generation, so an
unreachable discovery service does not hold fallback behind its timeout.
STUN DNS now runs as independently polled discovery work: an unanswered lookup
cannot hold local candidates or direct packet handling. Lookup completion wakes
the existing driver channel, and cancelling a gathering phase drops its lookup
futures before the restart acknowledgement. No new periodic timer or dedicated OS thread
is used. The existing resolver may still use Tokio blocking-pool workers;
dropping its future does not promise to stop an in-flight OS lookup. The older interrupt-channel evidence below remains historical.
It does not implement credential renewal or application coordination. Raw control-datagram event
pumping also fixes a first-SRTP-packet failure exposed by a one-sided TURN
baseline. Exact pre-fix failures and the unchanged passing eight-case scope at
5229dac are in `results/2026-11-04-late-relay-fallback/`. No application startup
or resource saving is claimed by those synthetic transport tests. The follow-up
STUN-first scope passes ten cases and strict Clippy at c7eb4dc; exact evidence
and limits are in `results/2026-11-04-stun-startup/`. The follow-up at f223998 passes fifteen selected Linux transport cases,
three new native macOS DNS cases and ten standalone SDK mock-runtime cases;
strict production library/examples Clippy passes. The identical direct-video
test fails on c022bfc before the production change. Graph distinctions,
metadata-download timeout, commands, source/cleanup and limits are retained in
`results/2026-11-04-stun-dns/`. The embedding host owns cached-STUN startup,
bounded credential lookup, authenticated generations and viewer coordination;
its full-application evidence belongs with that application. No whole-endpoint
resource or cost saving is established by these synthetic transport cases.

At `2ae3179905ebbf1a343b66fb3db596dadc79d6d6`, `ice_connected()` exposes ICE
reachability before DTLS/sender transport readiness. It must only retire optional
startup work, never authorize RTP or input. Terminal connection state cannot be
revived by late callbacks. Startup restart refuses ICE that becomes reachable
before or while old discovery is quiesced; the embedder resumes its old handle.
Proven srflx/prflx candidates nominate without the SDK's extra 500/1000 ms waits.
Relay nomination is immediate for STUN-only deferred peers and forced-relay
tests, avoiding a second delay after the embedder's direct grace. Legacy eager
TURN peers retain their relay nomination preference.

Six exact Linux callback, H.264/HEVC UDP/DTLS, STUN and closure cases pass with
locked metadata and strict library/examples Clippy. The initial fc15 source
fails on deprecated `AtomicU8::fetch_update`; its test build and six cases do
not run. The stable compare/exchange fix and final results are retained in
`results/2026-11-04-ice-nomination/`. These checks establish no native decoder,
quality, physical route, sustained resource or performance-budget result.


`WebRtcVideo::begin_ice_restart` provides a serialized live restart for trickle
peers. It retains the peer, DTLS certificate, codec, SSRC and RTP capture clock;
answers/candidate handles are bound to a monotonically increasing wire
generation. The caller must mark reconnecting, release input/control, carry
signaling inside its authenticated view, enforce a recovery deadline and
revalidate authority. `VideoUpdate::Disconnected` gates sending without making
that callback terminal; Failed/Closed and explicit close remain terminal. No
input authority or application credential timer is created by the media API.

Live restarts accept the original explicit ephemeral bindings and route policy;
they do not replace a host interface list. The driver acknowledges discovery
retirement and sends old TURN releases under their original username/socket
before the generation changes. Trickle restarts discard stale candidates and
rebind UDP sockets, allowing a new TURN username to obtain a fresh allocation
instead of trying to refresh the old allocation under another username (which
[RFC 8656 section 6](https://www.rfc-editor.org/rfc/rfc8656.html#section-6)
forbids). TCP/TLS adapters, their local IPC allowlist and allocation observations
are replaced rather than accumulated. Retired callbacks cannot overwrite or
fail a newer allocation that reuses the same relay address. The existing
STUN-only startup API still permits only its initial pre-connection fallback.

The six-second API bound closes the peer if negotiation preparation is cancelled
or fails after mutation; invalid preflight input leaves the current peer intact.
Only one offer can await its answer. Full-SDP callers keep their existing
non-restartable contract. The embedding host/phone/desktop lifecycle, live
credential refresh scheduling, interface changes and physical handover remain
required integration work; those callers have not adopted this API yet.


Exact live-restart evidence is in
[`results/2026-11-05-live-ice-restart/`](results/2026-11-05-live-ice-restart/).
Five selected Mac cases and three actual loopback coturn cases pass at
`af6478e446c0c05a98645ff54df088f826947945`, including the stronger fresh-readiness
oracle that fails before its seven-line production fix. Earlier weaker passes,
compile/setup failures, cleanup and missing application/physical/resource
coverage remain separately attributed. No caller pin or performance target is
changed by this qualification.


`WebRtcVideo::send_with_stamp` is an additive version of `send` returning the
actual RTP timestamp and signaling generation only after the complete access
unit succeeds. Owners can authenticate a new keyframe boundary alongside ICE
signaling and compare it to decoded per-frame RTP metadata, without guessing
from an old video-track callback or aggregate decoder counters. The existing
`send` method keeps its unit return and packet/clock behavior. No new per-frame
buffer, timer, dependency or authority is added. A receipt proves local send
completion, not remote decode, display, hardware acceleration or quality.

Exact receipt qualification is recorded in
[`results/2026-11-05-frame-receipts/`](results/2026-11-05-frame-receipts/):
three selected native Mac synthetic transport tests and strict library/test/example
Clippy pass at `6279c1f57fb268945ea9567643eb8e4331411091`. Tests compare
receipts with received RTP headers through clock wrap and three live ICE
restarts; pending replacements issue no receipt. This does not establish
native decode, physical network, resource budgets or new TURN coverage.

## AV1 transport and qualification

`av1::Av1Frame` / `WebRtcVideo::send_av1_with_stamp` carry sized low-overhead OBU
temporal units separately from `EncodedFrame`'s Annex B NAL units. Register
`VideoFormat::Av1Main(av1_main_profile(first_keyframe)?)` from actual encoder
output. Units are bounded to 8 MiB / 256 OBUs, one picture and one operating
point/layer. Complete OBU_FRAME or frame-header/tile-group layouts are supported.
Still-picture, timing/HRD and scalable-layer modes need explicit support. Every
random-access unit includes its sequence header; profile/level/tier changes
require new negotiation. Framing checks do not validate every AV1 syntax bit.

The existing RTP library performs AV1 aggregation/fragmentation; capture clocks,
marker placement, finite write deadline, keyframe feedback, route statistics and
terminal close use the same bounded writer. The answer's actual AV1 profile,
level and tier limits are checked before media admission, including the RTP
parameter defaults. This adds no dependency or crypto-provider feature.

This transport does not add an AV1 native encoder or advertise AV1 from an
application provider. The current VideoToolbox encoder remains H.264/HEVC; AV1
needs actual encoder/decoder admission, native application integration and
measured quality/resource/latency qualification before automatic selection.
CPU AV1 fixture generation does not admit software AV1 for interactive use.

References: [AV1 RTP payload format](https://aomediacodec.github.io/av1-rtp-spec/),
[approved AV1 bitstream specification](https://aomediacodec.github.io/av1-spec/av1-spec.pdf),
[SVT-AV1 parameters](https://github.com/AOMediaCodec/SVT-AV1/blob/main/Docs/Parameters.md).

### Network-control qualification

`WebRtcVideo::open_controlled` and `open_trickle_controlled` accept validated
`SendControl::new(min_kbps, initial_kbps, max_kbps)` limits. They are a separate
qualification path while the complete application loop is being admitted.
Existing constructors retain their ordinary behavior. The controlled path uses
the pinned SDK's GCC/TWCC algorithm and token bucket, with a rate-dependent 5 ms
RTP burst budget floored at one 1500-byte packet and capped at 6000 RTP bytes,
256 total held/released RTP packets and a 500 ms packet/frame deadline. It waits
for paced release every sixteen original packets and at each frame boundary;
accepting a driver command alone cannot finish a controlled send. Packet loss
inside the pacing queue is terminal, never a successful partial encoded frame.
Close retires queued RTP through the same synchronous authority flag. A cancelled
frame-send future now closes either sender mode, preventing reuse after a partial
access unit. These limits are resource bounds, not latency/performance admission.

Current-generation NACK retransmissions retain pacing without falsely finishing
a producer send. The SDK buffers raw RTP, so a bounded 1024-entry identity map
restores the private ticket lost at that boundary. Restart retires that map,
queued packets and old-path feedback; fresh authenticated keyframe/generation
rules still belong to the embedder. The sender additionally refuses a delta as
the first completed access unit of any generation, in either mode. A target is unknown until feedback exists;
a GCC target may still reflect its initial prior until enough reports arrive.
`bitrate_estimate` includes its age, and `VideoUpdate::BitrateTarget(kbps)` reserves
15% of the pacing target for packet overhead/retransmissions, floored at the
encoder's existing 250 kbps minimum. The embedder must enforce the original
viewer ceiling and apply updates on its codec worker. No new thread or recurring
idle polling is introduced. This path is not yet application default admission.

`macos::VideoEncoder::set_bitrate_target` retunes future submissions on the same
session, bounded by the original configured target. It requires exact typed
integer native readback; a partially applied or unverified change is terminal.
Hardware selection, color, geometry and frame clock do not change. Previously
submitted frames may retain the old target. The initial `bitrate_target_kbps`
getter is the configured request, not an initial native observation. An average
encoder target is not a hard network ceiling or measured quality/byte saving.
