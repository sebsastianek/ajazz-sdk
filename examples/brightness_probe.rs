use ajazz_sdk::{list_devices, new_hidapi, Ajazz};
use std::thread::sleep;
use std::time::Duration;

fn main() {
    let hid = new_hidapi().expect("hidapi");
    let devices = list_devices(&hid);
    let (kind, serial) = devices.first().expect("no AJAZZ devices found");
    println!("connecting to {:?}", kind);
    let device = Ajazz::connect_with_retries(&hid, *kind, serial, 10).expect("open");

    for pct in [100u8, 0, 100, 0, 100] {
        println!("brightness -> {}", pct);
        device.set_brightness(pct).expect("brightness");
        sleep(Duration::from_millis(1200));
    }
    println!("done");
}
