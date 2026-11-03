//! Orders for message lists ("Sort by…").
//!
//! Lists are sorted on the phone or computer, over the messages loaded
//! (the newest pages of the folder). Ties keep the newest first.

use crate::mail::Header;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortOrder {
    #[default]
    Newest,
    Oldest,
    Largest,
    Smallest,
    SenderAz,
    SenderZa,
    SubjectAz,
}

impl SortOrder {
    pub const ALL: [SortOrder; 7] = [
        SortOrder::Newest,
        SortOrder::Oldest,
        SortOrder::Largest,
        SortOrder::Smallest,
        SortOrder::SenderAz,
        SortOrder::SenderZa,
        SortOrder::SubjectAz,
    ];

    /// The label's translation key.
    pub fn key(self) -> &'static str {
        match self {
            SortOrder::Newest => "sort_newest",
            SortOrder::Oldest => "sort_oldest",
            SortOrder::Largest => "sort_largest",
            SortOrder::Smallest => "sort_smallest",
            SortOrder::SenderAz => "sort_sender_az",
            SortOrder::SenderZa => "sort_sender_za",
            SortOrder::SubjectAz => "sort_subject_az",
        }
    }

    /// Compares two messages for this order.
    pub fn compare(self, a: &Header, b: &Header) -> Ordering {
        let newest = || b.timestamp.cmp(&a.timestamp).then(b.uid.cmp(&a.uid));
        match self {
            SortOrder::Newest => newest(),
            SortOrder::Oldest => newest().reverse(),
            SortOrder::Largest => b.size.cmp(&a.size).then_with(newest),
            SortOrder::Smallest => a.size.cmp(&b.size).then_with(newest),
            SortOrder::SenderAz => text_key(&a.from).cmp(&text_key(&b.from)).then_with(newest),
            SortOrder::SenderZa => text_key(&b.from).cmp(&text_key(&a.from)).then_with(newest),
            SortOrder::SubjectAz => subject_key(&a.subject)
                .cmp(&subject_key(&b.subject))
                .then_with(newest),
        }
    }
}

/// Case- and accent-insensitive text for alphabetical order (quotes and
/// spaces around names are ignored).
fn text_key(s: &str) -> String {
    s.trim()
        .trim_matches(['"', '\''])
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// The subject without reply/forward prefixes ("Re:", "R:", "Fwd:", "I:"…),
/// so a conversation sorts with its original message.
fn subject_key(s: &str) -> String {
    let mut s = s.trim();
    loop {
        let lower = s.to_lowercase();
        let Some(rest) = [
            "re:", "r:", "fwd:", "fw:", "i:", "aw:", "wg:", "tr:", "rv:", "sv:", "vs:",
        ]
        .iter()
        .find(|p| lower.starts_with(*p))
        .map(|p| &s[p.len()..]) else {
            break;
        };
        s = rest.trim_start();
    }
    text_key(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(uid: u32, ts: i64, from: &str, subject: &str, size: u32) -> Header {
        Header {
            uid,
            timestamp: ts,
            from: from.into(),
            subject: subject.into(),
            size,
            ..Default::default()
        }
    }

    fn order(list: &[Header], o: SortOrder) -> Vec<u32> {
        let mut v = list.to_vec();
        v.sort_by(|a, b| o.compare(a, b));
        v.iter().map(|h| h.uid).collect()
    }

    #[test]
    fn orders() {
        let list = [
            h(1, 100, "Zeta Srl", "Re: Fattura", 5_000),
            h(2, 300, "\"anna\"", "Agenda", 900),
            h(3, 200, "Élise", "fattura aprile", 80_000),
        ];
        assert_eq!(order(&list, SortOrder::Newest), [2, 3, 1]);
        assert_eq!(order(&list, SortOrder::Oldest), [1, 3, 2]);
        assert_eq!(order(&list, SortOrder::Largest), [3, 1, 2]);
        assert_eq!(order(&list, SortOrder::Smallest), [2, 1, 3]);
        assert_eq!(
            order(&list, SortOrder::SenderAz),
            [2, 3, 1],
            "quotes and accents ignored"
        );
        assert_eq!(order(&list, SortOrder::SenderZa), [1, 3, 2]);
        // "Re: Fattura" sorts as "fattura"; equal keys: newest first.
        assert_eq!(order(&list, SortOrder::SubjectAz), [2, 1, 3]);
    }
}
