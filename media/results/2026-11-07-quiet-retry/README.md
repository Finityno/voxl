# One fresh quiet-resume retry

Compiled/tested source: `f19e14c78e0f1b5d9134163b650774bfd78e3a5c`. `with_quiet_resume_probe_retry()` opts into one fresh cluster after all 32 exact original reports describe a lossy first cluster. It never treats missing, malformed, duplicated, retired or stale feedback as capacity. The second cluster needs all 32 fresh arrivals and a nonzero receive span. Its sequence range advances, its ceiling can only tighten, and both attempts use the original absolute 500 ms ticket deadline. A second loss, unknown/late feedback, close or retirement remains terminal. Basic `with_quiet_resume_probes()` retains its single-attempt policy. Ordinary application/default behavior is unchanged.

The explicit `Retry` state owns and reuses the original boxed collection. It clears the old slots only on a validated retry transition, preserving its ticket, deadline and accumulated original-byte accounting. There is no new dependency, thread, periodic traffic or authority. Padding retains the old video timestamp and never creates a picture, marker, frame receipt or presentation-clock advance. Producer debt and normal pacing/close rules still apply.

All 29 selected Rust tests and strict library/test Clippy pass: flow epoch (1), lifetime/staged dispatch (4), wire/expiry/retry contracts (7), encrypted UDP (10), producer debt (6), feedback epoch (1). Pure tests include sequence wrap, stale first-cluster reports, tighter retry ceiling, exact deadline expiry, missing/malformed feedback and no third attempt. Prior baseline/tiny-delta/nominal probe tests remain selected.

| Actual encrypted-UDP condition | Result |
| --- | --- |
| One lost probe packet at 3 Mbps | Fresh cluster succeeds in 163.411 ms; full 64005-byte payload received in 352.123 ms |
| Second cluster also loses a packet | Closes in 163.942 ms; no fake picture |
| Authority closes after retry starts | Closes in 56.704 ms from the original request; this is not cancellation-only latency |
| Capacity falls to 250 kbps with loss | Original timer expires; caller returns in 500.952 ms including scheduling/close overhead; no fake picture |

The successful retry transmits 17408 original serialized RTP bytes across two clusters. It receives all 32 fresh arrivals; 64 empty padding packets are observed overall because NACK also retransmits an old packet. RTX is separate from original-byte accounting. Maximum proxy residence before resumed video is 29.681 ms; zero overflow. The successful resumed work is all 64005 synthetic bytes, not native codec/image quality evidence. The slow failure proves no extension of the original deadline; it is not a passing slow-resume result.

An initial 29-test run at `a5f07dbfeb9d01d8ed45d2990bc9ed70cbaee527` used a computed byte count. The next accounting experiment at `5b515ab807711cafd94cec4593f783048e9107ff` failed its network assertion: the pacing hook measured 16896 bytes, while final serialized RTP is 17408. That hook precedes TWCC extension insertion. The correction counts each exact matched sender-history report once, including a lost original; the SDK history stores `rtp_packet.marshal_size()` after extension insertion. The retained source excerpt and current runtime assertion cover that position. Missing bytes are not reported as zero or guessed by adding a constant.

Owner-only archive: `/Users/sondre/Documents/Fincode Desktop Lab Evidence/2026-11-07/quiet-retry` (41 indexed files, 30 command groups independently absent after exit). It retains both earlier runs, the failed byte-accounting assertion, final results, exact helpers/runner and the publish preflight that refused an unpublished SHA after a transient GitHub server error. Raw logs stay outside Git. Every command has an external process-group deadline; scoped builds/tests use the shared Mac lock, nice 19, two jobs and the owned target. No whole suite or formatting.

The README/API documentation is updated after testing. One private Rustdoc statement that said every loss closes is replaced with the explicit retry exception; the verifier permits only that exact comment replacement and requires every other tested source byte to match. No executable-source change follows the tested commit.

```text
python3 -B media/results/2026-11-07-quiet-retry/verify.py --archive <archive>
```

Run under an external process-group deadline from a clean proof checkout; the complete archive is mandatory. This verifies source equality, hashes, exits/counts and retry bounds. Actual Mac native fault/decode qualification is separate. Broader application recovery, sustained CPU/memory/helper/GPU/power/wire costs, matched decoded quality, physical phone/Windows/LAN/Internet and production default remain unqualified. RTP bytes exclude SRTP, RTX, IP/link, Connect, TURN and provider billing.

No Windows/manual visual work, installed/prepared app changes or launch, screenshots, persisted screen pixels, OS input, agents, SSH/key retries, deployment, release or merge.
