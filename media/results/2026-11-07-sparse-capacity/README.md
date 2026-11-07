# Sparse-update bulk capacity freshness

Exact source `6ddaad619557971c7e26db484ba76b64d830de83`. The selected 39 Rust tests and strict library/test Clippy pass. Counts: normal-capacity contracts 5, sparse UDP 2, active UDP 1, flow epoch 1, lifetime 6, probe/wire/retry 7, existing quiet UDP 10, producer debt 6, feedback epoch 1. This is opt-in component qualification, with no dependency/features or application default change.

The negative characterization at `c787b17ec40daffc90ba69b2497f8704f30f96f1` sent three 1005-byte updates 900 ms apart after a 3 Mbps link fell to 250 kbps. No inter-update gap reached two seconds and no padding was added. Feedback was just 6.861 ms old and the held target stayed 1500 kbps; the old byte budget admitted 64005 bytes without probing. Full receipt took 2134.469 ms and the queue peaked at 1997.381 ms. The characterization test passed by recording the defective behavior; that is not a latency pass.

The opt-in detector now keeps at most 32 normal original RTP identities, requiring eight adjacent packets and 8704 final RTP bytes from one timestamp. Exact sequence/SSRC/original departure bind feedback; padding, RTX, loss, mismatched reports, zero receive span and separate tiny frames cannot qualify. Bulk evidence expires from its original departure, so a later tiny ACK cannot renew it. Fresh normal-burst capacity also caps the exact byte budget. Startup retains the configuration prior for two seconds from the first actual media release, not connection creation. Small deltas remain padding-free. Epoch retirement and close discard the old authority and pending chain.

| Scenario | Probe | Resumed bytes | Probe plus receipt | Max queue |
| --- | ---: | ---: | ---: | ---: |
| Sparse, 250 kbps | 403.631 ms | 7436 | 662.768 ms | 292.373 ms |
| Sparse, 3 Mbps | 98.111 ms | 64005 | 286.803 ms | 54.296 ms |
| Active normal bursts, 3 Mbps | none | 64005 final | 13 frames over 3.560 s | 54.213 ms |

The slow case explicitly uses smaller synthetic output; it is not equal decoded quality. The fast case retains exactly the baseline 64005-byte payload. The active case proves normal bulk evidence renews capacity beyond the two-second startup grace without padding. All cases have zero queue overflow; probe/output and 750 ms bounds, byte admission and native 25 percent headroom are unchanged. The existing retry suite also retains positive fresh retry, second loss, authority close and the original shared deadline.

Owner-only originals: `/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-07/sparse-capacity`, 26 indexed files, 20 process groups independently absent after exit. Raw logs remain outside Git. Helpers retain exact commits, commands, counts, exits, hashes, source cleanliness and bounded cleanup. Shared Mac lock, nice 19, two jobs, own target and external process-group watchdogs; no full suite, formatting, screen pixels, OS input, app launch, Windows, agents, SSH, deployment, release or merge.

```text
python3 -B media/results/2026-11-07-sparse-capacity/verify.py --archive <archive>
```

Use an external process-group deadline. Native encoder/decoder/reference-chain behavior, image quality, sustained CPU/memory/helpers/GPU/power/full wire, physical endpoints/networks, broader application recovery and default admission remain separate. Earlier [cap](../2026-11-07-quiet-cap/README.md) and [retry](../2026-11-07-quiet-retry/README.md) proofs stay frozen at their own sources; do not replay their source guard against this changed code or claim their native results cover it.
