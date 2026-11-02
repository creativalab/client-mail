//! Wakes phones through Firebase Cloud Messaging (HTTP v1 API).
//!
//! The message is *data-only* and carries just the account id: no sender,
//! no subject. The app then fetches the mail itself, straight from the mail
//! server.

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Result of a notification.
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    /// The phone uninstalled the app or the token expired: forget it.
    Unregistered,
}

pub trait Notifier: Send + Sync {
    fn notify(&self, fcm_token: &str, account_id: &str) -> Result<Delivery>;
}

/// For development without Firebase: only logs.
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, _fcm_token: &str, account_id: &str) -> Result<Delivery> {
        eprintln!("push (log only): new mail for account {account_id}");
        Ok(Delivery::Sent)
    }
}

/// Fields used from a Firebase service account JSON key.
#[derive(Deserialize)]
struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    token_uri: String,
}

pub struct Fcm {
    project_id: String,
    client_email: String,
    key: jsonwebtoken::EncodingKey,
    token_uri: String,
    /// OAuth access token and when it expires.
    access: Mutex<Option<(String, Instant)>>,
}

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";

impl Fcm {
    /// From the service account key file downloaded from the Firebase console.
    pub fn from_file(path: &str) -> Result<Fcm> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let sa: ServiceAccount = serde_json::from_str(&text).context("not a service account key")?;
        Ok(Fcm {
            key: jsonwebtoken::EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
                .context("service account private key")?,
            project_id: sa.project_id,
            client_email: sa.client_email,
            token_uri: sa.token_uri,
            access: Mutex::new(None),
        })
    }

    /// OAuth access token for FCM, cached until shortly before it expires.
    fn access_token(&self) -> Result<String> {
        let mut cached = self.access.lock().unwrap_or_else(|p| p.into_inner());
        if let Some((token, until)) = cached.as_ref()
            && Instant::now() < *until
        {
            return Ok(token.clone());
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let claims = serde_json::json!({
            "iss": self.client_email,
            "scope": SCOPE,
            "aud": self.token_uri,
            "iat": now,
            "exp": now + 3600,
        });
        let jwt = jsonwebtoken::encode(
            &jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256),
            &claims,
            &self.key,
        )?;
        #[derive(Deserialize)]
        struct TokenReply {
            access_token: String,
            expires_in: u64,
        }
        let reply: TokenReply = ureq::post(&self.token_uri)
            .send_form([
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", jwt.as_str()),
            ])?
            .body_mut()
            .read_json()?;
        let until = Instant::now() + Duration::from_secs(reply.expires_in.saturating_sub(300));
        *cached = Some((reply.access_token.clone(), until));
        Ok(reply.access_token)
    }
}

impl Notifier for Fcm {
    fn notify(&self, fcm_token: &str, account_id: &str) -> Result<Delivery> {
        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            self.project_id
        );
        let body = serde_json::json!({
            "message": {
                "token": fcm_token,
                "data": { "type": "new_mail", "account": account_id },
                // High priority wakes the app even in Doze; data-only so the
                // app decides what to show.
                "android": { "priority": "HIGH", "ttl": "3600s" },
            }
        });
        let result = ureq::post(&url)
            .header("Authorization", &format!("Bearer {}", self.access_token()?))
            .config()
            .http_status_as_error(false)
            .build()
            .send_json(&body)?;
        match result.status().as_u16() {
            200 => Ok(Delivery::Sent),
            // UNREGISTERED / INVALID_ARGUMENT for a dead token.
            404 => Ok(Delivery::Unregistered),
            code => Err(anyhow!("FCM answered {code}")),
        }
    }
}
