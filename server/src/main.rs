//! Ferrowl push relay.
//!
//! Keeps an IMAP IDLE connection on the inbox of each registered account
//! and wakes the phone through Firebase Cloud Messaging when new mail
//! arrives. It never reads or stores message contents; account settings
//! are stored encrypted with keys that only the phones hold (see `seal`).
//!
//! Configuration (environment):
//! - `FERROWL_PUSH_DB`: SQLite file (default `ferrowl-push.sqlite`)
//! - `FERROWL_PUSH_BIND`: listen address (default `127.0.0.1:8080`, behind
//!   an HTTPS reverse proxy such as Caddy)
//! - `GOOGLE_APPLICATION_CREDENTIALS`: Firebase service account key (JSON).
//!   Without it notifications are only logged (development).

mod api;
mod fcm;
mod seal;
mod store;
mod watch;

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    ferrowl_core::i18n::set(Some("en"));

    let db = std::env::var("FERROWL_PUSH_DB").unwrap_or_else(|_| "ferrowl-push.sqlite".into());
    let bind = std::env::var("FERROWL_PUSH_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let notifier: Arc<dyn fcm::Notifier> = match std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
        Ok(path) => Arc::new(fcm::Fcm::from_file(&path)?),
        Err(_) => {
            eprintln!("GOOGLE_APPLICATION_CREDENTIALS not set: notifications are only logged");
            Arc::new(fcm::LogNotifier)
        }
    };

    let state = api::AppState {
        store: Arc::new(store::Store::open(&db)?),
        watchers: Arc::new(watch::Watchers::default()),
        notifier,
        keys: Arc::default(),
        check_login: api::real_login_check,
    };
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("ferrowl-push listening on {bind}");
    axum::serve(listener, api::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
