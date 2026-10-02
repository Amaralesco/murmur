//! The Jetstream wire model.
//!
//! Shared because it is the contract between whoever produces events and
//! whoever consumes them — `ingest` and the slice 5 consumer must agree on it
//! by definition. This is the kind of thing `murmur-common` exists for; see
//! `DECISIONS.md` 2026-09-02, and 2026-09-28 which moved that entry's review
//! trigger onto the consumer crate.
//!
//! Two couplings to remember, both from `docs/jetstream.md`:
//!
//! - `payload: Commit` is only sound while the subscription URL hardcodes
//!   `kinds=commit`. An `identity` or `account` event would not deserialise.
//! - `Record.text` is tied to `collections=app.bsky.feed.post`. A like has no
//!   text, and a reply is also a `feed.post`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The envelope. Everything useful is nested under `payload`.
#[derive(Deserialize, Debug)]
pub struct JetstreamMessage {
    pub payload: Commit,
}

#[derive(Deserialize, Debug)]
pub struct Commit {    pub did: String,
    pub collection: String,
    pub rkey: String,
    pub rev: String,
    pub seq: u64,
    pub time: DateTime<Utc>,

    pub operation: Operation,
    pub cid: Option<String>,
    pub record: Option<Record>,
}

impl Commit {
    /// The record's AT Protocol URI, `at://{did}/{collection}/{rkey}`.
    ///
    /// This is the identifier that makes two events the same record, which is
    /// what downstream deduplication keys on — see `clickhouse/schema-notes.md`.
    pub fn uri(&self) -> String {
        format!("at://{}/{}/{}", self.did, self.collection, self.rkey)
    }
}

#[derive(Deserialize, Debug)]
pub struct Record {
    /// Only ever set for `app.bsky.feed.post`.
    pub text: Option<String>,

    /// When the user's client claims the record was created.
    ///
    /// Deliberately a `String` rather than a `DateTime`: this value is
    /// supplied by the client and cannot be trusted. Wrong clocks, wrong
    /// timezones and deliberate backdating are all common, and a value that
    /// fails to parse would abort deserialisation of the whole event. Parsing
    /// belongs wherever the value is used, where a failure can be handled.
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Create,
    Update,
    Delete,
}
