use mysql::*;
use mysql::prelude::*;
use log::{info, error};
use crate::BaseConfig;
use crate::device_types::{get_device, Device};


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
    let devices: Vec<(i32, String, String, String, i32, String, String, i32, Option<String>)> = 
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
            let backup_result = device.backup(&device_name, &config.backup_directory, extension);

            match backup_result {
                Ok(_) => {
                    // Update database for completed backup
                },
                Err(e) => {
                    error!("Backup failed for device {}: {}", device_name, e);
                    // Update database for failed backup:
                },
            }
        } else {
            error!("Unknown device type for {}: {}", device_name, device_type);
        }
    }



}

