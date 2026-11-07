# Controlled encrypted-UDP network evidence

Exact executed source: `3495070e63ed455ba3cc2cc5953392f9b7a822fb`.
Only tests/documentation changed from production media source
`e3568aa21395996ba5403479f00b8eb06f12160c`, which remains the application pin.
Existing native codec/CPU/memory results are not reattributed to this test source.

The new fixture owns two loopback UDP sockets and a finite async forwarding
task. It changes only advertised loopback candidate ports; ICE credentials and
DTLS fingerprints stay intact, and actual UDP/DTLS/SRTP plus RTCP/TWCC/NACK pass
through it. The fixture makes no system network changes or external connections.
Selected candidate pairs are still host/direct; this is an emulated path, not
physical LAN/NAT, TURN or application authorization coverage.

Each direction serializes actual UDP payload bytes, with fixed delay and
bounded deterministic jitter. It holds at most 256 datagrams of at most 65536
bytes, plus two fixed receive buffers; recent RTP identities are capped at 1024.
The sockets are IPv4 loopback only. Dropping the owner aborts its forwarding
task; success explicitly aborts/joins forwarding and track readers. The external
process-group watchdog bounds failure too. No OS thread, dependency or
production queue/codec/default changes were introduced.

Four selected checks and strict library/tests/examples Clippy pass:

- Pure serialization/queue accounting preserves partial service on a capacity
  change and refuses queue growth beyond the cap.
- A 103281-byte synthetic H.264-shaped access unit at 1500 kbps media / 3750 kbps
  packet pacing is reconstructed exactly behind a 1500 kbps bottleneck with
  30 ms delay and up to 10 ms deterministic jitter. Sender completion is
  220365 us; last receiver payload arrives at 604881 us. Peak queue is 64;
  maximum residence is 385183 us, without overflow or deliberate loss.
- One deliberately dropped RTP packet is repaired by actual NACK retransmission:
  23 RTP arrivals include one repeated identity and one dropped original. Exact
  access-unit reconstruction passes, with peak queue 15 and no overflow.
- A simulated feedback-following producer holds its last verified target on
  stale/unknown feedback, matching the native worker's policy. The path runs
  3 Mbps for two seconds, 500 kbps for five, then 3 Mbps for three. Real TWCC
  lowers the media estimate from 1500 to 523 then 449 kbps; the final recovery
  estimate reaches 629 kbps. All estimates stay in the 250–1500 kbps range.
  Queue peak is 197 with zero overflow; 307 synthetic frames are submitted.

**The latency result is poor and remains unqualified.** The first reduction is
about 1.8 seconds after the capacity drop, and maximum residence reaches
3113361 us. The final-target/finite-queue assertions pass; the optimistic latency
goals do not. The larger packet-pacing rate can finish a sender call while a
slower path still queues the frame. UDP payload capacity excludes IP/link
headers and cannot establish provider-billed bandwidth. No native codec,
decoder, matched quality, physical network, resource or end-to-end latency claim
follows from these synthetic bytes.

Earlier attempts are retained at exact sources: two setup failures (repeated
candidate and unfinished gathering assumptions), then a recovery assertion
failure in an incorrect fixture that reset the producer target on stale
feedback and kept old-rate serialization times after capacity changed. Correct
remaining-work accounting and target retention preserve the original recovery
assertion. The next source passes four tests but fails two strict fixture lints;
the final source passes both. These are not product failures or passing runs at
the earlier sources. Test-build/check exit codes and missing lint steps remain
explicit in the receipts.

Twenty-seven recorded groups independently ended. Original helper scripts,
logs and receipts remain in the local owner-only archive. Exported receipts
normalize local home paths and retain original log hashes. Run
`python3 -B verify.py` for offline count/hash/numeric checks without native replay.
To replay only these tests, build `media/Cargo.toml` with `--features webrtc
--lib --no-run`, then run the `adaptive_network_` filter with `--test-threads=1`
under the existing bounded runner, shared build lock, nice 19 and two Cargo jobs.

Next: use the bounded target trace to diagnose the slow congestion response;
add a strict transient queue/response gate before changing a pacing default.
Keep native/physical loss/restart, matched-quality and complete endpoint resource
qualification separate. No performance target was relaxed and no merge/deploy
is claimed.
