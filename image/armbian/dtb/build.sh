#!/usr/bin/env bash
# Build the R36S kernel dtb with a panel baked in, for the Armbian noble image.
#
# The image's boot.ini applies ScreenFiles/Panel N/mipi-panel.dtbo with "fdt apply", but the
# kernel dtb is built without __symbols__, so the Panel 0 overlay (which uses fixups) fails
# and U-Boot boots a corrupt fdt -> kernel panic -> reboot. We merge at build time instead.
#
# usage:
#   build.sh [-p PANEL] [-b KERNEL_DTB] [-i BOOT_MOUNT]
#     -p PANEL       panel number, uses panels/panel$PANEL.dtsi (default 0)
#     -b KERNEL_DTB  regenerate base/rk3326-gameconsole-r36s.dts from this dtb first
#                    (e.g. <rootfs>/boot/dtb/rockchip/rk3326-gameconsole-r36s.dtb after a kernel update)
#     -i BOOT_MOUNT  copy the result and boot.ini to the SD BOOT partition (e.g. /Volumes/BOOT)
set -euo pipefail
cd "$(dirname "$0")"

panel=0 base_dtb= install=
while getopts p:b:i: opt; do
	case $opt in
		p) panel=$OPTARG ;;
		b) base_dtb=$OPTARG ;;
		i) install=$OPTARG ;;
		*) sed -n '/^# usage/,/^set /p' "$0" | sed '$d' >&2; exit 2 ;;
	esac
done

if [[ -n $base_dtb ]]; then
	dtc -q -I dtb -O dts -o base/rk3326-gameconsole-r36s.dts "$base_dtb"
	echo "regenerated base/rk3326-gameconsole-r36s.dts from $base_dtb"
fi

panel_dtsi=panels/panel$panel.dtsi
[[ -f $panel_dtsi ]] || { echo "missing $panel_dtsi (see extract-panel.sh)" >&2; exit 1; }

mkdir -p out
out=out/rk3326-gameconsole-r36s-merged.dtb
{
	echo '/include/ "base/rk3326-gameconsole-r36s.dts"'
	echo "/include/ \"$panel_dtsi\""
} >out/r36s-panel$panel.dts
dtc -q -i . -I dts -O dtb -o "$out" out/r36s-panel$panel.dts
echo "built $out (panel $panel)"

if [[ -n $install ]]; then
	dest="$install/ScreenFiles/Panel $panel"
	cp "$out" "$dest/"
	cp boot.ini "$install/boot.ini"
	echo "installed to $dest and $install/boot.ini"
fi
