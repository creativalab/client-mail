//! Conversations: groups a message list into threads.
//!
//! Two messages belong to the same conversation when they share a message
//! id: one's own Message-ID, or any id in References / In-Reply-To. Linking
//! through shared ids (rather than walking up to a root) also joins replies
//! whose original message is not in the list, e.g. two answers to a message
//! that was deleted or lives in another folder.
//!
//! Messages without ids stay on their own; subjects are not used to guess.

use crate::mail::Header;
use std::borrow::Borrow;
use std::collections::HashMap;

/// One conversation, as indices into the header list it was built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    /// Newest message first.
    pub members: Vec<usize>,
    pub unread: usize,
}

impl Thread {
    /// The newest message, shown as the conversation's row.
    pub fn latest(&self) -> usize {
        self.members[0]
    }
}

/// Groups `headers` into conversations, newest conversation first.
/// Takes headers or references to them (`&[Header]` or `&[&Header]`).
pub fn group<H: Borrow<Header>>(headers: &[H]) -> Vec<Thread> {
    let headers: Vec<&Header> = headers.iter().map(Borrow::borrow).collect();
    // Union-find over header indices.
    let mut parent: Vec<usize> = (0..headers.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    // The first header seen with each id; later ones are joined to it.
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, h) in headers.iter().enumerate() {
        let ids = std::iter::once(h.message_id.as_str())
            .chain(h.parents.iter().map(String::as_str))
            .filter(|id| !id.is_empty());
        for id in ids {
            match owner.get(id) {
                Some(&j) => {
                    let (a, b) = (find(&mut parent, i), find(&mut parent, j));
                    if a != b {
                        parent[a] = b;
                    }
                }
                None => {
                    owner.insert(id, i);
                }
            }
        }
    }

    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..headers.len() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(i);
    }
    let newest = |i: &usize| (headers[*i].timestamp, headers[*i].uid);
    let mut threads: Vec<Thread> = groups
        .into_values()
        .map(|mut members| {
            members.sort_by_key(|i| std::cmp::Reverse(newest(i)));
            let unread = members.iter().filter(|&&i| !headers[i].seen).count();
            Thread { members, unread }
        })
        .collect();
    threads.sort_by_key(|t| std::cmp::Reverse(newest(&t.latest())));
    threads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(uid: u32, ts: i64, id: &str, parents: &[&str], seen: bool) -> Header {
        Header {
            uid,
            timestamp: ts,
            message_id: id.into(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            seen,
            ..Default::default()
        }
    }

    #[test]
    fn groups_replies_and_orders_by_newest() {
        let list = vec![
            h(1, 100, "a", &[], true),         // original
            h(2, 200, "b", &["a"], false),     // reply to a
            h(3, 300, "c", &["a", "b"], true), // reply to b
            h(4, 250, "x", &[], false),        // unrelated
            h(5, 50, "", &[], true),           // no id at all
        ];
        let t = group(&list);
        assert_eq!(t.len(), 3);
        assert_eq!(t[0].members, vec![2, 1, 0]); // newest conversation first, newest message first
        assert_eq!(t[0].unread, 1);
        assert_eq!(t[1].members, vec![3]);
        assert_eq!(t[2].members, vec![4]);
    }

    #[test]
    fn joins_replies_whose_original_is_missing() {
        // Two answers to a message that is not in the list.
        let list = vec![h(1, 10, "r1", &["gone"], true), h(2, 20, "r2", &["gone"], true)];
        assert_eq!(group(&list).len(), 1);
    }
}
