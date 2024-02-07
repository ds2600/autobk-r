use mysql::*;
use mysql::prelude::*;
use log::{info, error};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use crate::BaseConfig;
use crate::device_types::{get_device, Device};

fn calculate_md5(file_path: &PathBuf) -> String {
    let mut file = File::open(file_path).expect("Unable to open file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Unable to read file");
    format!("{:x}", md5::compute(buffer))
}

pub fn run_autobackups(config: &BaseConfig) {
    log::info!("Running autobackups");

    // Database connection options
    let opts = OptsBuilder::new()
    .ip_or_hostname(Some(config.db_host.clone()))
    .db_name(Some(config.db_name.clone()))
    .user(Some(config.db_user.clone()))
    .pass(Some(config.db_password.clone()));

    // Connect to the database
    let pool = match Pool::new(opts) {
        Ok(pool) => {
            log::info!("Database pool created");
            pool
        },
        Err(e) => {
            log::error!("Failed to create database pool: {}", e);
            return;
        }
    };
    let mut conn = match pool.get_conn() {
        Ok(conn) => {
            log::info!("Connected to database");
            conn
        },
        Err(e) => {
            log::error!("Failed to get database connection: {}", e);
            return;
        }
    };
    
    // SQL query to fetch devices ready for backup
    let s_sql_get_ready = "
        SELECT dev.kSelf, dev.sName, dev.sIP, dev.sType, dev.iAutoWeeks, s.kSelf kSchedule, s.sState, s.iAttempt, s.sComment
        FROM Schedule s
        LEFT JOIN Device dev ON s.kDevice = dev.kSelf
        WHERE s.sState IN ('Auto','Manual') AND s.tTime <= CURRENT_TIMESTAMP";

    // Fetch devices and iterate over them
    let now = chrono::Utc::now().naive_utc();
    let devices: Vec<(u32, String, String, String, i32, String, String, i32, Option<String>)> = 
        conn.query_map(s_sql_get_ready, |(k_self, s_name, s_ip, s_type, i_auto_weeks, k_schedule, s_state, i_attempt, s_comment)| {
            (k_self, s_name, s_ip, s_type, i_auto_weeks, k_schedule, s_state, i_attempt, s_comment)
        }).unwrap();

    for (device_id, device_name, device_ip, device_type, auto_weeks, schedule_id, state, attempt, comment) in devices {
        if let Some(device) = get_device(&device_type) {
            let extension = match config.file_extensions.get(&device_type.to_lowercase()) {
                Some(extension) => extension,
                None => {
                    log::error!("No file extension for device type: {}", device_type);
                    return; 
                }
            };

            
            let device_id_str = format!("{:0>10}/", device_id);
            let backup_path = PathBuf::from(format!("{}/{}", &config.backup_directory, device_id_str));
            std::fs::create_dir_all(&backup_path).expect("Failed to create backup directory");
            let backup_result = device.backup(&device_id, &device_name, &device_ip, &backup_path, extension);

            match backup_result {
                Ok(backup_file_path) => {
                    let backup_hash = calculate_md5(&backup_file_path);
                    let backup_hash_for_db = backup_hash.clone();

                    // Retrieve the latest backup hash from the database
                    let latest_backup_hash: Option<String> = conn.query_first(format!(
                        "SELECT backupHash FROM Backup WHERE kDevice = {} ORDER BY tComplete DESC LIMIT 1",
                        device_id
                    )).unwrap_or(None);

                    let latest_version_query = format!(
                        "SELECT versionNumber FROM BackupVersion WHERE kDevice = {} ORDER BY createdAt DESC LIMIT 1",
                        device_id
                    );
                    let latest_version: f64 = conn.query_first(latest_version_query)
                        .unwrap_or(None)
                        .unwrap_or(0.0);
            
                    if latest_backup_hash == Some(backup_hash) {
                        // Hashes match
                        log::info!("Backup for {} is the same as the latest backup", device_name);
                        log::info!("Latest version for {} is: {}", device_name, latest_version);
                        log::info!("Removing file: {:?}", backup_file_path);
                        std::fs::remove_file(backup_file_path).expect("Failed to remove backup file");
                        conn.exec_drop(
                            "UPDATE Schedule SET sState = 'Complete', sComment = ? WHERE kSelf = ?",
                            ("No changes detected", schedule_id,)
                        ).expect("Failed to update schedule status");
                    } else {
                        // Hashes don't match or no previous backup, save the file and update the database
                        log::info!("Detected changes on {}, storing backup and updating version", device_name);
                        // Insert new backup record
                        conn.exec_drop(
                            "INSERT INTO Backup (kDevice, tComplete, sFile, backupHash) VALUES (?, NOW(), ?, ?)",
                            (device_id, backup_file_path.to_str().expect("Failed to convert path to string"), &backup_hash_for_db)
                        ).expect("Failed to insert into Backup");
            
                        // Get the last insert ID (kBackup)
                        let k_backup: u64 = conn.last_insert_id();
            
                        // Insert new backup version
                        let changed_version = latest_version + 1.0;
                        conn.exec_drop(
                            "INSERT INTO BackupVersion (kBackup, kDevice, versionNumber) VALUES (?, ?, ?)",
                            (k_backup, device_id, changed_version)
                        ).expect("Failed to insert into BackupVersion");
                        log::info!("Latest version for {} is now: {}", device_name, changed_version);
                        
                        // Update Schedule status to 'Complete'
                        let changed_comment = format!("Changes detected, new version: {}", changed_version);
                        conn.exec_drop(
                            "UPDATE Schedule SET sState = 'Complete', sComment = ? WHERE kSelf = ?",
                            (&changed_comment, schedule_id)
                        ).expect("Failed to update Schedule");

                        log::info!("Backup for {} completed", device_name);
                    }
                },
                Err(e) => {
                    error!("Backup failed for device {}: {}", device_name, e);
                    // Update Schedule status to 'Failed' or increment attempt count
                    conn.exec_drop(
                        "UPDATE Schedule SET sState = 'Fail', iAttempt = iAttempt + 1 WHERE kSelf = ?",
                        (schedule_id,)
                    ).expect("Failed to update Schedule on backup failure");
                },
            }            
        } else {
            error!("Unknown device type for {}: {}", device_name, device_type);
        }
    }



}

