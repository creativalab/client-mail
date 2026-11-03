//! Errors in words the user understands.
//!
//! Mail servers, TLS and the network report problems in technical terms
//! ("[AUTHENTICATIONFAILED] Invalid credentials", "failed to lookup
//! address information", "invalid peer certificate"…). [`humanize`] turns
//! the common ones into a short sentence in the user's language that says
//! what happened and what to do. Unknown errors keep their text after a
//! general sentence, so they can still be reported.

use crate::i18n::{t, tf};

/// Kinds of problem recognised in an error message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    WrongPassword,
    NoServer,
    Offline,
    Timeout,
    Refused,
    Certificate,
    SignedOut,
    Cancelled,
    MailboxFull,
    Rejected,
    Unknown,
}

/// What kind of problem `raw` describes.
pub fn kind(raw: &str) -> Kind {
    let m = raw.to_lowercase();
    let any = |keys: &[&str]| keys.iter().any(|k| m.contains(k));
    if any(&[
        "authenticationfailed",
        "invalid credentials",
        "authentication failed",
        "authentication unsuccessful",
        "username and password not accepted",
        "535 5.7.8",
        "login failed",
        "incorrect password",
        "invalid login",
        "[auth]",
    ]) && !any(&[
        "smtpclientauthentication is disabled",
        "authenticated but not connected",
    ]) {
        return Kind::WrongPassword;
    }
    if any(&[
        "invalid_grant",
        "err_oauth_signed_out",
        "token has been expired or revoked",
    ]) {
        return Kind::SignedOut;
    }
    if any(&["access_denied", "consent_required", "user cancel"]) {
        return Kind::Cancelled;
    }
    if any(&[
        "failed to lookup address",
        "name or service not known",
        "no address associated",
        "nodename nor servname",
        "temporary failure in name resolution",
        "no such host",
        "dns error",
    ]) {
        return Kind::NoServer;
    }
    if any(&[
        "network is unreachable",
        "no route to host",
        "network unreachable",
    ]) {
        return Kind::Offline;
    }
    if any(&["timed out", "timeout", "deadline has elapsed"]) {
        return Kind::Timeout;
    }
    if any(&["connection refused", "connection reset"]) {
        return Kind::Refused;
    }
    if any(&["certificate", "unknownissuer", "invalid peer", "handshake", "tls"]) {
        return Kind::Certificate;
    }
    if any(&[
        "over quota",
        "overquota",
        "quota exceeded",
        "mailbox full",
        "552 5.2.2",
    ]) {
        return Kind::MailboxFull;
    }
    if any(&[
        "550 5.1.1",
        "user unknown",
        "recipient address rejected",
        "no such user",
        "mailbox unavailable",
    ]) {
        return Kind::Rejected;
    }
    Kind::Unknown
}

/// `raw` as a sentence for the user. Ferrowl's own messages (already
/// written for the user) pass through unchanged.
pub fn humanize(raw: &str) -> String {
    let raw = raw.trim();
    if crate::i18n::is_own_error(raw) {
        return raw.to_string();
    }
    let key = match kind(raw) {
        Kind::WrongPassword => "err_h_password",
        Kind::NoServer => "err_h_no_server",
        Kind::Offline => "err_h_offline",
        Kind::Timeout => "err_h_timeout",
        Kind::Refused => "err_h_refused",
        Kind::Certificate => "err_h_certificate",
        Kind::SignedOut => "err_oauth_signed_out",
        Kind::Cancelled => "err_h_cancelled",
        Kind::MailboxFull => "err_h_quota",
        Kind::Rejected => "err_h_rejected",
        Kind::Unknown => {
            // Keep the first line of the technical text for support.
            let detail = raw.lines().next().unwrap_or("").trim();
            return if detail.is_empty() {
                crate::i18n::generic_error()
            } else {
                tf("err_h_generic_detail", &[("detail", detail)])
            };
        }
    };
    t(key).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_common_errors() {
        assert_eq!(
            kind("IMAP login failed: [AUTHENTICATIONFAILED] Invalid credentials (Failure)"),
            Kind::WrongPassword
        );
        assert_eq!(
            kind("535 5.7.8 Username and Password not accepted"),
            Kind::WrongPassword
        );
        assert_eq!(
            kind("failed to lookup address information: Name or service not known"),
            Kind::NoServer
        );
        assert_eq!(kind("Connection timed out (os error 110)"), Kind::Timeout);
        assert_eq!(kind("Connection refused (os error 111)"), Kind::Refused);
        assert_eq!(kind("invalid peer certificate: UnknownIssuer"), Kind::Certificate);
        assert_eq!(kind("invalid_grant: AADSTS70000"), Kind::SignedOut);
        assert_eq!(kind("access_denied: the user cancelled"), Kind::Cancelled);
        assert_eq!(kind("552 5.2.2 Mailbox full"), Kind::MailboxFull);
        assert_eq!(kind("550 5.1.1 User unknown"), Kind::Rejected);
        assert_eq!(kind("something odd"), Kind::Unknown);
        // Outlook with IMAP off is explained elsewhere, not as a wrong password.
        assert_ne!(
            kind("SmtpClientAuthentication is disabled for the Mailbox"),
            Kind::WrongPassword
        );
    }
}
