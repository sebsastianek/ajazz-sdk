// Bare hidapi probe: bypass the SDK entirely. Try delivering LIG (brightness)
// as a feature report instead of an interrupt-OUT write. If the backlight
// pulses in "feature" mode but not "output" mode, the N1 wants feature reports.
//
// Usage:
//   cargo run --example feature_probe output
//   cargo run --example feature_probe feature

use hidapi::HidApi;
use std::thread::sleep;
use std::time::Duration;

const VID: u16 = 0x0300;
const PID: u16 = 0x3007;
const PACKET_LEN: usize = 513; // 1 report id + 512 payload

fn lig_packet(pct: u8) -> Vec<u8> {
    let mut buf = vec![0x00u8, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, pct];
    buf.resize(PACKET_LEN, 0x00);
    buf
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "output".into());
    let api = HidApi::new().expect("hidapi");

    // Prefer usage=0x0002 (matches SDK's N1 selection)
    let mut candidates: Vec<_> = api
        .device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
        .collect();
    candidates.sort_by_key(|d| match (d.usage_page(), d.usage()) {
        (0xffa0, 0x0002) => 0u8,
        (0xffa0, 0x0001) => 1,
        _ => 2,
    });

    for d in &candidates {
        println!(
            "candidate: iface={} usage_page={:#06x} usage={:#06x} path={:?}",
            d.interface_number(),
            d.usage_page(),
            d.usage(),
            d.path()
        );
    }

    let info = candidates.first().expect("no N1 found");
    let dev = info.open_device(&api).expect("open");

    println!("mode={}", mode);
    for pct in [100u8, 0, 100, 0, 100] {
        let p = lig_packet(pct);
        let result_str = match mode.as_str() {
            "output" => format!("{:?}", dev.write(&p)),
            "feature" => format!("{:?}", dev.send_feature_report(&p)),
            other => panic!("unknown mode '{}'", other),
        };
        println!("  lig({}) mode={} -> {}", pct, mode, result_str);
        sleep(Duration::from_millis(1000));
    }
    println!("done");
}
