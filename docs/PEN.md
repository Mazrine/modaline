# Pen and tablet support

Modaline reads pen **pressure** directly from Linux evdev, so both a Wacom tablet
using the kernel/X driver and an OpenTabletDriver (OTD) virtual tablet work on
X11, where winit/egui expose no pressure at all.

## What is read, and what is not

| Signal | Source | Notes |
|---|---|---|
| Cursor position | OS pointer (X11/libinput) | OTD and the Wacom X driver both move the pointer |
| Tip click / buttons | OS pointer buttons | Pen tip = left click; barrel button = right click when mapped |
| Pressure | `ABS_PRESSURE` on `/dev/input/event*` | Normalized by the device's reported maximum |
| Tip contact | `BTN_TOUCH` | Pressure is only applied while the tip is down |
| Tilt / rotation / ExpressKeys | — | Not read; map ExpressKeys to keys in OTD/xsetwacom |

The reader is read-only: it never grabs the device, so the X pointer keeps
working. It rescans every few seconds, so unplugging the tablet or restarting
OTD recovers on its own.

## Requirements

The user running Modaline must be able to read `/dev/input/event*`:

```sh
groups            # needs "input"
sudo usermod -aG input "$USER"   # log out/in afterwards
```

On NixOS:

```nix
users.users.youruser.extraGroups = [ "input" ];
# OpenTabletDriver (also installs its udev rules):
hardware.opentabletdriver.enable = true;
```

OTD users: Modaline prefers a device whose name contains `OpenTabletDriver`, then
`Wacom`, then `pen`/`stylus`/`tablet`. When OTD is running it grabs the physical
tablet, so the virtual device is the one that reports pressure.

## Using it

- Draw normally; the tip pressure modulates the brush.
- The side panel has a **pen** checkbox and shows the detected device (or
  `not detected` / `permission denied`).
- `:pen` shows the device and the last pressure.
- `:pen off` ignores the pen until `:pen on` (persisted in `config.json` as
  `pen_enabled`).
- Pressure precedence stays manual → device → 1.0: `:pressure 60` overrides the
  pen, `:pressure auto` returns to it.
- Buttons: map a barrel button to right-click in OTD/xsetwacom for the
  secondary colour, or to `b`/`e`/`u`/`Ctrl-hjkl` via OTD's key bindings if you
  prefer tablet-only work.

## Limitations and next steps

- Tilt, rotation and ExpressKeys are not decoded. OTD can bind them to keys;
  a native mapping is tracked as **P7-03** in `docs/ROADMAP.md`.
- There is no per-app pressure curve yet; use `:pressure` or the tablet driver's
  own curve.
- Hardware is not available in CI. Unit tests cover normalization and the
  disabled path; `:pen` is the manual check.

## Troubleshooting

1. `:pen` says **not detected** → `libinput list-devices` or `sudo evtest`
   should list a tablet with `ABS_PRESSURE`. Check the device name contains one
   of the keywords above.
2. `:pen` says **permission denied** → the `input` group (above), or OTD's udev
   rules are missing.
3. Pressure is always 100% → a manual `:pressure` override is active; run
   `:pressure auto`.
4. Clicks work but no pressure → another process (OTD) grabbed the physical
   device; Modaline reads the OTD virtual tablet instead. Make sure OTD is running.
