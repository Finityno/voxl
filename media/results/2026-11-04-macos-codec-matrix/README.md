# Native Mac codec matrix

At exact `5bfd1c638b39347ab224e9aacb4e870c6434ab1f`, native release example
compilation passed. The hardware-required H.264 and HEVC sessions each encoded
all 60 deterministic NV12 frames at 1280x720 and 30 fps. Independent ffprobe
read all 60 decoded frames, and ffmpeg compared each output with the same raw
reference. Both hardware-status queries returned true.

| Hardware codec | Encoded bytes | Encoded kbps | Probe encode p50/p95 ms | Whole-frame SSIM |
| --- | ---: | ---: | --- | ---: |
| H.264 Baseline | 64,266 | 257.1 | 4.72 / 7.68 | 0.999721 |
| HEVC Main | 54,402 | 217.6 | 4.67 / 7.69 | 0.999697 |

These are single two-second synthetic runs at the same 2,500 kbps rate setting,
not a matched-quality comparison or a prediction for full desktops. The probe
polls every millisecond, so its latency includes polling and file-write overhead.
It measures neither capture-to-display latency nor native application behavior.
The raw time logs cover the probe process only, excluding system codec helpers,
GPU work and the application; they do not establish CPU or memory savings.
Sustained five-run performance targets remain unvalidated.

Both software attempts exited 1 after session preparation, specifically at
`query hardware encoder` with `kVTPropertyNotSupportedErr` (-12900). They encoded
zero frames. Apple documents that explicitly setting EnableHardware to false
prevents hardware encoding, and describes querying UsingHardware after enabling
hardware. The subsequent source change accepts only this unsupported-property
result for the explicitly software-only session. Other query failures and
hardware-required verification remain strict. A new native run is required to
establish software encoding; these failed attempts are retained here.

References: [hardware selection](https://developer.apple.com/documentation/videotoolbox/kvtvideoencoderspecification_enablehardwareacceleratedvideoencoder),
[status query](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_usinghardwareacceleratedvideoencoder).
The matching installed SDK VTCompressionProperties.h and VTErrors.h were read.

`5bfd1c6/artifact-pack.json` preserves every command, exit, watchdog, raw log,
source and output checksum. The harness asserted clean exact source before and
after every step. It used the shared Mac build lock, nice 19, two Cargo jobs,
an owned target directory and process-group deadlines. Host identity was
recorded immediately afterward, separately from the probe's environment log.
No screen capture, personal pixels, Windows use or visual inspection occurred.


## Successful software fallback retest

At exact `fec7f8f3989b047bba7e9daa9ef53475d9f7a707`, release example compilation
and strict native library/examples Clippy passed. All four encoder modes
produced 60 correctly timestamped 720p30 frames, each independently decoded by
ffprobe and compared against the same reference with ffmpeg. Hardware-required
sessions queried true; software sessions used the explicit hardware prohibition.

| Codec / mode | Encoded bytes | Encoded kbps | Probe encode p50/p95 ms | Whole-frame SSIM |
| --- | ---: | ---: | --- | ---: |
| H.264 hardware | 64,266 | 257.1 | 4.68 / 6.23 | 0.999721 |
| H.264 software | 97,077 | 388.3 | 6.94 / 8.87 | 0.999886 |
| HEVC hardware | 54,402 | 217.6 | 4.70 / 7.68 | 0.999697 |
| HEVC software | 61,645 | 246.6 | 12.18 / 22.49 | 0.999302 |

The previous failures are preserved above. These results establish this Mac's
software fallback and native HEVC encoder, including valid independently decoded
output. They do not establish other Macs, browser/phone HEVC negotiation,
sustained performance, coding-text ROI quality, CPU/power targets, full-desktop
bandwidth predictions or matched-quality savings. Software HEVC is probed for
comparison but is not the application's CPU fallback; that remains H.264 capped
at 720p30. The probe still polls at 1 ms and includes file/profile-output work in
latencies, so its timings must not be conflated with pure encoder latency.
`fec7f8f/artifact-pack.json` retains every raw log and command under the same
shared lock, priority, watchdog and source-cleanliness controls.
