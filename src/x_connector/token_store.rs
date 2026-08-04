use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const REFRESH_SKEW: Duration = Duration::seconds(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
}

impl TokenSet {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= now + REFRESH_SKEW
    }
}

#[derive(Debug, Clone)]
pub struct TokenStore {
    path: PathBuf,
}

impl TokenStore {
    pub fn from_root(root: PathBuf) -> Self {
        Self {
            path: root.join("x").join("credentials.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<TokenSet>> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&self.path)
            .with_context(|| format!("Failed to read X token store {}", self.path.display()))?;
        let tokens = serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid X token store {}", self.path.display()))?;
        Ok(Some(tokens))
    }

    pub fn save(&self, tokens: &TokenSet) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("X token store path has no parent directory")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create X token store {}", parent.display()))?;

        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let staging = parent.join(format!(".credentials-{}-{suffix}.tmp", std::process::id()));
        let bytes = serde_json::to_vec_pretty(tokens).context("Failed to encode X tokens")?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&staging)
            .with_context(|| {
                format!(
                    "Failed to create X token staging file {}",
                    staging.display()
                )
            })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| {
                    format!("Failed to restrict X token file {}", staging.display())
                })?;
        }
        let result = (|| -> Result<()> {
            file.write_all(&bytes)
                .context("Failed to write X token staging file")?;
            file.sync_all()
                .context("Failed to sync X token staging file")?;
            fs::rename(&staging, &self.path).with_context(|| {
                format!("Failed to activate X token store {}", self.path.display())
            })?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staging);
        }
        result
    }

    pub fn clear(&self) -> Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error)
                .with_context(|| format!("Failed to remove X token store {}", self.path.display())),
        }
    }
}
