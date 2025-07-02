/* src/setup/check.rs */

use super::config::{Config, RfsConfig};
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use uuid::Uuid;

// Main validation entry point
pub fn validate_server_config(config: &Config) -> Result<(), String> {
    println!("> Performing server configuration checks...");

    if let Some(rfs_list) = &config.rfs {
        if rfs_list.is_empty() {
            return Err(
                "Configuration error: 'rfs' table is present but empty. At least one volume must be defined for server mode."
                    .to_string(),
            );
        }
        validate_rfs_dev_names(rfs_list)?;
        validate_rfs_bind_paths(rfs_list)?;
    } else {
        return Err(
            "Configuration error: The 'rfs' table is missing, which is required for server mode."
                .to_string(),
        );
    }

    println!("+ Configuration checks passed successfully.");
    Ok(())
}

// dev_name must be valid format
fn validate_rfs_dev_names(rfs_list: &[RfsConfig]) -> Result<(), String> {
    let re = Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    for rfs_config in rfs_list {
        if !re.is_match(&rfs_config.dev_name) {
            return Err(format!(
                "Configuration error: dev_name '{}' contains invalid characters. Only a-z, A-Z, 0-9, _, - are allowed.",
                rfs_config.dev_name
            ));
        }
    }
    Ok(())
}

// bind_path must be unique and writable
fn validate_rfs_bind_paths(rfs_list: &[RfsConfig]) -> Result<(), String> {
    let mut seen_paths = HashSet::new();
    for rfs_config in rfs_list {
        // Check for uniqueness
        if !seen_paths.insert(&rfs_config.bind_path) {
            return Err(format!(
                "Configuration error: bind_path '{}' is duplicated. All bind_paths must be unique.",
                rfs_config.bind_path
            ));
        }

        // Check for writability
        let path = Path::new(&rfs_config.bind_path);

        // Ensure path exists and is a directory
        if !path.exists() || !path.is_dir() {
            return Err(format!(
                "Configuration error: bind_path '{}' for dev_name '{}' does not exist or is not a directory.",
                rfs_config.bind_path, rfs_config.dev_name
            ));
        }

        // Attempt to write and delete a temporary file
        let temp_filename = format!("anchr-write-check-{}.tmp", Uuid::new_v4());
        let temp_path = path.join(temp_filename);

        if fs::write(&temp_path, "test").is_err() {
            return Err(format!(
                "Configuration error: No write permission for bind_path '{}' (dev_name: '{}').",
                rfs_config.bind_path, rfs_config.dev_name
            ));
        }

        if fs::remove_file(&temp_path).is_err() {
            return Err(format!(
                "Configuration error: Failed to clean up temporary file in bind_path '{}' (dev_name: '{}'). Check permissions.",
                rfs_config.bind_path, rfs_config.dev_name
            ));
        }
    }
    Ok(())
}