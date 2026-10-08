# Remote controller

A handheld ground station for the KVN rover (R36S handheld, ExpressLRS control link, optional
WiFi video). [design.md](design.md) is the design, [plan.md](plan.md) the development plan,
[docs/results.md](docs/results.md) what is done and verified.

## Getting the sources

The CRSF library and the robot-side packages are git submodules, so this repo builds on its own:

```bash
git clone --recurse-submodules <url> remote_controller
# or, in an existing clone:
git submodule update --init
```

| Submodule | Used for |
|---|---|
| `deps/elrs_joy` | The C++ CRSF library the Rust crates bind to, its golden-frame fixtures, and `crsf_joy_node` |
| `deps/kvn-robot` | The robot-side packages: `kvn_status`, `kvn_video_streamer`, `kvn_robot_bringup` (bridge, teleop config) |

The robot packages also build in a ROS 2 workspace on their own (they are normal colcon packages).
Submodule URLs are HTTPS so CI can fetch them; to push over SSH locally:
`git config --global url."git@github.com:".insteadOf https://github.com/`.

## Layout

| Directory | Contents |
|---|---|
| `crates/elrs_crsf_sys`, `crates/elrs_crsf` | `cxx` bindings and safe Rust API over the robot's C++ CRSF library |
| `crates/control_daemon` | The daemon (`control_daemon`) and its test client (`ctl`) |
| `crates/test_tools` | `fake_tx` (pty TX module), `virtual_pad` (uinput), `crsf_capture` |
| `ui` | QML UI and thin C++ host; `ui/tools/mock_daemon.py` for UI work without the daemon |
| `lichtblick` | Pinned Lichtblick build and fixed layout |
| `systemd` | Handheld units and the `/dev/elrs_tx` udev rule |
| `image` | Armbian / dArkOS provisioning, cross-build helpers |
| `docs` | `ipc.md` (daemon ↔ UI), `budgets.md`, `hardware.md`, `results.md`, screenshots |

## Try it on a desktop (no hardware)

```bash
cargo build --bins                      # from this directory
target/debug/fake_tx --link /tmp/elrs_tx &
# --baud 0 only on macOS, where serialport cannot set a speed on a pty
target/debug/control_daemon --serial /tmp/elrs_tx --baud 0 --socket /tmp/rc.sock \
    --mapping crates/control_daemon/config/mapping.toml --config /nonexistent &
target/debug/ctl --socket /tmp/rc.sock rate           # ~10 Hz telemetry
target/debug/ctl --socket /tmp/rc.sock params         # the module's parameter tree
target/debug/ctl --socket /tmp/rc.sock param set 4 1  # change Max Power
```

On a Mac there is no gamepad, so the daemon stays disarmed (and says so). On Linux,
`virtual_pad` gives it one. The UI: see `ui/CMakeLists.txt`; run it with `--socket /tmp/rc.sock --windowed`.

## Tests

```bash
cargo test --workspace                                   # Rust
RC_TEST_UINPUT=1 cargo test -p control_daemon --test evdev   # Linux, needs /dev/uinput
cmake -S ui -B build/ui -G Ninja && cmake --build build/ui && ctest --test-dir build/ui
# ROS packages: copy deps/elrs_joy and deps/kvn-robot into a ROS 2 (Jazzy) workspace's src/, then
colcon build --packages-up-to elrs_joy_crsf_node kvn_status kvn_video_streamer kvn_robot_bringup
colcon test --packages-select elrs_joy_crsf_protocol elrs_joy_crsf_node kvn_status kvn_video_streamer
```

## Safety, in one paragraph

Control travels only over ELRS. The daemon starts disarmed and silent until the sticks are
neutral; AUX1 (the robot's teleop enable) is high only while armed (L1+R1 held for 1 s, sticks
neutral), R1 held, and the menu closed. No IPC request can arm or touch AUX1: the UI socket
can only change module parameters, set bounded axis overrides, and run calibration. A crash of
the UI, a stalled UI client or WiFi loss never changes the TX loop.
