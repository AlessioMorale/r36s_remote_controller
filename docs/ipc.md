# Daemon ↔ UI IPC

The contract between `control_daemon` and the native UI (design §3.4). The two processes
meet only here, so each can be rebuilt, restarted, or rewritten on its own.

## Transport

* Unix stream socket, default path `/run/rc/control.sock` (config `ipc.socket_path`).
  The daemon creates it with mode `0660`; the UI user must be in the socket's group.
* Every message is a **4-byte big-endian length** followed by that many bytes of
  **UTF-8 JSON** (one object). Maximum message size: 64 KiB. A longer length prefix or
  invalid JSON closes that connection; other clients are unaffected.
* Any number of clients may connect. Each one receives every daemon → UI message.
* The daemon never blocks on a client. If a client's send queue exceeds 64 messages,
  the daemon drops that client. The TX loop does not depend on IPC at all (R4).

## Safety Rules

The daemon treats every client as untrusted (design §6):

* **No request can arm, disarm, set AUX1 (channel 4), or command motion.** There is no
  `arm` request; arming is only possible with the physical L1+R1 gesture.
* Overrides are limited to the axis channels 0–3, and only under the rules in
  [Overrides](#overrides).
* Unknown request types are rejected with `ok: false`.

## Daemon → UI

### `hello`

Sent once, right after a client connects.

```json
{"type": "hello", "version": 1, "daemon": "0.1.0"}
```

### `telemetry`

A full snapshot, at 10 Hz. Fields that were never received are `null`. Every received
field carries `age_ms` (time since the value was last updated) and `stale` (age above the
configured threshold).

```json
{
  "type": "telemetry",
  "seq": 1234,
  "link": {"lq": 98, "rssi_dbm": -64, "snr_db": 9, "tx_power_mw": 100, "rf_mode": 7,
           "downlink_lq": 97, "downlink_rssi_dbm": -66, "age_ms": 120, "stale": false},
  "battery": {"voltage": 15.6, "current": 1.2, "used_mah": 450, "percent": 72,
              "age_ms": 400, "stale": false},
  "status": {"text": "RDY", "age_ms": 300, "stale": false},
  "arm": {"state": "armed", "deadman": true, "aux1": true},
  "input": {"mode": "drive", "device": true, "turbo": false},
  "channels": [1500, 1500, 1500, 1500, 2000, 1000, 1500, 1500,
               1500, 1500, 1500, 1500, 1500, 1500, 1500, 1500],
  "tx": {"serial_open": true, "synced": true, "frame_period_us": 4000, "frames": 81234},
  "module": {"name": "ELRS TX 2400", "connected": true},
  "overrides": [{"channel": 2, "value_us": 1600, "expires_in_ms": 300}]
}
```

| Field | Values |
|---|---|
| `arm.state` | `disarmed`, `arming` (L1+R1 held, timer running), `armed` |
| `arm.aux1` | the AUX1 value actually sent this frame |
| `input.mode` | `drive`, `menu` (sticks forced neutral, AUX1 low) |
| `input.device` | `false` while the gamepad is missing |
| `tx.synced` | `true` while `OPENTX_SYNC` frames are being received |

### `alarm`

Sent when an alarm becomes active or clears, and repeated for every active alarm when a
client connects.

```json
{"type": "alarm", "id": "elrs_lost", "level": "loud", "active": true,
 "message": "ELRS link lost"}
```

| `id` | `level` | Raised when |
|---|---|---|
| `elrs_lost` | `loud` | No `LINK_STATISTICS` within `telemetry.link_stale_ms`, or uplink LQ is 0 |
| `elrs_degraded` | `loud` | Uplink LQ below `telemetry.lq_alarm` |
| `serial_error` | `loud` | The TX module UART cannot be opened or fails |
| `input_lost` | `loud` | The gamepad disappeared (the daemon has disarmed) |
| `battery_stale` | `visible` | Robot battery older than its threshold |
| `status_stale` | `visible` | Robot status older than its threshold |

WiFi/VPN indicators are not alarms from the daemon; the UI reads them itself (T4.6).

### `menu_input`

Sent only while `input.mode` is `menu`. The daemon owns the gamepad (`EVIOCGRAB`), so
this is how the UI navigates.

```json
{"type": "menu_input", "button": "up"}
```

`button` is one of `up`, `down`, `left`, `right`, `a`, `b`, `x`, `y`, `start`.
The Select button toggles the menu inside the daemon; its effect shows up as
`input.mode` in the next snapshot.

### `params`

The ELRS module parameter tree. Sent after a `param_refresh`, after a write is
acknowledged by the module, and on connect if a tree is known.

```json
{
  "type": "params",
  "complete": true,
  "device": {"name": "ELRS TX 2400", "serial": 1162629715, "firmware": 197891,
             "parameters_total": 25},
  "params": [
    {"number": 5, "parent": 4, "type": "text_selection", "name": "Max Power",
     "hidden": false, "value": 3, "min": 0, "max": 4, "options": ["10", "25", "50", "100", "250"],
     "unit": "mW"},
    {"number": 4, "parent": 0, "type": "folder", "name": "TX Power", "hidden": false,
     "children": [5, 6]},
    {"number": 9, "parent": 0, "type": "command", "name": "Bind", "hidden": false,
     "status": 0, "info": ""}
  ]
}
```

`type` is one of `uint8`, `int8`, `uint16`, `int16`, `uint32`, `int32`, `float`,
`text_selection`, `string`, `folder`, `info`, `command`. Numeric types carry `value`,
`min`, `max`, `unit`; `float` adds `decimal_point` and `step`; `string`/`info` carry
`text`.

### `ack`

The reply to every request that has an `id`.

```json
{"type": "ack", "id": 7, "ok": false, "error": "overrides need disarmed or bounded ttl"}
```

## UI → Daemon

Every request carries an integer `id`, echoed in the `ack`.

| `type` | Fields | Effect |
|---|---|---|
| `get_state` | n/a | Resends `hello`, the active alarms, and `params` to this client |
| `param_refresh` | n/a | Re-reads the module's whole parameter tree |
| `param_write` | `number`, `value` | Writes a parameter; `value` is the option index for `text_selection`, the raw integer otherwise, the status for `command`. Acked after the module confirms by re-sending the entry (timeout 1 s). |
| `override_set` | `channel` (0–3), `value_us`, `ttl_ms` | Sets an axis override; see below |
| `override_clear` | `channel` (optional) | Clears one or all overrides |
| `menu_close` | n/a | Leaves the menu (same as pressing Select) |
| `calibration_start` | n/a | Starts recording raw stick ranges (menu only) |
| `calibration_finish` | `save` (bool) | Stops; with `save`, writes the ranges to the calibration file |

### Overrides

An override replaces one axis channel with a fixed value. It is accepted only if:

* the channel is 0–3 (never AUX), and `1000 ≤ value_us ≤ 2000`, and
* the system is disarmed, **or** `|value_us − 1500| ≤ safety.override_max_delta_us` and
  `ttl_ms ≤ safety.override_max_ttl_ms`.

Every override expires after `ttl_ms` unless it is set again; the UI refreshes it to keep
it. All overrides are cleared on disarm and while the menu is open. Active overrides are
listed in every `telemetry` snapshot, and the UI shows them in the status bar.
