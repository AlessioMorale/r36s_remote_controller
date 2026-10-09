#!/usr/bin/env bash
# Turn a fresh Armbian (Ubuntu 24.04 noble, mainline kernel) R36S install into the handheld.
# Runs ON the device as root; idempotent, one stage per concern, safe to re-run. Normally it is
# driven by image/noble/deploy.sh, which copies dist/noble over, runs this, reboots when it asks
# for it and runs it again.
#
#   sudo ./provision.sh [--dist DIR] [--verify]
#
# Exit status: 0 done, 10 a reboot is needed (run it again afterwards), anything else failed.
#
# Stages (each skipped when already done):
#   resize    grow the root partition to fill the SD card (online, no reboot)
#   packages  Qt 6 runtime, GStreamer plugins (v4l2 decode, libav, nice), plus fonts
#   desktop   no display manager: the UI owns the display on eglfs
#   files     rc user, binaries, gst-plugins-rs, config, systemd units, udev rule
#   uart      free UART2 for the ELRS TX module: no kernel console, no getty (needs reboot)
set -euo pipefail

dist=/tmp/rc-dist
base=/opt/kvn_remote_control     # everything installed lives here (bin, gst-rs, systemd, provision.sh)
verify_only=0
while [ $# -gt 0 ]; do
  case "$1" in
    --dist) dist=$2; shift 2 ;;
    --verify) verify_only=1; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
[ "$(id -u)" = 0 ] || { echo "run as root (sudo)" >&2; exit 2; }

state=/var/lib/rc-provision
mkdir -p "$state"
export DEBIAN_FRONTEND=noninteractive
log() { printf '\n== %s\n' "$*"; }
need_reboot=0

PACKAGES=(
  libqt6quick6 libqt6qml6 libqt6network6 libqt6multimedia6 libqt6shadertools6 qt6-qpa-plugins
  qml6-module-qtquick qml6-module-qtquick-window qml6-module-qtquick-shapes qml6-module-qtquick-layouts
  qml6-module-qtquick-templates qml6-module-qtqml qml6-module-qtqml-workerscript qml6-module-qtmultimedia
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav
  gstreamer1.0-nice gstreamer1.0-alsa gstreamer1.0-tools libgles2 fonts-dejavu-core evtest
)

stage_resize() {
  log "resize: root partition"
  local part dev num
  part=$(findmnt -n -o SOURCE /)                      # /dev/mmcblk1p2
  dev="/dev/$(lsblk -no PKNAME "$part")"              # /dev/mmcblk1
  num=$(cat "/sys/class/block/$(basename "$part")/partition")
  # Size in MiB of the free space after the last partition (the last line of the listing).
  local free_mib
  free_mib=$(parted -s -m "$dev" unit MiB print free | tail -n 1 | awk -F: '$5 ~ /^free/ {gsub("MiB","",$4); print int($4)}')
  if [ "${free_mib:-0}" -lt 64 ]; then
    echo "already uses the whole card ($(lsblk -bno SIZE "$part" | awk '{printf "%.1f GB", $1/1e9}'))"
    return
  fi
  echo "growing $part into ${free_mib} MiB of free space"
  # Same method as growpart: rewrite only this partition's size, then tell the kernel (parted
  # refuses to resize a mounted partition non-interactively).
  echo ", +" | sfdisk --force --no-reread -N "$num" "$dev"
  partx -u --nr "$num" "$dev"
  resize2fs "$part"
  df -h /
}

stage_packages() {
  log "packages"
  apt-get update -qq
  apt-get install -y --no-install-recommends "${PACKAGES[@]}"
  apt-get clean
}

stage_desktop() {
  log "desktop: no display manager"
  if [ "$(systemctl get-default)" != multi-user.target ]; then
    systemctl set-default multi-user.target
  fi
  for dm in lightdm gdm3 sddm; do
    if systemctl cat "$dm.service" >/dev/null 2>&1; then
      systemctl disable "$dm.service" 2>/dev/null || true
      if systemctl is-active --quiet "$dm.service"; then systemctl stop "$dm.service" || true; fi
    fi
  done
}

stage_files() {
  log "files from $dist into $base"
  [ -x "$dist/bin/control_daemon" ] || { echo "no $dist/bin/control_daemon: run image/noble/build.sh" >&2; exit 1; }
  id rc >/dev/null 2>&1 || useradd --system --create-home --home-dir /var/lib/rc --shell /usr/sbin/nologin rc
  usermod -aG input,dialout,video,render,audio rc
  install -d -o rc -g rc /var/lib/rc

  # Stop the services before replacing their binaries (running executables cannot be overwritten).
  systemctl stop rc-ui.service rc-control-daemon.service 2>/dev/null || true

  install -d "$base/bin" "$base/gst-rs/lib/gstreamer-1.0" "$base/systemd"
  install -m 0755 "$dist"/bin/control_daemon "$dist"/bin/ctl "$dist"/bin/rc_ui "$base/bin/"
  install -m 0644 "$dist"/gst-rs/lib/gstreamer-1.0/*.so "$base/gst-rs/lib/gstreamer-1.0/"
  install -m 0644 "$dist"/systemd/* "$base/systemd/"
  install -m 0755 "$dist/provision.sh" "$base/provision.sh"

  # Older passes of this script installed to other places: remove those copies.
  rm -f /usr/local/bin/control_daemon /usr/local/bin/ctl /usr/local/bin/rc_ui
  rm -rf /opt/rc
  for u in rc-control-daemon.service rc-ui.service; do
    [ -L "/etc/systemd/system/$u" ] || rm -f "/etc/systemd/system/$u"
  done
  [ -L /etc/udev/rules.d/99-elrs-tx.rules ] || rm -f /etc/udev/rules.d/99-elrs-tx.rules

  # Config: written once, then owned by the operator (re-runs never overwrite edits).
  install -d /etc/rc
  if [ ! -e /etc/rc/daemon.toml ]; then
    install -m 0644 "$dist/config/daemon.toml" /etc/rc/daemon.toml
    # This kernel exposes one combined pad, "r36s_Gamepad" (odroidgo3-joypad); the repo default
    # expects the adc-joystick + gpio-keys pair of other boards.
    sed -i 's|^devices *=.*|devices = ["r36s_Gamepad"]|' /etc/rc/daemon.toml
  fi
  [ -e /etc/rc/mapping.toml ] || install -m 0644 "$dist/config/mapping.toml" /etc/rc/mapping.toml
  [ -e /etc/rc/ui.json ] || install -m 0644 "$dist/config/rc_ui.json" /etc/rc/ui.json

  # Units and the udev rule stay in $base; the system directories only get links to them.
  ln -sf "$base/systemd/99-elrs-tx.rules" /etc/udev/rules.d/99-elrs-tx.rules
  udevadm control --reload
  systemctl link "$base/systemd/rc-control-daemon.service" "$base/systemd/rc-ui.service"
  systemctl daemon-reload
  systemctl enable rc-control-daemon.service rc-ui.service
  systemctl restart rc-control-daemon.service rc-ui.service
}

stage_uart() {
  log "uart: free UART2 (ttyS2) for the ELRS TX module"
  local ini=/boot/u-boot/boot.ini
  if [ -f "$ini" ] && grep -q 'console=ttyS2' "$ini"; then
    [ -e "$ini.rc-backup" ] || cp -a "$ini" "$ini.rc-backup"
    # Drop only the serial console; console=tty0 stays, and so does everything else on the line.
    sed -i -E 's/ ?console=ttyS2(,[0-9]+[a-z0-9]*)?//g; s/ ?earlycon[^ "]*//g' "$ini"
    grep -n 'setenv bootargs' "$ini"
    grep -q 'console=ttyS2' "$ini" && { echo "failed to remove console=ttyS2" >&2; cp -a "$ini.rc-backup" "$ini"; exit 1; }
    sync
    need_reboot=1
  fi
  systemctl mask serial-getty@ttyS2.service
  if systemctl is-active --quiet serial-getty@ttyS2.service; then
    systemctl stop serial-getty@ttyS2.service || true
  fi
  # The kernel console only goes away on the next boot.
  if grep -q 'console=ttyS2' /proc/cmdline; then need_reboot=1; fi
}

stage_verify() {
  log "verify"
  local bad=0
  check() { # description, command...
    local d=$1; shift
    if "$@" >/dev/null 2>&1; then printf '  ok    %s\n' "$d"; else printf '  FAIL  %s\n' "$d"; bad=1; fi
  }
  check "root partition uses the card (>20 GB)" test "$(lsblk -bno SIZE "$(findmnt -n -o SOURCE /)")" -gt 20000000000
  check "no console on ttyS2 in /proc/cmdline" bash -c '! grep -q "console=ttyS2" /proc/cmdline'
  check "serial-getty@ttyS2 masked" bash -c '[ "$(systemctl is-enabled serial-getty@ttyS2.service)" = masked ]'
  check "/dev/elrs_tx -> ttyS2" bash -c '[ "$(readlink -f /dev/elrs_tx)" = /dev/ttyS2 ]'
  check "webrtcsrc plugin loads" env GST_PLUGIN_PATH="$base/gst-rs/lib/gstreamer-1.0" gst-inspect-1.0 webrtcsrc
  check "H.264 decoder (v4l2slh264dec)" gst-inspect-1.0 v4l2slh264dec
  check "software H.264 decoder (avdec_h264)" gst-inspect-1.0 avdec_h264
  check "rc-control-daemon active" systemctl is-active --quiet rc-control-daemon.service
  check "rc-ui active" systemctl is-active --quiet rc-ui.service
  check "daemon IPC answers (ctl state)" timeout 5 "$base/bin/ctl" --socket /run/rc/control.sock state
  return $bad
}

if [ "$verify_only" = 1 ]; then stage_verify; exit $?; fi

run_stage() { # name function
  # files always runs: it is cheap and applies new binaries, units and config
  if [ "$1" != files ] && [ -e "$state/$1.done" ]; then echo "== $1: done earlier"; return; fi
  "$2"
  # uart is "done" only once the reboot happened; the others are done now
  if [ "$1" != uart ] || [ "$need_reboot" = 0 ]; then touch "$state/$1.done"; fi
}

run_stage resize   stage_resize
run_stage packages stage_packages
run_stage desktop  stage_desktop
run_stage files    stage_files
run_stage uart     stage_uart

if [ "$need_reboot" = 1 ]; then
  log "reboot needed to release the UART"
  exit 10
fi
log "all stages done"
stage_verify || { echo "some checks failed (see above)" >&2; exit 1; }
