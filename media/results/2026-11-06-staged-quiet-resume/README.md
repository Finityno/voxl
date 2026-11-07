# Staged quiet resume for a native producer

Exact component source: `0570b08f9510774d2bfd627a0319171ec910f0a3`. This follows the frozen component proof in `../2026-11-06-quiet-resume/`; its results remain attributed to their earlier source. Fincode now has a separate opt-in native qualification mode. Its ordinary application factory remains unchanged until native runtime and failure recovery qualify.

A synchronous native producer can prepare its actual encoded byte count with `VideoCloseHandle::prepare_frame`. Safely small updates use the conservative floor without padding. A large update after silence obtains an opaque epoch/serial/deadline ticket, wakes the existing asynchronous writer through `VideoUpdate::ResumeProbeRequested`, and holds its one unsent reference chain while that writer services the ticket. Dispatch consumes the original 500 ms budget. Retired tickets cannot publish old output in a replacement epoch. No new thread, dependency version or feature is introduced.

An unprobed quiet fence now survives tiny updates and their fresh acknowledgements: that feedback proves liveness but not large-burst capacity. Only original normal-media release updates media activity; retransmission cannot clear the quiet interval. Pending dispatch is claimed once, and all exact probe arrivals still require the original epoch, sequence and departure slots. Existing pacing, debt, queue, 25% headroom and codec/PTS accounting remain in force. Probe loss remains terminal for this component attempt; robust application recovery and default admission remain separate work.

All 22 selected Rust tests pass: flow epoch (1), probe lifetime/staged dispatch (4), wire/expiry (4), actual encrypted UDP (6), producer debt (6), feedback epoch (1). Strict library/test Clippy passes. The retained first compilation failed because one exhaustive state pattern omitted the newly added ticket; that correction and the original failure are retained.

| Three-second quiet interval | Resumed bytes | Probe | Probe + full receipt | Maximum proxy residence |
| --- | ---: | ---: | ---: | ---: |
| Stale baseline, 250 kbps | 64005 | none | 2134.921 ms | 1998.539 ms |
| Staged probe, 3 Mbps | 64005 | 65.962 ms | 255.829 ms | 54.026 ms |
| Staged probe, 250 kbps | 7409 | 370.516 ms | 630.004 ms | 294.622 ms |

The slow case has smaller synthetic work, not equal-quality video. Both positive cases use 32 padding packets (8704 serialized RTP bytes), no overflow and a fresh byte budget. The tiny-then-large case sends two real RTP packets before its request, then the 32 padding packets, then all 64005 resumed bytes within 288.077 ms. A lost probe packet closes in 61.811 ms; explicit authority close cancels in 11.719 ms. Neither creates a video picture. During each three-second idle window: zero RTP, 384 forward and 444 reverse UDP-payload bytes, six datagrams each way. These short component measurements exclude IP/link headers, Connect/Noise, TURN/TLS and provider billing.

Owner-only originals: `/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-06/staged-quiet-resume` (22 files). Every one of 15 command groups was independently absent after exit. Raw logs stay outside Git. The verifier requires that complete archive and checks hashes, exact sources, exits/test counts, numeric bounds and source equivalence:

```text
python3 -B media/results/2026-11-06-staged-quiet-resume/verify.py --archive <archive>
```

Run under an external process-group deadline. This does not establish native capture/decoding, codec quality, sustained performance, physical-network routing, CPU/RAM/GPU/power savings, complete application integration or production admission. No Windows, screenshots, persisted screen pixels, OS input, installed-app changes, agents, SSH retry, deployment, release or merge.
