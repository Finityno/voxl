# Native TURN allocation grace

Production source `7c44f12b9725b6c2d8e8108e26940ac4de3e36fe` gives only
Automatic trickle peers with actual TURN configuration a 250 ms grace. The
existing SDK driver timer keeps host/STUN gathering, reads, DTLS and Close live.
A Connected event cancels deferred allocation; otherwise TURN starts at the
deadline. Full-SDP and Relay-only peers remain immediate, and the SDK runtime
hook defaults to zero. Rebind clears stale deferred work. No new thread or
per-frame policy lookup is introduced.

At that source, locked prebuild and strict library/examples Clippy passed.
Seven selected tests passed: close before the allocation deadline, held-STUN
direct video, direct video with zero native TURN allocations, three Relay-only
trickle transports, and a full-SDP direct/unused-relay regression. The direct
case used two bindings per peer and observed zero sender allocations versus one
on the ordinary receiver. This is a native-side allocation reduction, not a
claim that browser viewers also skip allocations or that CPU/RSS were measured.

Three delayed fallback cases initially failed their selected-route assertion:
both peers were on loopback, and filtering SDP candidates did not prevent a
peer-reflexive direct path. Each case allocated but then reported actual Direct
routing; these failures remain in `7c44f12/artifact-pack.json`.

Test-only follow-up `de198d971ed8df48f1293471efb3cc1f770899bc` requires a
private rootless network namespace and probes packet loss on all four direct
host-interface paths before opening the native peer. Receiver relay candidates
are also held until a sender relay appears, so a one-sided allocation cannot
bypass the sender's 250 ms deadline. The first namespace run lacked a multicast
route required by the SDK's default mDNS socket; all three opens failed before
any TURN authentication. Its raw evidence is in `de198d9-initial-namespace/`.
An independent socket probe reproduced multicast join errno 19 and verified the
fix: enable multicast on the private loopback and add its multicast route.

With that environment correction, the unchanged exact test source passed all
three delayed fallback cases: UDP, TCP and verified TLS. All twelve direct-path
preflights dropped normally sent UDP packets, media used the nominated Relayed
route and correct local relay transport, and both synthetic payloads arrived.
Each endpoint retained one allocation across two bindings. Coturn independently
counted six allocations, six permissions and nine channel binds. The existing
binaries were unchanged; no rebuild, strict-Clippy repeat or assertion weakening
occurred. Raw results are in `de198d9-multicast/artifact-pack.json`.

The private namespace contained every firewall/routing change and the server.
Owned groups were reaped, ports rebindable inside it before exit, and private
REST/CA state and xtables locks removed. No host firewall was changed. Source
was clean in each cloud run. A hash-verified Git bundle imported exact `de198d9`
while GitHub pushes returned internal errors.

The strengthened fixture at `610ffea2317ff8183ae2230cc0ffe57f5dc2f04d`
forwards valid host candidates in both directions while their UDP paths are
blackholed. All three fallback cases passed, with 68 drops per case (204 total),
including failed ICE checks beyond the twelve preflight probes. Coturn counted
two allocations per case. Raw results are in `610ffea-direct-checks/`; unlike
the earlier namespace pass, this explicitly exercises unavailable direct paths
with their candidates present.

A matched comparison at exact
`75638d6d34bcd210c90a565f72a135dc6fe8c79a` keeps both roles, bindings,
Automatic/trickle policy, TURN/UDP server, candidate exchange and payloads the
same. Only native TURN grace changes: zero for the immediate baseline, 250 ms
for the new policy. The baseline sender/receiver used one allocation each; the
grace sender used zero and the same ordinary receiver used one. Coturn
independently recorded two allocations for baseline and one for grace. Both
cases required Direct selected routes and actual payloads. Locked prebuild and
strict Clippy also passed; all two tests passed without retries. Raw evidence is
in `75638d6-comparison/artifact-pack.json`. The helper extraction used for the
baseline preserves the production policy used at `7c44f12`.

This does not prove native capture/decoding, physical LAN/NAT/VPS routing,
matched-quality codec performance, CPU/RSS/power or startup p95. The provisional
250 ms fallback tradeoff requires physical measurements. Optional credential
HTTP lookup still has its separate two-second bound, and browser allocation,
credential renewal/ICE restart and the optimistic performance targets remain
required work.
