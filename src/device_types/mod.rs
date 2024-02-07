mod fake_device;

use std::path::PathBuf;

pub trait Device {
    fn backup(&self, device_id: &u32,device_name: &str, device_ip: &str, backup_path: &PathBuf, file_extension: &str) -> Result<PathBuf, String>;
}

pub fn get_device(device_type: &str) -> Option<Box<dyn Device>> {
    match device_type {
        "FakeDevice" => Some(Box::new(fake_device::FakeDevice::new())),
        _ => None,
    }
}
