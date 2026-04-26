use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use ajazz_sdk::{list_devices, new_hidapi, Ajazz, Kind};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        return Err(help());
    };

    match command.as_str() {
        "help" | "--help" | "-h" => Err(help()),
        "list" => {
            ensure_no_args(args)?;
            list()?;
            Ok(())
        }
        "info" => {
            let serial = parse_serial_flag(args)?;
            info(serial)
        }
        "brightness" => {
            let Some(percent) = args.next() else {
                return Err("missing brightness percent".to_string());
            };
            let percent = parse_u8_arg("brightness percent", &percent)?;
            let serial = parse_serial_flag(args)?;
            brightness(serial, percent)
        }
        "clear-all" => {
            let serial = parse_serial_flag(args)?;
            clear_all(serial)
        }
        "reset" => {
            let serial = parse_serial_flag(args)?;
            reset(serial)
        }
        "set-logo" => {
            let Some(path) = args.next() else {
                return Err("missing image path".to_string());
            };
            let serial = parse_serial_flag(args)?;
            set_logo(serial, PathBuf::from(path))
        }
        "set-key" => {
            let Some(key) = args.next() else {
                return Err("missing key index".to_string());
            };
            let Some(path) = args.next() else {
                return Err("missing image path".to_string());
            };
            let key = parse_u8_arg("key index", &key)?;
            let serial = parse_serial_flag(args)?;
            set_key(serial, key, PathBuf::from(path))
        }
        "set-all" => {
            let Some(path) = args.next() else {
                return Err("missing image path".to_string());
            };
            let serial = parse_serial_flag(args)?;
            set_all(serial, PathBuf::from(path))
        }
        _ => Err(format!("unknown command '{command}'\n\n{}", help())),
    }
}

fn list() -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let mut devices = list_devices(&hid);
    devices.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| kind_name(left.0).cmp(kind_name(right.0)))
    });

    if devices.is_empty() {
        println!("No supported Ajazz devices found.");
        return Ok(());
    }

    for (kind, serial) in devices {
        println!("{:<10} {}", kind_name(kind), serial);
    }

    Ok(())
}

fn info(serial: Option<String>) -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;

    println!("kind: {}", kind_name(kind));
    println!(
        "serial: {}",
        device.serial_number().map_err(|e| e.to_string())?
    );
    println!(
        "manufacturer: {}",
        device.manufacturer().map_err(|e| e.to_string())?
    );
    println!("product: {}", device.product().map_err(|e| e.to_string())?);
    println!(
        "firmware: {}",
        device.firmware_version().map_err(|e| e.to_string())?
    );
    println!("keys: {}", kind.key_count());
    println!("display keys: {}", kind.display_key_count());
    println!("encoders: {}", kind.encoder_count());

    if let Some((width, height)) = kind.lcd_strip_size() {
        println!("lcd strip: {}x{}", width, height);
    }

    Ok(())
}

fn brightness(serial: Option<String>, percent: u8) -> Result<(), String> {
    if percent > 100 {
        return Err("brightness percent must be in the range 0..=100".to_string());
    }

    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;
    device.set_brightness(percent).map_err(|e| e.to_string())
}

fn clear_all(serial: Option<String>) -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;
    device.clear_all_button_images().map_err(|e| e.to_string())
}

fn reset(serial: Option<String>) -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;
    device.reset().map_err(|e| e.to_string())
}

fn set_logo(serial: Option<String>, path: PathBuf) -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;
    let image = image::open(&path).map_err(|e| e.to_string())?;
    device.set_logo_image(image).map_err(|e| e.to_string())
}

fn set_key(serial: Option<String>, key: u8, path: PathBuf) -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;
    let image = image::open(&path).map_err(|e| e.to_string())?;
    device
        .set_button_image(key, image)
        .map_err(|e| e.to_string())?;
    device.flush().map_err(|e| e.to_string())
}

fn set_all(serial: Option<String>, path: PathBuf) -> Result<(), String> {
    let hid = new_hidapi().map_err(|e| e.to_string())?;
    let (kind, serial) = select_device(&hid, serial)?;
    let device =
        Ajazz::connect_with_retries(&hid, kind, &serial, 10).map_err(|e| e.to_string())?;
    let image = image::open(&path).map_err(|e| e.to_string())?;

    for key in 0..kind.display_key_count() {
        device
            .set_button_image(key, image.clone())
            .map_err(|e| e.to_string())?;
    }

    device.flush().map_err(|e| e.to_string())
}

fn select_device(
    hid: &hidapi::HidApi,
    requested_serial: Option<String>,
) -> Result<(Kind, String), String> {
    let mut devices = list_devices(hid);
    devices.sort_by(|left, right| {
        left.1
            .cmp(&right.1)
            .then_with(|| kind_name(left.0).cmp(kind_name(right.0)))
    });

    if devices.is_empty() {
        return Err("no supported Ajazz devices found".to_string());
    }

    if let Some(serial) = requested_serial {
        return devices
            .into_iter()
            .find(|(_, current_serial)| current_serial == &serial)
            .ok_or_else(|| format!("no supported Ajazz device found with serial {serial}"));
    }

    if devices.len() == 1 {
        return Ok(devices.remove(0));
    }

    let options = devices
        .into_iter()
        .map(|(kind, serial)| format!("  {} {}", kind_name(kind), serial))
        .collect::<Vec<_>>()
        .join("\n");
    Err(format!(
        "multiple Ajazz devices found, pass --serial SERIAL:\n{}",
        options
    ))
}

fn parse_serial_flag(
    mut args: impl Iterator<Item = String>,
) -> Result<Option<String>, String> {
    let mut serial = None;

    while let Some(arg) = args.next() {
        if arg != "--serial" {
            return Err(format!("unexpected argument '{arg}'"));
        }

        let Some(value) = args.next() else {
            return Err("missing value after --serial".to_string());
        };

        if serial.replace(value).is_some() {
            return Err("--serial can only be passed once".to_string());
        }
    }

    Ok(serial)
}

fn ensure_no_args(args: impl Iterator<Item = String>) -> Result<(), String> {
    let extra = args.collect::<Vec<_>>();
    if extra.is_empty() {
        Ok(())
    } else {
        Err(format!("unexpected arguments: {}", extra.join(" ")))
    }
}

fn parse_u8_arg(name: &str, value: &str) -> Result<u8, String> {
    value
        .parse::<u8>()
        .map_err(|_| format!("invalid {name}: {value}"))
}

fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Akp153 => "AKP153",
        Kind::Akp153E => "AKP153E",
        Kind::Akp153R => "AKP153R",
        Kind::Akp815 => "AKP815",
        Kind::Akp03 => "AKP03",
        Kind::Akp03E => "AKP03E",
        Kind::Akp03R => "AKP03R",
        Kind::Akp03RRev2 => "AKP03RV2",
        Kind::AkpN1 => "N1",
    }
}

fn help() -> String {
    [
        "Usage:",
        "  cargo run --bin ajazz -- list",
        "  cargo run --bin ajazz -- info [--serial SERIAL]",
        "  cargo run --bin ajazz -- brightness <0-100> [--serial SERIAL]",
        "  cargo run --bin ajazz -- clear-all [--serial SERIAL]",
        "  cargo run --bin ajazz -- reset [--serial SERIAL]",
        "  cargo run --bin ajazz -- set-logo <image-path> [--serial SERIAL]",
        "  cargo run --bin ajazz -- set-key <key-index> <image-path> [--serial SERIAL]",
        "  cargo run --bin ajazz -- set-all <image-path> [--serial SERIAL]",
    ]
    .join("\n")
}
