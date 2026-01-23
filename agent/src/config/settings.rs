use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use anyhow::Result;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub server: ServerSettings,
    #[serde(default)]
    pub security: SecuritySettings,
    #[serde(default)]
    pub printing: PrintingSettings,
    #[serde(default)]
    pub logging: LoggingSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: ServerSettings::default(),
            security: SecuritySettings::default(),
            printing: PrintingSettings::default(),
            logging: LoggingSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    9718
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecuritySettings {
    #[serde(default = "default_allowed_origins")]
    pub allowed_origins: Vec<String>,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default = "default_true")]
    pub https_only: bool,
}

fn default_allowed_origins() -> Vec<String> {
    vec!["*".to_string()]
}

fn default_true() -> bool {
    true
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            allowed_origins: default_allowed_origins(),
            auth_token: None,
            https_only: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintingSettings {
    #[serde(default)]
    pub temp_directory: Option<PathBuf>,
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval: u64,
    #[serde(default = "default_max_cache_age")]
    pub max_cache_age: u64,
    #[serde(default)]
    pub default_printer: Option<String>,
}

fn default_cleanup_interval() -> u64 {
    3600 // 1 hour
}

fn default_max_cache_age() -> u64 {
    86400 // 24 hours
}

impl Default for PrintingSettings {
    fn default() -> Self {
        Self {
            temp_directory: None,
            cleanup_interval: default_cleanup_interval(),
            max_cache_age: default_max_cache_age(),
            default_printer: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingSettings {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_max_files")]
    pub max_files: u32,
    #[serde(default = "default_max_size")]
    pub max_size: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_max_files() -> u32 {
    5
}

fn default_max_size() -> String {
    "10MB".to_string()
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            level: default_log_level(),
            max_files: default_max_files(),
            max_size: default_max_size(),
        }
    }
}

impl Settings {
    /// Get the config directory path
    pub fn config_dir() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine config directory"))?
            .join("PrintBridge");

        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }

        Ok(dir)
    }

    /// Get the config file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    /// Get the temp directory for downloads/cache
    pub fn temp_dir(&self) -> Result<PathBuf> {
        let dir = if let Some(ref custom) = self.printing.temp_directory {
            custom.clone()
        } else {
            dirs::data_local_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?
                .join("PrintBridge")
                .join("cache")
        };

        if !dir.exists() {
            fs::create_dir_all(&dir)?;
        }

        Ok(dir)
    }

    /// Load settings from disk
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let settings: Settings = serde_json::from_str(&content)?;
            Ok(settings)
        } else {
            // Create default settings file
            let settings = Settings::default();
            settings.save()?;
            Ok(settings)
        }
    }

    /// Save settings to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Check if an origin is allowed
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        for pattern in &self.security.allowed_origins {
            if pattern == "*" {
                return true;
            }
            if pattern == origin {
                return true;
            }
            // Simple wildcard matching for subdomains
            if pattern.starts_with("*.") {
                let suffix = &pattern[1..]; // e.g., ".example.com"
                if origin.ends_with(suffix) {
                    return true;
                }
            }
            // Handle https://*.example.com pattern
            if pattern.contains("*") {
                let parts: Vec<&str> = pattern.split('*').collect();
                if parts.len() == 2 {
                    let prefix = parts[0];
                    let suffix = parts[1];
                    if origin.starts_with(prefix) && origin.ends_with(suffix) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Check if auth token matches
    pub fn check_auth(&self, token: Option<&str>) -> bool {
        match &self.security.auth_token {
            Some(expected) => token == Some(expected.as_str()),
            None => true, // No auth required
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_origin_matching() {
        let mut settings = Settings::default();

        // Wildcard allows all
        assert!(settings.is_origin_allowed("https://example.com"));
        assert!(settings.is_origin_allowed("http://localhost:3000"));

        // Specific origin
        settings.security.allowed_origins = vec!["https://example.com".to_string()];
        assert!(settings.is_origin_allowed("https://example.com"));
        assert!(!settings.is_origin_allowed("https://other.com"));

        // Subdomain wildcard
        settings.security.allowed_origins = vec!["https://*.example.com".to_string()];
        assert!(settings.is_origin_allowed("https://app.example.com"));
        assert!(settings.is_origin_allowed("https://api.example.com"));
        assert!(!settings.is_origin_allowed("https://example.com"));
    }

    #[test]
    fn test_auth_check() {
        let mut settings = Settings::default();

        // No auth required by default
        assert!(settings.check_auth(None));
        assert!(settings.check_auth(Some("anything")));

        // With auth token
        settings.security.auth_token = Some("secret".to_string());
        assert!(!settings.check_auth(None));
        assert!(!settings.check_auth(Some("wrong")));
        assert!(settings.check_auth(Some("secret")));
    }
}
