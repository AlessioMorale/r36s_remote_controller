#!/bin/bash
# Runs inside the image chroot during the Armbian build. The overlay directory (userpatches/
# overlay/) is available as /tmp/overlay. Put the output of image/build.sh there first:
#   overlay/bin/{control_daemon,ctl,rc_ui}  overlay/systemd/*  overlay/*.toml  overlay/rc_ui.json
#
# STATUS: not run (plan T3.9). Package names are trixie's; check them on the first build.
set -euxo pipefail

# --- packages -------------------------------------------------------------------------------
apt-get update
apt-get install -y --no-install-recommends \
  libqt6quick6 libqt6quickcontrols2-6 libqt6multimedia6 libqt6webenginequick6 \
  qml6-module-qtquick qml6-module-qtquick-controls qml6-module-qtquick-layouts \
  qml6-module-qtquick-window qml6-module-qtquick-templates qml6-module-qtmultimedia \
  qml6-module-qtwebengine qml6-module-qtqml-workerscript \
  libgles2 libgbm1 libdrm2 libinput10 libxkbcommon0 fonts-dejavu-core \
  alsa-utils wpasupplicant iproute2 iw rfkill ca-certificates curl \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav

# ZeroTier is not in trixie main. Pin the version so the image is reproducible.
ZEROTIER_VERSION="1.14.2"
curl -fsSL "https://download.zerotier.com/debian/trixie/pool/main/z/zerotier-one/zerotier-one_${ZEROTIER_VERSION}_arm64.deb" -o /tmp/zerotier.deb
dpkg -i /tmp/zerotier.deb || apt-get install -f -y
# WiFi/VPN are optional (design R3): the handheld must boot and run without them
systemctl enable zerotier-one.service

# --- our software ---------------------------------------------------------------------------
install -d /opt/kvn_remote_control/bin /etc/rc /var/lib/rc
install -m 0755 /tmp/overlay/bin/control_daemon /tmp/overlay/bin/ctl /tmp/overlay/bin/rc_ui /opt/kvn_remote_control/bin/
install -m 0644 /tmp/overlay/daemon.toml /tmp/overlay/mapping.toml /etc/rc/
install -m 0644 /tmp/overlay/rc_ui.json /etc/rc/ui.json
install -m 0644 /tmp/overlay/systemd/*.service /etc/systemd/system/
install -m 0644 /tmp/overlay/systemd/99-elrs-tx.rules /etc/udev/rules.d/
[ -d /tmp/overlay/lichtblick ] && cp -r /tmp/overlay/lichtblick /opt/kvn_remote_control/lichtblick

id rc >/dev/null 2>&1 || useradd --system --create-home --home-dir /var/lib/rc --shell /usr/sbin/nologin rc
usermod -aG input,dialout,video,render,audio rc
chown -R rc:rc /var/lib/rc

# --- the TX module's UART must carry CRSF only (design §9.3) -----------------------------------
systemctl mask serial-getty@ttyS2.service getty@tty1.service
# The kernel console must not be on ttyS2: remove console=ttyS2... from the boot arguments.
# (Where they live depends on the R36S-Armbian boot scheme: /boot/armbianEnv.txt "extraargs",
# boot.cmd, or extlinux.conf. Verify with `cat /proc/cmdline` on the device, plan T1.3.)
sed -i -E 's/console=ttyS2[^ ]*//g; s/earlycon[^ ]*//g' /boot/armbianEnv.txt /boot/boot.cmd 2>/dev/null || true
echo "extraargs=console=tty1 quiet" >> /boot/armbianEnv.txt || true

# --- boot straight to the UI, no network dependencies --------------------------------------
systemctl enable rc-control-daemon.service rc-ui.service
systemctl set-default multi-user.target
systemctl disable NetworkManager-wait-online.service systemd-networkd-wait-online.service 2>/dev/null || true
