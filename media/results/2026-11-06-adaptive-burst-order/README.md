# Final rate-mutation order

Exact source `9e65865c425d6e7cbe4044381de3d2a1f3d3d26c` sets average bitrate before
the burst property and verifies both after the final mutation. Three selected
native generated320x180 component tests pass (hardware H.264/HEVC and CPU H.264),
as does strict library/tests/examples Clippy. Six owned groups were independently
absent. Full commands, sources, deadlines and normalized logs are retained here;
originals remain in the local owner-only archive.

This verifies native numeric metadata and generated small-frame output only.
A full-size CPU application trial at the earlier mutation order exceeded its
configured byte policy despite successful readback. The current order still
needs full-size application qualification. Neither echo/readback nor these small
frames establish emitted-frame bounds, quality, wire savings or default admission.
Earlier seventeen-test and failed-property evidence remains in the sibling
adaptive-burst pack with its original source; it is not re-attributed here.

Run `python3 -B verify.py` to check retained counts and hashes without replay.
