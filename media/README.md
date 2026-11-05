# Embedded desktop media

This crate is maintained in the Fincode custom fork. It has no dependency on
Voxl's process-owning server entry point, web asset build, libx264 or audio stack.
It currently provides a macOS VideoToolbox H.264/HEVC encoder, not a complete desktop
stream. Screen capture and native application integration belong to the embedder. An
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

Give the encoder immutable NV12 buffers from the capture API. The hardware-first
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

`codec_probe` also accepts `--width=N --height=N --fps=N --bitrate-kbps=N`
within the encoder's normal bounds. Defaults remain 1280x720, 30fps, 2500kbps.
`--input-nv12=PATH` reads a regular, non-symlink, tightly packed NV12 file directly
into each native buffer's active planes, before the timed encode interval.
Its exact byte length must match `(warmup + observed) * width * height * 3 / 2`
before output files are created. It cannot combine with a generated color-bar
pattern. Supply the actual file's range/color tags explicitly; the probe does
not infer them from a filename. Partial reads fail and release the native buffer.
Input read/generation costs are not production capture or endpoint CPU evidence.

`media/tools/desktop-workload.swift` generates a fixed offscreen coding fixture:
30 warmup and 240 observed frames, split into still text, typing, scrolling and
window movement at 30fps. It uses the installed Menlo font, fixed source text and
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
halo. Per-frame integer error sums determine regressions against the declared
baseline. The uncompressed 4:2:0-to-RGB control remains separate, so source chroma
loss cannot be credited to a codec. RGB crops must contain exactly the 240
observed frames with matching geometry/color interpretation; run decoding and
analysis under external process-group deadlines and retain their commands.

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
video (H.264 or HEVC Main) and selects AWS-LC crypto explicitly. Register the actual encoder
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
