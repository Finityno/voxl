# Optional sustained producer debt

Exact native Mac execution: `fe094ddc75c635a67fdb24c2a95880f6c216b180`.
Six new debt tests, 34 selected adaptive tests, test compilation and strict
library/tests/examples Clippy passed. All eight supervised command groups
ended; logs and receipts are retained with hashes. `verify.py` checks this
evidence offline and verifies current production media bytes still match it.

The controlled-clock tests cover many small packets, old-rate repayment before
repricing, no idle credit, out-of-order timestamps, finite overflow/debt failure,
actual pacer retransmission accounting, retired-epoch NACK exclusion and close.
Accounting has fixed memory. Defaults remain disabled; an embedder must apply
the reported wait before encoding. The 500 ms send deadline is unchanged.

This component evidence proves neither lower queue latency nor CPU, memory,
quality, physical-LAN or full-wire savings. The 64-byte per-packet reserve is
explicitly an estimate; actual encrypted UDP/TURN traffic is a separate metric.
