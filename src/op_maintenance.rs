use mysql::*;
use mysql::prelude::*;
use std::fs;
use log::{info, error};
use std::path::{ Path, PathBuf };
use crate::BaseConfig;

pub fn run_maintenance(config: &BaseConfig) {
    log::info!("Running maintenance tasks");

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

    // SQL Queries
    let s_sql_get_expired = r"SELECT bk.kSelf, bk.kDevice, bk.sFile, dev.sName
                               FROM Backup bk
                               LEFT JOIN Device dev ON bk.kDevice=dev.kSelf
                               WHERE bk.tExpires <= CURRENT_TIMESTAMP";
    let s_sql_del_backup = "DELETE FROM Backup WHERE kSelf = :k_self";

    // Execute query to get expired backups
    let expired_backups: Vec<(i32, i32, String, String)> = match conn.query(s_sql_get_expired) {
        Ok(backups) => backups,
        Err(e) => {
            error!("Failed to query expired backups: {}", e);
            return;
        }
    };

    // Delete expired backups
    for (k_self, k_device, s_file, _) in expired_backups {
        let file = s_file.clone();
        // Delete backup file
        let backup_location = s_file.clone();
        if Path::new(&backup_location).exists() {
            log::info!("Deleting expired backup: {}", file);
            if let Err(e) = fs::remove_file(file) {
                error!("Failed to delete backup file: {}", e);
                continue;
            }
        } else {
            log::error!("Backup file not found: {}", file);
        }

        // Delete corresponding row from BackupVersion table
        let s_sql_del_backup_version = "DELETE FROM BackupVersion WHERE kBackup = :k_self";
        if let Err(e) = conn.exec_drop(s_sql_del_backup_version, params! {
            "k_self" => k_self
        }) {
            error!("Failed to delete backup version from database: {}", e);
            continue;
        }

        // Delete backup from database
        if let Err(e) = conn.exec_drop(s_sql_del_backup, params! {
            "k_self" => k_self
        }) {
            error!("Failed to delete backup from database: {}", e);
            continue;
        }
        log::info!("Deleted expired backup: {}", backup_location);
    }
    log::info!("Maintenance tasks completed");
}