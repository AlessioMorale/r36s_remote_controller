#!/usr/bin/env python3
"""Checks that the robot's foxglove_bridge is read-only (plan T4.1, design R1/§8).

Speaks the Foxglove WebSocket protocol (subprotocol foxglove.websocket.v1, or
foxglove.sdk.v1 for the SDK-based foxglove_bridge 3.x) and verifies:

  1. serverInfo advertises none of the write/introspection capabilities
     (clientPublish, services, parameters, parametersSubscribe, assets, connectionGraph);
  2. a client publish to /cmd_vel is refused: clientAdvertise is rejected and an
     injected Twist never comes back on the /cmd_vel subscription;
  3. service calls are refused (no services advertised, a call gets no response);
  4. parameter get/set are refused (no parameterValues with content);
  5. a topic that is not on the whitelist (--forbidden-topic) is not advertised,
     and every advertised topic is on the expected list.

Exit code 0 = all checks pass, 1 = a check failed, 2 = could not connect.

Usage:
  bridge_check.py [--url ws://<robot-zt-ip>:8765] [--forbidden-topic /secret]
                  [--require-topic /robot_status ...]
Needs the `websockets` package (python3-websockets).
"""

import argparse
import asyncio
import json
import struct
import sys
import time

import websockets

# foxglove_bridge < 3 speaks foxglove.websocket.v1; the SDK-based 3.x bridge requires
# foxglove.sdk.v1 (same JSON/binary messages). Offer both; the server picks one.
SUBPROTOCOLS = ['foxglove.websocket.v1', 'foxglove.sdk.v1']
FORBIDDEN_CAPABILITIES = {
    'clientPublish', 'services', 'parameters', 'parametersSubscribe', 'assets',
    'connectionGraph',
}
DEFAULT_ALLOWED_TOPICS = [
    '/video/compressed', '/robot_status', '/battery_state', '/odom', '/cmd_vel',
    '/diagnostics_agg', '/diagnostics',
]

# Binary opcodes (client -> server)
OP_CLIENT_MESSAGE_DATA = 0x01
OP_SERVICE_CALL_REQUEST = 0x02
# Binary opcodes (server -> client)
OP_MESSAGE_DATA = 0x01
OP_SERVICE_CALL_RESPONSE = 0x03

MAGIC_LINEAR_X = 12345.0


def twist_cdr(linear_x):
    """CDR (little endian) geometry_msgs/Twist with linear.x set."""
    return b'\x00\x01\x00\x00' + struct.pack('<6d', linear_x, 0, 0, 0, 0, 0)


class Session:
    def __init__(self, ws):
        self.ws = ws
        self.server_info = None
        self.channels = {}          # channel id -> channel dict
        self.services = {}          # service id -> service dict
        self.status = []            # status / error messages
        self.parameter_values = []  # parameterValues messages
        self.service_responses = []
        self.service_failures = []
        self.messages = {}          # subscription id -> list of payloads
        self.text_ops = []

    async def pump(self, duration):
        """Reads server messages for `duration` seconds."""
        deadline = time.monotonic() + duration
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return
            try:
                raw = await asyncio.wait_for(self.ws.recv(), timeout=remaining)
            except asyncio.TimeoutError:
                return
            except websockets.ConnectionClosed as exc:
                self.status.append({'op': 'closed', 'message': str(exc)})
                return
            self.handle(raw)

    def handle(self, raw):
        if isinstance(raw, bytes):
            if not raw:
                return
            opcode = raw[0]
            if opcode == OP_MESSAGE_DATA and len(raw) >= 13:
                sub_id = struct.unpack_from('<I', raw, 1)[0]
                self.messages.setdefault(sub_id, []).append(raw[13:])
            elif opcode == OP_SERVICE_CALL_RESPONSE:
                self.service_responses.append(raw)
            return
        msg = json.loads(raw)
        op = msg.get('op')
        self.text_ops.append(op)
        if op == 'serverInfo':
            self.server_info = msg
        elif op == 'advertise':
            for ch in msg.get('channels', []):
                self.channels[ch['id']] = ch
        elif op == 'unadvertise':
            for cid in msg.get('channelIds', []):
                self.channels.pop(cid, None)
        elif op == 'advertiseServices':
            for srv in msg.get('services', []):
                self.services[srv['id']] = srv
        elif op == 'status':
            self.status.append(msg)
        elif op == 'parameterValues':
            self.parameter_values.append(msg)
        elif op == 'serviceCallFailure':
            self.service_failures.append(msg)

    async def send_json(self, obj):
        await self.ws.send(json.dumps(obj))

    def topic_channel(self, topic):
        for ch in self.channels.values():
            if ch.get('topic') == topic:
                return ch
        return None


class Report:
    def __init__(self):
        self.failures = 0

    def check(self, name, ok, detail=''):
        print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f': {detail}' if detail else ''))
        if not ok:
            self.failures += 1


async def run(args):
    report = Report()
    try:
        ws = await websockets.connect(
            args.url, subprotocols=SUBPROTOCOLS, max_size=None, open_timeout=args.timeout)
    except Exception as exc:  # noqa: BLE001 - any connection error is fatal here
        print(f'[FAIL] connect to {args.url}: {exc}')
        return 2

    try:
        s = Session(ws)
        await s.pump(args.discovery_time)

        # 1. capabilities
        report.check('serverInfo received', s.server_info is not None)
        caps = set((s.server_info or {}).get('capabilities', []))
        bad = sorted(caps & FORBIDDEN_CAPABILITIES)
        report.check('no write/introspection capabilities', not bad,
                     f'capabilities={sorted(caps)}')
        print(f'       subprotocol={ws.subprotocol} '
              f'advertised={sorted(ch["topic"] for ch in s.channels.values())}')

        # 5. whitelist
        topics = {ch['topic'] for ch in s.channels.values()}
        report.check(f'{args.forbidden_topic} not advertised', args.forbidden_topic not in topics)
        extra = sorted(topics - set(args.allowed_topic))
        report.check('only whitelisted topics advertised', not extra, f'unexpected={extra}')
        for topic in args.require_topic:
            report.check(f'{topic} advertised (whitelist positive)', topic in topics)

        # 2. client publish to /cmd_vel
        cmd_ch = s.topic_channel('/cmd_vel')
        sub_id = 7
        if cmd_ch is not None:
            await s.send_json({'op': 'subscribe',
                               'subscriptions': [{'id': sub_id, 'channelId': cmd_ch['id']}]})
        n_status = len(s.status)
        client_ch = 101
        await s.send_json({'op': 'clientAdvertise', 'channels': [{
            'id': client_ch, 'topic': '/cmd_vel', 'encoding': 'cdr',
            'schemaName': 'geometry_msgs/msg/Twist'}]})
        await s.pump(0.5)
        payload = twist_cdr(MAGIC_LINEAR_X)
        for _ in range(5):
            await ws.send(struct.pack('<BI', OP_CLIENT_MESSAGE_DATA, client_ch) + payload)
            await asyncio.sleep(0.05)
        await s.pump(args.echo_time)
        new_status = s.status[n_status:]
        echoed = [
            p for p in s.messages.get(sub_id, [])
            if len(p) >= 12 and struct.unpack_from('<d', p, 4)[0] == MAGIC_LINEAR_X
        ]
        rejected = any(m.get('level', 0) >= 1 or m.get('op') == 'closed' for m in new_status)
        print('       clientAdvertise reply: ' +
              ('; '.join(str(m.get('message', '')) for m in new_status)[:200] or '(none)'))
        if cmd_ch is not None:
            # End-to-end: the injected Twist would come back on our /cmd_vel subscription.
            report.check('client publish to /cmd_vel refused (injected Twist never seen)',
                         not echoed,
                         f'{len(s.messages.get(sub_id, []))} /cmd_vel msgs seen, '
                         f'{len(echoed)} injected, server status error={rejected}')
            await s.send_json({'op': 'unsubscribe', 'subscriptionIds': [sub_id]})
        else:
            # Without /cmd_vel on the whitelist the echo cannot be observed: require an
            # explicit rejection from the server.
            report.check('client publish to /cmd_vel refused (server status error)', rejected)

        # 3. services
        report.check('no services advertised', not s.services,
                     f'services={[v.get("name") for v in s.services.values()]}')
        n_status = len(s.status)
        service_ids = list(s.services) or [1]
        for i, sid in enumerate(service_ids[:3]):
            enc = b'cdr'
            await ws.send(struct.pack('<BIII', OP_SERVICE_CALL_REQUEST, sid, 1000 + i, len(enc))
                          + enc + b'\x00\x01\x00\x00')
        await s.send_json({'op': 'callService', 'serviceId': service_ids[0], 'callId': 2000,
                           'encoding': 'json', 'payload': '{}'})
        await s.pump(1.0)
        report.check('service call refused (no response)', not s.service_responses,
                     f'status={[m.get("message") for m in s.status[n_status:]]}'[:200])

        # 4. parameters
        n_status = len(s.status)
        await s.send_json({'op': 'getParameters', 'parameterNames': [], 'id': 'get-all'})
        await s.send_json({'op': 'getParameters',
                           'parameterNames': ['/foxglove_bridge.port', 'foxglove_bridge.port'],
                           'id': 'get-port'})
        await s.send_json({'op': 'setParameters', 'id': 'set-1', 'parameters': [
            {'name': '/foxglove_bridge.send_buffer_limit', 'value': 1},
            {'name': '/video_streamer.bitrate_kbps', 'value': 1},
            {'name': '/teleop_twist_joy_node.scale_linear.x', 'value': 9.0},
        ]})
        await s.send_json({'op': 'subscribeParameterUpdates',
                           'parameterNames': ['/foxglove_bridge.port']})
        await s.pump(1.5)
        with_content = [m for m in s.parameter_values if m.get('parameters')]
        report.check('parameter get/set refused (no parameter values returned)',
                     not with_content, f'{len(with_content)} parameterValues with content; '
                     f'status={[m.get("message") for m in s.status[n_status:]]}'[:300])

    finally:
        await ws.close()

    print('RESULT:', 'PASS' if report.failures == 0 else f'FAIL ({report.failures} checks)')
    return 0 if report.failures == 0 else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--url', default='ws://127.0.0.1:8765')
    parser.add_argument('--forbidden-topic', default='/kvn_bridge_check/secret',
                        help='a topic published on the robot but not whitelisted')
    parser.add_argument('--allowed-topic', action='append', default=None,
                        help='topics allowed to be advertised (repeat); default: layout topics')
    parser.add_argument('--require-topic', action='append', default=[],
                        help='topic that must be advertised (repeat)')
    parser.add_argument('--discovery-time', type=float, default=3.0,
                        help='seconds to collect serverInfo/advertise messages')
    parser.add_argument('--echo-time', type=float, default=2.0,
                        help='seconds to watch /cmd_vel for the injected message')
    parser.add_argument('--timeout', type=float, default=5.0)
    args = parser.parse_args()
    if args.allowed_topic is None:
        args.allowed_topic = DEFAULT_ALLOWED_TOPICS
    sys.exit(asyncio.run(run(args)))


if __name__ == '__main__':
    main()
