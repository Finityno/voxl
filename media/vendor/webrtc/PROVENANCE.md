# Pinned WebRTC async driver

Imported library sources from crates.io `webrtc` version 0.21.1, whose packaged
VCS metadata records commit `d658b5a803dd8dc389547450e245877310c9cbed` from
https://github.com/webrtc-rs/webrtc. No upstream submission is made by this fork.

The downloaded crate archive was verified against the registry lock checksum:
`b621e736302bef784948cc8d8099fd0a8fdb5757693c80a344ffbe38c49387d2`.
MIT and Apache licenses are preserved verbatim. This copy includes `src`, the
normalized and original package manifests, README, licenses and VCS metadata. Example and
integration-test target declarations are omitted because those targets are not
vendored. Numeric GitHub issue references in source comments use bare numbers
to comply with this fork's reference policy. Production dependencies and their
version constraints are unchanged. Defaults remain disabled by the parent.

The maintained behavior change is in `peer_connection/driver.rs`:
drain pending TURN writes after draining core writes, before the driver waits
for another event. Core media can enqueue a relay datagram after the earlier
TURN drain; leaving that datagram queued makes an idle final frame wait for an
unrelated network or timer event. Direct UDP/GSO behavior is unchanged. The
forced-relay tests in the parent media crate cover actual coturn transport.

When updating, verify the new package checksum, preserve licenses, reapply this
small driver change and rerun the forced UDP/TCP/TLS relay fixtures. URL parsing,
loopback transport tests and native cross-compilation do not replace them.
