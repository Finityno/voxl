# Relay interface allocation evidence

All results use the custom fork and one Linux cloud session:
`01a1a3f0-1541-7477-8ab6-beaf03e5b067`. These are selected sanitized raw logs
exported by the cloud agent; original full logs remain in the corresponding
`/workspace/voxl-relay-fanout-validation-<short-sha>/` directories.
Each source directory contains `artifact-pack.json`, preserving the exported
filename-to-text mappings, including exact newlines and command whitespace.

At `a3a6a13a741b8ab6bef7c285d6f28a3ffe955f6b`, the locked prebuild, two
interface-policy units, direct H.264/HEVC and STUN tests passed. Four coturn
cases failed before media because the new test counted two SDP rows as two
allocations. The fifth fixture, unknown-root rejection, passed. Clippy found
one return-of-binding spelling, fixed in the next source. The failed results
are preserved; they do not count as passing media evidence.

At `a5e0f7cfe3906a3185a869ce70e8154693c3fc61`, the prebuild, all five coturn
fixtures and strict library/examples Clippy passed. Each peer had two local
IPv4 bindings but one unique relay transport/address/port. Its two SDP rows
represented components 1 and 2 on that same allocation. Coturn independently
recorded eight successful allocations across the four media scenarios, plus
10 permissions and 16 channel binds. The certificate-rejection case created
no allocation. The source was clean before and after both jobs.

The independent comparison at `d8a751af9212fddae81096638bbfe38006cd0c32`
passed its build, selected actual-coturn test and strict Clippy. With two
bindings on each side, the optimized sender used one allocation and the
default SDK receiver used two. Coturn independently recorded three allocations,
four permissions and four channel binds; actual encrypted media and cleanup
passed. The optimized receiver in the preceding run used one. This demonstrates
two-to-one allocation reduction for this topology, while the SDK's default
policy still preserves its previous behavior. The comparison's full logs are
under `/workspace/voxl-relay-comparison-d8a751a/`.

Forced UDP, TCP and trusted TLS carried both synthetic NAL payloads over
nominated relay paths and passed cleanup assertions. The automatic TCP case
kept both host candidates and delivered direct video after closing its unused
relay. TLS rejected an unknown root. The fixture used fresh REST credentials,
a private CA, owner-only state and loopback exceptions only. Every child group
was gone afterward, ports rebindable and private keys/config/DB deleted.

These assertions concern allocations and synthetic encrypted transport, not
decoding, physical NAT traversal, native capture, CPU/RSS savings or capacity.
Real UDP preserves explicit alternative bindings when the OS-selected route
is unavailable, so the allocation reduction is conditional on a known bound
route. Browser viewers own their separate ICE policies. Trickle ICE, credential
renewal/restart, real hardware and sustained resource targets remain open.
