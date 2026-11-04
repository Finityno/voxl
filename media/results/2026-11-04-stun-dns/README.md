# Direct desktop traffic progresses while STUN DNS is pending

Qualified production source: `f223998fc3399831c5a31f64f054adfcf540b956`.
Negative control: `c022bfc1b9915c49a17118309bbc6607ce9e4e87`.
Only three vendored SDK production files differ. The complete test file is
byte-identical at both sources (Git blob
`278b3df371a01b7639ba75f511152285cc043767`).

Previously the driver awaited a STUN hostname lookup inside its gathering event.
The restart barrier could interrupt that wait, but an otherwise usable direct
connection could not progress while DNS remained pending. The unchanged direct
video regression builds successfully on the negative source, applies its answer,
then fails its inner five-second deadline: exit 101, zero passed, one failed.
The external watchdog does not expire. This is a functional failure, not a
controlled startup-latency benchmark.

Resolver futures now belong to the STUN discovery phase and are polled
independently of packet processing. Host candidates are queued immediately.
Resolution completion wakes the existing coalesced driver channel; a newly
created STUN client schedules its initial write there too. Retiring discovery
drops its resolvers and closes its clients before acknowledging the restart
boundary. Completion requires both the lookup and client sets to be empty.
The separate interrupt channel from the earlier implementation is removed.
There is no new dedicated OS thread, periodic timer or steady-media select
listener. The existing Tokio resolver still uses its blocking pool for hostnames.
Dropping a future stops its delivery into this gathering phase; it does not
promise to interrupt a synchronous OS lookup already running in that pool.
The held-DNS fixtures exercise future ownership, not OS resolver termination.
Concurrent resolver CPU/thread/RAM costs have not been qualified. No CPU,
allocation or memory saving has been measured for this change.

## Production-feature Linux validation

Cargo 1.99.0, clean exact checkout and its own target, two jobs, AWS-LC features,
locked dependencies and external process-group deadlines. Scoped library test
compilation, library/examples strict Clippy and diff check exit zero.
Fifteen selected executions pass, zero failed/ignored, no selected execution
missing. Build exit is retained; its elapsed wall time was not separately
recorded. Every case has its own exact filter, exit, count and deadline.

| Directly relevant scope | Passed |
| --- | ---: |
| Direct nominated UDP/DTLS/SRTP payload with DNS still pending and gathering incomplete | 1 |
| DNS completion wakes a driver whose protocol timers are all parked | 1 |
| One pending STUN lookup leaves another server usable | 1 |
| Existing actual STUN exchange and direct payload before held STUN completes | 2 |
| Existing HEVC IDR payload/capture-clock/keyframe-feedback loopback | 1 |
| Generation fences, pending UDP/DNS retirement, cancellation at the barrier, close during restart | 5 |
| Actual one-sided TURN UDP baseline and late UDP/TCP/verified-TLS after unanswered STUN | 4 |

The timer-parked case publishes its host candidate before releasing gated DNS,
then requires an actual STUN binding request, mapped candidate and completion
without a protocol timer or unrelated input. The two-server case keeps its first
lookup pending while the second real UDP server produces a mapped candidate.
The direct-video case requires DNS to remain pending through actual nominated
payload and requires the future to be dropped on peer close. Fixture hostnames
are intercepted in memory and never sent to external DNS.

Each coturn case uses fresh REST credentials, CA/certificate and private SQLite
state in an owned rootless user/network namespace. Actual host UDP is blocked;
the expected payload arrives over the nominated native transport. Each case
records one allocation, two permissions, four channel bindings, 36 actual UDP
drops and no receiver allocation. All owned groups are gone, ports rebind,
private state/namespace and xtables locks are removed. No external packet,
host firewall rule or Docker container is used.

## Native macOS validation

The same clean exact production source passes the three new DNS cases on
macOS 27.2 / Darwin 27.2 arm64, Cargo 1.99.0. Scoped library-test build exits
zero (48.570 seconds); each exact execution exits zero with one passed,
zero failed/ignored. Runs use the shared build lock, nice 19, two jobs, an
owned target and external process-group watchdogs. All groups are gone and
source remains clean. No Simulator, capture, input or visual check is performed.

## Standalone SDK mock-runtime coverage

A byte-identical disposable copy of all 29 SDK manifest/source files builds its
library unit target with `--no-default-features --features runtime-mock,crypto-aws-lc-rs`.
Only the directly edited STUN gatherer module is executed: ten passed,
zero failed/ignored, exit zero. Source and manifest hashes remain unchanged,
all owned groups are gone, and the active graph has AWS-LC with no active ring.

This is a separate scratch graph, not the production media lock. Its lock
preparation adds 85 entries, removes five and retains socket2 0.6.5 alongside
an additional 0.5.10 dev dependency. The all-platform metadata request times
out during downloads with exit 124 and produces no metadata JSON; the generated
scratch lock then supports the successful locked Linux build and tests.
The failed metadata step remains recorded and is not counted as a pass.
Production source/manifests/lock are untouched. No broader SDK module is run.

`mock-artifact-pack.json`: 29,601 bytes, SHA-256
`eae42348a5a9dc3d5f0bc174e6855476772cd94938035ed60c6da5c4b4ea0b11`.
All 15 included file hashes and transfer lengths/hashes were checked. It
retains graph deltas, lock/metadata/build hashes, selected diagnostics, complete
unit test output, source-integrity evidence and cleanup. Transfer metadata is
in `mock-transfer-metadata.json`.

## Evidence and remaining integration

`cloud-artifact-pack.json`: 107,864 bytes, SHA-256
`b171c0f7fc3e8f256548bc63456b9f1a73a0dc77be6b10999b29c954e778b6a3`.
It retains the negative and positive sources/results, all small test logs,
commands, fixture scripts, oracles and cleanup, and hashes of remote raw build
JSON. All 64 included file hashes and transfer lengths/hashes were checked
after export. `cloud-transfer-metadata.json` retains transfer metadata.

`native-artifact-pack.json`: 13,503 bytes, SHA-256
`62958ab9e66e364a180d2d06a0cc098000765356d8a2c6827b6f43b8063111e6`.
All 13 included file hashes were checked. The full native Cargo JSON is excluded
but its size/hash and selected executable/diagnostics are retained. No pack
contains pixels, input events, raw SDP/ICE, private coturn configuration,
authentication, keys/HMAC values, unsanitized coturn logs or binaries.

Fincode still pins `5229daca7471b6f041013a4bfe0eb67982c4475c` until its next
integration/qualification change. Cached STUN metadata exists, but the native
factory still performs its bounded two-second credential lookup and viewers
still allocate eagerly. Deferred credentials and generation coordination in the
host/desktop/phone applications remain in progress. General renewal/handover
is separate work. The preceding interrupt-channel pack remains historical at
`../2026-11-04-stun-startup/`.

These results prove synthetic transport behavior, not valid codec decoding,
quality, physical phone/LAN/NAT/VPS routing, application end-to-end behavior,
Windows execution, endpoint CPU/RAM/GPU/power, matched codec bandwidth savings
or sustained performance-budget qualification. Windows hosts are available for
subsequent native qualification; none was used for this scope.
