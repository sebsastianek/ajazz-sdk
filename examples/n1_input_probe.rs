//! Read-only diagnostic: opens each N1 HID interface in turn and prints any input it receives.
//! Run, then press buttons / turn the encoder. Shows which interface delivers events.

use std::time::Duration;

use ajazz_sdk::{new_hidapi, Kind};

fn main() {
    let hid = new_hidapi().expect("hidapi");

    let infos: Vec<_> = hid
        .device_list()
        .filter(|info| Kind::from_vid_pid(info.vendor_id(), info.product_id()) == Some(Kind::AkpN1))
        .collect();

    if infos.is_empty() {
        println!("no N1 found");
        return;
    }

    for info in infos {
        println!(
            "iface={} usage_page=0x{:04x} usage=0x{:04x} — reading 15s, press things now",
            info.interface_number(),
            info.usage_page(),
            info.usage()
        );

        let device = match info.open_device(&hid) {
            Ok(d) => d,
            Err(e) => {
                println!("  open failed: {}\n", e);
                continue;
            }
        };

        let mut buf = [0u8; 512];
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        let mut got_anything = false;
        while std::time::Instant::now() < deadline {
            match device.read_timeout(&mut buf, 200) {
                Ok(n) if n > 0 => {
                    got_anything = true;
                    let preview: Vec<String> =
                        buf[..n.min(20)].iter().map(|b| format!("{:02x}", b)).collect();
                    println!("  rx {} bytes: {}", n, preview.join(" "));
                }
                Ok(_) => {}
                Err(e) => {
                    println!("  read err: {}", e);
                    break;
                }
            }
        }
        if !got_anything {
            println!("  (no data in 15s)");
        }
        println!();
    }
}
