/* src/rfs/verify.rs */

use crate::rfs::{upload, worker, UploadMetadata};
use crate::setup::config::Config;
use sha2::{Digest, Sha256};
use tokio::fs as tokio_fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// [SERVER-SIDE] Assembles all chunks, verifies the final hash, and cleans up.
pub async fn assemble_and_verify(metadata: &UploadMetadata, cfg: &Config) -> bool {
    let final_path = match upload::resolve_and_validate_path(&metadata.target_dir, cfg) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("! Finalize Error: {}", e);
            return false;
        }
    };
    let final_file_path = final_path.join(&metadata.file_name);
    let tmp_dir_path = final_path.join(format!("{}.tmp", metadata.file_name));
    let total_chunks = (metadata.file_size as f64 / worker::CHUNK_SIZE as f64).ceil() as u64;

    // Verify all chunks exist
    for i in 0..total_chunks {
        let chunk_path = tmp_dir_path.join(format!("chunk_{}", i));
        if !tokio_fs::try_exists(&chunk_path).await.unwrap_or(false) {
            eprintln!("! Finalize Error: Missing chunk #{}", i);
            return false;
        }
    }
    println!("   - All {} chunks verified.", total_chunks);

    // Assemble file
    let mut final_file = match tokio_fs::File::create(&final_file_path).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("! Finalize Error: Could not create final file: {}", e);
            return false;
        }
    };

    for i in 0..total_chunks {
        let chunk_path = tmp_dir_path.join(format!("chunk_{}", i));
        match tokio_fs::read(&chunk_path).await {
            Ok(data) => {
                if final_file.write_all(&data).await.is_err() {
                    eprintln!("! Finalize Error: Failed to write chunk #{}", i);
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
    println!("   - File assembled successfully.");
    final_file.sync_all().await.ok();

    // Verify final hash efficiently
    let mut final_file_reader = match tokio_fs::File::open(&final_file_path).await {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut hasher = Sha256::new();
    let mut buf = [0; 8192];
    loop {
        match final_file_reader.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => hasher.update(&buf[..n]),
            Err(_) => return false,
        }
    }
    let final_hash = hex::encode(hasher.finalize());

    if final_hash != metadata.file_hash {
        eprintln!("! Finalize Error: Final file hash mismatch!");
        eprintln!("   - Expected: {}", metadata.file_hash);
        eprintln!("   - Got:      {}", final_hash);
        return false;
    }
    println!("   - Final hash verified successfully.");

    // Cleanup
    let lock_file_path = final_path.join(format!("{}.lock", metadata.file_name));
    let hash_file_path = final_path.join(format!("{}.hash", metadata.file_name));
    tokio_fs::remove_file(lock_file_path).await.ok();
    tokio_fs::remove_file(hash_file_path).await.ok();
    tokio_fs::remove_dir_all(tmp_dir_path).await.ok();
    println!("   - Cleanup complete.");

    true
}