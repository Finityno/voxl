# Native encoder failure causes and prompt wake

Measured source `f098e462a62833b73d6a6103e4f50ff810fb7ffb`, Apple M5 Pro,
macOS 27.2. The Mac encoder retains its first terminal cause in a synchronized
one-shot state and wakes a waiting worker through the existing bounded output
channel on failure. A full queue already has output available to wake the worker;
terminal checks prevent publishing queued deltas. Later encode/output calls
still return Closed, and `failure_cause()` supplies the first diagnostic. The
probe reports that cause. Queue/pending bounds and fail-closed policy stay intact;
no thread, external dependency, codec default or application pin changes.

The release probe build and strict library/example/tests Clippy pass. Three
selected callback tests cover native status/dropped/null-sample failure delivery,
first-cause retention, queue saturation and panic containment. Four existing
selected color-configuration tests pass. Hardware H.264/HEVC and software H.264
default controls independently decode matching old/current hashes, 32 generated
320×180 frames per version and pair (192 frames total). These controls do not
qualify performance, text quality or production CPU fallback.

The same low-latency HEVC fixture/1080p30/1500-kbps request that previously
reported Closed now reports `Platform { operation: "encoder dropped output",
code: 2 }`. Only the first frame completes. The preserved one-frame prefix
independently decodes; the planned 270-frame case fails and remains unqualified.
The dropped-output callback is identified, but its underlying driver/rate-control
reason is not established. No timing/resource improvement is inferred by
comparing failed runs, and no missing observation becomes a zero/passed result.

The first supervisor exits 1 during postprocessing because its parser starts at
byte zero instead of skipping the six-byte VXFR header. It retains the successful
build, Clippy, seven selected tests, default controls and native failure. A separate
bounded completion corrects the parser, verifies original artifacts/group absence
and decodes the retained prefix; it exits zero without rerunning the encoder or
tests. Both receipts and helpers remain. All **22 owned command groups** are
independently absent, including the failed codec case. This does not count them
as 22 passing commands or prove shared OS-helper teardown.

`index.json` hashes normalized-path JSON/helpers and deterministic gzip logs;
original hashes remain distinct. No bitstream, binary, screen/decoded image,
font, credential or private application source is included. App integration,
1080p60, full resources/GPU/power, native Windows/Linux/physical phone/LAN/NAT/
input and wire/relay/deployment qualification remain open. Targets stay unchanged.

A subsequent consuming application draft pins this measured source for the
failure-wakeup behavior. Its own Rust/native validation is pending; it does not
request the experimental priority/tuning modes. Component checks here do not
qualify that updated application.
