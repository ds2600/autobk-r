// src/main.rs

mod logger;
mod device_types;
mod op_maintenance;
mod op_autobackups;

use std::sync::atomic::{AtomicBool, Ordering};
use std::{thread, time};
use std::collections::HashMap;
use std::path::PathBuf;
use signal_hook::{consts::signal::{SIGINT, SIGTERM}, iterator::Signals};
use config::Config as SettingsConfig;


static RUNNING: AtomicBool = AtomicBool::new(true);
pub struct BaseConfig {
    pub db_user: String,
    pub db_password: String,
    pub db_name: String,
    pub db_host: String,
    pub backup_directory: String,
    pub file_extensions: HashMap<String, String>,
    pub python_scripts: String,
}

fn main() {
    // Settings Declaration
    let settings = SettingsConfig::builder()
    .add_source(config::File::with_name("settings"))
    .add_source(config::Environment::with_prefix("APP"))
    .build()
    .unwrap_or_else(|_| panic!("Failed to build configuration"));

    let file_extensions: std::collections::HashMap<String, String> = settings
    .get("Extensions")
    .expect("Failed to load file extensions");

    // Setup Logging
    let log_file_path = settings.get::<String>("Paths.LogFile")
        .unwrap_or_else(|_| panic!("Failed to get LogFile from settings"));
    logger::init_logger(&log_file_path);

    let sleep_duration = settings.get::<u64>("Tasks.SleepDuration").unwrap_or(60);
    let maintenance_hz = settings.get::<u64>("Tasks.MaintenanceHz").unwrap_or(86400);
    let scheduling_hz = settings.get::<u64>("Tasks.SchedulingHz").unwrap_or(300);
    let autobackups_hz = settings.get::<u64>("Tasks.AutoBackupsHz").unwrap_or(900);

    let db_user = settings.get::<String>("Database.Usr").expect("Database user not set in config");
    let db_password = settings.get::<String>("Database.Pwd").expect("Database password not set in config");
    let db_name = settings.get::<String>("Database.DB").expect("Database name not set in config");
    let db_host = settings.get::<String>("Database.Host").expect("Database host not set in config");
    let backup_directory = settings.get::<String>("Paths.BackupDir").expect("Backup directory not set in config");
    let python_scripts = settings.get::<String>("Paths.PythonScripts").expect("Python scripts directory not set in config");

    std::fs::create_dir_all(&backup_directory).expect("Failed to create backup directory");

    let base_config = BaseConfig {
        db_user,
        db_password,
        db_name,
        db_host,
        backup_directory,
        file_extensions,
        python_scripts,        
    };

    log::info!("Service is running");

    let mut signals = Signals::new(&[SIGINT, SIGTERM]).unwrap();
    let signals_handle = signals.handle();
    let signals_thread = std::thread::spawn(move || {
        for sig in signals.forever() {
            if sig == SIGTERM || sig == SIGINT {
                RUNNING.store(false, Ordering::SeqCst);
                println!("Service is stopping due to signal {}", sig);
                log::info!("Service is stopping due to signal {}", sig);
                break;
            }
        }
    });

    let mut next_maintenance = time::Instant::now();
    let mut next_scheduling = time::Instant::now();
    let mut next_autobackups = time::Instant::now();

    while RUNNING.load(Ordering::SeqCst) {
        if next_maintenance.elapsed().as_secs() >= maintenance_hz {
            next_maintenance = time::Instant::now();
            op_maintenance::run_maintenance(&base_config);
        }

        if next_scheduling.elapsed().as_secs() >= scheduling_hz {
            next_scheduling = time::Instant::now();
            log::info!("Running scheduling tasks");
        }

        if next_autobackups.elapsed().as_secs() >= autobackups_hz {
            next_autobackups = time::Instant::now();
            op_autobackups::run_autobackups(&base_config)
        }

        thread::sleep(time::Duration::from_secs(sleep_duration));
    }

    signals_handle.close();
    signals_thread.join().unwrap();

    log::info!("Service is stopped");
}
