mod fake_device;

use std::path::PathBuf;

pub trait Device {
    fn backup(&self, device_name: &str, backup_file: PathBuf) -> Result<PathBuf, String>;
}

pub fn get_device(device_type: &str) -> Option<Box<dyn Device>> {
    match device_type {
        "FakeDevice" => Some(Box::new(fake_device::FakeDevice::new())),
        _ => None,
    }
}
