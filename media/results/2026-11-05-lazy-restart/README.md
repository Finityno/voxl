# Avoid unused TURN allocations during live recovery

Media runtime `f7bc0059588727dc08637552d5d1866492f685e0` preserves the
Automatic trickle sender's 250 ms direct-first allocation grace during live
ICE restart. The retained runtime now adopts both replacement IPC addresses and
the generation's allocation delay. Previously, replacement prepared a zero-delay
runtime and copied only its addresses; a peer initially opened without TURN
therefore allocated immediately when TURN was later introduced.

The generation barrier, old-allocation release, callback fences, six-second
preparation bound and close ownership remain. RelayOnly stays immediate, as do
full-SDP opening and the distinct one-time startup fallback. A live Automatic
path that needs its own allocation may now wait up to 250 ms longer; the
application's original recovery deadline remains. Direct connectivity is not
delayed. No new worker or healthy periodic timer is introduced. The vendored
SDK tree, dependency versions/features and media manifests/lockfile are unchanged.

## Transport regression

The refined regression is byte-identical at baseline
`2a9cab3258140fa9066caadb4d2b5f3c58bd6488` and fixed f7bc005. It begins
server-free, introduces actual local coturn credentials and performs three live
restarts on one peer/receiver. Four synthetic H.264 RTP payloads preserve the
certificate, SSRC, sequence and capture-clock increments; retired answers and
candidates cannot admit replacement video. Every generation uses a nominated
direct path. Gathering completes before relay-candidate counts are checked.

| Scope | Before | After |
| --- | --- | --- |
| Relay candidates in generations 0 through 3 | `[0, 1, 1, 1]`, test fails | `[0, 0, 0, 0]`, test passes |
| Actual coturn allocations in direct scenario | 3 unused allocations | 0 allocations |
| Forced RelayOnly UDP rotation | Not rerun here | 1 case passes, 4 allocations and forwarded payloads across three restarts |
| Close before replacement answer/grace expires | Not run here | 1 case passes, no TURN datagram after 400 ms |
| Replacement IPC/grace state | Not run here | 1 case passes, 250 ms and zero-delay configurations replace each other |

Fixed-source build and strict library/examples Clippy pass in 1.713/0.731
seconds. Both protocol cases pass separately, zero failures/ignored. The two
selected state/cancellation cases also pass separately, with strict library/
test/examples Clippy passing in 0.732 seconds. These are four selected local
executions, not a suite or native encoder/decoder test. The zero-allocation
server needs bounded SIGKILL (-9); the forced-relay server exits normally (0).
All owned UDP bindings rebind and recorded server groups are absent.

The refined baseline's test exits 101 after delivering media and closing peers.
Its cleanup raises PermissionError during a kill attempt as the server exits;
the outer supervisor exits 1, while the original inner result incorrectly says
0. That inconsistency is retained and is **not a pass**. Independent follow-up
finds the exact server group absent, rebinds all 17 UDP ports and removes its
configuration. The original server exit status is unavailable. The subsequent
helper corrects terminal reporting and tolerates a refused kill only when the
owned server finishes within the bound.

An earlier fixture at e8258bc also fails with three allocations, but alternated
an empty server list. The imported core retains previous servers for an empty
update. The refined regression keeps TURN in all three replacements, isolating
the grace defect. Empty-list removal remains a separate unqualified API edge;
this change does not claim to fix it. Both failed attempts remain in the packs.

## Actual production-issuer Mac-to-phone recovery

Fincode source `826ec4b55f0842950896f96f0c02ee6ad7ac34ee` pins both media
and its paired SDK to f7bc005. Its actual fixture/provider/issuer/authority blobs
are byte-identical to the preceding 8208e7e application case; the phone fixture
remains `38a919ddcd06bbdc8ebe9aaaa1184165046fe713`. Only the paired Git IDs
change in the application manifest/lockfile, with no other resolution changes.

One selected native Swift case and one Rust fixture pass together, zero failed/
skipped/ignored. Actual Mac capture/VideoToolbox H.264 passes through production
AppModel/Connect/Noise and phone DesktopSession/WebKit in the owned Simulator.
The normal hosting factory/client/weak signer calls the real local HTTP issuer.
Initial direct media uses zero credentials. A fixture receiver policy switch
and injected viewer interruption trigger generation-one direct retry followed
by generation-two TURN recovery on the same admitted view and receiver peer.

| Actual application observation | Previous pin | Fixed pin |
| --- | ---: | ---: |
| Local coturn allocations | 2 | 1 |
| Permissions / channel bindings | 4 / 2 | 2 / 1 |
| Single debug recovery trial | 743 ms | 738 ms |
| Watching without control / host samples | 513 ms / 5 | 516 ms / 5 |
| Final decoded H.264 frames | 67 | 74 |
| Successful issuer calls / injected issuer calls | 1 / 0 | 1 / 0 |
| Client releases before server shutdown | 2 | 1 |
| Server exit | -9, forced | 0, normal |

These timings/counters vary with scheduling and content; they are not a latency
speedup or matched bandwidth comparison. The fixed nominated pair is receiver
`relay` to sender `host`, connected DTLS. Actual decoded RTP 278604337 equals
the authenticated current sent-keyframe boundary; frames advance 43 to 45 and
finish at 1670x1080. Recovery never restores control. Explicit reacquisition is
renewed beyond the initial lease, then signed revocation closes host/phone.
The exact captured fresh host grant receives HTTP 403 after revocation, following
one 200/no-store/account-first response. The injected issuer remains unused and
production hosting stops. No OS input is posted.

Native build/strict target Clippy/workspace-dependency guard pass in
89.815/14.725/0.317 seconds. Phone build takes 5.179 seconds with no warning/
error diagnostics. Swift/Rust executions take 32.812/33.751 seconds concurrently.
The one allocation processes lifetime-zero refresh and closes before normal
coturn 4.18.0 shutdown; all 17 UDP bindings rebind. The configured 300-second
allocation cap still returns 600 seconds and remains unqualified.

## Evidence and remaining limits

Six packs retain exact sources, helpers, commands/exits, selected artifacts,
failed outcomes and source/pin comparisons: **58 verified embedded text hashes**
and **52 independently absent recorded process groups** across separate attempts.
The owned Simulator is confirmed Shutdown after its already-off exit 149.
Private credentials/secrets/configuration, HTTP bodies/headers/signed grant,
raw server logs, SDP/ICE, pixels and xcresult attachments are excluded. Fixture
bearers are redacted with original private hashes; temporary private configs
are removed. Anonymized allocation events establish ordering, not release time.

The cloud was disconnected; these authorized native runs use shared locking,
nice 19, two jobs, exact frozen sources and external group deadlines. No fresh
Windows run, formatter, visual verification or merge occurs. This demonstrates
fewer allocations in these scenarios. It does not imply a universal halving,
less relayed video egress, measured CPU/RSS/power savings or performance-budget
admission. Physical networks/devices, application TCP/verified-TLS recovery,
deployed backend/migration/quotas and sustained matched endpoint/helper/relay
resources, quality/bandwidth and input latency remain open. Optimistic targets
and codec defaults remain unchanged.
