# RTP frame receipts

Executed source: `6279c1f57fb268945ea9567643eb8e4331411091` on native Mac arm64.
The additive `send_with_stamp` returns the actual signaling generation and RTP
timestamp only after every packet of the access unit was written. Existing
`send` delegates to it and keeps its unit return and clock behavior.

| Selected check | Exit | Result | Seconds |
| --- | ---: | --- | ---: |
| Media library test build, WebRTC feature | 0 | Built | 17.921 |
| H.264 UDP/DTLS/SRTP loopback | 0 | 1 passed, 0 failed, 0 ignored | 1.090 |
| HEVC UDP/DTLS/SRTP loopback | 0 | 1 passed, 0 failed, 0 ignored | 0.720 |
| Three live ICE restart generations | 0 | 1 passed, 0 failed, 0 ignored | 0.467 |
| Strict library/test/example Clippy | 0 | Passed | 4.318 |

H.264 and HEVC compare the returned stamp with actual received RTP headers,
then force the RTP clock across wrap using the original `send` API. The live
restart case compares stamps in generations one through three and rejects
receipt creation while a replacement is pending. Payloads are synthetic; this
is no native encoder/decoder, image quality, physical LAN or performance result.
TURN was not rerun at this source; preceding transport qualification stays at
its original source. No per-frame allocation, timer or dependency was added.

The frozen clean checkout used a private target, two jobs, nice 19, the shared
build lock and external process-group deadlines. All eight recorded child
and queue groups were independently absent at evidence export. Selected test
logs must contain one executed passing test, preventing a zero-test pass.
No formatter, GUI verification, upstream push or merge ran.

`evidence.json` embeds logs, helper and results with original/normalized hashes.
Private filesystem paths are placeholders. The reused helper's descriptive
scope/limits were stale; they described a different coordinator fixture. This
export corrects those two fields and retains original byte hashes. The command
arrays, test counts and exit codes above are the authoritative coverage.
