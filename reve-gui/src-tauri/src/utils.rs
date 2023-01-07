use std::{env::current_dir, fs::OpenOptions, io::Write, path::PathBuf};

use crate::configuration::{self, ConfigData, LOG_FILE};

pub struct Logger {
    path: PathBuf,
}

impl Logger {
    /// Create a new logger.
    pub fn new() -> Self {
        let path = current_dir()
            .expect("Failed to locate cache directory")
            .join(LOG_FILE);
        Self { path }
    }

    /// Returns the path to the log file.
    pub fn log_file_path(&self) -> String {
        self.path
            .to_str()
            .expect("Failed to convert path to string")
            .to_string()
    }

    /// Write a message to the log file. If the file does not exist, it will be created. If it does exist, it will be overwritten.
    pub fn log(&self, message: &str) {
        let config = match load_configuration() {
            Ok(config) => config,
            Err(_) => ConfigData::default(),
        };
        if !config.get_is_active_application_logs() {
            return;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .expect("Failed to open log file");
        file.write_all(
            format!(
                "{}\n###################################################################\n",
                message
            )
            .as_bytes(),
        )
        .expect("Failed to write to log file");
    }
}

/* /// Replaces the suffix of the given path with `_upscaled-<upscale_factor>x.<extension>`
#[tauri::command]
pub fn replace_file_suffix(path: String, upscale_factor: String) -> String {
    let path = PathBuf::from(path);
    let file_name = path
        .file_name()
        .expect("Failed to get file name")
        .to_str()
        .expect("Failed to convert file name to string");
    let file_name = file_name.replace(
        &format!(".{}", path.extension().expect("Failed to get file extension").to_str().expect("Failed to convert file extension to string")),
        &format!("_upscaled-{}x.{}", upscale_factor, path.extension().expect("Failed to get file extension").to_str().expect("Failed to convert file extension to string")),
    );
    write_log(&format!(
        "Final path: {}",
        path.with_file_name(&file_name)
            .to_str()
            .expect("Failed to convert path to string")
    ));

    path.with_file_name(file_name)
        .to_str()
        .expect("Failed to convert path to string")
        .to_string()
} */

/// Loads the configuration file and creates a default one if it does not exist or if it is invalid.
#[tauri::command]
pub fn load_configuration() -> Result<ConfigData, String> {
    let mut config = configuration::Config::new(None);
    match config.load() {
        Ok(config) => Ok(config),
        Err(_) => Ok(config
            .create_default_config_file()
            .map_err(|err| err.to_string())?),
    }
}

/// Validates the ConfigData values and writes the configuration file.
#[tauri::command]
pub fn write_configuration(config: ConfigData) -> Result<(), String> {
    let config = configuration::Config::new(Some(config));
    config.save().map_err(|err| err.to_string())
}

/// Write to the log file.
#[tauri::command]
pub fn write_log(message: &str) {
    let logger = Logger::new();
    logger.log(message);
}

#[tauri::command]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[tauri::command]
pub fn check_if_file_exists(path: String) -> bool {
    PathBuf::from(path).exists()
}
