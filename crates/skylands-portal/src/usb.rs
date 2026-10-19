use crate::protocol::*;
use hidapi::{HidApi, HidDevice, HidError};
use log::info;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum UsbPortalError {
    #[error("HID API error: {0}")]
    Hid(#[from] HidError),
    #[error("Portal device not found (VID: 0x{TRAPTANIUM_VID:04X}, PID: 0x{TRAPTANIUM_PID:04X})")]
    DeviceNotFound,
    #[error("Communication error: {0}")]
    Comm(String),
}

pub struct UsbPortal {
    device: HidDevice,
}

impl UsbPortal {
    pub fn open_first() -> Result<Self, UsbPortalError> {
        let api = HidApi::new()?;
        
        let mut target_device = None;
        for dev in api.device_list() {
            if dev.vendor_id() == TRAPTANIUM_VID {
                if dev.product_id() == TRAPTANIUM_PID || dev.product_id() == SWAPFORCE_PID {
                    info!(
                        "Found Portal: VID 0x{:04x} PID 0x{:04x} ({:?})",
                        dev.vendor_id(),
                        dev.product_id(),
                        dev.product_string()
                    );
                    target_device = Some(dev.clone());
                    break;
                }
            }
        }

        let dev_info = target_device.ok_or(UsbPortalError::DeviceNotFound)?;
        let device = dev_info.open_device(&api)?;
        device.set_blocking_mode(false)?;

        let mut portal = Self { device };
        portal.init()?;
        Ok(portal)
    }

    pub fn init(&mut self) -> Result<(), UsbPortalError> {
        // Handshake: Reset / Ready
        self.send_packet(&encode_ready_packet())?;
        // Enable RFID antenna
        self.send_packet(&encode_antenna_packet(true))?;
        // Set default cyan portal color
        self.send_packet(&encode_color_packet(PortalZone::All, 0, 160, 255))?;
        // Set trap slot light
        self.send_packet(&encode_trap_light_packet(255, 100, 0))?;
        Ok(())
    }

    pub fn send_packet(&mut self, pkt: &[u8; REPORT_SIZE]) -> Result<(), UsbPortalError> {
        // Windows HID write typically needs report ID 0 prepended (33 bytes total)
        // while Linux udev handles raw 32 bytes or 33 bytes.
        #[cfg(target_os = "windows")]
        {
            let mut out = [0u8; REPORT_SIZE + 1];
            out[0] = 0; // Report ID
            out[1..].copy_from_slice(pkt);
            self.device.write(&out)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On Linux, try direct 32-byte write, fallback to 33-byte if rejected
            if let Err(_) = self.device.write(pkt) {
                let mut out = [0u8; REPORT_SIZE + 1];
                out[0] = 0;
                out[1..].copy_from_slice(pkt);
                self.device.write(&out)?;
            }
        }

        Ok(())
    }

    pub fn poll_packet(&mut self) -> Result<Option<[u8; REPORT_SIZE]>, UsbPortalError> {
        let mut buf = [0u8; REPORT_SIZE + 1];
        match self.device.read(&mut buf) {
            Ok(bytes_read) if bytes_read >= REPORT_SIZE => {
                let mut report = [0u8; REPORT_SIZE];
                // Handle potential 1-byte report ID offset
                if bytes_read == REPORT_SIZE + 1 {
                    report.copy_from_slice(&buf[1..=REPORT_SIZE]);
                } else {
                    report.copy_from_slice(&buf[..REPORT_SIZE]);
                }
                Ok(Some(report))
            }
            Ok(_) => Ok(None),
            Err(e) => Err(UsbPortalError::Hid(e)),
        }
    }

    pub fn set_led_color(&mut self, zone: PortalZone, r: u8, g: u8, b: u8) -> Result<(), UsbPortalError> {
        self.send_packet(&encode_color_packet(zone, r, g, b))
    }

    pub fn set_trap_light(&mut self, r: u8, g: u8, b: u8) -> Result<(), UsbPortalError> {
        self.send_packet(&encode_trap_light_packet(r, g, b))
    }

    pub fn query_tag_block(&mut self, tag_index: u8, block: u8) -> Result<(), UsbPortalError> {
        self.send_packet(&encode_query_packet(tag_index, block))
    }

    pub fn write_tag_block(&mut self, tag_index: u8, block: u8, data: &[u8; 16]) -> Result<(), UsbPortalError> {
        self.send_packet(&encode_write_packet(tag_index, block, data))
    }
}
