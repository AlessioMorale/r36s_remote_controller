# UI tour

640x480, no touch, no keyboard. Everything is driven from the gamepad. Screenshots come from the offscreen UI with mock data; the video area shows a stand-in image, not a real camera.

## Layout

| Area | Content |
|---|---|
| Left, large | Video (needs WiFi). Below it, a plot strip (needs WiFi). |
| Right column | Four tiles that never move: **Link** (LQ, RSSI, SNR, TX power), **Robot batt** (V, %, A, mAh), **Robot** (status string), **Drive** (armed or disarmed, deadman, AUX1). The Drive tile is Acid Lime while armed, the one accent on the screen. |
| Bottom bar | On Slam Violet. Mode (`Drive` / `Menu`), turbo state, WiFi/VPN state and latency. |

Essential telemetry (right column and bottom bar) comes over ELRS and is the same in every mode. Only the video and plot areas depend on WiFi.

![Normal drive screen](screenshots/full.png)

## Video

`video.source` in `/etc/rc/ui.json` picks the video: `webrtc` (native view, the robot's `webrtcsink` stream, no plots), `lichtblick` (WebEngine view with video and plots) or `none`. `auto` (default) is `webrtc` when the UI was built with it, else `lichtblick`.

The WebRTC view needs the gst-plugins-rs `webrtcsrc` plugin, which Ubuntu does not package: build it with `tools/webrtc_spike/build_plugins.sh` and install it in `/opt/kvn_remote_control/gst-rs/lib/gstreamer-1.0` (the path `rc-ui.service` sets). It connects to the robot's signalling server (`robot.host:8443`), retries with a backoff of 1 s to 15 s, and shows `Connecting video…` or `Video restarting` with the reason while it has no picture.

![Live WebRTC video](screenshots/webrtc_video.png)

The picture above is the real UI on Qt 6.4 (Ubuntu noble) showing a `videotestsrc` stream from a fake robot.

## Look and feel

The UI uses the Slamming Works look. Flat fills, radius 0, 2 px borders, sentence case, no gradients or glow.

| Element | Rule |
|---|---|
| Palette | Night `#0E0B14` and Paper `#F2EFE8`, Slam Violet `#9400D3` for fills, Ultraviolet `#C77DFF` for violet text on dark, Acid Lime `#C6FF3D` once per screen |
| Fonts | Archivo Black for big values and headings, IBM Plex Sans for body, IBM Plex Mono for small labels. Bundled in `ui/resources/fonts` (SIL OFL, licenses alongside), so nothing is installed on the handheld. `ui.font_family` overrides the body face |
| Logo | The mark sits on its violet tile in the menu header, at 32 px with its clear space. It is the one tilted element |
| Status colors | Green (ok), amber (warn) and red (critical) are safety signals, not brand colors, so they stay outside the palette |
| Not applied | The landing animation and hard offset shadow: the UI has no pointer, so nothing is pressed |

All text pairs were chosen for WCAG AA contrast: Paper on Night and on Violet, Ultraviolet on Night, Night on Lime. Text on the Violet status bar is always Paper.

## Navigation

```mermaid
stateDiagram-v2
    [*] --> Drive
    Drive --> Menu: Select
    Menu --> Drive: Select or B at root
    Menu --> Page: A on folder
    Page --> Menu: B
    Drive --> ElrsLost: no link stats
    ElrsLost --> Drive: link back
```

| Button | Drive | Menu |
|---|---|---|
| Select | open menu | close menu |
| L1 + R1 (hold 1 s) | arm | n/a |
| R1 | deadman (AUX1 high when armed) | n/a |
| R2 | turbo | n/a |
| D-pad up/down | n/a | move |
| D-pad left/right | n/a | change a value |
| A | n/a | open folder, apply value, run action |
| B | n/a | revert value, go back, close at root |

While the menu is open the sticks are forced neutral and the robot is not driven.

## Menu

Sections, top to bottom: **ELRS module** (the TX module's own parameters: packet rate, telemetry ratio, power, bind), **Input** (stick calibration), **System** (WiFi/VPN, robot bridge, theme, versions). Values with `‹ ›` are editable; a change is sent to the module only when you press A.

| ELRS parameters | System |
|---|---|
| ![Menu](screenshots/menu.png) | ![System section](screenshots/menu_system.png) |

## Degraded states

| No WiFi | Poor ELRS link | ELRS link lost |
|---|---|---|
| ![No video link](screenshots/degraded.png) | ![Link degraded](screenshots/lq_degraded.png) | ![Link lost](screenshots/elrs_lost.png) |
| Video and plots show placeholders; the right column is unchanged. | Banner at the top, Link tile turns amber. | Full-screen red alert and tone. Tiles are marked `Stale` with a dashed border and dimmed text. |

## Light theme

Switch it in the menu (System, Theme) for sunlight: Paper surfaces and Night text instead of Night and Paper. Layout is identical.

| Drive | Menu | Link lost |
|---|---|---|
| ![](screenshots/full_light.png) | ![](screenshots/menu_light.png) | ![](screenshots/elrs_lost_light.png) |

More variants in [screenshots/](screenshots/): `degraded_light`, `lq_degraded_light`, `menu_system_light`, and `live_daemon_*` (the real daemon driving the UI over IPC).
