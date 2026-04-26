use ajazz_sdk::{new_hidapi, Kind};

fn main() {
    let hid = match new_hidapi() {
        Ok(hid) => hid,
        Err(e) => {
            eprintln!("Failed to create HidApi instance: {}", e);
            return;
        }
    };

    let mut found = false;

    for info in hid.device_list() {
        let Some(kind) = Kind::from_vid_pid(info.vendor_id(), info.product_id()) else {
            continue;
        };

        found = true;

        println!("kind={kind:?}");
        println!(
            "vid=0x{:04x} pid=0x{:04x} iface={} usage_page=0x{:04x} usage=0x{:04x}",
            info.vendor_id(),
            info.product_id(),
            info.interface_number(),
            info.usage_page(),
            info.usage()
        );
        println!("product={:?}", info.product_string());
        println!("serial={:?}", info.serial_number());
        println!("path={}", info.path().to_string_lossy());

        match info.open_device(&hid) {
            Ok(device) => {
                println!("open_device=ok");
                println!("manufacturer={:?}", device.get_manufacturer_string());
                println!("device_product={:?}", device.get_product_string());
                println!("device_serial={:?}", device.get_serial_number_string());
            }
            Err(e) => {
                println!("open_device=err {}", e);
            }
        }

        println!();
    }

    if !found {
        println!("No supported Ajazz/Mirabox devices found.");
    }
}
