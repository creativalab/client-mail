use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Account {
    /// Name shown as sender, e.g. "Fabio Carli"
    pub display_name: String,
    /// Email address, e.g. "name@domain.com"
    pub email: String,
    pub username: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    /// 465 = implicit TLS, 587 = STARTTLS
    pub smtp_port: u16,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            display_name: String::new(),
            email: String::new(),
            username: String::new(),
            password: String::new(),
            imap_host: String::new(),
            imap_port: 993,
            smtp_host: String::new(),
            smtp_port: 465,
        }
    }
}

impl Account {
    pub fn is_complete(&self) -> bool {
        !self.email.is_empty()
            && !self.username.is_empty()
            && !self.password.is_empty()
            && !self.imap_host.is_empty()
            && !self.smtp_host.is_empty()
    }

    /// Initials for the avatar, e.g. "Fabio Carli" -> "FC".
    pub fn initials(&self) -> String {
        let source = if self.display_name.trim().is_empty() {
            &self.email
        } else {
            &self.display_name
        };
        crate::ui::initials(source)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    /// Language code ("it", "en", …) or "auto" to follow the system.
    pub language: String,
    pub theme: String,
    pub density: Density,
    pub show_preview: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            language: "auto".into(),
            theme: "ruggine".into(),
            density: Density::Comfortable,
            show_preview: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub account: Option<Account>,
    pub ui: Ui,
}

fn config_dir() -> Result<PathBuf> {
    Ok(dirs::config_dir()
        .context("configuration directory not found")?
        .join("ferrowl"))
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

impl Config {
    pub fn load() -> Config {
        let Ok(path) = config_path() else {
            return Config::default();
        };
        // Migration from the first version ("client-mail").
        let legacy = path
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("client-mail").join("config.toml"));
        let text = std::fs::read_to_string(&path)
            .ok()
            .or_else(|| legacy.and_then(|l| std::fs::read_to_string(l).ok()));
        text.and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        std::fs::create_dir_all(config_dir()?)?;
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        // The file contains the password: readable by the owner only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
