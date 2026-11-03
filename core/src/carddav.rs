//! Contacts from the account's CardDAV server (see `dav` for discovery,
//! security and sync).

use crate::config::Account;
use crate::dav::{self, Client, Kind};
use crate::vcard;
use anyhow::Result;

pub use dav::{AuthRefused, Collection as Book};

/// A downloaded card.
#[derive(Debug, Clone)]
pub struct Card {
    /// Absolute URL of the card (its identity).
    pub url: String,
    pub etag: String,
    pub vcard: String,
    pub contact: vcard::Contact,
}

/// What a sync changed, for the cache to store.
#[derive(Debug, Default)]
pub struct Changes {
    pub books: Vec<Book>,
    /// New or changed cards, with their address book's URL.
    pub cards: Vec<(String, Card)>,
    /// URLs of cards no longer on the server.
    pub gone: Vec<String>,
}

/// Compares the server with `known` (card URL -> ETag, from the cache) and
/// downloads what changed. Cards that are not valid vCards are skipped.
pub fn sync(account: &Account, known: &[(String, String)]) -> Result<Changes> {
    let changes = Client::new(account, Kind::Contacts).sync(account, known)?;
    let cards = changes
        .items
        .into_iter()
        .filter_map(|(book, item)| {
            let contact = vcard::parse(&item.data)?;
            Some((
                book,
                Card {
                    url: item.url,
                    etag: item.etag,
                    vcard: item.data,
                    contact,
                },
            ))
        })
        .collect();
    Ok(Changes {
        books: changes.collections,
        cards,
        gone: changes.gone,
    })
}
