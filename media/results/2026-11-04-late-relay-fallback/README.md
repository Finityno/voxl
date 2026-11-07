# Late relay startup and first-packet ordering

This is transport work in the custom fork. Application source
`e9d2b0a380908182de0d5a03b5b0061bfe0608a3` now pins
`5229daca7471b6f041013a4bfe0eb67982c4475c` to apply the first-packet fix;
application qualification is separate private evidence. The application does
not yet use the new startup API. Credential lookup and eager viewer allocation
remain separate work.

## Startup primitive

An initially server-free Automatic trickle peer can return its initial offer,
accept its initial answer and attempt direct connectivity before credentials
exist. If it remains unconnected after the embedder's grace, the embedder can
fetch per-view credentials and call `begin_relay_fallback` with the same codec,
bindings and policy. The six-second bounded operation updates configuration and
creates a real ICE restart offer, retaining the DTLS identity. TCP/TLS adapters
created during that operation share the view's cancellation boundary.

Authenticate the returned SDP and generation inside the existing admitted view.
Stop the old candidate poll, replace its signaling handle, carry the new
`VideoSignaling::generation()` on every candidate envelope and use
`answer_generation` for the matching answer. Old handles reject candidates and
end pending polls without closing the new peer; old answers cannot satisfy the
new phase. Each generation retains the existing 64-candidate direction limit.
Cancellation after committing reconfiguration closes the peer and its adapters.
The API performs no HTTP, grants no authority and must not log SDP/credentials.

This is one startup upgrade, not credential renewal. STUN-only initial peers,
canceling an unreachable STUN gather without delaying fallback, general refresh/
handover, and host/phone/desktop-viewer protocol coordination remain required.
Internet-direct discovery needs an explicit UDP STUN URL in this pinned SDK;
an empty-server LAN test does not establish Internet-direct cost or coverage.

## Failure and fix

With actual host-to-host UDP dropped, the new late UDP/TCP/TLS cases allocated
successfully and reached ICE/DTLS Connected, gathering completion and RTP write,
but delivered no payload. An initially configured one-sided UDP baseline failed
the same way. Sender RTP accounting increased by five payload bytes, the receiver
had a nominated pair, but no receiver track was created. Bounded diagnostic
logging reported an SRTP-context error after the write. It prints only a fixed
category and Rust module, never the underlying message/addresses/authentication.

The raw socket path processed an entire UDP/GRO or TCP batch before pumping core
events. DTLS completion installs SRTP keys through those events; a following RTP
datagram in the same burst could therefore be processed without its keys. The
driver now pumps and dispatches those events after control datagrams while
retaining the existing core lock. RTP/RTCP uses the core's RFC 7983 byte range
and keeps its ordinary end-of-burst event drain. No extra immediate event pump
is added to steady media. This is distinct from the existing track-event-before-
media drain on the TURN decapsulation path.

Source `d49fbe8532ebb4be82660210aeb397311f1e145f` changes only `driver.rs` from
`b01f93e53f7189e810d525759f7cca037556859b`; the test source and oracles are
byte-identical. All eight cases pass at d49. The control-only refinement also passes the identical eight-case scope and
strict Clippy at `5229daca7471b6f041013a4bfe0eb67982c4475c`.
An additional exact HEVC loopback passes at that same source: nine focused
tests in total, zero failed/ignored. The application dependency guard also
passes at e9d2b0a. `extra-artifact-pack.json` retains these separate results,
commands, clean source checks and independently verified file hashes.

| Source | Evidence |
| --- | --- |
| c335419 | Build fails on ownership of a Drop type; no tests execute |
| 345b9fb | Direct late configuration, close and ordinary H.264 loopback pass; strict Clippy passes; three forced late TURN cases fail |
| 9794c3e, 0d32068, bada37c | Focused failed UDP diagnostics locate the stall after Connected/gathering/RTP write |
| 908a83d | Diagnostic test build fails on a counter field path; no tests execute |
| 9458dfc | Initially configured one-sided UDP and late UDP both fail payload delivery |
| b01f93e | One-sided baseline fails with the redacted SRTP-context category; build passes |
| d49fbe8 | Four actual-coturn cases and four ordinary cases pass; strict Clippy passes |
| 5229dac | Control-only pump refinement: eight cases pass; strict Clippy passes |
| 5229dac, extra scope | Existing HEVC loopback passes; application e9d2b0a dependency guard passes |

The forced cases are the initially configured one-sided UDP baseline and late
UDP, TCP and verified-TLS configuration. Each passing d49/5229 case delivers the
synthetic H.264 NAL payload over the nominated native relay transport, with
exactly one coturn allocation, two permissions, four channel bindings and 36
actual host UDP drops. The receiver has no TURN allocation. Merely configuring
TURN, or reaching Connected, is never the media oracle.

The other four exact cases cover late configuration/direct media with fresh ICE
credentials and stable DTLS identity, stale signaling fences, close while a
restart answer is outstanding, the ordinary H.264 payload/capture-clock/feedback
boundary, and bounded candidate queues. They count as four tests because the
late-configuration case combines its direct/fence assertions. Builds target
only the library test binary; strict Clippy targets library/examples. No suite
or application build is implied.

## Isolation and limits

Tests use an owned Linux cloud checkout/target and rootless user/network
namespace. Its loopback-only coturn fixture creates fresh REST credentials,
private CA/key and SQLite state, allows only the named test loopback peers and
drops actual direct host UDP. Per-process-group deadlines and an outer namespace
deadline supervise execution. All child groups are gone, private state deleted,
ports rebindable inside the namespace and the xtables lock removed after both passing runs.
The bare network-namespace command initially fails EPERM; it is not test coverage.
Hosted continuation reconnects the existing session when idle direct execution
is unavailable. No Docker container or host firewall rule is created.

Artifacts retain exact sources, commands, exits, counts, safe logs and hashes,
including earlier failures. Private configuration, keys, SQLite contents, HMAC
values, SDP/ICE, unsanitized coturn logs and binaries are excluded. These cases run no screen,
input, native codec, decoder, physical-device, NAT, VPS or visual check.
The five-byte synthetic payload proves transport integrity, not video quality.
No CPU/RAM, bandwidth, power, startup-latency or end-user saving is measured by
these tests. No optimistic performance target changes.

The exported bundle is 140,510 bytes, SHA-256
`754df3c1bda7c307aa34bfe79540a2256eb02d217626d00689796650b99efc65`.
Its 61 file entries have independently checked hashes after transfer. The
manifest declares eight absent prior paths: the 345b9fb build lives in its
separate build directory, which is included, and the 908a83d fixture artifacts
never existed because compilation failed. Those absences are not passing tests.
The bundle excludes every coturn log and private generated file. It includes
the exact isolated fixture scripts and safe allocation/cleanup summaries.
The additional pack is 3,280 raw bytes, SHA-256
`bad21d9867fc9ca26e9c40aa981202de77ab0db587f8f32da3c11f6acdb05aac`.
Its transfer metadata and nine internal file hashes are checked independently.
