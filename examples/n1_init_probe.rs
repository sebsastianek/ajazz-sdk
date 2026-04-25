//! Mirrors the vendor app's exact N1 startup sequence (no HAN), then reads input.

use std::time::Duration;

use ajazz_sdk::{new_hidapi, Kind};

fn pad(mut buf: Vec<u8>) -> Vec<u8> {
    if buf.len() < 1025 {
        buf.extend(std::iter::repeat(0u8).take(1025 - buf.len()));
    }
    buf
}

fn cmd(opcode: &[u8]) -> Vec<u8> {
    let mut v = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00];
    v.extend_from_slice(opcode);
    pad(v)
}

fn cmd_with_body(opcode: &[u8], body: &[u8]) -> Vec<u8> {
    let mut v = vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00];
    v.extend_from_slice(opcode);
    v.extend_from_slice(body);
    pad(v)
}

fn main() {
    let hid = new_hidapi().expect("hidapi");

    let info = hid
        .device_list()
        .filter(|i| Kind::from_vid_pid(i.vendor_id(), i.product_id()) == Some(Kind::AkpN1))
        .find(|i| i.usage_page() == 0xffa0 && i.usage() == 0x0001)
        .expect("N1 vendor interface 0x0001 not found");

    println!("opening N1 (usage=0x0001)…");
    let dev = info.open_device(&hid).expect("open");

    // Read firmware via feature report id=1
    let mut fw = vec![0x01; 21];
    match dev.get_feature_report(&mut fw) {
        Ok(n) => println!("  firmware: {:?}", String::from_utf8_lossy(&fw[1..n])),
        Err(e) => println!("  firmware read err: {}", e),
    }

    let send = |label: &str, packet: &[u8]| match dev.write(packet) {
        Ok(n) => println!("  -> {} ({} bytes)", label, n),
        Err(e) => println!("  -> {} ERR: {}", label, e),
    };

    println!("\nReplaying vendor init sequence:");
    send("DIS", &cmd(b"DIS\0\0"));
    std::thread::sleep(Duration::from_millis(20));
    send("LIG=0x19", &cmd_with_body(b"LIG\0\0", &[0x19]));
    std::thread::sleep(Duration::from_millis(20));
    send(
        "QUCMD 11 11 00 11 00 11",
        &cmd_with_body(b"QUCMD", &[0x11, 0x11, 0x00, 0x11, 0x00, 0x11]),
    );
    std::thread::sleep(Duration::from_millis(20));
    send("MOD=0x33", &cmd_with_body(b"MOD\0\0", &[0x33]));
    std::thread::sleep(Duration::from_millis(20));
    send("LIG=0x19 (again)", &cmd_with_body(b"LIG\0\0", &[0x19]));

    println!("\nNow reading input for 30s — press buttons!");
    let mut buf = [0u8; 512];
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        match dev.read_timeout(&mut buf, 200) {
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
}
