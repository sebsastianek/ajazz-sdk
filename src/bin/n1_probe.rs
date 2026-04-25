use std::ffi::CStr;
use std::fs;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

const VID: u16 = 0x0300;
const PID: u16 = 0x3007;

fn main() {
    let hid = HidApi::new().expect("failed to create HidApi");

    let infos = hid
        .device_list()
        .filter(|info| info.vendor_id() == VID && info.product_id() == PID)
        .collect::<Vec<_>>();

    if infos.is_empty() {
        eprintln!("N1 not found");
        return;
    }

    for info in infos {
        println!(
            "iface={} usage_page=0x{:04x} usage=0x{:04x} serial={:?} path={}",
            info.interface_number(),
            info.usage_page(),
            info.usage(),
            info.serial_number(),
            cstr_to_string(info.path())
        );

        match info.open_device(&hid) {
            Ok(device) => {
                println!("open=ok");
                probe_device(&device);
            }
            Err(err) => {
                println!("open=err {err}");
            }
        }

        println!();
    }
}

fn probe_device(device: &HidDevice) {
    dump_feature_version(device);

    send_packet(device, "initialize", initialize_packet());
    send_packet(device, "brightness", brightness_packet(35));
    send_packet(device, "clear-all", clear_all_packet());
    send_packet(device, "flush", flush_packet());
    send_packet(device, "logo-announce", logo_packet_v1());
    push_logo_test_image(device);
}

fn dump_feature_version(device: &HidDevice) {
    let mut buf = vec![0u8; 21];
    buf[0] = 0x01;

    match device.get_feature_report(buf.as_mut_slice()) {
        Ok(size) => {
            println!(
                "feature_version size={size} bytes={:02x?}",
                &buf[..size.min(21)]
            );
        }
        Err(err) => {
            println!("feature_version err {err}");
        }
    }
}

fn send_packet(device: &HidDevice, label: &str, packet: Vec<u8>) {
    println!("{label} write_len={}", packet.len());
    match device.write(packet.as_slice()) {
        Ok(size) => println!("{label} write_ok={size}"),
        Err(err) => {
            println!("{label} write_err {err}");
            return;
        }
    }

    let mut buf = [0u8; 512];
    match device.read_timeout(&mut buf, Duration::from_millis(500).as_millis() as i32) {
        Ok(size) if size > 0 => {
            println!("{label} read_ok={size} bytes={:02x?}", &buf[..size.min(32)]);
        }
        Ok(_) => {
            println!("{label} read_timeout");
        }
        Err(err) => {
            println!("{label} read_err {err}");
        }
    }
}

fn initialize_packet() -> Vec<u8> {
    pad_packet(vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x44, 0x49, 0x53])
}

fn brightness_packet(percent: u8) -> Vec<u8> {
    pad_packet(vec![
        0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x49, 0x47, 0x00, 0x00, percent,
    ])
}

fn clear_all_packet() -> Vec<u8> {
    pad_packet(vec![
        0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x43, 0x4c, 0x45, 0x00, 0x00, 0x00, 0xff,
    ])
}

fn flush_packet() -> Vec<u8> {
    pad_packet(vec![0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x53, 0x54, 0x50])
}

fn logo_packet_v1() -> Vec<u8> {
    pad_packet(vec![
        0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x4c, 0x4f, 0x47, 0x00, 0x12, 0xc3, 0xc0, 0x01,
    ])
}

fn show_logo_packet() -> Vec<u8> {
    pad_packet(vec![
        0x00, 0x43, 0x52, 0x54, 0x00, 0x00, 0x43, 0x4c, 0x45, 0x00, 0x00, 0x44, 0x43,
    ])
}

fn push_logo_test_image(device: &HidDevice) {
    let path = "/Users/sebsastianek/Workspace/ajazz/ajazz-sdk/testdata/n1-logo-test.jpg";
    let image = match fs::read(path) {
        Ok(image) => image,
        Err(err) => {
            println!("logo-data read_err {err}");
            return;
        }
    };

    println!("logo-data bytes={}", image.len());
    let mut page_number = 0usize;
    let mut offset = 0usize;

    while offset < image.len() {
        let end = (offset + 512).min(image.len());
        let mut packet = vec![0x00];
        packet.extend_from_slice(&image[offset..end]);
        packet.resize(513, 0x00);

        match device.write(packet.as_slice()) {
            Ok(size) => println!("logo-data page={page_number} write_ok={size}"),
            Err(err) => {
                println!("logo-data page={page_number} write_err {err}");
                return;
            }
        }

        offset = end;
        page_number += 1;
    }

    send_packet(device, "show-logo", show_logo_packet());
}

fn pad_packet(mut packet: Vec<u8>) -> Vec<u8> {
    packet.resize(513, 0x00);
    packet
}

fn cstr_to_string(path: &CStr) -> String {
    path.to_string_lossy().into_owned()
}
