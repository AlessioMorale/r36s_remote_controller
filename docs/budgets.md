# Acceptance budgets (plan T0.4)

Proposed values from the plan. **Not signed off.** Review the table, change what you disagree
with, then fill in the sign-off line. A4 measures every metric against it.

| Metric | Budget | Measured by (plan A4) | Daemon's own trace |
|---|---|---|---|
| Input-to-UART latency (evdev event → CRSF frame written), p99 | ≤ 10 ms | daemon trace timestamps | `ctl state` → `tx.input_latency_p99_us` |
| CRSF frame period jitter at the handheld UART, p99 | ≤ 0.5 ms | logic analyzer on UART TX | `tx.period_jitter_p99_us` (software view, includes scheduler wake-up only) |
| Robot `/joy` → `/cmd_vel` period jitter with bridge and video at full load, p99 | ≤ 5 ms | rosbag timestamps, `kvn_robot_bringup/scripts/joy_cmdvel_jitter.py` | — |
| Robot stop after ELRS loss | ≤ `failsafe_timeout_ms` (300) + 50 ms | rosbag | — |
| Video glass-to-glass latency, direct VPN path, median | ≤ 250 ms | phone video of a millisecond clock | — |
| Handheld RAM used, with Lichtblick running | ≤ 700 MB | `free -m` | — |
| Handheld boot to "ready to arm" | ≤ 30 s | `systemd-analyze`, stopwatch | — |
| Daemon restart to link re-established | ≤ 2 s | `kill -9`, frames seen at the module | `rc-control-daemon.service` `RestartSec=200ms` |
| ELRS "link lost" alarm after TX module power loss | ≤ 1 s | scope + stopwatch | `link_stale_ms = 800` in `daemon.toml`; verified by test |

Two choices the plan left open, made here so the daemon could be built:

* The "link lost" alarm needs `LINK_STATISTICS` to stop arriving. The TX module sends it to the
  handset about every 100 ms on its own, whatever the RF telemetry ratio, so `link_stale_ms`
  is 800 ms (alarm raised within about 850 ms). Confirm on hardware (T3.5).
* Arming needs L1+R1 held for 1000 ms (`safety.arm_hold_ms`), as in design §6.

**Sign-off**: _name_ ____________________ _date_ ____________________
