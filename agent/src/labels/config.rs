use anyhow::{bail, ensure, Context, Result};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io::Write, path::PathBuf};

pub const MAX_COPIES: u16 = 20;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct LabelSettings {
    pub trusted_origins: Vec<String>,
    pub document_origins: Vec<String>,
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub printer: String,
    pub width_mm: f64,
    pub height_mm: f64,
    #[serde(default)]
    pub scale: Scale,
    #[serde(default)]
    pub auto_rotate: bool,
    #[serde(default = "one_copy")]
    pub copies: u16,
    #[serde(default)]
    pub paper_kind: Option<i16>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Scale {
    #[default]
    None,
}

fn one_copy() -> u16 {
    1
}

pub fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn exact_origin(value: &str, allow_dev: bool) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    let https = url.scheme() == "https"
        && (allow_dev || (url.port().is_none() && matches!(url.host(), Some(url::Host::Domain(_)))));
    let local_dev = allow_dev && url.scheme() == "http"
        && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    (https || local_dev) && url.origin().ascii_serialization() == value
}

impl LabelSettings {
    pub fn directory() -> Result<PathBuf> {
        Ok(dirs::data_local_dir().context("Local application data is unavailable")?.join("PrintBridge"))
    }

    pub fn load() -> Result<Self> {
        let directory = Self::directory()?;
        fs::create_dir_all(&directory).context("Cannot create local PrintBridge data directory")?;
        let path = directory.join("label-settings.json");
        if !path.exists() {
            let mut file = tempfile::NamedTempFile::new_in(&directory)?;
            serde_json::to_writer_pretty(&mut file, &Self::default())?;
            file.write_all(b"\n")?;
            if let Err(error) = file.persist_noclobber(&path) {
                if error.error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(error.error.into());
                }
            }
        }
        let bytes = fs::read(&path).context("Cannot read label-settings.json")?;
        let settings: Self = serde_json::from_slice(&bytes).context("Invalid label-settings.json")?;
        settings.validate()?;
        Ok(settings)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(self.trusted_origins.len() <= 16 && self.document_origins.len() <= 16,
            "Configure at most 16 entries per origin list");
        ensure!(self.trusted_origins.iter().all(|origin| exact_origin(origin, true)),
            "trustedOrigins must contain exact HTTPS origins or HTTP loopback development origins");
        ensure!(self.document_origins.iter().all(|origin| exact_origin(origin, false)),
            "documentOrigins must contain exact HTTPS DNS origins on port 443");
        ensure!(self.profiles.len() <= 100, "At most 100 profiles are supported");
        for (name, profile) in &self.profiles {
            ensure!(valid_name(name), "Invalid profile name: {name}");
            profile.validate().with_context(|| format!("Invalid profile {name}"))?;
        }
        Ok(())
    }

    pub fn profile(&self, name: &str) -> Result<&Profile> {
        ensure!(valid_name(name), "Invalid profile name");
        self.profiles.get(name).context("Printer profile not found")
    }
}

impl Profile {
    fn validate(&self) -> Result<()> {
        let name = &self.printer;
        ensure!(!name.is_empty() && name.len() <= 512 && name.trim() == name
            && !name.starts_with('-') && !name.starts_with('/') && !name.contains('"')
            && !name.chars().any(char::is_control),
            "Enter an exact Windows printer queue name");
        ensure!((10.0..=1000.0).contains(&self.width_mm)
            && (10.0..=1000.0).contains(&self.height_mm),
            "Width and height must each be between 10 and 1000 mm");
        ensure!(self.paper_kind.map_or(true, |kind| kind > 0), "paperKind must be a positive driver media ID");
        self.copies(None)?;
        Ok(())
    }

    pub fn copies(&self, requested: Option<u16>) -> Result<u16> {
        let copies = requested.unwrap_or(self.copies);
        if !(1..=MAX_COPIES).contains(&copies) {
            bail!("Copies must be between 1 and {MAX_COPIES}");
        }
        Ok(copies)
    }

    pub fn expected_media(&self) -> (f64, f64) {
        if self.auto_rotate && self.width_mm > self.height_mm {
            (self.height_mm, self.width_mm)
        } else {
            (self.width_mm, self.height_mm)
        }
    }
}
