# Apple Silicon AES/CTR component comparison

At exact clean source `e3cbe96bff0c07f29a6007b71fd3977275e71cc0`, the isolated
`media/tools/aes-probe` workspace built the same release-opt3 source into separate
owned targets with default flags and with task-scoped `--cfg aes_armv8`.
All 51 command steps exited 0, including 48 measured executions. Each binary
checked the independent NIST CTR vector and decrypt round trip before timing;
each paired synthetic-output checksum matched. The raw pack includes every
sample, source assertion, process-group watchdog, command, exit and binary hash.

The Mac is the same Apple M5 Pro/macOS 27.2 host used for private native media
checks. The component run records Cargo/rustc 1.99.0, uses the shared lock,
nice 19 and two build jobs. No desktop capture, media, credentials, screenshots
or Windows execution occurs in this experiment. Neither global flags nor the
application's target was changed for the comparison.

The versions match rtc-crypto 0.21.1's AES-CM payload component: AES 0.8.4 and
CTR 0.9.2. Each packet copies its synthetic bytes, constructs CTR state with
the key and a nonoverlapping counter range, and applies the keystream. Warmup
is 1000 packets before each timed execution. Timings include process CPU and
wall time; checksums prevent dead-code removal. Final-payload checksum work is
included in the timed call and negligible relative to each packet batch.

| Payload | Packets/run | Default CPU median | ARM candidate CPU median | Median paired CPU reduction |
| --- | ---: | ---: | ---: | ---: |
| 32 bytes | 200,000 | 116.008 ms | 10.742 ms | 90.70% |
| 1200 bytes | 40,000 | 153.405 ms | 4.334 ms | 97.16% |
| 4096 bytes | 12,000 | 134.410 ms | 3.138 ms | 97.66% |

Eight pairs per payload alternate which binary runs first. Every pair favors
the candidate, and all ranges are retained. Short candidate batches make small
differences noisy; these large component differences do not establish an
overall media or application speedup. The default executable has no ARM AES
instruction lines; the candidate contains 200, with examples in the binary pack.
The host reports AES instruction availability. AES object size remains 704
bytes in both builds, so no memory reduction follows from this comparison.

The application sample that motivated this probe reached software fixslice
under actual SRTP encryption. This standalone comparison does not run the
whole SRTP context, HMAC, key derivation, RTP, DTLS, sockets, codecs, capture or
receiver. It does not measure bandwidth, whole-endpoint CPU, GPU, power or
performance-target qualification. No target changed. Full native interoperability
after enabling runtime detection must be recorded separately; it is not supplied
by a matching AES vector alone.

Following this measurement, the fork and embedding application's Cargo configs
enable `aes_armv8` only for `aarch64-apple-darwin`. This enables runtime detection
with the library's constant-time software fallback; it does not globally require
AES instructions. Dependency Cargo configs do not propagate, so embedders need
their own target setting. x86 and other targets retain existing settings.

Sources: [exact AES implementation and runtime detector](https://github.com/RustCrypto/block-ciphers/blob/aes-v0.8.4/aes/src/lib.rs),
[NIST CTR-AES128 vector, F.5.1](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication800-38a.pdf).
