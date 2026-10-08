# Handheld systemd units

| File | Install to | Purpose |
|---|---|---|
| `rc-control-daemon.service` | `/etc/systemd/system/` | The safety-critical daemon: `Restart=always`, real-time, no network dependency |
| `rc-ui.service` | `/etc/systemd/system/` | The native UI on `eglfs`; independent of the daemon |
| `99-elrs-tx.rules` | `/etc/udev/rules.d/` | `/dev/elrs_tx` symlink to the internal UART |

Also needed on the device (the provisioning scripts in `../image/` do all of this):

```bash
useradd --system --create-home --home-dir /var/lib/rc rc
usermod -aG input,dialout,video,render,audio rc
# The UART must not carry a login prompt or kernel console (design §9.3)
systemctl mask serial-getty@ttyS2.service
systemctl enable rc-control-daemon.service rc-ui.service
```

Kernel command line: remove `console=ttyS2,...` and `earlycon` (Armbian) or disable the
`fiq-debugger` node in the device tree (dArkOS BSP kernel). See `../docs/hardware.md`.

Verification on the device is plan T1.3 and T3.9 (`systemd-analyze verify` is run in CI).
