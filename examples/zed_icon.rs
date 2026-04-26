use ajazz_sdk::{list_devices, new_hidapi, Ajazz};

fn main() {
    let image_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "testdata/zed-key.png".to_string());

    let hid = new_hidapi().expect("failed to init hidapi");
    let devices = list_devices(&hid);
    let (kind, serial) = devices.first().expect("no AJAZZ devices found");
    println!("connecting to {:?} serial={}", kind, serial);

    let device = Ajazz::connect_with_retries(&hid, *kind, serial, 10)
        .expect("failed to open device");

    device.set_brightness(80).expect("set_brightness");
    device.clear_all_button_images().expect("clear");

    let image = image::open(&image_path).expect("open image");
    let count: u8 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    for key in 0..count.min(kind.display_key_count()) {
        device
            .set_button_image(key, image.clone())
            .expect("set_button_image");
    }
    device.flush().expect("flush");

    for zone in 0..kind.strip_zone_count() {
        device
            .set_strip_zone_image(zone, image.clone())
            .expect("set_strip_zone_image");
    }
    println!(
        "pushed '{}' to {} grid keys + {} strip zones",
        image_path,
        count,
        kind.strip_zone_count()
    );
}
