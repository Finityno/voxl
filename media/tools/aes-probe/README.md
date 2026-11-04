# AES/CTR component probe

This independent tiny workspace pins the AES 0.8.4 and CTR 0.9.2 used by the
current rtc-crypto AES-CM path. It copies a synthetic payload and creates a fresh
CTR cipher per packet, matching that component's setup. It does not execute
SRTP key derivation, HMAC, RTP, DTLS, a socket, capture or a codec. Fixed public
test inputs never leave the process; no production secret or media is read.

Build the identical source twice into separate owned targets, once with defaults
and once with task-scoped `CARGO_ENCODED_RUSTFLAGS=--cfg\x1faes_armv8`. Use an
external process-group deadline, the shared build lock, nice 19 and two jobs.
Never change global flags or the application target for this experiment.
Run `<binary> <payload-bytes:1..4096> <packets:1..2000000>` in alternating order
for both builds, retaining commands, binary/source hashes and every sample.
The NIST CTR vector and decrypt round trip run before timing. Warmup is 1000
packets, then process CPU and wall time are measured with a bounded checksum.

`armAesAvailable` reports CPU instruction availability, not selected decoder/GPU
use. The build flags and generated instructions establish the candidate code
path. This experiment alone cannot establish whole-SRTP, endpoint CPU, energy,
bandwidth or overall application speedup. It does not modify production builds.

Sources: [pinned AES implementation and runtime detection](https://github.com/RustCrypto/block-ciphers/blob/aes-v0.8.4/aes/src/lib.rs),
[NIST SP 800-38A, F.5.1](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-38a.pdf).
