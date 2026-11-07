# Native NV12 row visitor

At `d7b46350673f89ccbdfa1f975a61889872fde91c`, two selected native Mac tests and strict default-feature library/tests/examples Clippy pass. The exact commands, exit codes, timings and log hashes are in [evidence.json](evidence.json). They ran under the stable shared Mac lock, nice 19, one Cargo job and external process-group deadlines; all owned groups exited without cleanup signals. Full logs and frozen source remain in the embedding application's private evidence archive.

`visit_nv12_bytes` exposes read-only active Y rows, followed by interleaved UV rows, excluding padding. The visitor allocates no image and retains no pixel surface beyond the call. Metadata describes the validated dimensions/range and recognized native colour tags. Missing/unrecognized tags stay unknown. Callback row borrows cannot escape, and panic unwinding releases the native read lock.

The tests compare independently padded native images, verify byte order/range/missing colour tags and sRGB mapping, contain a deliberately panicking visitor, and reject malformed attachments before visiting. These are API contracts. No host/peer hash comparison, received-picture equality, perceptual quality, bandwidth/resource saving or new production default is admitted.
