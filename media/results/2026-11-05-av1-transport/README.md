# AV1 transport foundation and native receiver evidence

Runtime media/SDK/core: `636e04d62998883ccd8a6cf824e812a18b56dac8`.
Fincode pin/check: `a93bb74bf26ad37d8a5492d279560e355433ccdd`.
Phone fixture: `f64d3611bb91b6350bb704059e0839ae809e304d`.

The custom media package now has a separate sized-OBU AV1 API and RTP payloader.
It bounds temporal units to 8 MiB / 256 OBUs, checks complete single-picture
framing and one operating point/layer, extracts Main profile level/tier from
random-access data, and checks the answer's asymmetric receiver limits before
admission. Every random-access unit needs its sequence header. The shared RTP
writer retains capture timestamps, fragmentation/markers, finite write deadline,
keyframe feedback, route observations and terminal close. Full AV1 syntax,
geometry, encoder/decoder resource admission and quality are separate concerns.

| Scope | Exact source | Result |
| --- | --- | --- |
| Initial AV1 build/filter | media `147c0eb494302d094ff2a47ba0a1984b33b4e378` | Check/build pass. 5 tests pass; the receiver-limit fixture fails because it omitted the SDK-required event handler. No later steps count as run. |
| Corrected transport filter + shared writer | media `d992e60bd30227696717fa77dbc25edb3a43ebd4` | 6 AV1 tests pass, including fragmented real synthetic encoded OBUs over UDP/DTLS/SRTP, wrap/capture clock, PLI, direct route, malformed input, lower receiver-limit refusal and close. H.264/HEVC loopbacks each pass (8 total). Strict test Clippy then fails on an unnecessary test-data Vec; no production failure. |
| Final strict Clippy/builds | media `636e04d62998883ccd8a6cf824e812a18b56dac8` | Library/examples/tests strict Clippy and both selected example builds pass. The correction replaces only an intermediate test-data allocation; transport/native codec/SDK/core production code is unchanged from the preceding passing runtime. |
| Positive native AV1 replay | final media + phone sources above | **0 passed, 1 failed, 0 skipped.** No AV1 in this iOS Simulator 27.1 WebRTC receiver capability list. Bridge answer fails with WKErrorDomain Code 4; the sender receives no answer and exits 1 after its deadline. No decoded AV1 frame or native AV1 interoperability is qualified. |
| Separate native H.264 / forced HEVC replays | same final media + phone sources | One selected Swift case passes per codec (2 total, no failures/skips). 60 frames sent per case; 59 / 60 presented at 1280×720, direct media with positive sent-byte counters, zero missed frame callbacks. Phone production source trees are unchanged. |
| Application pin | Fincode source above | desktop_media/browser/settings_ui Mac library check and strict Clippy pass, with dependency/runtime guards. Locked resolution selects the three paired custom-fork packages; only their source IDs change. Versions/features/defaults stay unchanged; RTC/SDK/Rustls retain AWS-LC. This is not a claim that the whole workspace has no independent Ring dependency. |

The Mac's non-prompting inventory reports AV1 hardware decoding availability,
but its VideoToolbox encoder list contains no AV1. The Simulator separately
reports AV1 and HEVC hardware availability false, and no AV1 WebRTC codec.
Neither establishes the active decoder's GPU identity or physical-phone support.
Native AV1 encoders, usable endpoint receivers, measured admission and automatic
application selection remain unfinished. The application still selects from its
established H.264/HEVC capabilities; no native AV1 encoder is added by this pin.

The original generated AV1 fixture has 20 320×180 8-bit YUV420 frames at 10 fps,
independently decoded by libdav1d with frame hashes. Independent frame metadata
confirms random-access frames 0/10 and displayed 320×180 dimensions. SVT-AV1
4.2.0 maps requested preset 12 to 11; the full encoder warnings/configuration
are retained. This tiny pre-encoded signal is not a CPU fallback, codec quality,
bitrate saving, sustained-resource or latency qualification. Short frame
processing histograms from the replays do not qualify any performance target.

The packs retain 88 hashed text exports, 59 independently absent process groups
and three rebound HTTP ports. Both native attempts independently confirm the
owned Simulator is Shutdown. Raw SDP/ICE, media buffers and xcresult attachments
stay private. Numeric diagnostic labels in two FFmpeg logs use words instead
of hash prefixes to avoid GitHub issue cross-references; their original hashes
and the export transformation are recorded. The initial helper's pre-execution
Python syntax failure is retained and supplies no native coverage.

Runs used frozen clean sources, the shared build lock, nice 19, two build jobs,
owned targets/Simulator and external process-group TERM/KILL/reap deadlines.
No formatter, screenshots, visual verification, fresh Windows/cloud execution,
VPS deployment, secret rotation or merge is part of this checkpoint. The full
cross-platform desktop/backend/performance objective remains open.
