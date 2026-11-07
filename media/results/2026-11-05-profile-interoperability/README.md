# Native profile interoperability

Transport source `b66d27db1a159a0e546dd7a3780972f18052f64c`, custom fork only.
The embedding application keeps its prior media/SDK pin and encoder defaults.
No new profile/rate-control mode is automatically admitted.

The preceding encoder experiments emit actual H.264 High (`64001f`) and
Constrained High (`640c1f`) SPS values. The transport's older Baseline-only
guard instead rejects all four regular/low-latency High/Constrained High
requests before an offer at 7677d66. This mismatch is fixed narrowly: the
existing Baseline behavior remains, and only High with constraint byte 00 or
0c is added. Other new profiles/constraint variants remain rejected. Offers
preserve actual SPS bytes; neither SPS nor SDP is rewritten as Baseline.

[RFC 7742, section 6.2](https://www.rfc-editor.org/rfc/rfc7742.html#section-6.2)
requires Constrained Baseline and recommends Constrained High. That is not
proof that an endpoint accepts ordinary High, a particular level, or a
particular resource budget. Native/browser negotiation remains necessary.

## Native replay

An owned standalone native WKWebView runs the embedding application's
unmodified production receiver page, identified by SHA256 in
`native-summary.json`. File-based private signaling connects the actual
`encoded_peer_probe` UDP/DTLS/SRTP sender. This qualifies the page in a separate
native wrapper, not the application's full bridge/authority/entry flow.

The sender replays existing deterministic 1280x720 / 30 fps / 210-frame streams
from encoder source 7677d66. Encoder/probe source blobs are unchanged at b66;
stream hashes match. There is no new encoder execution, screen capture, input,
screenshot, or recording of the user's desktop. Each case owns fresh sender
and receiver processes. The final batch has **seven passes and two failures**:

| Encoded candidate | Result | Decoded frames / keyframes | Observed receiver profile |
| --- | --- | ---: | --- |
| Hardware compatible H.264, regular | Pass | 210 / 4 | 42e01f |
| Hardware Constrained High H.264, regular | Pass | 210 / 4 | 640c1f |
| Hardware High H.264, regular | Fail before answer | none | none |
| Hardware compatible H.264, low latency | Pass | 210 / 4 | 42e01f |
| Hardware Constrained High H.264, low latency | Pass | 210 / 4 | 640c1f |
| Hardware High H.264, low latency | Fail before answer | none | none |
| Hardware HEVC Main, regular | Pass | 210 / 4 | H265 level-id 93 |
| Hardware HEVC Main, low latency | Pass | 210 / 4 | H265 level-id 93 |
| Explicit software compatible H.264 | Pass | 210 / 4 | 42e01f |

Every passing case observes one peer, one answer and first-frame event, actual
1280x720 decoded geometry, nonzero media, nominated direct candidates and
connected DTLS. The loopback source binds 127.0.0.1; this is not physical LAN or
NAT evidence. Decoder implementation and power-efficient identity are omitted
by WebKit, so hardware decoding stays unknown. The receiver's 42e0 SDP
representation is observed without modifying the compatible 42c0 input SPS.
Browser keyframe counts establish normal stream decoding, not a fresh-browser
restart or packet-loss recovery experiment.

A focused diagnostic rerun has **two passes and two failures**: regular
Constrained High/compatible H.264 pass again; both ordinary High modes fail
again. The browser advertises 42e01f and 640c1f with packetization modes 0/1,
without 64001f. High reaches resolved remote description/createAnswer, then
begins local description before the page ends without an answer. The exact
underlying DOM error is not captured; profile absence and repeatable failures
support keeping ordinary High out of this receiver's admission. No profile is
silently substituted to turn those failures into passes.

## Retained failures and harness corrections

Initial Swift compilation fails on actor isolation and executes zero native
cases. A corrected initial compatible control passes, with Promise-to-Swift
unsupported-return diagnostics. Returning null after starting the async probe
removes that harness warning. The first nine-case batch at 7677 passes five
and fails four at the local transport guard; these precede the b66 fix and are
not browser rejections.

The earlier native helper can exit zero through NSApplication termination
after persisting a false oracle. The nine-case final batch still rejects both
High failures through the sender's 30-second answer deadline, and its seven
passing persisted reports independently satisfy the complete 210-frame /
four-keyframe oracle. The focused helper explicitly exits with its oracle code;
its supervisor also requires a persisted true result and exact frame counts.
Both failing receivers exit 1 there; owned waiting senders receive SIGTERM and
are reaped. This is bounded cancellation, not graceful sender completion.
Earlier results/failures remain attributed to their actual helper versions.

Native release sender build and scoped strict Clippy pass at b66. One selected
Linux real-offer test passes, zero failed/ignored; it preserves Baseline/High/
Constrained High fmtp and rejects six unsupported combinations. Library test
build, strict library/examples Clippy, diff and protected/crypto guards pass.
All 36 manifest/lock/vendor/config inputs are unchanged; AWS-LC remains active
without active Ring. The cloud pack includes 30 locally verified text hashes;
raw successful Cargo artifact JSON remains remote with size/hash and selected
executable metadata. Cloud execution provides no browser/performance evidence.

All 63 recorded native build/supervisor/sender/receiver groups are independently
gone; both frozen native checkouts remain clean at their recorded source.
This does not assert that persistent OS WebKit/codec/GPU helpers terminate.
Full helper/log evidence and excluded SDP/config/binary hashes stay with the
private embedder. This public directory contains only selected observations,
public cloud validation, host metadata and verified file hashes; no private
page source, SDP, credentials or protocol endpoints are published.

These are short synthetic replays, not measured codec admission or savings at
matched readable-text/chroma quality. The earlier encoder performance samples
remain attributed to 7677 and are not rerun/relabelled here. Full endpoint and
helper CPU/RAM/GPU/power, thermal behavior, sustained five-run qualification,
physical devices/networks, phone compatibility, AV1/providers, backend hosting
and application authority/input integration still need their own coverage.
No optimistic target is lowered or marked achieved.
