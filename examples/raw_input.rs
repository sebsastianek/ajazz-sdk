use std::time::Duration;

use ajazz_sdk::{new_hidapi, Kind};

fn main() {
    let hid = match new_hidapi() {
        Ok(hid) => hid,
        Err(e) => {
            eprintln!("Failed to create HidApi instance: {}", e);
            return;
        }
    };

    let mut matched = false;

    for info in hid.device_list() {
        let Some(kind) = Kind::from_vid_pid(info.vendor_id(), info.product_id()) else {
            continue;
        };

        matched = true;
        println!(
            "opening kind={kind:?} iface={} usage_page=0x{:04x} usage=0x{:04x} serial={:?}",
            info.interface_number(),
            info.usage_page(),
            info.usage(),
            info.serial_number()
        );

        let Ok(device) = info.open_device(&hid) else {
            println!("  open failed");
            continue;
        };

        let mut buf = [0u8; 512];
        for _ in 0..50 {
            match device.read_timeout(&mut buf, Duration::from_millis(500).as_millis() as i32)
            {
                Ok(size) if size > 0 => {
                    println!("  read {} bytes", size);
                    println!("  first 32 bytes: {:02x?}", &buf[..32]);
                }
                Ok(_) => {}
                Err(e) => {
                    println!("  read error: {}", e);
                    break;
                }
            }
        }

        println!();
    }

    if !matched {
        println!("No supported Ajazz/Mirabox devices found.");
    }
}
