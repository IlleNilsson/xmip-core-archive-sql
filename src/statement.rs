//! One archived item as one ISO SQL `INSERT`, on one line, written and read
//! without a parser crate: the statement has a fixed shape, and reading it back
//! is a matter of two literal forms — a string with its quotes doubled, and a
//! hex string `X'...'` for the bytes. Both are ISO/IEC 9075, so the script
//! loads into any database an operator has.

use std::fmt::Write;

/// The row one statement carries: the four item columns every archive
/// technology shares, `metadata` already encoded to one text, and the moment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statement {
    pub data_type: String,
    pub identifier: String,
    pub bytes: Vec<u8>,
    pub metadata: String,
    pub archived_at: String,
}

/// Every statement opens the same way; what follows is the five values.
const HEAD: &str =
    "INSERT INTO archive (data_type, identifier, bytes, metadata, archived_at) VALUES (";

impl Statement {
    /// The statement as one line of SQL, ending in `;`.
    #[must_use]
    pub fn to_sql(&self) -> String {
        format!(
            "{HEAD}{}, {}, X'{}', {}, {});",
            quote(&self.data_type),
            quote(&self.identifier),
            hex(&self.bytes),
            quote(&self.metadata),
            quote(&self.archived_at)
        )
    }

    /// A line of the script back into its statement.
    ///
    /// # Errors
    ///
    /// A line that does not open as [`Statement::to_sql`] writes it, a string
    /// literal left unterminated, a hex literal with an odd digit or an odd
    /// count of them, or anything after the closing `);`.
    pub fn parse(line: &str) -> Result<Self, String> {
        let rest = line
            .trim()
            .strip_prefix(HEAD)
            .ok_or_else(|| "not an INSERT INTO archive statement".to_string())?;
        let mut cursor = Cursor { text: rest, at: 0 };
        let data_type = cursor.string()?;
        cursor.expect(", ")?;
        let identifier = cursor.string()?;
        cursor.expect(", ")?;
        let bytes = cursor.hex()?;
        cursor.expect(", ")?;
        let metadata = cursor.string()?;
        cursor.expect(", ")?;
        let archived_at = cursor.string()?;
        cursor.expect(");")?;
        cursor.end()?;
        Ok(Self {
            data_type,
            identifier,
            bytes,
            metadata,
            archived_at,
        })
    }
}

/// A position in the text after `VALUES (`. Every token it consumes is ASCII
/// or ends at an ASCII quote, so the position is always a character boundary.
struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl Cursor<'_> {
    fn rest(&self) -> &str {
        self.text.get(self.at..).unwrap_or("")
    }

    fn expect(&mut self, token: &str) -> Result<(), String> {
        if self.rest().starts_with(token) {
            self.at += token.len();
            Ok(())
        } else {
            Err(format!(
                "expected {token:?} at column {}",
                self.at + HEAD.len()
            ))
        }
    }

    fn end(&self) -> Result<(), String> {
        if self.rest().is_empty() {
            Ok(())
        } else {
            Err(format!("text after the statement: {:?}", self.rest()))
        }
    }

    /// A string literal: `'...'` with an embedded quote written twice.
    fn string(&mut self) -> Result<String, String> {
        self.expect("'")?;
        let mut out = String::new();
        loop {
            let rest = self.rest();
            let end = rest
                .find('\'')
                .ok_or_else(|| "a string literal is not terminated".to_string())?;
            out.push_str(&rest[..end]);
            self.at += end + 1;
            if self.rest().starts_with('\'') {
                out.push('\'');
                self.at += 1;
            } else {
                return Ok(out);
            }
        }
    }

    /// A hex string literal: `X'6b...'`.
    fn hex(&mut self) -> Result<Vec<u8>, String> {
        self.expect("X'")?;
        let rest = self.rest();
        let end = rest
            .find('\'')
            .ok_or_else(|| "a hex literal is not terminated".to_string())?;
        let bytes = unhex(&rest[..end]);
        self.at += end + 1;
        bytes
    }
}

/// `text` as an SQL string literal.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// `bytes` as lowercase hex digits.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn unhex(digits: &str) -> Result<Vec<u8>, String> {
    if !digits.len().is_multiple_of(2) {
        return Err(format!("a hex literal of {} digits", digits.len()));
    }
    (0..digits.len())
        .step_by(2)
        .map(|start| {
            digits
                .get(start..start + 2)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(|| format!("not hex digits in X'{digits}'"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statement() -> Statement {
        Statement {
            data_type: "json".to_string(),
            identifier: "it's #1".to_string(),
            bytes: b"{\"kept\":true}".to_vec(),
            metadata: "source\u{1f}playground".to_string(),
            archived_at: "2026-09-09T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn the_statement_is_one_line_of_iso_sql() {
        let sql = statement().to_sql();
        assert_eq!(
            sql,
            "INSERT INTO archive (data_type, identifier, bytes, metadata, archived_at) \
             VALUES ('json', 'it''s #1', X'7b226b657074223a747275657d', \
             'source\u{1f}playground', '2026-09-09T00:00:00Z');"
        );
        assert!(!sql.contains('\n'));
    }

    #[test]
    fn the_statement_parses_back_to_itself() {
        let sql = statement().to_sql();
        assert_eq!(Statement::parse(&sql).expect("parse"), statement());
    }

    #[test]
    fn empty_bytes_and_empty_metadata_are_kept() {
        let empty = Statement {
            bytes: Vec::new(),
            metadata: String::new(),
            ..statement()
        };
        let sql = empty.to_sql();
        assert!(sql.contains("X'', ''"), "{sql}");
        assert_eq!(Statement::parse(&sql).expect("parse"), empty);
    }

    #[test]
    fn a_line_that_is_not_an_insert_is_refused() {
        let refused = Statement::parse("SELECT 1;").expect_err("not an insert");
        assert!(refused.contains("INSERT"), "{refused}");
    }

    #[test]
    fn an_unterminated_string_is_refused() {
        let line = format!("{HEAD}'json', 'open");
        let refused = Statement::parse(&line).expect_err("unterminated");
        assert!(refused.contains("not terminated"), "{refused}");
    }

    #[test]
    fn bad_hex_is_refused() {
        let line = format!("{HEAD}'json', 'id', X'zz', '', '');");
        let refused = Statement::parse(&line).expect_err("bad hex");
        assert!(refused.contains("hex"), "{refused}");
    }
}
