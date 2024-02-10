use mysql::*;
use mysql::prelude::*;
use log::{info, error};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;
use crate::BaseConfig;
use crate::device_types::{get_device, Device};
use chrono::Local;

fn calculate_md5(file_path: &PathBuf) -> String {
    let mut file = File::open(file_path).expect("Unable to open file");
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).expect("Unable to read file");
    format!("{:x}", md5::compute(buffer))
}

fn call_python_script(script_path: &str, args: &[&str]) -> Result<String, String> {
    log::info!("Calling Python module: {}", script_path);
    let output = Command::new("python3.8")
        .arg(script_path)
        .args(args)
        .output()
        .expect("Failed to execute Python script");

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

// Database Operations
fn insert_backup(conn: &mut PooledConn, device_id: u32, backup_file_path: &PathBuf, backup_hash: &str) -> Result<u64, mysql::Error> {
    conn.exec_drop(
        "INSERT INTO Backup (kDevice, tComplete, sFile, backupHash) VALUES (?, NOW(), ?, ?)",
        (device_id, backup_file_path.to_str().expect("Failed to convert path to string"), backup_hash)
    )?;
    Ok(conn.last_insert_id())
}

fn insert_backup_version(conn: &mut PooledConn, k_backup: u64, device_id: u32, version: f64) -> Result<(), mysql::Error> {
    conn.exec_drop(
        "INSERT INTO BackupVersion (kBackup, kDevice, versionNumber) VALUES (?, ?, ?)",
        (k_backup, device_id, version)
    )
}

fn update_schedule(conn: &mut PooledConn, schedule_id: &str, state: &str, comment: &str) -> Result<(), mysql::Error> {
    conn.exec_drop(
        "UPDATE Schedule SET sState = ?, sComment = ? WHERE kSelf = ?",
        (state, comment, schedule_id)
    )
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

        let dt_now = Local::now();
        let formatted_dt = dt_now.format("%Y-%m-%d_%H-%M-%S");

        let filename = format!("{}_{}.{}", device_name, formatted_dt, extension);
        let filename = filename.to_lowercase().replace(" ", "_");
        let backup_file = backup_path.join(filename);

        if let Some(device) = get_device(&device_type) {

            let backup_result = device.backup(&device_name, backup_file);

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
                        let k_backup = insert_backup(&mut conn, device_id, &backup_file, &backup_hash)?;
            
                        // Insert new backup version
                        insert_backup_version(&mut conn, k_backup, device_id, latest_version + 1.0)?;

                        log::info!("Latest version for {} is now: {}", device_name, latest_version + 1.0);
                        
                        // Update Schedule status to 'Complete'
                        update_schedule(&mut conn, &schedule_id, "Complete", "Changes detected, new version")?;

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
            log::info!("Device type not found, looking for Python script: {}", device_type);
            // Fallback to Python script
            let script_path = format!("{}/op_autobk_backup_{}.py", &config.python_scripts, device_type);
            let backup_file_str = backup_file.to_str().expect("Failed to convert path to string");
            let args = vec![&device_ip, backup_file_str];
            match call_python_script(&script_path, &args) {
                Ok(output) => {
                    log::info!("Python script output: {}", output);
                },
                Err(e) => {
                    log::error!("Python script failed: {}", e);
                },
            }
        }
    }



}

