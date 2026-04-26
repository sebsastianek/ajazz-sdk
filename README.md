# Ajazz SDK — N1 fork

Rust library for talking directly to Ajazz Stream Dock macro pads.

This is a fork of [`mishamyrt/ajazz-sdk`](https://github.com/mishamyrt/ajazz-sdk)
maintained by an Ajazz **N1** owner who wanted to drop the vendor's "Stream Dock
AJAZZ" macOS app and drive the device from Rust instead. Upstream covers the
older AKP153 / AKP815 / AKP03 family; this fork adds first-class **N1** input
and per-key image upload support, decoded by reverse-engineering the vendor
app's HID traffic.

## Why this fork

The vendor app on macOS is heavyweight and fragile. The N1 wasn't supported by
the upstream SDK at all — its action-code map, HID packet sizes, init sequence,
and image-upload protocol are all different from the AKP153 family it shares
hardware lineage with. Without those, neither input events nor button images
worked. This fork closes that gap.

The N1 work is intentionally additive — every existing AKP153/AKP815/AKP03
code path is unchanged. Tests for those still pass. Adding more devices
follows the same pattern (a new `Kind` variant + per-device branches in
`info.rs`, `parser.rs`, `request.rs`, `device.rs`).

## Supported devices

- Ajazz AKP153 / AKP153E / AKP153R *(upstream)*
- Ajazz AKP815 *(upstream)*
- Ajazz AKP03 / AKP03E / AKP03R / AKP03RV2 *(upstream)*
- **Ajazz N1** *(this fork — VID `0x0300`, PID `0x3007`)*

## Features

Upstream + N1-specific additions:

- Reading button / encoder events from the device.
- Setting a custom boot logo *(non-N1)*.
- Setting a custom per-button image.
- **N1: per-key 96×96 JPEG upload** for the 15 LCD grid keys via `set_button_image`.
- **N1: 80×80 JPEG upload** for each of the 3 LCD strip zones via `set_strip_zone_image`.
- **N1: full input dispatch** — 15 grid keys, 2 top function buttons, 1
  encoder (twist + click), all routed as `Event::ButtonDown/Up` and
  `Event::EncoderTwist/Down/Up`.

## What the N1 work uncovered

(Useful if you fork this further or port to another protocol-CRT device.)

- The N1 boots in **HID-keyboard mode** — every grid press shows up to the OS
  as a typed keystroke, and the vendor `0xffa0` HID interface stays silent.
  Sending `CRT MOD\0\0 0x33` flips the device into "software" mode where
  presses come through as vendor input reports and the host owns the screen.
  This is the magic init step. Without it, every other SDK call looks like
  it works but no input ever flows.
- The N1 uses **1024-byte HID output reports** (not 512 like the AKP153
  family), but **v1-style `CRT`-prefixed protocol commands** — so it doesn't
  fit cleanly into the existing `is_v1_api()` / `is_v2_api()` split. The
  fork handles this with explicit `is_n1()` branches in `request.rs` and
  `images.rs`.
- N1 input action codes are linear: `0x01..0x0f` for grid keys 1–15,
  `0x1e/0x1f` for the top function buttons, `0x32/0x33` for encoder CCW/CW,
  `0x23` for encoder press. None of the existing AKP153/AKP03 codes apply.
- Per-key images are uploaded with a `BAT` command that announces a
  big-endian 16-bit size + 1-based element index (1..15 grid, 16..18 strip),
  followed by the JPEG bytes split into 1024-byte chunks, terminated with
  a `STP` separator.

## Installation

```bash
cargo add ajazz-sdk --git https://github.com/sebsastianek/ajazz-sdk --branch wip/n1-support
```

Or, while iterating locally, depend on the path directly:

```toml
[dependencies]
ajazz-sdk = { path = "../ajazz-sdk" }
```

## Usage

```rust
use ajazz_sdk::{new_hidapi, list_devices, Ajazz};

let hid = new_hidapi().expect("hidapi");
let (kind, serial) = list_devices(&hid).remove(0);

let device = Ajazz::connect(&hid, kind, &serial).expect("connect");

println!(
    "Connected to '{}' with version '{}'",
    device.serial_number().unwrap(),
    device.firmware_version().unwrap()
);

device.set_brightness(35).unwrap();

let image = image::open("zed-key.jpg").unwrap();
device.set_button_image(0, image.clone()).unwrap();

// N1 also has 3 strip zones above the grid:
if kind.is_n1() {
    device.set_strip_zone_image(0, image).unwrap();
}

device.flush().unwrap();
```

Reading input is the same on N1 as on the rest of the family — `get_reader()`,
loop over `reader.read(...)`. See [`examples/events.rs`](examples/events.rs)
for the full pattern.

## CLI

A small `ajazz` CLI ships in `src/bin/ajazz.rs` for ad-hoc poking without
writing Rust:

```sh
cargo run --bin ajazz -- list
cargo run --bin ajazz -- info [--serial SERIAL]
cargo run --bin ajazz -- brightness 50
cargo run --bin ajazz -- set-key 0 zed-key.jpg
cargo run --bin ajazz -- set-all zed-key.jpg
cargo run --bin ajazz -- clear-all
```

Works against any supported device including the N1.

## Examples

<img src="docs/doom.jpg" width="300" align="right">

- [`events`](examples/events.rs) — read all input events, including the N1 encoder.
- [`zed_icon`](examples/zed_icon.rs) — push a JPEG to N grid keys + every strip zone.
- [`pizza`](examples/pizza) — async demo from upstream, reacts to presses + twists.
- [`boot_logo`](examples/boot_logo.rs) — set a boot logo (non-N1 devices).
- [`screen_mirroring`](examples/screen_mirroring) — mirror the screen to the device.
- [`n1_input_probe`](examples/n1_input_probe.rs), [`n1_init_probe`](examples/n1_init_probe.rs),
  [`n1_dual_probe`](examples/n1_dual_probe.rs) — diagnostics used during the
  N1 reverse-engineering work; useful if you're porting to another device.

For a full headless companion daemon (profile-switching, app launchers,
keystroke injection) built on top of this SDK, see the sibling
[`companion/`](../companion) crate in the parent workspace.

## Trademarks

`ajazz-sdk` is an unofficial product and is not affiliated with Ajazz company.

## Credits

- [@mishamyrt](https://github.com/mishamyrt) and
  [@TheJebForge](https://github.com/TheJebForge) for the original SDK and
  the AKP153/AKP815/AKP03 protocol work.
- N1 reverse-engineering and integration in this fork.
