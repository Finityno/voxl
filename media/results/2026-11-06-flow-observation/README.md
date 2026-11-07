# Bounded numeric flow observations

Exact final source `0a252d18af96ade1bf13d6319c45c61edef87238` passes five
new flow tests, six debt and 34 adaptive tests, test compilation and strict
library/tests/examples Clippy. The initial source passed all selected tests but
failed one spacing lint; both attempts and all 18 ended command groups remain.

A real UDP/DTLS/SRTP/TWCC loopback proves that the SDK's feedback identities
match paced-release metadata. Controlled clocks cover lost and repeated
reports, distinct retransmission departure identities, visible 512-slot eviction,
frame markers, retirement and close. No RTP payload/address/remote absolute
clock is retained. Default monitoring is disabled. Pending means not yet matched
to a feedback report; it does not prove a packet remains in the network.

These counters observe behavior and do not change controller policy, admit
codec performance or prove queue/quality/CPU/RAM savings. `verify.py` checks
exact-source receipts/hashes offline; it does not repeat native execution.
