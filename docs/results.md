# Results

One section per plan task. A task with no recorded result is not done. **Verified** means
the plan's check was run and passed; **Partly** says what ran and what is left; **Pending**
needs something this work did not have (the device, a TX module, the robot, or a decision).

Environment of the checks below: macOS 26 on Apple Silicon (host), Docker on Colima with an
aarch64 Ubuntu 24.04 / ROS 2 Jazzy container (Linux, uinput available), Qt 6.11 on the host.
Nothing has been run on the R36S, on a real TX module, or on the robot.

Summary of what the automated suites cover today:

| Suite | Where it ran | Result |
|---|---|---|
| ROS: `elrs_joy_crsf_protocol`, `elrs_joy_crsf_node`, `kvn_status`, `kvn_video_streamer`, `kvn_robot_bringup` (colcon test, Jazzy) | aarch64 Linux container | 108 tests, 0 failures |
| Rust workspace (`cargo test --workspace`) | macOS host and aarch64 Linux container | all pass: daemon 35 unit + 12 engine + 6 end-to-end, `elrs_crsf` 7 fixture + 2 robustness, `test_tools` 5 |
| Real evdev input against a uinput virtual gamepad (`RC_TEST_UINPUT=1`) | aarch64 Linux container, privileged | pass, including the exclusive grab |
| UI backend tests (`ctest`) | macOS | 15 cases pass (UI agent's run) |
| Broken golden fixture makes the tests fail | macOS | confirmed (2 tests fail), fixture restored |

---

## Phase 0

### T0.1 Fix the turbo-button mapping — Partly
`teleop.yaml` now has `enable_turbo_button: 1` (AUX2 = `buttons[1]`), matching its header, and
design §5.1 no longer carries the inconsistency note. **Pending**: the on-robot check (AUX2
high gives 2.0 m/s on `/cmd_vel`, AUX4 has no effect).

### T0.2 Wire the TX module to the internal UART — Pending
Needs the device and module. `hardware.md` has the wiring diagram and the table to fill in.

### T0.3 Choose the robot camera chain — Pending
Needs the robot. `kvn_video_streamer` takes the camera source and the encoder as parameters,
so the choice is configuration. Encoder properties (Annex B, no B-frames, keyframe ≤ 1 s) are
tested with `x264enc`; hardware encoders are not.

### T0.4 Set acceptance budgets — Partly
`budgets.md` holds the proposed table plus the two choices made while building (link-lost
alarm timing, arm hold time). **Pending**: your review and sign-off.

### T0.5 Identify board revision and panel — Pending
### T0.6 Choose the WiFi dongle — Pending

### T0.7 Choose the screen layout — Pending (decision needed)
Not decided here. The UI is built to the design's recommendation, **Option B (Instrument
Split)**, with the layout in its own QML component (`LayoutB.qml`) so A or C can replace it.
Screenshots: `screenshots/{full,degraded,menu,elrs_lost,lq_degraded}.png` and `_light` variants.

---

## Phase 1

### T1.1 Build or flash both candidate images — Pending
`image/armbian/` (config, `customize-image.sh`, README) and `image/darkos/provision.sh` are
written and syntax-checked, **not built or flashed**. They assume package names and boot
file locations that must be checked on the first build.

### T1.2 Cross-compilation toolchain — Partly
* `cargo build`/`test` of the whole workspace, including the C++ CRSF library through `cxx`,
  works natively on aarch64 Linux (Ubuntu 24.04 container). `remote_controller/.cargo/config.toml`
  sets the cross linker and `CXX_aarch64_unknown_linux_gnu`.
* `image/build.sh` builds daemon, tools and UI in a Debian trixie arm64 container;
  `toolchain-aarch64.cmake` and `sysroot.sh` are the x86 cross variant. **None of these ran**
  (the Docker disk had no room for a trixie image).
* **Pending**: the build running on the device.

### T1.3 Internal UART bring-up — Partly
`systemd/99-elrs-tx.rules` (`/dev/elrs_tx`), the two units and `test_tools/crsf_capture`
exist. The capture tool read 93 valid frames, 0 CRC errors, from the fake module (LINK_STATISTICS,
DEVICE_INFO after ping, battery, flight mode, sync). `systemd-analyze verify` passes on both
units. **Pending**: everything on the device (the UART resolves, `stty 921600`, 5 reboots).

### T1.4 WiFi and ZeroTier — Pending
### T1.5 Lichtblick on device — Partly
`lichtblick/build.sh` (pinned v1.29.1) and `layout.json` are written; neither was run or
loaded in Lichtblick. The UI hosts a `WebEngineView` with a watchdog; it was exercised only
with a local stand-in page on macOS. **Pending**: all device measurements (GPU, H.264 fps, RAM).

### Gate G1 — Pending
Needs T1.1–T1.5 on both images.

---

## Phase 2

### T2.1 CRSF library: handset-role extensions — Verified
* `0xEA` accepted in `VALID_SYNC_BYTES`; `OpenTxSyncPayload`/`OpenTxSyncMessage` (RADIO_ID 0x3A
  sub-type 0x10); `parameter.hpp/.cpp` (typed parameter-entry parser, chunk assembler, value
  encoder).
* 15 golden fixtures in `test/fixtures` (`generate.py` builds them from the spec, independently
  of the C++ code). All decode and re-encode byte-exact. They are **synthetic**: the plan
  asks for hardware recordings "where possible"; replace a file with a capture (keep the name,
  note `# source: hardware`) when you have one.
* Found and fixed a bug on the way: `Message::validate` read before the buffer for a length
  byte below 2 (regression test added).
* The existing `crsf_joy_node` tests still pass.

### T2.2 Rust bindings — Partly
`elrs_crsf_sys` (cxx facade + `build.rs`) and `elrs_crsf` (safe API) are done. `cargo test`
passes, including every fixture through the bindings and 50 000 randomized valid-CRC frames.
Cross-built test binary on the device: **pending**.

Fuzz: 10 minutes, 19.1 M executions, no crash (details at the end of this file). The C++ side
had coverage instrumentation but not ASan (Apple clang's ASan runtime does not link with Rust's);
the CI job runs on Linux, where it does.

### T2.3 `crsf_joy_node` forwards robot status — Partly
`telemetry_status_enabled`, `status_topic`, `status_period_ms` (500), `status_timeout_ms` (2000)
added; the string is cut to 15 characters and non-printable bytes become `?`; sent on change
and every 500 ms, and not repeated after the source goes quiet (so the handheld marks it
stale). Four new unit tests check the serialized frame, truncation, repeat/stop and disable.
**Pending**: the stock EdgeTX handset check on hardware.

### T2.4 Status node — Partly
`kvn_status` (30 tests): severities, fault beats warn, oldest wins ties, stale input → FLT, the
15-character limit, node-level tests with fake diagnostics. In a Jazzy container the node
published at 1.995–2.000 Hz and `FLT:JOY` when `/joy` went silent. **Pending**: killing the
real hwmon node on the robot.

**M2 check** — Pending (needs the robot and a handset).

---

## Phase 3

### T3.1 Test tools — Verified
`fake_tx` (pty; answers ping and parameter reads with a chunked tree, applies writes, emits
`OPENTX_SYNC` and telemetry, logs RC frames, can replay frames from a hex file, can "lose power")
and `virtual_pad` (uinput, scriptable). Verified: `fake_tx` answers a ping from `elrs_crsf`;
`virtual_pad` is read by the daemon's evdev reader in the Linux container. Both run on a desktop
and in CI (`virtual_pad` on Linux only). The `--replay` mode is implemented but has no test.

### T3.2 Daemon serial and TX loop — Partly
SCHED_FIFO + `mlockall`, absolute deadlines, frame period from `OPENTX_SYNC` (interval, plus the
phase offset applied once and clamped to half a period). Against `fake_tx`: the period
follows sync changes (4 → 6.67 → 2 → 4 ms in the engine test, 4 → 10 ms with real threads and a
real pty), converging within the 1 s limit; absurd intervals are clamped. **Pending**: the scope
histogram on the device.

### T3.3 Input mapping — Partly
`mapping.toml` produces the §5.1 contract; tests cover neutral, full deflection per axis, each
AUX source, hat buttons, calibration. The exclusive grab is verified with uinput (a second
reader sees no events). **Pending**: the on-device stick calibration. The axis ranges in
`mapping.toml` (±1800) are placeholders.

### T3.4 Safety state machine — Verified
Pure module, 14 tests, including: starts disarmed and silent until sticks are neutral; refuses
to arm off-centre; the 1 s hold; AUX1 only while armed, R1 held and menu closed (R1 must be
released and pressed again after arming and after the menu); L1+R1 disarms; the menu forces
neutral; overrides expire, are bounded while armed, never reach AUX; losing the pad disarms.

### T3.5 Telemetry model — Partly
Link, battery and status with ages and staleness; alarms `elrs_lost`, `elrs_degraded`,
`serial_error`, `input_lost`, `battery_stale`, `status_stale`. Tested against the fake module,
including: alarm within 1 s of the module going silent (engine test with a simulated clock; the
threaded test asserts < 1.1 s), cleared on recovery, replayed to a late client. **Pending**: staleness
thresholds against a real module's telemetry rates (budgets §5.2, A4).

### T3.6 Module configuration — Partly
Discovery (ping → DEVICE_INFO), full tree read with chunk assembly and retries (also with every
3rd reply dropped), confirmed writes with range checks. Against the fake module's model only.
**Pending**: comparison with the real module's Lua/web UI and the power-cycle persistence check.

### T3.7 IPC server — Verified
`docs/ipc.md`; Unix socket server; `ctl` client. On a desktop with the real daemon: `ctl rate`
reports 9.96 Hz telemetry; `param set` is acknowledged and visible in the next `params`; `arm`,
`set_aux1` and `override_set` on channel 4 are rejected with `ok: false`; tests sweep every
channel and value of `override_set` and assert AUX1/AUX2 never leave low without the
physical gesture; `kill -9` of a client and a stalled client leave the frame period unchanged
(period drift < 0.8 ms in the test).

### T3.8 Native UI — Partly
All screens built (full, degraded, menu with the module-settings tree from `params`, ELRS-lost
alarm with tone, light theme). Screenshots in `screenshots/`. **The real UI was run against the
real daemon and fake module** (`live_daemon_degraded.png`, `live_daemon_elrs_lost.png`): live link,
battery and status tiles, the loud gamepad-lost banner, and the full-screen ELRS-lost screen with
stale values once the module went silent. **Pending**: eglfs on the device, outdoor readability,
the gamepad-only menu pass with real buttons, and the "pull the module's power" test (the same
alarm path was verified with the fake module).

### T3.9 Image provisioning and supervision — Partly
`rc-control-daemon.service` (`Restart=always`, 200 ms, real-time, no network ordering) and
`rc-ui.service` (`Wants` only, so neither can stop the other) pass `systemd-analyze verify`.
Provisioning scripts are written, **not run**. **Pending**: boot test with no network, `kill -9`
of the daemon and the UI on the device.

### T3.10 CI — Partly
`.github/workflows/remote_controller.yml`: ROS colcon tests, `cargo test`, the uinput test, a
2-minute fuzz, the aarch64 cross-build, the UI build with `ctest`, unit and script checks. The
jobs' commands were each run locally (see the table at the top), but **the workflow itself has
not run on GitHub**. The broken-fixture check was done locally.

**M3 check** — Pending (needs everything above on hardware).

---

## Phase 4

### T4.1 Bridge hardening — Verified (in a container)
`kvn_foxglove_bridge.launch.py`: capabilities `['time']` only, whitelists for topics, services,
parameters and client topics, `send_buffer_limit` 4 MB. With the real `foxglove_bridge` 3.6.0 on
Jazzy, `bridge_check.py` exits 0: only whitelisted topics advertised, a client publish to
`/cmd_vel` never reaches the robot (27 messages seen, 0 injected), service and parameter
access refused. A negative control (stock bridge) makes it exit 1 with 5 failures. Note: that
bridge version speaks `foxglove.sdk.v1`; the checker offers both subprotocols.

### T4.2 Video streamer — Partly
`kvn_video_streamer`, 23 tests: Annex B, no B-slices, IDR gaps ≤ 1.15 s with SPS/PPS before each,
nothing encoded without a subscriber, a late subscriber gets an IDR within 0.7 s (the forced
keyframe path is exercised: the test fails with it disabled). **Pending**: hardware encoder,
`v4l2src`, the < 2 % idle CPU and Lichtblick playback.

### T4.3 Robot CPU isolation — Partly
`kvn-ros.env` (`ROS_AUTOMATIC_DISCOVERY_RANGE=LOCALHOST`), `kvn-control.service`,
`kvn-wifi.service` (`Nice=10`, `CPUQuota=150%`, ordered after `zerotier-one`),
`joy_cmdvel_jitter.py`. Units verified with `systemd-analyze`. **Pending**: the jitter
measurement under load and `ros2 node list` from a VPN laptop.

### T4.4 VPN and firewall — Partly
`nftables-kvn.conf` (port 8765 only on `zt*`): `nft -c` passes and a real load/reload works in a
container. **Pending**: the LAN-vs-ZeroTier `nc` check and survival of a reboot.

### T4.5 Lichtblick integration — Partly
`LichtblickWeb.qml` with watchdog and native "no video link" panel, started only when the bridge
is reachable; WebEngine optional at build time (`RC_UI_WEBENGINE`). **Pending**: everything
that needs real Lichtblick on the device. The native-video alternative (G1 rule 2/3) is not
written.

### T4.6 Link-health indicators — Partly
`NetStatus` polls `zerotier-cli` and probes the bridge; "unknown" if the CLI is missing; quiet
severity only. Parsers have unit tests. **Pending**: a real access point and relayed path.

---

## Phase 5

A1–A5 not started: they run on the target hardware with the provisioned image.

---

## Handheld deployment on Armbian noble (T1.1, T1.3, T3.9) — Partly

`image/noble/` builds the arm64 noble binaries in a container (`build.sh`, about 10 minutes with a cold cache) and provisions the device over SSH (`deploy.sh user@host`, with `RC_SUDO_PASSWORD` for sudo): stages resize, packages, desktop, files, uart, verify, with automatic reboots. Everything installs under `/opt/kvn_remote_control`; the systemd units and the udev rule are links to it. Run on the real R36S (Armbian 25.08 noble, kernel 6.12.32) on 2026-10-09:

Verified on the device:

* The root partition grew from 5.2 GB to the whole 59.5 GB card (`sfdisk -N` + `partx -u` + `resize2fs`, online).
* UART2 is free: `console=ttyS2` removed from `/boot/u-boot/boot.ini` (backup `boot.ini.rc-backup`), `serial-getty@ttyS2` masked, `/dev/elrs_tx` -> `ttyS2` after the reboot. The ELRS module itself was not attached.
* `control_daemon` runs as `rc`, disarmed, found the pad (`r36s_Gamepad`, `/dev/input/event3`), opened the serial port and sends frames at 4 ms with 175 us p99 period jitter; `ctl state` answers over the socket.
* `rc_ui` runs on `eglfs` as DRM master with no QML or display errors, 3% CPU and 133 MB RSS when idle, 340 MB RAM free.
* `webrtcsrc` loads from `/opt/kvn_remote_control/gst-rs`; `v4l2slh264dec` (Hantro) and `avdec_h264` are present.

Problems found and fixed on the way:

* `parted` refuses to resize a mounted partition non-interactively (the script now uses `sfdisk`).
* `/tmp` is cleared by a reboot, so `deploy.sh` copies the files before every pass.
* The UI showed only the text console: `rc` is not the first opener of `/dev/dri/card0` (plymouth is), so setting DRM master failed with EACCES; `rc-ui.service` now has `AmbientCapabilities=CAP_SYS_ADMIN`. That was not enough at boot: the UI started while plymouth still held the display and never retried, so it worked after a manual restart but not after a reboot. The unit is now ordered after `plymouth-quit.service` and `plymouth-quit-wait.service` and runs `plymouth quit --wait` first; after that, a real reboot gave `master y` and no atomic-commit errors.
* The UI's `Theme` and `Fmt` singletons were undefined: Qt 6.4 does not embed a module `qmldir` for an executable, and the earlier tests had found one next to the build tree. `ui/qml/qmldir` now declares them, and the check is to run the binary alone, outside the build directory.

Not verified:

* The picture on the screen: I cannot see the display, so the user has to confirm it (no UI screenshot from the device yet).
* Alarm tone: `rc` has no PulseAudio session (`pa_context_connect() failed`, "No audio device detected"), so the loud alarm may be silent.
* H.264 decode through the Hantro decoder with a real WebRTC stream, and the CPU cost of either decoder.
* ELRS module on UART2 (CRSF at 921600 baud, T1.3 reboot test), the WiFi/VPN path (no WLAN or ZeroTier installed).
* A second run from a fresh image; the passes so far reused one device.

## WebRTC spike (gst-plugins-rs `webrtcsink` / `webrtcsrc`) — Partly

Question: can video go straight to the handheld over WebRTC instead of through `foxglove_bridge` and Lichtblick. Scripts in `tools/webrtc_spike/`.

Verified (Ubuntu 24.04 aarch64 container, GStreamer 1.24.2, gst-plugins-rs 0.13.7):

* Ubuntu has no package for these plugins. They build from source in about 4 minutes; the signalling server takes 40 s more.
* `videotestsrc` 640x480 at 15 fps through `webrtcsink`, the signalling server and `webrtcsrc` plays at 15 fps with 0 dropped frames and no errors for 25 s, with no STUN server reachable (host candidates only).
* With `tc netem` on loopback, confirmed by the qdisc drop counters (15, 60 and 154 packets dropped), the stream kept 14-15 fps and 0 dropped frames at 2% loss + 20 ms, 5% + 40 ms and 10% + 60 ms. The homegrown congestion controller was active (increase and decrease steps in its log).

Also verified: forcing H.264 (`video-caps="video/x-h264"` on the sink, `video-codecs="<H264>"` on the source) negotiates H.264 and decodes with `avdec_h264`; 14.8 fps, 0 dropped, at 5% loss + 40 ms. Software `x264enc` + `webrtcbin` took about 20% of one core on the sender and about 5% on the receiver, measured on a laptop VM core, far faster than the robot or the R36S, so not a budget figure. With no viewer connected `webrtcsink` does not encode: sender CPU was 6% idle, 20% with a viewer, and back to 6% after the viewer left (source and conversion only).

Target versions: the robot runs ROS Lyrical on Ubuntu 26.04 (GStreamer 1.28.2, no packaged `webrtcsink`), the handheld Ubuntu 24.04 (GStreamer 1.24.2). `gst-plugins-rs` 0.15 builds on 26.04 (about 10 minutes the first time, with `rustup`; scripts `build_plugins_resolute.sh`). Cross-version test, two containers over a Docker network, 26.04 + plugin 0.15 sender and 24.04 + plugin 0.13 receiver, H.264 forced, `tc netem` 5% loss + 40 ms on both ends (27 packets dropped on the receiver side): 15 fps, 0 dropped frames, no errors (`cross_robot.sh`, `cross_handheld.sh`). The plugin builds are separate per distro; the two ends run different plugin versions.

Robot side (kvn-robot repo, commit 56aa295): `kvn_video_streamer` has an optional WebRTC branch (`webrtc.enabled`), tested in a `ros:lyrical-ros-base-resolute` container with the 0.15 plugins: 28 tests pass, 0 skipped. They include the valve gating of the Foxglove branch and the fallback when WebRTC fails (no signalling server: Foxglove still streams, no pipeline crash loop; WebRTC retried every 10 s while no Foxglove viewer is connected). With the node running, a `webrtcsrc` viewer got 15 fps while `ros2 topic hz /video/compressed` also read 15 Hz. New `kvn-webrtc-signalling.service`, nftables rule for TCP 8443 on `zt*` (syntax checked with `nft -c`). The systemd units were not started on a real robot.

Handheld side (`WebrtcVideo`, `ui/src/WebrtcVideo.cpp`, QML `WebrtcView.qml`): `webrtcsrc ! decodebin ! videoconvert ! appsink` into a `QVideoSink`, with stall and connect watchdogs and an exponential backoff. In a Linux container on Ubuntu 24.04 (Qt 6.4.2, GStreamer 1.24.2, plugins 0.13) the QtTest `tst_webrtc` passes: frames at 640x480 and above 8 fps from a fake `webrtcsink` sender, state goes to `retrying` when the sender is killed, and it recovers on its own when the sender returns. The real `rc_ui` under Xvfb shows the stream (`docs/screenshots/webrtc_video.png`). Linux was the only place `webrtcsrc` ran; macOS builds with WebRTC off (no GStreamer there).

Running the UI on Qt 6.4, which macOS (Qt 6.11) never exercised, exposed three bugs that are fixed: a startup hang at 100% CPU in AOT-compiled singleton lookups (QML cache generation is now skipped below Qt 6.5), `Theme`/`Fmt` undefined in QML files because the singletons were not imported (explicit `import RcUi`), and optional Qt modules not detected when WebEngine was missing (one `find_package` per module). A fourth, cosmetic one, "Binding loop detected" in `MenuOverlay.qml` (two bindings), came from `rows` being first evaluated inside another binding while its `onRowsChanged` handler wrote `current`; `row` is now a function and the handler waits for `Component.onCompleted`. On Qt 6.4 the menu now logs no QML warnings, and scripted navigation (down, then right on a value) moves the focus and shows the pending value; the same on Qt 6.11.

Not verified:

* Latency (no glass-to-glass measurement), whether the sent bitrate actually adapts (only the controller's log lines were seen), and the jitter-buffer setting for teleop.
* The robot is a Raspberry Pi 5, which has no hardware H.264 encoder, so software x264 is the encoder. (The handheld runs Ubuntu noble, the same GStreamer 1.24 as the test container.) Background: In gst-plugins-rs 0.13.7 `webrtcsink` adapts the bitrate only for `x264enc`, `openh264enc`, `vp8enc`/`vp9enc`, `nvh264enc`, `vaapih264enc`, `qsvh264enc`, `nvv4l2h264enc` and `vpuenc_h264`. It does not for the generic `v4l2h264enc` or `mpph264enc` ("Bitrate handling is not supported yet"), so on those the choice is software x264, a small patch to the plugin, or a fixed bitrate with a pre-encoded input.
* The R36S side: `webrtcsrc` on the handheld image, hardware decode, CPU and RAM.
* Integration into `kvn_video_streamer` and the Qt UI.

## Fuzz run (T2.2)

`cargo fuzz run parser_feed`, 600 s, max input 512 bytes, seeded with the 15 fixtures,
nightly toolchain on macOS (aarch64): **19 143 120 executions, no crash, no leak or
timeout artifact**; 1633 coverage points, 1201 corpus entries. The fuzz target feeds the parser in
two chunks and runs every decoder, the re-encoder and the parameter-entry parser on every
frame. (An earlier run was interrupted at 2.2 M executions, also with no crash.) Plan T2.2's
"10-minute run finds no crash": met. Limitation: ASan covered the Rust side only.
