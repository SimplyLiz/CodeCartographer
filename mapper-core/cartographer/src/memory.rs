use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const MEMORY_FILE: &str = ".cartographer_memory.json";

impl Memory {
    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(MEMORY_FILE);
        let temp_path = path.with_extension("json.tmp");

        // Serialize to JSON
        let data = serde_json::to_string_pretty(self)?;

        // Write to temporary file first (atomic write pattern)
        {
            let mut temp_file = fs::File::create(&temp_path)
                .map_err(|e| anyhow::anyhow!("Failed to create temp file: {}", e))?;

            temp_file.write_all(data.as_bytes())
                .map_err(|e| anyhow::anyhow!("Failed to write temp file: {}", e))?;

            temp_file.flush()
                .map_err(|e| anyhow::anyhow!("Failed to flush temp file: {}", e))?;

            // Sync to disk to ensure durability
            temp_file.sync_all()
                .map_err(|e| anyhow::anyhow!("Failed to sync temp file: {}", e))?;
        }

        // Atomic rename (fails if destination exists on Windows, but overwrites on Unix)
        fs::rename(&temp_path, &path)
            .map_err(|e| anyhow::anyhow!("Failed to rename temp file to {}: {}", path.display(), e))?;

        // Restrict file permissions to owner only (Unix/Linux/macOS)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o600); // rw-------
            fs::set_permissions(&path, perms)
                .map_err(|e| anyhow::anyhow!("Failed to set permissions: {}", e))?;
        }

        // Windows: no equivalent to chmod; file is created with default permissions
        // (usually readable by owner and system only)

        Ok(())
    }

    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(MEMORY_FILE);
        if path.exists() {
            // Verify file permissions before loading (Unix only)
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = fs::metadata(&path)?;
                let mode = meta.permissions().mode();
                // Warn if world-readable (other bits set)
                if (mode & 0o077) != 0 {
                    eprintln!(
                        "[CARTOGRAPHER] WARNING: {} is world-readable (mode={:o}). \
                         Consider running: chmod 600 {}",
                        path.display(),
                        mode,
                        path.display()
                    );
                }
            }

            let data = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&data)?)
        } else {
            Ok(Self::default())
        }
    }
}
