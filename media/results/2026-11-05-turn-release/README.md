# Live TURN allocation release

Final test source `884e46bf76bc46259982f5bf9f89f6563000471d`, custom fork only.
Production transport, encoder, application pins and defaults are unchanged.

The new cases establish actual nominated TURN UDP/TCP/verified-TLS media and
check server resources while coturn remains live. Before closing, sender and
receiver each advertise one distinct relay allocation, and attempts to bind
both relay addresses fail with AddrInUse. Two synthetic RTP payloads cross the
selected authenticated DTLS/SRTP path. The public close handle is then invoked
twice; the sender immediately reports ended and rejects another RTP send.

A bounded loop attempts to bind the sender's actual allocated server UDP port.
It must become available within three seconds while binding the receiver's
separate allocation still fails. The rebound sender socket remains owned until
the check ends. This verifies resource reclamation without stopping coturn,
closing the independent receiver, or relying on post-server-shutdown cleanup.
The three-second regression bound is not a newly achieved product budget.

| Live-server observation | Initial c184233 | Final 884e46b |
| --- | ---: | ---: |
| UDP sender port observed rebindable | 1.008573 s | 1.010519 s |
| TCP sender port observed rebindable | 10.450 ms | 11.579 ms |
| Verified TLS sender port observed rebindable | 11.529 ms | 11.526 ms |
| Independent receiver allocation | Still busy | Still busy |

These are one execution per transport at each source, with ten-millisecond
polling. They are elapsed times to observing a successful bind and the receiver
negative control, not exact server deletion times, percentiles, matched
performance/cost improvements or a physical-network result. Production close
code is identical between sources. The test limit and optimistic product
performance targets are unchanged.

## Outcome and source attribution

Source inspection found that TCP/TLS adapters abort before the SDK's best-effort
TURN release request is sent. The initial hypothesis was delayed server
reclamation. Baseline live-server observations instead show prompt port release
on this coturn fixture. No production drain/close state is added on an unproven
leak. The observations are consistent with reclamation on stream disconnection;
they do not independently prove that an authenticated Refresh(LIFETIME=0)
request traverses TCP/TLS. The SDK's explicit-delete behavior and the protocol's
recommended release are distinct from this server-resource oracle. See
[RFC 8656, section 8](https://www.rfc-editor.org/rfc/rfc8656.html#section-8).
The slower roughly one-second UDP observation remains measured as-is; its cause
and broader distribution are not established here.

At c184233, all three release tests, library test build, strict production
library/examples Clippy and source/crypto guards pass. A later strict
`--lib --tests` lint fails on six older while-let/collapsible-if sites. Its exit
101 and full diagnostics are retained in
`initial-c184233/test-target-clippy.json`; this is not a test execution failure.
Final test-only changes preserve candidate/completion and predicate semantics
using explicit while-let/let-chain forms. No formatter or lint suppression is
used, and no production lifecycle, timeout or policy changes.

At 884e46b, **nine selected tests pass, zero failed/ignored**: three live-release
cases, forced-relay UDP trickle, held-STUN direct media, direct media with
pending DNS, a parked driver woken by DNS completion, an independent STUN
lookup and restart/signaling-generation fences. They directly cover the new
close oracle and six rewritten test sites. The ignored fixture cases are
explicitly executed with `--ignored`; they are not counted from a skipped run.
Library test build and strict `--lib --tests --examples` Clippy exit zero.
All 73 production source files and all 36 protected manifest/lock/vendor/config
inputs match the baseline. AWS-LC remains active without active Ring. No root
server or native application is built or run.

The close-handle implementation and five adapter/SDK source blobs also match
the embedding application's prior media revision, as recorded in
`production-comparison.json`. That comparison is not an application/native
execution or proof of its full authority/lease teardown.

## Evidence and limits

Each transport gets fresh rootless loopback coturn, ten-minute REST test
credentials, a small relay range and a throwaway CA/IP-SAN certificate. TLS
uses test-only trust injection; production roots are unchanged. Each fixture
finishes under an external process-group watchdog. After in-test live-server
checks, coturn is separately stopped with supervised SIGTERM (exit -15), all
owned groups are reaped, private credentials/keys/config/database are removed,
and listener/relay-range ports rebind. These cleanup observations are not
relabeled as exit-zero graceful server or TLS close-notify completion.

The packs retain commands, deadlines, exits/counts, exact selected test
executable metadata/hash, supervisors/fixture source, sanitized logs, source
comparisons, crypto and cleanup records. Included-file and raw/compressed
transfer hashes are independently verified locally. Successful large raw Cargo
artifact JSON remains remote with size/hash and selected artifact metadata.
Raw coturn logs and generated secrets/SDP/keys are not published.

This is synthetic Linux localhost transport/resource-release coverage. It does
not qualify native capture/decoding/input, hardware identity, full application
close/revoke/lease behavior, all receiver allocations, credential renewal,
physical LAN/NAT/mobile/VPS behavior, packet-loss deletion reliability, total
endpoint/relay CPU/RAM/GPU/power/bandwidth or sustained budgets. The server CPU
and monetary cost are not measured. General cleanup under packet loss, full
application/VPS teardown, renewal/handover and the other platform/provider work
remain open. No optimistic target is lowered or declared achieved.
