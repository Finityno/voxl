# Exact quiet byte-budget observation

Exact source `b16f75110b683dfc619b1bf262bc2c3788f5c34a`. All 30 selected Rust tests and strict library/test Clippy pass. Counts: flow epoch 1, lifetime/staged dispatch/exact-cap 5, probe/wire/retry 7, encrypted UDP 10, producer debt 6, feedback epoch 1. No dependency version/features or default changes.

`FrameBudget::quiet_cap_bps` exposes the optional cap already applied to that exact budget snapshot. The payload budget remains exactly the smaller of `pacing_target_bps × 315 / 8000` and, when present, `quiet_cap_bps × 315 / 8000`, with integer division. This changes observation, not the existing admission limit. A native caller can validate the smaller quiet budget without inferring capacity from a different estimator snapshot or weakening the ordinary pacing check. The new contract covers a fresh 1 Mbps cap, its expiry to the 250 kbps floor, actual media-feedback clearing to `None`, epoch retirement and authority close. Unknown/absent is not zero.

The same complete fresh-retry suite remains selected. One loss at 3 Mbps completes a fresh 32-packet cluster in 153.228 ms and receives all 64005 synthetic bytes in 342.636 ms. Two original clusters account 17408 post-TWCC RTP bytes; the 64 empty packets observed include an old NACK retransmission. Second loss closes in 164.094 ms; authority close during retry returns in 50.422 ms from the original request. Slow 250 kbps loss returns at 500.618 ms including scheduling/close overhead after the unchanged absolute timer expires. No fake picture or overflow. These are component results, not native image quality, cancellation-only latency, sustained resource or physical-network admission.

The [earlier 29-test retry proof](../2026-11-07-quiet-retry/README.md) is frozen at source `f19e14c78e0f1b5d9134163b650774bfd78e3a5c`, proof commit `ef4656f8eeeaaeae63a1510b4ae8e8df990aa05c`. Its archive retains the failed pre-TWCC byte hook and subsequent exact-history correction. Its source guard should be replayed from that checkpoint, not attributed to this newer source. The original deadline, full fresh-cluster requirement, byte/debt/headroom, epoch and authority gates remain unchanged.

Owner-only originals: `/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-07/quiet-cap`. Fifteen files and 11 command groups independently absent after exit. Raw logs stay outside Git. Scoped commands use process-group deadlines, the shared Mac lock, nice 19, two jobs and the owned target. No whole suite or formatting.

```text
python3 -B media/results/2026-11-07-quiet-cap/verify.py --archive <archive>
```

Use an external process-group deadline; the complete archive is mandatory. The verifier checks exact source, hashes, counts, exits, cleanup and positive/negative retry bounds. Actual native loss/cap validation, sustained CPU/memory/helpers/GPU/power/wire, matched decoded quality, physical endpoints, broader recovery and production default remain separate work. No user app, Windows/manual visual work, screenshots, persisted screen pixels, OS input, agents, SSH/key retries, deployment, release or merge.
