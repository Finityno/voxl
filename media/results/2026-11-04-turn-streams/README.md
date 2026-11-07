# Forced relay transport evidence

Linux cloud, isolated rootless coturn 4.6.1-2; listener and relay bind only to
127.0.0.1. Fresh ten-minute REST test authentication and a disposable CA with a
signed IP-SAN server certificate are generated per fixture. Production trust
roots reject that certificate; test-only trust injection enables the positive
TLS case. No VPS policy or account data changes.

At exact source `6c3fc43e7184dba53d6ba8745aee958e957d3345`, five selected
integration tests passed after locked library build and strict Clippy:
forced UDP/TCP/TLS, untrusted-certificate rejection and direct video surviving
loss of an unused TCP relay. Raw artifacts are in `6c3fc43/`.

At exact source `32569b3f6816353f85e5d7a453099dd1a565996b`, the adapter's
2000-byte limit also passed both framing/boundary tests and those five
integration tests: seven passed, none failed or skipped. Locked library build
and strict library/examples Clippy passed. Raw artifacts are in `32569b3/`.

The three direct H.264/HEVC/change-wait regressions passed separately at
`8596aea50d372f606dd8b39d170a901bf5e1fde5`; do not count them as additional
tests executed in either later run. Native decode and capture are not tested.

Both fixtures terminated/reaped their owned process groups, removed private
credentials, keys, configuration and database, and verified ports rebindable.
Only sanitized commands, stdout, counts and cleanup metadata are retained.
No CPU, memory, bandwidth or physical-device performance improvement is claimed.
