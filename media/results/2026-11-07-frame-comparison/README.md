# Exact CPU and Metal frame comparison

Measured source: `69c924898ae38678de4fef888938f8b0e62d1b2a`. This is a standalone,
memory-only experiment, with no library or application behavior change.

The CPU path compares every active NV12 byte with a tight owned snapshot and
copies active rows when changed. The Metal path maps the two native planes,
compares and updates a private GPU snapshot, then reads a four-byte flag after
command completion. It includes mapping, submission and completion overhead in
wall time. Both ignore row padding. Neither uses a hash or sampled equality.

All seven patterns pass at 1112 × 720, 1670 × 1080 and 3840 × 2160: unchanged,
changed padding/strides, first/last luma bit, first U/last V bit and full change.
The small byte-conversion oracle covers all 256 values in each component and
reads the complete GPU snapshot back for exact comparison. Reset, epoch,
geometry, pixel range, unsupported input and byte-cap cases also pass. Pixels
remain in memory; there is no capture, UI, input, image file or stream.

## Short Mac observations

Host: Apple M5 Pro, 18 CPU cores, 24 GiB RAM, macOS sysctl product version 27.2
(build 26B5101f), Apple Swift 6.4 in Swift 5 language mode, optimized with
warnings denied. All commands run at nice 19 under the shared build slot and
external process-group deadlines. The slot is not a machine-wide idle guarantee:
the pre-run load averages are 15.22 / 10.33 / 12.30.

The matrix has 126 rows: two backends, seven patterns, three geometries and
three **within-process** repeats, each with ten warmup and 60 measured calls.
Backend order alternates by repeat. A separate explicit CPU-only process has
21 rows with 20 measured calls each. These are unpaced calls on two repeatedly
used immutable generated surfaces, not independent process repeats or a
sustained 30/60 FPS capture workload.

For unchanged frames, the median of the three per-repeat wall p50 values is:

| Geometry | CPU | Metal | Owned snapshot allocation, either path |
| --- | ---: | ---: | ---: |
| 1112 × 720 | 32.208 µs | 259.542 µs | 1,212,416 bytes |
| 1670 × 1080 | 71.958 µs | 447.042 µs | 2,719,744 bytes |
| 3840 × 2160 | 309.208 µs | 540.375 µs | 12,451,840 bytes |

CPU has lower wall cost for all 21 geometry/pattern comparisons when using
the median of the three p50 values. Metal reduces measured process CPU for
some larger cases: unchanged 4K takes 92.383 µs of process CPU per call versus
309.417 µs for the CPU path, again medians of three repeats. It still takes
longer wall time and uses GPU resources that process CPU does not count.
GPU utilization, power and driver/helper attribution are unmeasured.

Tail behavior remains material. The largest per-row p95 is 829 µs for CPU
and 6322.750 µs for Metal; the largest individual wall samples are 8919.375
and 12380.208 µs respectively. Metal setup takes 520418.209 µs in this process.
The complete raw distributions remain in the compressed numeric artifacts.

Peak process RSS is 51,265,536 bytes in the CPU-only control and 206,143,488
bytes in the combined matrix. These processes have different work and runtime;
their difference is **not** an isolated GPU memory overhead measurement.
Snapshot allocation excludes input surfaces, Metal/compiler/driver resources
and helpers. The four-byte shared result allocation is reported separately.
The 64 MiB limit bounds a logical snapshot, not total process memory or
transient old/new allocation overlap on a signature change. Both detectors
release their final snapshots. No production memory saving is established.

The CPU path is the next candidate for native capture measurement, especially
at Fincode's smaller streaming sizes. This evidence does not select a product
default or establish codec, bandwidth, latency or endpoint resource savings.

## Retained attempts and verification

An early uncommitted syntax attempt exits after a 45-second shared-slot wait;
its output directory is empty and it has no source-bound step receipt. At
`65de1a344c8deeb9cb8396f33ebb0fd7421637f9`, five selected process-guard tests
pass, then strict Swift compilation fails because the semaphore signal's return
value is unused. No comparison executes. Explicitly discarding that value is
the only Swift change before the completed run. A coordinator invocation with
an incorrectly transcribed requested SHA is rejected before any step; it is
retained as a rejected request, never as execution of that SHA.

At the measured source, all six steps exit zero: load, five guard tests,
toolchain, strict build, CPU-only control and Metal matrix. The guard reserves
the exited leader PID until its group is quiet and exercises normal exit,
natural descendant drain, deadline, cancellation and stubborn-descendant
cleanup. It is specific to this single-threaded POSIX experiment; older
streaming runners are unchanged. Ten recorded command groups are independently
absent and both acquired build slots are released.

Owner-only originals contain 17 run files and four frozen tool sources. Exported
receipts normalize local paths; the original hashes and optional complete-archive
verification preserve provenance. `verify.py` checks hashes, case coverage,
numeric bounds, table values, failures and cleanup without running native code:

```text
python3 -B media/results/2026-11-07-frame-comparison/verify.py --archive <archive>
```

## Before production frame suppression

These snapshots advance on **observation**, not acknowledged publication. A
dropped/retired/unsent frame must not become the baseline for skipping the next
frame. Source identity, colour attachments, authority epoch, forced IDR/PLI,
encoder/recovery changes and comparison failure need explicit invalidation or
bypass. Identical source pixels also do not prove that a lossy decoder has
finished improving its picture. Matched decoded quality and existing viewer
freshness/health behavior must be qualified without fake timestamps/progress
or relaxed limits. Native capture synchronization, paced/sustained CPU/RSS/GPU/
power, physical endpoints and full-wire cost remain open. No suppression or
application pin change is made here.
