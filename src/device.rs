use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hidapi::{HidApi, HidDevice, HidError};
use image::imageops::overlay;
use image::{DynamicImage, RgbaImage};

use crate::images::{convert_image, WriteImageParameters};
use crate::info::Kind;
use crate::protocol::{codes, extract_string, request, AjazzProtocolParser, AjazzRequestBuilder};
use crate::{convert_image_with_format, AjazzError, AjazzInput, DeviceState, Event};

/// Interface for an Ajazz device
pub struct Ajazz {
    /// Kind of the device
    kind: Kind,
    /// Connected HIDDevice
    hid: HidDevice,
    /// Temporarily cache the image before sending it to the device
    image_cache: RwLock<Vec<ImageCache>>,
    /// Device needs to be initialized
    initialized: AtomicBool,
}

struct ImageCache {
    key: u8,
    image_data: Vec<u8>,
}

/// Static functions of the struct
impl Ajazz {
    /// Attempts to connect to the device
    /// If the connection fails, it will retry up to `attempts` times
    /// If the connection fails after `attempts` retries, it will return an last error
    pub fn connect_with_retries(
        hidapi: &HidApi,
        kind: Kind,
        serial: &str,
        attempts: u8,
    ) -> Result<Ajazz, AjazzError> {
        if attempts == 0 {
            return Err(AjazzError::UnsupportedOperation);
        }

        let mut last_error = None;
        for _ in 0..attempts {
            match Self::try_connect(hidapi, kind, serial) {
                Ok(device) => return Ok(device),
                Err(e) => {
                    std::thread::sleep(Duration::from_millis(100));
                    last_error = Some(e);
                    continue;
                }
            }
        }

        Err(last_error.expect("error must never be empty at this point"))
    }

    /// Attempts to connect to the device
    pub fn connect(hidapi: &HidApi, kind: Kind, serial: &str) -> Result<Ajazz, AjazzError> {
        Self::try_connect(hidapi, kind, serial)
    }

    // Internal function to connect to the device
    fn try_connect(hidapi: &HidApi, kind: Kind, serial: &str) -> Result<Ajazz, AjazzError> {
        let mut candidates = hidapi
            .device_list()
            .filter(|info| {
                info.vendor_id() == kind.vendor_id() && info.product_id() == kind.product_id()
            })
            .filter(|info| info.serial_number() == Some(serial))
            .collect::<Vec<_>>();

        if kind.is_n1() {
            // Prefer the input/control interface (0xffa0 / 0x0001). The 0x0002 interface
            // appears to be for output (image data) only — opening it first leaves the
            // SDK reading from a HID handle that never delivers button/encoder reports.
            candidates.sort_by_key(|info| match (info.usage_page(), info.usage()) {
                (0xffa0, 0x0001) => 0u8,
                (0xffa0, 0x0002) => 1u8,
                _ => 2u8,
            });
        }

        let mut last_error = None;
        for info in candidates {
            match info.open_device(hidapi) {
                Ok(device) => {
                    return Ok(Ajazz {
                        kind,
                        hid: device,
                        image_cache: RwLock::new(vec![]),
                        initialized: false.into(),
                    });
                }
                Err(e) => last_error = Some(e),
            }
        }

        let device = match last_error {
            Some(e) => return Err(e.into()),
            None => hidapi.open_serial(kind.vendor_id(), kind.product_id(), serial)?,
        };

        Ok(Ajazz {
            kind,
            hid: device,
            image_cache: RwLock::new(vec![]),
            initialized: false.into(),
        })
    }
}

/// Instance methods of the struct
impl Ajazz {
    /// Returns kind of the Ajazz device
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Returns manufacturer string of the device
    pub fn manufacturer(&self) -> Result<String, AjazzError> {
        Ok(self
            .hid
            .get_manufacturer_string()?
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Returns product string of the device
    pub fn product(&self) -> Result<String, AjazzError> {
        Ok(self
            .hid
            .get_product_string()?
            .unwrap_or_else(|| "Unknown".to_string()))
    }

    /// Returns serial number of the device
    pub fn serial_number(&self) -> Result<String, AjazzError> {
        let serial = self.hid.get_serial_number_string()?;
        match serial {
            Some(serial) => {
                if serial.is_empty() {
                    Ok("Unknown".to_string())
                } else {
                    Ok(serial)
                }
            }
            None => Ok("Unknown".to_string()),
        }
    }

    /// Returns firmware version of the device
    pub fn firmware_version(&self) -> Result<String, AjazzError> {
        let mut buff = request::FEATURE_REPORT_VERSION.clone();
        self.hid.get_feature_report(buff.as_mut_slice())?;

        let version = extract_string(&buff[0..])?;
        Ok(version)
    }

    /// Sleeps the device
    pub fn sleep(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.sleep_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Make periodic events to the device, to keep it alive
    pub fn keep_alive(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.keep_alive_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Returns device state reader for this device
    pub fn get_reader(self: &Arc<Self>) -> Arc<DeviceStateReader> {
        #[allow(clippy::arc_with_non_send_sync)]
        Arc::new(DeviceStateReader {
            device: self.clone(),
            states: Mutex::new(DeviceState {
                buttons: vec![false; self.kind.key_count() as usize],
                encoders: vec![false; self.kind.encoder_count() as usize],
            }),
        })
    }

    /// Shutdown the device
    pub fn shutdown(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.shutdown_packet();
        self.hid.write(packet.as_slice())?;

        let packet = self.kind.sleep_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Reads input from the device
    pub fn read_input(&self, timeout: Option<Duration>) -> Result<AjazzInput, AjazzError> {
        self.initialize()?;

        let data = self.read_data(codes::INPUT_PACKET_LENGTH, timeout)?;
        self.kind.parse_input(&data)
    }

    /// Resets the device
    pub fn reset(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        self.set_brightness(100)?;
        self.clear_all_button_images()
    }

    /// Sets brightness of the device, value range is 0 - 100
    pub fn set_brightness(&self, percent: u8) -> Result<(), AjazzError> {
        self.initialize()?;

        let buf = self.kind.brightness_packet(percent);
        self.hid.write(buf.as_slice())?;

        Ok(())
    }

    /// Sets button's image to blank, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn clear_button_image(&self, key: u8) -> Result<(), AjazzError> {
        self.initialize()?;

        let packet = self.kind.clear_button_image_packet(key);
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Flushes the button's image to the device
    pub fn flush(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        let is_empty = {
            let images = self
                .image_cache
                .read()
                .map_err(|_| AjazzError::PoisonError)?;

            images.is_empty()
        };

        if is_empty {
            return Ok(());
        }

        let mut images = self
            .image_cache
            .write()
            .map_err(|_| AjazzError::PoisonError)?;

        if self.kind.is_n1() {
            let image_data = self.compose_n1_key_grid(&images)?;
            self.write_n1_secondary_screen_image(
                &image_data,
                self.kind.logo_image_format().size.0 as u16,
                self.kind.logo_image_format().size.1 as u16,
            )?;
            images.clear();
            return Ok(());
        }

        for image in images.iter() {
            self.write_key_image(image.key, &image.image_data)?;
        }

        let packet = self.kind.flush_packet();
        self.hid.write(packet.as_slice())?;
        images.clear();

        Ok(())
    }

    /// Sets blank images to every button, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn clear_all_button_images(&self) -> Result<(), AjazzError> {
        self.initialize()?;

        if self.kind.is_n1() {
            let (width, height) = self
                .kind
                .lcd_strip_size()
                .ok_or(AjazzError::UnsupportedOperation)?;
            let image = DynamicImage::ImageRgba8(RgbaImage::new(width as u32, height as u32));
            let image_data = convert_image_with_format(self.kind.logo_image_format(), image)?;
            self.write_n1_secondary_screen_image(&image_data, width as u16, height as u16)?;

            let mut images = self
                .image_cache
                .write()
                .map_err(|_| AjazzError::PoisonError)?;
            images.clear();
            return Ok(());
        }

        self.clear_button_image(codes::CMD_CLEAR_ALL)?;

        if self.kind.is_v2_api() {
            // Mirabox "v2" requires flush to commit clearing the background
            let packet = self.kind.flush_packet();
            self.hid.write(packet.as_slice())?;
        }

        Ok(())
    }

    /// Sets specified button's image, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn set_button_image_data(&self, key: u8, image_data: &[u8]) -> Result<(), AjazzError> {
        self.initialize()?;
        self.write_image_to_cache(key, image_data)?;
        Ok(())
    }

    /// Sets specified button's image, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    pub fn set_button_image(&self, key: u8, image: DynamicImage) -> Result<(), AjazzError> {
        self.initialize()?;
        let image_data = convert_image(self.kind, image)?;
        self.write_image_to_cache(key, &image_data)?;
        Ok(())
    }

    /// Set logo image
    pub fn set_logo_image(&self, image: DynamicImage) -> Result<(), AjazzError> {
        self.initialize()?;

        if self.kind.boot_logo_size().is_none() {
            return Err(AjazzError::UnsupportedOperation);
        }

        let image_data = convert_image_with_format(self.kind.logo_image_format(), image)?;

        if self.kind.is_n1() {
            self.write_n1_secondary_screen_image(
                &image_data,
                self.kind.logo_image_format().size.0 as u16,
                self.kind.logo_image_format().size.1 as u16,
            )?;
            return Ok(());
        }

        self.hid
            .write(self.kind.logo_image_packet(&image_data).as_slice())?;
        self.hid.write(self.kind.flush_packet().as_slice())?;
        self.write_image_data_reports(&image_data, WriteImageParameters::for_kind(self.kind))?;
        self.assert_write_complete()?;

        Ok(())
    }

    /// Initializes the device
    fn initialize(&self) -> Result<(), AjazzError> {
        if self.initialized.load(Ordering::Acquire) {
            return Ok(());
        }

        self.initialized.store(true, Ordering::Release);

        if self.kind.is_n1() {
            // N1 powers up in keyboard mode (each grid press becomes an OS key event).
            // Send MOD\0\0 33 to switch into software mode where the device emits
            // vendor input reports via the 0xffa0 interface and the host owns the screen.
            // The vendor app's startup capture sends DIS first, then this; replaying that.
            let init = self.kind.initialize_packet();
            self.hid.write(init.as_slice())?;
            std::thread::sleep(Duration::from_millis(20));

            let mode = self.kind.n1_software_mode_packet();
            self.hid.write(mode.as_slice())?;
            std::thread::sleep(Duration::from_millis(20));

            return Ok(());
        }

        let packet = self.kind.initialize_packet();
        self.hid.write(packet.as_slice())?;

        Ok(())
    }

    /// Writes image data to Ajazz device, changes must be flushed with `.flush()` before
    /// they will appear on the device!
    fn write_image_to_cache(&self, key: u8, image_data: &[u8]) -> Result<(), AjazzError> {
        if key >= self.kind.display_key_count() {
            return Err(AjazzError::InvalidKeyIndex(key));
        }

        let cache_entry = ImageCache {
            key,
            image_data: image_data.to_vec(), // Convert &[u8] to Vec<u8>
        };

        let Ok(mut image_cache) = self.image_cache.write() else {
            return Err(AjazzError::PoisonError);
        };

        image_cache.push(cache_entry);

        Ok(())
    }

    /// Writes key image to the device
    fn write_key_image(&self, key: u8, image_data: &[u8]) -> Result<(), AjazzError> {
        if key >= self.kind.display_key_count() {
            return Err(AjazzError::InvalidKeyIndex(key));
        }

        let packet = self.kind.key_image_announce_packet(key, image_data);
        self.hid.write(packet.as_slice())?;

        self.write_image_data_reports(image_data, WriteImageParameters::for_kind(self.kind))?;
        Ok(())
    }

    fn write_image_data_reports(
        &self,
        image_data: &[u8],
        parameters: WriteImageParameters,
    ) -> Result<(), AjazzError> {
        let image_report_length = parameters.image_report_length;
        let image_report_payload_length = parameters.image_report_payload_length;

        let mut page_number = 0;
        let mut bytes_remaining = image_data.len();

        while bytes_remaining > 0 {
            let this_length = bytes_remaining.min(image_report_payload_length);
            let bytes_sent = page_number * image_report_payload_length;

            let mut buf: Vec<u8> = vec![0x00];
            buf.extend(&image_data[bytes_sent..bytes_sent + this_length]);
            buf.extend(vec![0x00; image_report_length - buf.len()]);

            self.hid.write(buf.as_slice())?;
            bytes_remaining -= this_length;
            page_number += 1;
        }

        Ok(())
    }

    fn write_n1_secondary_screen_image(
        &self,
        image_data: &[u8],
        width: u16,
        height: u16,
    ) -> Result<(), AjazzError> {
        let metadata =
            self.n1_background_metadata_packet(image_data, 0, 0, width, height, 0x01)?;
        self.write_n1_background_payload(&metadata, 0x00, 0x00)?;
        std::thread::sleep(Duration::from_millis(20));
        self.write_n1_background_payload(image_data, 0x01, 0x00)?;
        Ok(())
    }

    fn write_n1_background_payload(
        &self,
        payload: &[u8],
        byte_a: u8,
        byte_b: u8,
    ) -> Result<(), AjazzError> {
        const MAX_CHUNK: usize = 0xFFFF;
        let params = WriteImageParameters::for_kind(self.kind);

        if payload.is_empty() {
            let header = self.n1_settings_pack_head(0, byte_a, byte_b, false)?;
            self.hid.write(header.as_slice())?;
            std::thread::sleep(Duration::from_millis(20));
            return Ok(());
        }

        let mut offset = 0;
        while offset < payload.len() {
            let end = (offset + MAX_CHUNK).min(payload.len());
            let chunk = &payload[offset..end];

            let header = self.n1_settings_pack_head(chunk.len(), byte_a, byte_b, false)?;
            self.hid.write(header.as_slice())?;
            std::thread::sleep(Duration::from_millis(20));
            self.write_image_data_reports(chunk, params)?;
            std::thread::sleep(Duration::from_millis(20));

            offset = end;
        }
        Ok(())
    }

    fn n1_settings_pack_head(
        &self,
        size: usize,
        byte_a: u8,
        byte_b: u8,
        final_stage: bool,
    ) -> Result<Vec<u8>, AjazzError> {
        if size > 0xffff {
            return Err(AjazzError::UnsupportedOperation);
        }

        let mut packet =
            vec![0u8; WriteImageParameters::for_kind(self.kind).image_report_length];
        packet[1..4].copy_from_slice(b"CRT");
        packet[9] = if final_stage { 0x0f } else { 0x05 };
        packet[10] = ((size >> 8) & 0xff) as u8;
        packet[11] = (size & 0xff) as u8;
        packet[12] = byte_a;
        packet[13] = byte_b;
        Ok(packet)
    }

    fn n1_background_metadata_packet(
        &self,
        image_data: &[u8],
        x: u16,
        y: u16,
        width: u16,
        height: u16,
        mode: u8,
    ) -> Result<Vec<u8>, AjazzError> {
        if image_data.len() > u32::MAX as usize {
            return Err(AjazzError::UnsupportedOperation);
        }

        let mut packet = Vec::with_capacity(23);
        packet.extend_from_slice(b"CRT\0\0BGPIC");
        packet.extend_from_slice(&(image_data.len() as u32).to_be_bytes());
        packet.extend_from_slice(&x.to_be_bytes());
        packet.extend_from_slice(&y.to_be_bytes());
        packet.extend_from_slice(&width.to_be_bytes());
        packet.extend_from_slice(&height.to_be_bytes());
        packet.push(mode);
        Ok(packet)
    }

    fn compose_n1_key_grid(&self, images: &[ImageCache]) -> Result<Vec<u8>, AjazzError> {
        let (screen_width, screen_height) = self
            .kind
            .lcd_strip_size()
            .ok_or(AjazzError::UnsupportedOperation)?;

        let rows = self.kind.row_count() as u32;
        let cols = self.kind.column_count() as u32;
        let slot_width = self.kind.key_image_format().size.0 as u32;
        let slot_height = self.kind.key_image_format().size.1 as u32;
        let gap_x =
            ((screen_width as u32).saturating_sub(cols * slot_width) / (cols + 1)).max(1);
        let gap_y =
            ((screen_height as u32).saturating_sub(rows * slot_height) / (rows + 1)).max(1);

        let mut canvas = DynamicImage::ImageRgba8(RgbaImage::new(
            screen_width as u32,
            screen_height as u32,
        ));

        for image in images {
            if image.key >= self.kind.display_key_count() {
                continue;
            }

            let decoded = image::load_from_memory(&image.image_data)?;
            let key = image.key as u32;
            let row = key / cols;
            let col = key % cols;
            let x = gap_x + col * (slot_width + gap_x);
            let y = gap_y + row * (slot_height + gap_y);

            overlay(&mut canvas, &decoded.to_rgba8(), x as i64, y as i64);
        }

        Ok(convert_image_with_format(
            self.kind.logo_image_format(),
            canvas,
        )?)
    }

    fn assert_write_complete(&self) -> Result<(), AjazzError> {
        let data = self.read_data(512, Some(Duration::from_millis(1000)))?;
        if data.len() != 512 {
            return Err(AjazzError::BadData);
        }

        if !self.kind.is_ack_ok(&data) {
            return Err(AjazzError::NoAck);
        }

        Ok(())
    }

    /// Reads data from [HidDevice]. Blocking mode is used if timeout is specified
    fn read_data(
        &self,
        length: usize,
        timeout: Option<Duration>,
    ) -> Result<Vec<u8>, HidError> {
        self.hid.set_blocking_mode(timeout.is_some())?;

        let mut buf = vec![0u8; length];

        match timeout {
            Some(timeout) => self
                .hid
                .read_timeout(buf.as_mut_slice(), timeout.as_millis() as i32),
            None => self.hid.read(buf.as_mut_slice()),
        }?;

        Ok(buf)
    }
}

/// Button reader that keeps state of the Ajazz and returns events instead of full states
pub struct DeviceStateReader {
    device: Arc<Ajazz>,
    states: Mutex<DeviceState>,
}

pub(crate) fn handle_input_state_change(
    input: AjazzInput,
    current_state: &mut DeviceState,
) -> Result<Vec<Event>, AjazzError> {
    let mut updates = vec![];
    match input {
        AjazzInput::ButtonStateChange(buttons) => {
            for (index, is_changed) in buttons.iter().enumerate() {
                if !is_changed {
                    continue;
                }

                current_state.buttons[index] = !current_state.buttons[index];
                if current_state.buttons[index] {
                    updates.push(Event::ButtonDown(index as u8));
                } else {
                    updates.push(Event::ButtonUp(index as u8));
                }
            }
        }

        AjazzInput::EncoderStateChange(encoders) => {
            for (index, is_changed) in encoders.iter().enumerate() {
                if !is_changed {
                    continue;
                }

                current_state.encoders[index] = !current_state.encoders[index];
                if current_state.encoders[index] {
                    updates.push(Event::EncoderDown(index as u8));
                } else {
                    updates.push(Event::EncoderUp(index as u8));
                }
            }
        }

        AjazzInput::EncoderTwist(twist) => {
            for (index, change) in twist.iter().enumerate() {
                if *change != 0 {
                    updates.push(Event::EncoderTwist(index as u8, *change));
                }
            }
        }

        _ => {}
    }

    Ok(updates)
}

impl DeviceStateReader {
    /// Reads states and returns updates
    pub fn read(&self, timeout: Option<Duration>) -> Result<Vec<Event>, AjazzError> {
        let input = self.device.read_input(timeout)?;
        let mut current_state = self.states.lock().map_err(|_| AjazzError::PoisonError)?;

        let updates = handle_input_state_change(input, &mut current_state)?;
        Ok(updates)
    }
}
