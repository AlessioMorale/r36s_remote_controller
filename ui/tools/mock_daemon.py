#!/usr/bin/env python3
"""Mock control_daemon for UI development (stdlib only). Implements the daemon
side of docs/ipc.md: hello, 10 Hz telemetry, alarms, params, menu_input, acks.

    mock_daemon.py --socket /tmp/rc.sock --scenario full [--bridge-port 18765]

Scenarios: full | degraded | menu | elrs_lost | lq_degraded | cycle
  full        ELRS healthy, armed, robot status RDY (also listens on --bridge-port so
              the UI's reachability probe succeeds; 'degraded' does not)
  degraded    same ELRS data, no bridge listener (UI shows NO VIDEO LINK)
  menu        input.mode == "menu" with the ELRS params tree; scripted menu_input
              events are NOT sent (navigate with the keyboard or --script)
  elrs_lost   link stale, elrs_lost alarm active, last-known values kept
  lq_degraded loud elrs_degraded alarm, LQ 31 %
  cycle       full -> elrs_lost -> full every 8 s

Requests are logged to stderr (and --log); acks follow the contract:
no request can arm / set AUX1; unknown types -> ok:false.
"""
import argparse
import json
import os
import socket
import struct
import sys
import threading
import time

MAX_MSG = 64 * 1024


def frame(obj):
    b = json.dumps(obj, separators=(",", ":")).encode()
    return struct.pack(">I", len(b)) + b


def params_tree(power_idx=3):
    return {
        "type": "params", "complete": True,
        "device": {"name": "ELRS TX 2400", "serial": 1162629715, "firmware": 197891, "parameters_total": 12},
        "params": [
            {"number": 1, "parent": 0, "type": "text_selection", "name": "Packet Rate", "hidden": False,
             "value": 3, "min": 0, "max": 3, "options": ["50Hz", "100Hz", "150Hz", "250Hz"], "unit": ""},
            {"number": 2, "parent": 0, "type": "text_selection", "name": "Telem Ratio", "hidden": False,
             "value": 4, "min": 0, "max": 7, "options": ["Std", "Off", "1:128", "1:64", "1:32", "1:16", "1:8", "1:4"],
             "unit": ""},
            {"number": 4, "parent": 0, "type": "folder", "name": "TX Power", "hidden": False, "children": [5, 6]},
            {"number": 5, "parent": 4, "type": "text_selection", "name": "Max Power", "hidden": False,
             "value": power_idx, "min": 0, "max": 4, "options": ["10", "25", "50", "100", "250"], "unit": "mW"},
            {"number": 6, "parent": 4, "type": "text_selection", "name": "Dynamic", "hidden": False,
             "value": 0, "min": 0, "max": 2, "options": ["Off", "On", "AUX9+"], "unit": ""},
            {"number": 8, "parent": 0, "type": "text_selection", "name": "VTX Admin", "hidden": False,
             "value": 0, "min": 0, "max": 1, "options": ["Off", "On"], "unit": ""},
            {"number": 9, "parent": 0, "type": "command", "name": "Bind", "hidden": False, "status": 0, "info": ""},
            {"number": 10, "parent": 0, "type": "info", "name": "Version", "hidden": False, "text": "3.5.3 ISM2G4"},
            {"number": 11, "parent": 0, "type": "uint8", "name": "Fan thresh", "hidden": False,
             "value": 25, "min": 0, "max": 50, "unit": "mW"},
        ],
    }


class State:
    def __init__(self, scenario):
        self.scenario = scenario
        self.lock = threading.Lock()
        self.t0 = time.time()
        self.seq = 0
        self.power_idx = 3
        self.calibrating = False
        self.clients = []
        self.log_fh = None

    def effective(self):
        if self.scenario != "cycle":
            return self.scenario
        return "elrs_lost" if int((time.time() - self.t0) // 8) % 2 else "full"

    def telemetry(self):
        sc = self.effective()
        self.seq += 1
        lost = sc == "elrs_lost"
        age = int((time.time() - self.t0) * 1000) % 100000 if lost else 120
        link = {"lq": 98, "rssi_dbm": -64, "snr_db": 9, "tx_power_mw": [10, 25, 50, 100, 250][self.power_idx],
                "rf_mode": 7, "downlink_lq": 97, "downlink_rssi_dbm": -66, "age_ms": 120, "stale": False}
        if lost:
            link.update(lq=0, age_ms=2400, stale=True)
        if sc == "lq_degraded":
            link.update(lq=31, rssi_dbm=-101, snr_db=-2)
        menu = sc == "menu"
        return {
            "type": "telemetry", "seq": self.seq,
            "link": link,
            "battery": {"voltage": 15.6, "current": 1.2, "used_mah": 450, "percent": 72,
                        "age_ms": 2600 if lost else 400, "stale": lost},
            "status": {"text": "RDY", "age_ms": 2800 if lost else 300, "stale": lost},
            "arm": {"state": "disarmed" if (menu or lost) else "armed", "deadman": not (menu or lost),
                    "aux1": not (menu or lost)},
            "input": {"mode": "menu" if menu else "drive", "device": True, "turbo": False},
            "channels": [1500] * 4 + [1000 if (menu or lost) else 2000, 1000] + [1500] * 10,
            "tx": {"serial_open": True, "synced": not lost, "frame_period_us": 4000, "frames": 81234 + self.seq * 25},
            "module": {"name": "ELRS TX 2400", "connected": not lost},
            "overrides": [],
        }

    def alarms(self):
        sc = self.effective()
        if sc == "elrs_lost":
            return [{"type": "alarm", "id": "elrs_lost", "level": "loud", "active": True, "message": "ELRS link lost"}]
        if sc == "lq_degraded":
            return [{"type": "alarm", "id": "elrs_degraded", "level": "loud", "active": True,
                     "message": "ELRS link degraded"}]
        return []

    def log(self, msg):
        line = f"{time.strftime('%H:%M:%S')} {msg}"
        print(line, file=sys.stderr, flush=True)
        if self.log_fh:
            self.log_fh.write(line + "\n")
            self.log_fh.flush()


def handle_request(st, msg):
    t = msg.get("type")
    rid = msg.get("id")
    ack = {"type": "ack", "id": rid, "ok": True}
    if t == "get_state":
        pass
    elif t == "param_refresh":
        time.sleep(0.2)
    elif t == "param_write":
        if msg.get("number") == 5:
            st.power_idx = int(msg["value"])
        elif msg.get("number") == 99:
            ack.update(ok=False, error="module rejected value")
    elif t in ("override_set", "override_clear", "menu_close"):
        pass
    elif t == "calibration_start":
        if st.effective() != "menu":
            ack.update(ok=False, error="calibration only in menu")
        else:
            st.calibrating = True
    elif t == "calibration_finish":
        st.calibrating = False
    elif t in ("arm", "disarm", "set_aux1"):
        ack.update(ok=False, error="unknown request type")
    else:
        ack.update(ok=False, error="unknown request type")
    return ack


def serve_client(st, conn):
    conn.settimeout(None)
    out_lock = threading.Lock()

    def send(obj):
        with out_lock:
            conn.sendall(frame(obj))

    def snapshot():
        send({"type": "hello", "version": 1, "daemon": "0.1.0-mock"})
        for a in st.alarms():
            send(a)
        send(params_tree(st.power_idx))

    try:
        snapshot()
        with st.lock:
            st.clients.append(send)
        buf = b""
        while True:
            data = conn.recv(4096)
            if not data:
                break
            buf += data
            while len(buf) >= 4:
                (n,) = struct.unpack(">I", buf[:4])
                if n > MAX_MSG:
                    return
                if len(buf) < 4 + n:
                    break
                msg = json.loads(buf[4:4 + n])
                buf = buf[4 + n:]
                st.log(f"<- {json.dumps(msg)}")
                if msg.get("type") == "get_state":
                    snapshot()
                ack = handle_request(st, msg)
                st.log(f"-> {json.dumps(ack)}")
                send(ack)
                if msg.get("type") == "param_write" and ack["ok"]:
                    send(params_tree(st.power_idx))
    except (OSError, ValueError):
        pass
    finally:
        with st.lock:
            if send in st.clients:
                st.clients.remove(send)
        conn.close()


def broadcaster(st, script):
    last_alarm = None
    start = time.time()
    script = list(script)
    while True:
        time.sleep(0.1)
        with st.lock:
            clients = list(st.clients)
        if st.scenario == "cycle":
            cur = st.effective()
            if cur != last_alarm:
                last_alarm = cur
                msgs = st.alarms() or [{"type": "alarm", "id": "elrs_lost", "level": "loud", "active": False}]
                for c in clients:
                    try:
                        for m in msgs:
                            c(m)
                    except OSError:
                        pass
        tel = st.telemetry()
        while script and time.time() - start >= script[0][0]:
            _, btn = script.pop(0)
            for c in clients:
                try:
                    c({"type": "menu_input", "button": btn})
                except OSError:
                    pass
        for c in clients:
            try:
                c(tel)
            except OSError:
                pass


def bridge_listener(port, st):
    s = socket.socket()
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.bind(("127.0.0.1", port))
    s.listen(8)
    st.log(f"fake foxglove bridge listening on 127.0.0.1:{port}")
    while True:
        c, _ = s.accept()
        c.close()


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--socket", default="/tmp/rc_mock.sock")
    ap.add_argument("--scenario", default="full",
                    choices=["full", "degraded", "menu", "elrs_lost", "lq_degraded", "cycle"])
    ap.add_argument("--bridge-port", type=int, default=0,
                    help="listen on this TCP port (127.0.0.1) as a stand-in robot bridge; default: only for full/cycle if given")
    ap.add_argument("--log", help="also append the request log to this file")
    ap.add_argument("--script", default="",
                    help="menu_input script 'sec:button,...' e.g. 2:down,3:right,4:a")
    args = ap.parse_args()

    st = State(args.scenario)
    if args.log:
        st.log_fh = open(args.log, "a")
    script = []
    for item in filter(None, args.script.split(",")):
        sec, btn = item.split(":")
        script.append((float(sec), btn))

    if os.path.exists(args.socket):
        os.unlink(args.socket)
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(args.socket)
    os.chmod(args.socket, 0o660)
    srv.listen(8)
    st.log(f"mock daemon: scenario={args.scenario} socket={args.socket}")

    if args.bridge_port and args.scenario != "degraded":
        threading.Thread(target=bridge_listener, args=(args.bridge_port, st), daemon=True).start()
    threading.Thread(target=broadcaster, args=(st, script), daemon=True).start()
    try:
        while True:
            conn, _ = srv.accept()
            st.log("client connected")
            threading.Thread(target=serve_client, args=(st, conn), daemon=True).start()
    except KeyboardInterrupt:
        pass
    finally:
        os.unlink(args.socket)


if __name__ == "__main__":
    main()
