use crate::device_types::Device;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use chrono::Local;


pub struct FakeDevice {

}

impl FakeDevice {
    pub fn new() -> Self {
        FakeDevice{}
    }
}

impl Device for FakeDevice {
    fn backup(&self, device_name: &str, backup_directory: &str, file_extension: &str) -> Result<(), String> {
        log::info!("Running backup for device: {}", device_name);
        let dt_now = Local::now();
        let formatted_dt = dt_now.format("%Y-%m-%d_%H-%M-%S");

        let backup_path = PathBuf::from(backup_directory);
        let filename = format!("{}_{}.{}", device_name, formatted_dt, file_extension);
        let filename = filename.to_lowercase().replace(" ", "_");
        let backup_file = backup_path.join(filename);

        let mut file = match File::create(&backup_file) {
            Ok(file) => {
                log::info!("Backup file created: {:?}", backup_file);
                file
            },
            Err(e) => return Err(format!("Failed to create backup file: {}", e)),
        };

        let now = chrono::Utc::now();
        if let Err(e) = writeln!(file, "Backup created at: {}", now.to_string()) {
            return Err(format!("Failed to write to backup file: {}", e));
        }

        Ok(())
    }
}