# Remove retired ICE servers during recovery

Core/media/SDK runtime `a355a959ddeb2411360a46834bf28772b9928c6f` fixes
empty-list configuration replacement. The imported rtc 0.21.1 setter previously
skipped assignment when a replacement list was empty. Its getter retained old
TURN credentials/endpoints, and the async driver's next gathering contacted the
retired server. The setter now validates supplied servers and assigns the list,
including an empty one. Existing restart barriers, release ownership, grace,
transport/codec/crypto logic and application authority are unchanged.

## Causal packet regression

The test is byte-identical at baseline
`0c4f027c76a4da6685d71fa910058652c6116d3a` and fixed a355a95. It starts
real direct UDP/DTLS/SRTP with configured optional TURN and a quiet owned UDP
probe endpoint. After the acknowledged live restart clears all servers, it reads
the public configuration and listens at the old endpoint for 400 ms. Baseline
reports **one retained server and a decoded TURN Allocate request**, then closes
peers and fails. Fixed source reports **zero servers and no datagram**, then
carries synthetic H.264 through removal and re-addition with fresh configuration
on the same peer/certificate/receiver track. SSRC, sequence increments, capture
clock and actual generation/RTP receipts remain. No old request is needed to
resume direct video.

The probe is not an authenticated TURN server. Its observed Allocate is the
initial unauthenticated request, not proof of credential replay, an allocation
or media forwarding. The bounded quiet window is not a physical network test.

| Fixed-source selected execution | Result | Wrapper seconds |
| --- | --- | ---: |
| Clear/getter/retired UDP endpoint, remove and re-add while carrying H.264 | 1 passed | 1.226 |
| Close during deferred live restart | 1 passed | 0.702 |
| HEVC IDR types and capture clock loopback | 1 passed | 0.534 |
| Actual coturn UDP RelayOnly rotation across three restarts | 1 passed | 1.548 |

All four have zero failures/ignored. Locked library test build and strict media
library/test/examples Clippy pass in 6.372/2.865 seconds. The separate actual
coturn scenario obtains four allocations and forwards payloads; its server exits
normally, its group is absent and all 17 owned UDP bindings rebind. It does not
share a server with the quiet-endpoint regression. No full RTC/workspace suite
or native decoder is implied by these selected transport cases.

The initial test-only source c29eb4d fails to compile with E0308 because the
timeout's nested receive result was not unwrapped. It executes zero tests.
Corrected baseline 0c4f027 builds and passes strict Clippy, then executes one
failing regression with the stale Allocate above. Both attempts are retained.

## Verified core import and dependency boundary

The core library is vendored beside the existing async SDK so the setter itself,
public getter and driver agree. Media and SDK both use that one core path/type
universe; no shadow configuration, extra worker or global Cargo patch is added.
The registry archive SHA-256 is
`4e2ca201de5ccc83aa3c08d49a2f08d7ed0aeeb2052fb1ccc517a8b77a6927ce`,
verified against the previous lockfile. Packaged VCS metadata identifies
2a732df55ca4bcf26b80914fa0f570cfaf74cfcb. Licenses/original manifest/VCS
metadata are retained verbatim. `media/vendor/rtc/IMPORT.json` and provenance
record the import and update rules.

The included verifier reconstructs **all 143 retained files** from that archive.
Numeric comment references are normalized in twelve files; documentation/
generated-manifest trailing whitespace is cleaned in ten files using scoped
edits, without a formatter. Non-vendored example/integration target declarations
and development-only dependency tables are omitted. All production dependency
tables/features remain identical. Only the configuration setter file differs
functionally from the normalized import; the exact patch is included.

Fincode source `1413e43d1e24331eca01a85bc38f6816b7e6661d` pins media,
SDK and core to a355a95. Three lockfile source IDs change, with rtc moving from
the verified registry source into the same fork. All versions, feature lists,
other package nodes/edges and application fixture/provider/issuer/authority
sources are unchanged. The SDK source tree itself is unchanged. Existing public
notes that empty updates retained servers are superseded at this source.

## Actual application and crypto selection

One selected native Swift case and one Rust fixture pass together at that
Fincode source and unchanged phone fixture
`38a919ddcd06bbdc8ebe9aaaa1184165046fe713`, zero failed/skipped/ignored.
Production Mac capture/VideoToolbox H.264 passes through AppModel/Connect/Noise
and the phone DesktopSession/WebKit receiver on the owned Simulator. The normal
hosting factory/client/weak signer uses the real local HTTP TURN issuer.

Initial direct media makes zero credential requests. The existing test-only
receiver route-policy switch and interruption recover the same view/peer onto
nominated TURN generation two in **765 ms**, connected DTLS. Actual decoded
RTP equals authenticated current keyframe RTP 998073485; frames advance 34 to 35,
finishing at 53 H.264 1670x1080 frames. Five host samples observe 517 ms watching
without control. Explicit reacquisition and renewed lease pass, then signed
revocation ends media/control without reopening. The exact still-fresh successful
grant receives HTTP 403 after the only 200/no-store/account-first issuance;
the injected issuer is unused and production hosting stops. No OS input is posted.

One actual coturn 4.18.0 allocation forwards video, processes lifetime-zero
refresh and closes before normal server exit. All owned UDP bindings rebind.
The configured 300-second allocation maximum still returns 600 seconds; this
does not qualify that cap. The application case revalidates the new dependency
boundary; it does not inject empty-list removal in the phone scenario. The
separate socket regression supplies that specific behavior evidence.

An initial application attempt passes native/phone builds, strict target Clippy
and dependency guard, then fails an overly broad helper assertion forbidding
any ring artifact. It starts no native session/TURN server and executes zero
Swift/Rust cases. The same standalone ring artifact exists in the previous
826ec4b native build. Inverse paths include existing JWT and Noise dependencies.
Rustls has AWS-LC enabled and no ring feature; rtc/SDK have crypto-aws-lc-rs and
no crypto-ring. The isolated media build has no compiled ring artifact. One
inverse-tree command's `media-only` label does not isolate workspace parents;
its output is retained without an absence claim. Compiler-artifact selections
provide the distinct scopes.

The corrected provider-specific helper reruns the **same source** with no
production change and passes the native case. Its native build/Clippy reuse
take 0.512/0.508 seconds; the earlier same-source build/Clippy take 93.736/14.898
seconds. This is cache reuse, not a build speedup. Final phone build takes
3.487 seconds with no warning/error diagnostics; Swift/Rust executions take
31.531/32.462 seconds concurrently. The dependency guard passes in 0.321 seconds.

## Evidence and limits

Six packs verify **65 embedded text hashes** and **52 independently absent
recorded process groups** across the failed builds/oracles, passing executions
and dependency inspections. These are bookkeeping counts, not extra tests.
The owned Simulator is confirmed Shutdown after its already-off exit 149.
Exact commands, sources, selected artifacts, import audit, scalar HTTP and
anonymized allocation ordering are retained. Temporary private configurations/
secret files are removed. Credentials, keys, captured signed grant, HTTP bodies/
authorization headers, raw server logs, SDP/ICE, pixels and xcresult attachments
are excluded. Disposable fixture bearer values are redacted with original hashes.

No cloud result is claimed in this slice. User-authorized native runs use the shared lock,
nice 19, two jobs, frozen sources and external group deadlines. No fresh Windows
job, formatter, visual verification, deployment or merge occurs. Physical phones/
LAN/cellular/NAT/interfaces, native-origin/lease-expiry held-input cleanup,
application TCP/verified-TLS recovery, deployed backend/migration/quotas and
sustained matched endpoint/helper/relay CPU/RAM/GPU/power/quality/bandwidth/input
latency remain open. Single debug timings qualify no percentile or performance
target; this patch supplies no measured resource or bandwidth saving.
