use crate::device_types::Device;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

pub struct FakeDevice {

}

impl FakeDevice {
    pub fn new() -> Self {
        FakeDevice{}
    }
}

impl Device for FakeDevice {
    fn backup(&self, device_name: &str, backup_file: PathBuf) -> Result<PathBuf, String> {
        log::info!("Running backup for device: {}", device_name);


        let mut file = match File::create(&backup_file) {
            Ok(file) => {
                log::info!("Backup file created: {:?}", backup_file);
                file
            },
            Err(e) => return Err(format!("Failed to create backup file: {}", e)),
        };

        let now = chrono::Utc::now();
        // if let Err(e) = writeln!(file, "Static Backup File") {
        //     return Err(format!("Failed to write to backup file: {}", e));
        // }
        if let Err(e) = writeln!(file, "Backup for {} created at: {}", device_name, now.to_string()) {
            return Err(format!("Failed to write to backup file: {}", e));
        }

        Ok(backup_file)
    }
}