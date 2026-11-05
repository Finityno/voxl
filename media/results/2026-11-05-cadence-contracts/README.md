# Cadence-aware evidence contracts

Exact source `2dd18efdfe6f4cd98966045a6fbb2ba14503c936` extends the offscreen
workload, matched runner and foreground analyzer from default 30fps to explicit
30/60fps. Prefix/cycle lengths, two-second keyframes/phases, sample clocks,
one-second payload windows, timebases and source mapping follow the requested
cadence. The 18,000-observed-frame and 1,800-warmup-frame limits are unchanged.
60fps plans therefore allow at most 300 observed and 30 warmup seconds.

Seven parser tests and six metric tests pass, zero skipped. New cases cover the
60fps capture clock/payload windows, two-second SSIM phase boundaries, consistent
manifest counts and cyclic-source mapping without repeating capture timestamps.
Existing scalar/NumPy, per-channel and malformed-stream contracts still pass.
These use tiny synthetic bytes, not encoded/decoded pictures or native codecs.

Three CLI plans refuse for their intended reasons before an output directory,
build or encoder can start: unsupported 120fps (exit2), 301 observed seconds at
60fps (exit1), and 31 warmup seconds at60fps (exit1). Expected refusals retain
their actual nonzero exit codes. All six owned command groups are independently
absent; that does not count all six as passing native commands.

`summary.json` retains scopes, original hashes and runtime-source identity with
the previously measured failure-wakeup source. Media Rust, Cargo manifest/lock
and vendored trees are unchanged; only tools/tests/docs change. `results.json`
and normalized helper/gzip logs retain commands and process-group deadlines.
`index.json` hashes published records. No font, screen, rendered/decoded image,
bitstream, credential or private application source is published.

Swift generator compilation, default30 pixel/hash compatibility, generated60
assets and actual 60fps native encoding/decoding/quality remain **unexecuted**
at this checkpoint. The ongoing 30fps five-session measurement stays frozen at
its earlier exact source and is not disturbed. No result here establishes
hardware support, decoder identity, app/resource/power/wire budgets, production
profile admission or a default/target/application-pin change.

The subsequent [native generation and short 60fps pack](../2026-11-05-cadence-native/README.md)
verifies the previously unexecuted generator/default30 controls and native
codec pilots at its later exact source. It does not rerun or reattribute these
13 parser/metric tests, and the measured HEVC foreground regressions prevent
profile promotion.
