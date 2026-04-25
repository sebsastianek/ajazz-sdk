//! Opens both N1 vendor HID interfaces simultaneously: writes init to one and reads from the other.
//! Designed to confirm whether N1 needs split read/write across two HID handles.

use std::time::Duration;

use ajazz_sdk::{new_hidapi, Kind};

fn pad(buf: &mut Vec<u8>, total_len: usize) {
    if buf.len() < total_len {
        buf.extend(std::iter::repeat(0u8).take(total_len - buf.len()));
    }
}

fn handshake() -> Vec<u8> {
    // 0x00 (report id) + "HAN" + padding to 1025
    let mut v = vec![0x00, 0x48, 0x41, 0x4e];
    pad(&mut v, 1025);
    v
}

fn initialize() -> Vec<u8> {
    // 0x00 + CRT\0\0 + DIS\0\0 + padding
    let mut v = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x44, 0x49, 0x53, 0x00, 0x00];
    pad(&mut v, 1025);
    v
}

fn brightness(p: u8) -> Vec<u8> {
    let mut v = vec![
        0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, p,
    ];
    pad(&mut v, 1025);
    v
}

fn main() {
    let hid = new_hidapi().expect("hidapi");

    let infos: Vec<_> = hid
        .device_list()
        .filter(|i| Kind::from_vid_pid(i.vendor_id(), i.product_id()) == Some(Kind::AkpN1))
        .filter(|i| i.usage_page() == 0xffa0)
        .collect();

    let out_info = infos
        .iter()
        .find(|i| i.usage() == 0x0002)
        .expect("0xffa0/0x0002 not found");
    let in_info = infos
        .iter()
        .find(|i| i.usage() == 0x0001)
        .expect("0xffa0/0x0001 not found");

    println!("opening output (usage=0x0002)…");
    let out_dev = out_info.open_device(&hid).expect("open output");
    println!("opening input (usage=0x0001)…");
    let in_dev = in_info.open_device(&hid).expect("open input");

    println!("sending handshake on output channel…");
    let n = out_dev.write(&handshake()).expect("write handshake");
    println!("  wrote {} bytes", n);
    std::thread::sleep(Duration::from_millis(50));

    println!("sending init on output channel…");
    let n = out_dev.write(&initialize()).expect("write init");
    println!("  wrote {} bytes", n);
    std::thread::sleep(Duration::from_millis(50));

    println!("sending brightness=50…");
    let n = out_dev.write(&brightness(50)).expect("write brightness");
    println!("  wrote {} bytes", n);

    println!("\nNow reading input (usage=0x0001) for 30s — press buttons!");
    let mut buf = [0u8; 512];
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        match in_dev.read_timeout(&mut buf, 200) {
            Ok(n) if n > 0 => {
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

    println!("\nAlso trying input on output channel itself (in case input comes back through 0x0002)…");
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while std::time::Instant::now() < deadline {
        match out_dev.read_timeout(&mut buf, 200) {
            Ok(n) if n > 0 => {
                let preview: Vec<String> =
                    buf[..n.min(20)].iter().map(|b| format!("{:02x}", b)).collect();
                println!("  rx-on-0x0002 {} bytes: {}", n, preview.join(" "));
            }
            Ok(_) => {}
            Err(e) => {
                println!("  read err: {}", e);
                break;
            }
        }
    }
}
