//! Helpers for links found in received HTML mail.

/// Links that may be opened from a message.
pub fn is_openable(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openable_links() {
        assert!(is_openable("https://x.it"));
        assert!(is_openable("mailto:a@b.it"));
        assert!(!is_openable("javascript:alert(1)"));
        assert!(!is_openable("file:///etc/passwd"));
    }
}
