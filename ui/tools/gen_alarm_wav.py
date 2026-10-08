#!/usr/bin/env python3
"""Generate resources/alarm.wav, the loud-alarm tone (stdlib only).

A 1.0 s loop: two 880/1320 Hz beeps then a pause, 16-bit mono 22050 Hz.
SoundEffect loops it while any loud alarm is active.

    python3 tools/gen_alarm_wav.py [out.wav]
"""
import math
import struct
import sys
import wave
from pathlib import Path

RATE = 22050
AMP = 0.6


def tone(freq: float, dur: float) -> list[float]:
    n = int(RATE * dur)
    fade = int(RATE * 0.01)  # 10 ms ramps: no clicks
    out = []
    for i in range(n):
        env = min(1.0, i / fade, (n - 1 - i) / fade)
        # Square-ish wave (fundamental + 3rd harmonic) carries better on a small speaker.
        t = i / RATE
        s = math.sin(2 * math.pi * freq * t) + 0.3 * math.sin(2 * math.pi * 3 * freq * t)
        out.append(AMP * env * s / 1.3)
    return out


def silence(dur: float) -> list[float]:
    return [0.0] * int(RATE * dur)


def main() -> None:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent.parent / "resources" / "alarm.wav"
    samples = tone(880, 0.18) + silence(0.07) + tone(1320, 0.18) + silence(0.07) + tone(880, 0.18) + silence(0.32)
    out.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(out), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1.0, min(1.0, s)) * 32767)) for s in samples))
    print(f"wrote {out} ({len(samples) / RATE:.2f} s)")


if __name__ == "__main__":
    main()
