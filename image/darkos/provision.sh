#!/usr/bin/env bash
# Provision a dArkOS (R36S fork) install into the handheld appliance: the fallback platform
# (design §9.1). Run as root on the device after flashing; the steps are idempotent.
#
#   scp -r dist/ root@r36s:/tmp/rc-dist && ssh root@r36s 'bash -s' < image/darkos/provision.sh
#
# STATUS: not run (plan T1.1). dArkOS is Debian 13 with a BSP 4.4 kernel, so the UART needs a
# device-tree change as well (see below) and WebEngine runs on the libmali blob, untested.
set -euxo pipefail
dist=/tmp/rc-dist

# Stop the gaming frontend
systemctl disable --now emulationstation.service 2>/dev/null || true
systemctl mask emulationstation.service 2>/dev/null || true

apt-get update
apt-get install -y --no-install-recommends \
  libqt6quick6 libqt6quickcontrols2-6 libqt6multimedia6 libqt6webenginequick6 \
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtmultimedia \
  qml6-module-qtwebengine alsa-utils wpasupplicant iproute2 gstreamer1.0-plugins-good

install -m 0755 "$dist"/bin/control_daemon "$dist"/bin/ctl "$dist"/bin/rc_ui /usr/local/bin/
install -d /etc/rc /var/lib/rc
install -m 0644 "$dist"/daemon.toml "$dist"/mapping.toml /etc/rc/
install -m 0644 "$dist"/rc_ui.json /etc/rc/ui.json
install -m 0644 "$dist"/systemd/*.service /etc/systemd/system/
install -m 0644 "$dist"/systemd/99-elrs-tx.rules /etc/udev/rules.d/
id rc >/dev/null 2>&1 || useradd --system --create-home --home-dir /var/lib/rc --shell /usr/sbin/nologin rc
usermod -aG input,dialout,video,render,audio rc
chown -R rc:rc /var/lib/rc

# UART2 carries CRSF, not a console (design §9.3). On the BSP kernel it is claimed by the
# Rockchip FIQ debugger: disable the `fiq-debugger` node in the device tree so it appears as
# a normal ttyS2, and remove the console from the kernel command line. The exact files depend
# on the dArkOS R36S build (extlinux.conf / boot.ini / dtb); do this by hand on first install
# and record the steps in docs/results.md (T1.3), then script them here.
systemctl mask serial-getty@ttyS2.service serial-getty@ttyFIQ0.service

systemctl daemon-reload
systemctl enable rc-control-daemon.service rc-ui.service
