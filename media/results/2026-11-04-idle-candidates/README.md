# Wake the driver for late trickle candidates

Remote candidate registration can arm ICE checks or mDNS output while the SDK
driver is parked on its previous deadline. UDP and other non-passive-TCP
candidates previously changed the core without waking that driver. The fix
calls the existing coalesced `wake_writes`; passive TCP keeps its existing
connect-event order. No new thread or per-frame polling is added.

The corrected fixture parks both real-UDP peers after local gathering and the
answer's own wake, strips candidates from both wire SDPs, and holds only their
already-created timer futures. Before adding late candidates it permits new
protocol timer futures: a driver must first wake and replace its old wait to
arm those ICE checks. It then requires connection, an actual synthetic H.264
payload and the nominated Direct route within the unchanged functional bound.

The fixture is byte-identical in the control and fixed sources (Git blob
`c8d4c44d994f96cbba40a664b50e7db78b97728e`):

- Control `b38e14bc9f3da844abc23dde438ae9c10d413b32`, based on public
  `5ae44c0`: prebuild passed; zero tests passed and one failed at the 250 ms
  late-candidate deadline after setup completed.
- Fixed `5bfd1c638b39347ab224e9aacb4e870c6434ab1f`: prebuild, the one
  selected test, and strict library/examples Clippy passed.

Production source differs only by the coalesced candidate wake and comments;
the provenance document also differs. Both cloud checkouts were clean. Raw
commands, complete diff, test output and exits are in `no-wake-b38e14b/` and
`wake-5bfd1c6/`. The 1,253-byte `no-wake-control.bundle` preserves the exact
control commit against its public `5ae44c0` prerequisite. SHA-256:
`d62f4575c5cd830ba2b752fd0d1e941c412d8ec5f4ac91143e765101b2d3388d`.
This permits exact-source reproduction without an extra public branch.

The initial fixture suppressed all driver timers, including newly required ICE
check timers. It failed both the old and fixed variants; those runs are retained
in `initial-all-timers-5ae44c0/` and `initial-all-timers-a5ee62e/` but establish
neither causality nor a failed production fix. Assertions and time bounds were
not loosened; the corrected timer model distinguishes old unrelated waits from
new protocol work. No test was retried on unchanged source/environment.

At production-equivalent `a5ee62e`, the held-STUN direct and cancelled-wait cases
and all three real-blocked-direct UDP/TCP/TLS fallback cases passed, along with
locked prebuild and strict Clippy. The namespace observed 204 dropped packets
and six allocations across three fallback cases; private processes, ports,
namespace and authentication state were cleaned up. The earlier strengthened
`610ffea` cases also passed with the same 204 direct-path drops. Their raw
results are in the neighboring lazy-turn results directory.

This is functional liveness and transport evidence. The 250 ms regression bound
is not measured startup p95 or physical latency. Native capture/decoding,
hardware/CPU codec performance, physical networks and the unchanged optimistic
performance targets remain unvalidated by this test.
