#![forbid(unsafe_code)]

//! SQL script archive: an [`ArchiveStore`] that appends each retained item as
//! one `INSERT` statement to a script, and restores it by reading the script
//! back for the statement that names it.
//!
//! A xmip-core-archive **technology** (repository-model.md): it depends on the
//! archive capability for the [`ArchiveStore`] trait and its item, receipt and
//! error types, never the reverse. One script per data type at
//! `<root>/<data_type>.sql`; one item is one line of vendor-neutral ISO SQL
//! with the four columns every archive technology shares — `data_type`,
//! `identifier`, `bytes`, `metadata` — and `archived_at`, so an operator loads
//! the script into whatever database is to hand, and this crate reads it
//! without one. The receipt carries the SHA-256 of the bytes, and `restore`
//! checks it. The metadata text, the moment, the safe script name and the
//! checksum come from the capability — `archive::metadata`, `archive::timestamp`,
//! `archive::layout` and `archive::checksum` (ADR-0044); the statement itself is
//! this technology's own.

mod statement;

use std::fmt::Display;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use archive::checksum::sha256_hex;
use archive::layout::sanitise;
use archive::{ArchiveError, ArchiveItem, ArchiveReceipt, ArchiveStore, metadata, timestamp};

use crate::statement::Statement;

/// An archive that persists items as `INSERT` statements in scripts rooted at a
/// directory, one script per data type.
pub struct SqlScriptArchive {
    root: PathBuf,
}

impl SqlScriptArchive {
    /// An archive writing under `root`; the directory is created on demand and
    /// each script the first time its data type is archived.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// What every receipt of this root shares: `sql:///<root>/`.
    fn prefix(&self) -> String {
        format!("sql://{}/", uri_path(&self.root))
    }

    /// The script and identifier a receipt names, refusing a receipt from
    /// under another root. The script name is a sanitised segment, so nothing
    /// outside the root is ever read.
    fn target_of(&self, location: &str) -> Result<(PathBuf, String), ArchiveError> {
        let rest = location
            .strip_prefix(&self.prefix())
            .ok_or_else(|| ArchiveError {
                message: format!(
                    "{location} is not a receipt of the archive at {}",
                    self.root.display()
                ),
            })?;
        let (script, identifier) = rest
            .split_once('#')
            .filter(|(script, _)| {
                script
                    .strip_suffix(".sql")
                    .is_some_and(|stem| !stem.is_empty() && sanitise(stem) == stem)
            })
            .ok_or_else(|| ArchiveError {
                message: format!("{location} does not name a script and an identifier"),
            })?;
        Ok((self.root.join(script), identifier.to_string()))
    }
}

impl ArchiveStore for SqlScriptArchive {
    fn archive(&self, item: ArchiveItem) -> Result<ArchiveReceipt, ArchiveError> {
        let script = format!("{}.sql", sanitise(&item.data_type));
        let path = self.root.join(&script);
        std::fs::create_dir_all(&self.root).map_err(ArchiveError::caused_by)?;
        let statement = Statement {
            data_type: item.data_type,
            identifier: item.identifier,
            bytes: item.bytes,
            metadata: metadata::encode(&item.metadata),
            archived_at: timestamp::now(),
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|cause| at(&path, cause))?;
        writeln!(file, "{}", statement.to_sql()).map_err(|cause| at(&path, cause))?;
        Ok(ArchiveReceipt {
            location: format!("{}{script}#{}", self.prefix(), statement.identifier),
            checksum: Some(sha256_hex(&statement.bytes)),
        })
    }

    fn restore(&self, receipt: &ArchiveReceipt) -> Result<ArchiveItem, ArchiveError> {
        let (path, identifier) = self.target_of(&receipt.location)?;
        let text = std::fs::read_to_string(&path).map_err(|cause| at(&path, cause))?;
        let mut found = None;
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let statement = Statement::parse(line).map_err(|reason| ArchiveError {
                message: format!("{}:{}: {reason}", path.display(), index + 1),
            })?;
            if statement.identifier == identifier {
                found = Some(statement);
            }
        }
        let statement = found.ok_or_else(|| ArchiveError {
            message: format!("no statement for {identifier} in {}", receipt.location),
        })?;
        if let Some(expected) = &receipt.checksum {
            let actual = sha256_hex(&statement.bytes);
            if actual != *expected {
                return Err(ArchiveError {
                    message: format!(
                        "checksum mismatch at {}: the receipt says {expected}, the script {actual}",
                        receipt.location
                    ),
                });
            }
        }
        Ok(ArchiveItem {
            data_type: statement.data_type,
            identifier: statement.identifier,
            bytes: statement.bytes,
            metadata: metadata::decode(&statement.metadata),
        })
    }
}

/// `path` as the path part of a URI: forward slashes, and a leading slash so a
/// Windows drive reads `/C:/...` after the `sql://` authority.
fn uri_path(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    if text.starts_with('/') {
        text
    } else {
        format!("/{text}")
    }
}

fn at(path: &Path, cause: impl Display) -> ArchiveError {
    ArchiveError {
        message: format!("{}: {cause}", path.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use archive::fixture::item;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("xmip-sql-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        dir
    }

    #[test]
    fn an_archived_item_restores_from_its_statement() {
        let root = scratch("roundtrip");
        let store = SqlScriptArchive::new(&root);
        let original = item("json#1");
        let receipt = store.archive(original.clone()).expect("archive");
        assert!(
            receipt.location.starts_with("sql:///"),
            "{}",
            receipt.location
        );
        assert!(
            receipt.location.ends_with("/json.sql#json#1"),
            "{}",
            receipt.location
        );
        assert_eq!(receipt.checksum, Some(sha256_hex(&original.bytes)));
        let restored = store.restore(&receipt).expect("restore");
        assert_eq!(restored, original, "the script gives the item back");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn two_items_share_one_script_and_each_restores() {
        let root = scratch("two");
        let store = SqlScriptArchive::new(&root);
        let first = store.archive(item("json#1")).expect("first");
        let second = store.archive(item("json#2")).expect("second");
        let script = std::fs::read_to_string(root.join("json.sql")).expect("the script");
        assert_eq!(script.lines().count(), 2, "{script}");
        assert_eq!(store.restore(&first).expect("restore").identifier, "json#1");
        assert_eq!(
            store.restore(&second).expect("restore").identifier,
            "json#2"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_identifier_with_a_quote_round_trips() {
        let root = scratch("quote");
        let store = SqlScriptArchive::new(&root);
        let original = item("it's 'quoted'");
        let receipt = store.archive(original.clone()).expect("archive");
        let script = std::fs::read_to_string(root.join("json.sql")).expect("the script");
        assert!(script.contains("'it''s ''quoted'''"), "{script}");
        assert_eq!(store.restore(&receipt).expect("restore"), original);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_last_statement_for_an_identifier_wins() {
        let root = scratch("last");
        let store = SqlScriptArchive::new(&root);
        store.archive(item("json#1")).expect("first");
        let later = ArchiveItem {
            bytes: b"{\"kept\":\"again\"}".to_vec(),
            ..item("json#1")
        };
        let receipt = store.archive(later.clone()).expect("second");
        assert_eq!(store.restore(&receipt).expect("restore"), later);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_unknown_identifier_is_an_error() {
        let root = scratch("unknown");
        let store = SqlScriptArchive::new(&root);
        store.archive(item("json#1")).expect("archive");
        let receipt = ArchiveReceipt {
            location: format!("{}json.sql#never", store.prefix()),
            checksum: None,
        };
        let refused = store.restore(&receipt).expect_err("no such statement");
        assert!(refused.message.contains("never"), "{refused}");
        assert!(refused.message.contains(&receipt.location), "{refused}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_missing_script_is_an_error_naming_the_path() {
        let root = scratch("missing");
        let store = SqlScriptArchive::new(&root);
        let receipt = ArchiveReceipt {
            location: format!("{}csv.sql#x", store.prefix()),
            checksum: None,
        };
        let refused = store.restore(&receipt).expect_err("no script");
        let path = root.join("csv.sql");
        assert!(
            refused.message.contains(&path.display().to_string()),
            "{refused}"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_mismatched_checksum_in_the_receipt_is_refused() {
        let root = scratch("mismatch");
        let store = SqlScriptArchive::new(&root);
        let mut receipt = store.archive(item("json#1")).expect("archive");
        receipt.checksum = Some("0".repeat(64));
        let refused = store.restore(&receipt).expect_err("the receipt lies");
        assert!(refused.message.contains("checksum mismatch"), "{refused}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_script_is_loadable_text() {
        let root = scratch("loadable");
        let store = SqlScriptArchive::new(&root);
        store.archive(item("json#1")).expect("first");
        store.archive(item("json#2")).expect("second");
        let mut csv = item("csv-1");
        csv.data_type = "csv".to_string();
        store.archive(csv).expect("another data type");
        let script = std::fs::read_to_string(root.join("json.sql")).expect("the script");
        for line in script.lines() {
            assert!(line.starts_with("INSERT INTO archive ("), "{line}");
            assert!(line.ends_with(");"), "{line}");
        }
        assert!(root.join("csv.sql").exists(), "one script per data type");
        std::fs::remove_dir_all(&root).ok();
    }
}
