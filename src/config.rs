use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Nome mostrato come mittente, es. "Fabio Carli"
    pub display_name: String,
    /// Indirizzo email, es. "nome@dominio.it"
    pub email: String,
    pub username: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    /// 465 = TLS implicito, 587 = STARTTLS
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub account: Option<Account>,
}

fn config_path() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .context("cartella di configurazione non trovata")?
        .join("client-mail");
    Ok(dir.join("config.toml"))
}

impl Config {
    pub fn load() -> Config {
        let Ok(path) = config_path() else {
            return Config::default();
        };
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        // Il file contiene la password: leggibile solo dal proprietario.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}
