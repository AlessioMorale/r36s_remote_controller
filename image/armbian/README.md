# Armbian image (preferred platform, design §9.1)

Fork [R36S-Stuff/R36S-Armbian](https://github.com/R36S-Stuff/R36S-Armbian) and overlay:

```bash
git clone https://github.com/R36S-Stuff/R36S-Armbian && cd R36S-Armbian
cp <repo>/remote_controller/image/armbian/config-r36s-trixie-minimal.conf userpatches/
cp <repo>/remote_controller/image/armbian/userpatches/customize-image.sh userpatches/
mkdir -p userpatches/overlay && cp -r <repo>/remote_controller/dist/* userpatches/overlay/
./compile.sh config-r36s-trixie-minimal
```

**Status: not built.** Everything here is untested until plan T1.1 is done on the build host
and the device. Open points to check on the first build:

* the board/branch names in the config against the fork's existing configs;
* the Debian package names and ZeroTier version in `customize-image.sh`;
* where the fork keeps the kernel command line (`console=ttyS2`), and that the DT enables
  UART2 as a plain 8250 `ttyS2` (T1.3);
* the panel overlay for the board revision from T0.5.
