# Shared host and Windows provider evidence — 2026-11-05

Application runtime: `a519edf252ff0fcbcb65c4d74434228f0c4b52fb`.
Custom media runtime: `f81e553ba5c1da15223c3374e57d44531c64ca8c`.
Phone fixture: `f64d3611bb91b6350bb704059e0839ae809e304d`.

The app now reuses its admitted-view lifecycle for Mac and Windows, including
cancellation before geometry discovery, exact-generation retirement, signaling,
recovery and close. Windows native capture/encoding/input is wired into the
factory. This is draft implementation, **not native Windows qualification**.
The fork adds coalesced wake notifications, monitor proof, selected-rate updates
and optional QPC capture age. Native callbacks/queues remain bounded.

## Exact-source results

Each JSON pack embeds named text files with exported SHA-256, original SHA-256
and redaction status, plus commands, source-clean checks, deadlines and exits.
No result is a whole-suite or current-documentation-head pass.

| Pack | Source | Result and scope |
| --- | --- | --- |
| shared-host | c0d7ca9 | Mac desktop_media check/strict library Clippy/dependency guard pass. No runtime test. |
| provider-initial | 50f198c | Mac provider+host check/Clippy, build, 15 host/2 pure-policy/1 clock tests pass. Strict test Clippy fails on three new lint sites; later guards did not run. |
| provider-retest | cb6fc98 | Check/Clippy/build and 15+3+1 tests pass; strict test Clippy/dependency guard pass. State guard fails on the protocol DTO capability bits. Runtime guard did not run. |
| provider-final | 5482fe9 | Mac provider+host check/Clippy/build; **15 host, 3 Windows pure-policy, 1 unknown-clock tests pass**, none failed/ignored. Strict test Clippy and dependency/state/runtime/diff guards pass. DTO annotation describes validated wire bits; checker/allowlist not loosened. |
| provider-marker | a519edf | Build and **4 Windows pure-policy tests pass** on Mac; strict test Clippy and dependency/state/runtime/diff guards pass. These are scan-map/coordinates/scroll/marker tests, not Windows OS input. |
| api-initial | b135337 | Windows API-boundary metadata/check from Mac pass; strict Clippy fails on input byte-range guard. |
| api-final-failed-fixture | 5482fe9 | API check passes; strict copied-workspace Clippy fails only on six fixture `unimplemented!` stub sites. Preserved as failure. |
| api-final | a519edf | Metadata, Windows-target API check and strict copied-workspace Clippy pass with only the fixture stub module allowing `unimplemented`. Actual Windows provider/input/keys/geometry/statistics/protocol and real default-feature fork media are checked. **Host/authority/wake types are stubs**: no full app build, link, execution or authority evidence. |
| media-wake | f7914d8 | Default-feature Windows-target library/examples check and strict library/examples/tests Clippy pass from Mac. No tests executed. |
| media-final | f81e553 | Same target check/strict Clippy pass from Mac. No tests executed or Windows native linking/capture/encoding. |
| native-mac-phone | a519edf + phone f64d361 | Actual native Mac target build/strict Clippy pass; **one Rust and one Swift scenario pass**, zero failed/skipped. Real capture/Noise/Connect/phone H.264 decode, finite control lease, consumed modifier release and signed revocation. |

At 5482fe9 and a519edf, shared-host/Mac/geometry/statistics inputs have identical
tree/blob identities (`comparison.json`); previous tests are still attributed
to the source that actually executed them. Windows pacing and marker changed
at final source. The final API check and native Mac/phone run use final source.

## Native Mac/phone observations

The actual phone AppModel uses local-network Connect with real Noise, one
receiver and nominated direct media (peer-reflexive/host), DTLS connected,
generation zero and no TURN-issuer calls. The short observation ends with 159
decoded H.264 frames at 1670×1080 and 1,551,799 bytes received; these are scenario
counters, **not matched bitrate/quality, active decoder identity or sustained
performance results**.

Only the phone control-pump clock is frozen; host time, watchdog, capture and
transport stay real. The 2000ms host lease releases held Shift after 2011ms.
Stale input adds no OS event; frames increase from 52 to 117 while the view stays
on the same receiver/display. Viewing without control lasts 538ms, followed by
explicit control reacquisition and sustained renewal. Signed revocation ends
the view and releases the new held Shift without automatic reopen. The active
backend-PID-filtered observer consumes exactly four Shift events; user events
are not consumed. No general pointer/text/IME delivery is established.

`observations.json` is embedded in `native-mac-phone.json`. The native private
build JSON is represented by selected compiler records plus its full-log hash;
the executable is hashed. Private token config is deleted; xcresult and screen/
encoded payloads stay private. Public fixture-token occurrences in phone logs
are redacted, retaining original and exported hashes.

## Supervision and reproducibility

All builds/tests ran under the existing shared build lock, nice 19, two build
jobs, isolated targets and external TERM/KILL/reap process-group deadlines.
Sources were frozen and checked clean around commands. Eleven packs retain
144 hashed text exports, including fixture sources/generator. The exported
`api-generator.json` is not a full app harness; archived fixture manifests and
sources include the exact copied workspace lint configuration.

`cleanup.json` independently verifies all **97 original process groups absent**,
owned HTTP port rebound, token config deleted and owned Simulator Shutdown.
The shutdown command returned **149**, since that Simulator was already shut
down; the original confirmation and independent inventory both prove Shutdown.
It is not represented as an exit-0 command. The evidence exporter is archived
as `exporter.py`; `index.json` covers exported files and hashes.

`pin-comparison.json` proves no app package/version additions/removals: only
three paired custom Git source IDs and desktop_media's reference to existing
windows 0.61.3 differ. Mac media, transport, vendored RTC/SDK and media lock trees
are unchanged. This does not imply unexecuted tests passed.

## Required native qualification

No Windows-native job ran. Remaining checks include full Windows app compile/
strict Clippy, link/runtime, actual capture/conversion/color oracle, startup and
idle PLI, close/lock/resize/hotplug, same-device textures/release leases, CPU
fallback, successful and blocked input/cleanup, viewer/host composition and
HEVC/AV1 hardware/receiver admission. Physical phone/LAN/cellular/NAT/Internet
and sustained CPU/RAM/GPU/power/input-latency/matched text-motion-color-bandwidth
qualification remain open. Registration/device input are not active-GPU proof.
Linux hosting/viewing, deployed relay/signing/quota/certificate migration and
other full-goal requirements remain unfinished. No target was relaxed, no
formatter/visual verification/full local suite/deployment or merge ran.
