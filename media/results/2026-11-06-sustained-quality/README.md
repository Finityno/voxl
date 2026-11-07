# Sustained quality-priority HEVC pair and native capability inventory

The [short quality-priority pilot](../2026-11-06-hevc-quality/README.md) identified
HEVC compatible/8000/quality as a candidate that removed the measured scrolling
regression at lower requests. This follow-up uses **30 seconds warmup and 300
seconds observation** per fresh session at generated 1920×1080/30fps. Exact
encoder source remains `8a4a52bf66ec46da5a2bb323c6b56f92e0bd1b9b`; its previously
built/checked release executable is hash-verified before and after, with no
repeated build or Clippy. Both codecs decode all **9,900** frames independently.

Host: Apple M5 Pro, Mac17,8, macOS 27.2 build 26B5101f. The immutable generated
clip, full-range NV12/color tags, two-second forced keyframes and source cycle
are identical. Repeated eight-second cycles are not independent content.

| Request | Observed payload Mbps | Encode callback p95 ms | Whole-probe CPU, one core % | Peak probe RSS MiB |
| --- | ---: | ---: | ---: | ---: |
| H.264 compatible/3000 | 1.036773 | 12.236 | 3.60 | 40.48 |
| HEVC compatible/8000/quality | 0.905449 | 14.991 | 3.46 | 40.58 |

HEVC uses **12.67% fewer encoded payload bytes** in the observed window. All
**9,000** paired observed text-SSIM values improve, and no frame's foreground
aggregate or individual RGB-channel error sum is worse. This does not prove
every pixel/glyph or human legibility. The measured callback p95 is slower;
the saving here is 12.67%, compared with 15.37% in the short pilot.
Actual bytes differ from the requested rate, which remains no output ceiling.

This is **one** sustained session per profile. Five independent sessions and a
broader content corpus remain required. Small CPU/RSS differences are not an
established resource improvement. Process CPU includes initialization, file I/O
and warmup; RSS is not physical footprint. GPU/OS helpers, viewer, baseline
subtraction, power, capture, wire/relay billing and actual presentation/input
latency are outside this encoder component. No 60fps, full-app resource or
codec admission follows; this 30fps run does not establish the optimistic
3ms encode target for the separate 1080p60 profile.
No consuming pin, tuning/default or performance target changed.

## Native hardware inventory

At separate exact tool source `8ea15b1496231eaed74062c8fba856f90b31f25f`,
`media/tools/macos-codec-capabilities.swift` compiles for macOS 12.3 arm64 using
Swift 6 strict concurrency and warnings-as-errors. The native query and output
shape/count/unknown validation pass. It uses the public VideoToolbox encoder
list and hardware-decode flag. No session, pixels, UI, input or network are used.

| Codec | Registered encoders | Explicit hardware encoders | Entries without hardware flag | Hardware decode reported |
| --- | ---: | ---: | ---: | --- |
| H.264 | 2 | 1 | 1 | true |
| HEVC | 2 | 1 | 1 | true |
| AV1 | 0 | 0 | 0 | true |

This Mac reports AV1 hardware decoding but **no registered VideoToolbox AV1
encoder**. Other APIs/software are unprobed. Unmarked entries are not assumed
to be software; when none reports hardware and any lacks the flag, the tool
returns unknown/null. Inventory flags do not prove resources remain available,
an exact profile/geometry/rate/color works, a browser selects hardware, or an
application provider admits that codec. The current Mac provider still does
not advertise AV1. Encoder and viewer capabilities remain separate.

To compile the inventory on another Mac, use an installed Xcode SDK:

```sh
xcrun --sdk macosx swiftc -O -swift-version 6 -strict-concurrency=complete -warnings-as-errors media/tools/macos-codec-capabilities.swift -o /tmp/macos-codec-capabilities
```

Run the executable under an external process-group deadline; the recorded
native execution uses 30 seconds. Collect only its JSON, not a personalized
machine inventory. The SDK's `VTVideoEncoderList.h` and
`VTDecompressionSession.h` define these metadata semantics.

## Evidence

`summary.json`, `results.json.gz` and `observations.json.gz` retain numeric
samples, independent decoder metadata, per-frame text/foreground evidence,
commands, exits and source attribution. `inventory.json` and its qualification
retain the separate native metadata result. `python3 -B verify.py` recomputes
payload, callback p95, process CPU, foreground channel/aggregate comparisons
and text comparisons, and verifies indexed file hashes. It does not decode
video or turn these metrics into a profile admission.

All **18** recorded command groups (11 sustained, seven inventory) are
independently absent after completion, with no timeout or cleanup signals.
Native cases retain the shared lock, nice 19 and external group deadlines;
the reused encoder build used two Cargo jobs. The inventory compiler ran only after both encoder
processes exited, so it did not overlap their timing windows. The prepared
user app was closed; no application was launched or killed by this work.

Published paths are normalized. The index identifies the owner-only archive
containing original commands/logs, bitstreams, SSIM rows and qualifier scripts
by manifest hash. No rendered image, font, bitstream or private application
source is published. Windows/physical-device/manual visual testing and hosted
backend deployment remain separate unfinished qualification.
