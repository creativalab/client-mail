//! Known mail providers: picking one in the new-account form fills in the
//! servers and ports, so the user only types address and password.
//! Kept in sync with the setup guides on ferrowl.com.

pub struct Provider {
    pub name: &'static str,
    /// Slug of the setup guide on ferrowl.com (`/guides/<slug>/`), if any.
    pub guide: Option<&'static str>,
    pub imap: (&'static str, u16),
    /// Port 465 = implicit TLS, 587 = STARTTLS.
    pub smtp: (&'static str, u16),
    /// Address domains that identify this provider.
    pub domains: &'static [&'static str],
    pub login: Login,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Login {
    /// The provider requires an app-specific password.
    AppPassword,
    /// Password login is turned off: needs "Sign in with Microsoft".
    MicrosoftSignIn,
}

/// In the order shown in the form; "Other" (manual setup) comes after.
pub const PROVIDERS: &[Provider] = &[
    Provider {
        name: "iCloud",
        guide: Some("icloud"),
        imap: ("imap.mail.me.com", 993),
        smtp: ("smtp.mail.me.com", 587),
        domains: &["icloud.com", "me.com", "mac.com"],
        login: Login::AppPassword,
    },
    Provider {
        name: "Microsoft Exchange",
        guide: None,
        imap: ("outlook.office365.com", 993),
        smtp: ("smtp.office365.com", 587),
        // Work and school accounts use their own domains.
        domains: &[],
        login: Login::MicrosoftSignIn,
    },
    Provider {
        name: "Google",
        guide: Some("gmail"),
        imap: ("imap.gmail.com", 993),
        smtp: ("smtp.gmail.com", 465),
        domains: &["gmail.com", "googlemail.com"],
        login: Login::AppPassword,
    },
    Provider {
        name: "Yahoo",
        guide: Some("yahoo"),
        imap: ("imap.mail.yahoo.com", 993),
        smtp: ("smtp.mail.yahoo.com", 465),
        domains: &[
            "yahoo.com",
            "yahoo.it",
            "yahoo.fr",
            "yahoo.de",
            "yahoo.es",
            "yahoo.co.uk",
            "ymail.com",
        ],
        login: Login::AppPassword,
    },
    Provider {
        name: "AOL",
        guide: None,
        imap: ("imap.aol.com", 993),
        smtp: ("smtp.aol.com", 465),
        domains: &["aol.com", "aol.it", "aim.com"],
        login: Login::AppPassword,
    },
    Provider {
        name: "Outlook.com",
        guide: None,
        imap: ("outlook.office365.com", 993),
        smtp: ("smtp-mail.outlook.com", 587),
        domains: &[
            "outlook.com",
            "hotmail.com",
            "live.com",
            "msn.com",
            "outlook.it",
            "hotmail.it",
            "live.it",
            "outlook.fr",
            "hotmail.fr",
            "live.fr",
            "outlook.de",
            "hotmail.de",
            "outlook.es",
            "hotmail.es",
            "hotmail.co.uk",
            "live.co.uk",
        ],
        login: Login::MicrosoftSignIn,
    },
];

/// The provider an address belongs to, from its domain.
pub fn for_email(email: &str) -> Option<usize> {
    let domain = email.trim().rsplit_once('@')?.1.to_ascii_lowercase();
    PROVIDERS
        .iter()
        .position(|p| p.domains.contains(&domain.as_str()))
}

/// Link to the setup guide in the interface language.
pub fn guide_url(slug: &str, lang: &str) -> String {
    if lang == "en" {
        format!("https://ferrowl.com/guides/{slug}/")
    } else {
        format!("https://ferrowl.com/{lang}/guides/{slug}/")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_provider_from_address() {
        assert_eq!(
            for_email("Someone@GMAIL.com").map(|i| PROVIDERS[i].name),
            Some("Google")
        );
        assert_eq!(
            for_email("a@hotmail.it").map(|i| PROVIDERS[i].name),
            Some("Outlook.com")
        );
        assert_eq!(for_email("a@example.org"), None);
        assert_eq!(for_email("no-at-sign"), None);
    }

    #[test]
    fn guide_links() {
        assert_eq!(guide_url("gmail", "en"), "https://ferrowl.com/guides/gmail/");
        assert_eq!(guide_url("gmail", "it"), "https://ferrowl.com/it/guides/gmail/");
    }
}
