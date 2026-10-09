#!/usr/bin/env bash
# Provision an Armbian noble R36S over SSH: build, copy, run the on-device stages, reboot when
# they ask for it, run them again until done, then show the verification.
#
#   RC_SUDO_PASSWORD=... image/noble/deploy.sh user@host [--no-build]
#
# RC_SUDO_PASSWORD is the device user's sudo password (leave unset for passwordless sudo). It is
# sent on stdin to `sudo -S` and never written to disk or the command line.
# Needs SSH key login to the device. Reboots the device when provision.sh exits with status 10.
set -euo pipefail

target=${1:?usage: deploy.sh user@host [--no-build]}
shift
build=1
for a in "$@"; do [ "$a" = --no-build ] && build=0; done

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
dist="$root/dist/noble"
ssh_opts=(-o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new)

remote() { ssh "${ssh_opts[@]}" "$target" "$@"; }
remote_sudo() { # command string; password on stdin when set
  if [ -n "${RC_SUDO_PASSWORD:-}" ]; then
    printf '%s\n' "$RC_SUDO_PASSWORD" | ssh "${ssh_opts[@]}" "$target" "sudo -S -p '' $*"
  else
    ssh "${ssh_opts[@]}" "$target" "sudo -n $*"
  fi
}

if [ "$build" = 1 ]; then "$here/build.sh" "$dist"; fi
[ -x "$dist/bin/control_daemon" ] || { echo "nothing in $dist: run image/noble/build.sh" >&2; exit 1; }

copy_dist() { # /tmp is cleared by a reboot, so this runs before every pass
  echo "== copying $dist to $target:/tmp/rc-dist"
  cp "$here/provision.sh" "$dist/provision.sh"   # always the current script, not the one from build time
  remote 'rm -rf /tmp/rc-dist && mkdir -p /tmp/rc-dist'
  COPYFILE_DISABLE=1 tar -C "$dist" -cf - . | remote 'tar -C /tmp/rc-dist -xf -'
}

wait_for_ssh() { # wait until the device went down and came back
  echo "== waiting for the device to go down"
  for _ in $(seq 1 30); do remote true 2>/dev/null || break; sleep 2; done
  echo "== waiting for the device to come back"
  for _ in $(seq 1 90); do remote true 2>/dev/null && return 0; sleep 4; done
  echo "device did not come back within 6 minutes" >&2; return 1
}

for attempt in 1 2 3 4; do
  copy_dist
  echo "== provisioning, pass $attempt"
  rc=0
  remote_sudo "bash /tmp/rc-dist/provision.sh --dist /tmp/rc-dist" || rc=$?
  case "$rc" in
    0)  echo "== done"; exit 0 ;;
    10) echo "== rebooting the device"
        remote_sudo "systemctl reboot" || true
        wait_for_ssh ;;
    *)  echo "provision.sh failed with status $rc" >&2; exit "$rc" ;;
  esac
done
echo "still not finished after 4 passes" >&2
exit 1
