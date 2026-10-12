//! One archived item as one ISO SQL `INSERT`, on one line, written and read
//! without a parser crate: the statement has a fixed shape, and reading it back
//! is a matter of two literal forms — a string with its quotes doubled, and a
//! hex string `X'...'` for the bytes. Both are ISO/IEC 9075, so the script
//! loads into any database an operator has. Both literals are ISO SQL's as
//! `xmip-core-library-sql` writes and reads them (`sql::STANDARD`).

use sql::{Literal, STANDARD};

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
            "{HEAD}{}, {}, {}, {}, {});",
            STANDARD.quote_text(&self.data_type),
            STANDARD.quote_text(&self.identifier),
            STANDARD.quote_bytes(&self.bytes),
            STANDARD.quote_text(&self.metadata),
            STANDARD.quote_text(&self.archived_at)
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
        let line = line.trim();
        let rest = line
            .strip_prefix(HEAD)
            .ok_or_else(|| "not an INSERT INTO archive statement".to_string())?;
        let (data_type, rest) = string(line, rest)?;
        let rest = expect(line, rest, ", ")?;
        let (identifier, rest) = string(line, rest)?;
        let rest = expect(line, rest, ", ")?;
        let (bytes, rest) = hex_literal(line, rest)?;
        let rest = expect(line, rest, ", ")?;
        let (metadata, rest) = string(line, rest)?;
        let rest = expect(line, rest, ", ")?;
        let (archived_at, rest) = string(line, rest)?;
        let rest = expect(line, rest, ");")?;
        if !rest.is_empty() {
            return Err(format!("text after the statement: {rest:?}"));
        }
        Ok(Self {
            data_type,
            identifier,
            bytes,
            metadata,
            archived_at,
        })
    }
}

/// The column of `line`, counted in characters from 1, that `rest` starts at.
fn column(line: &str, rest: &str) -> usize {
    line[..line.len() - rest.len()].chars().count() + 1
}

/// `rest` after `token`, or a refusal naming the column.
fn expect<'a>(line: &str, rest: &'a str, token: &str) -> Result<&'a str, String> {
    rest.strip_prefix(token)
        .ok_or_else(|| format!("expected {token:?} at column {}", column(line, rest)))
}

/// A string literal: `'...'` with an embedded quote written twice.
fn string<'a>(line: &str, rest: &'a str) -> Result<(String, &'a str), String> {
    if !rest.starts_with('\'') {
        return Err(format!(
            "expected a string literal at column {}",
            column(line, rest)
        ));
    }
    match STANDARD.read_literal(rest) {
        Some((Literal::Text(text), after)) => Ok((text, after)),
        _ => Err(format!(
            "the string literal at column {} is not terminated",
            column(line, rest)
        )),
    }
}

/// A hex string literal: `X'6b...'`.
fn hex_literal<'a>(line: &str, rest: &'a str) -> Result<(Vec<u8>, &'a str), String> {
    if !rest.starts_with("X'") {
        return Err(format!(
            "expected a hex literal at column {}",
            column(line, rest)
        ));
    }
    match STANDARD.read_literal(rest) {
        Some((Literal::Bytes(bytes), after)) => Ok((bytes, after)),
        _ => Err(format!(
            "the hex literal at column {} is not terminated or not whole bytes",
            column(line, rest)
        )),
    }
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
    fn multibyte_text_parses_back_and_a_cut_is_refused_without_panic() {
        let wide = Statement {
            identifier: "Zoë's 名前\u{a0}\u{1f600}".to_string(),
            metadata: "größe\u{3000}é".to_string(),
            ..statement()
        };
        let sql = wide.to_sql();
        assert_eq!(Statement::parse(&sql).expect("parse"), wide);
        for cut in (0..sql.len()).filter(|&at| sql.is_char_boundary(at)) {
            assert!(Statement::parse(&sql[..cut]).is_err(), "{cut}");
        }
        let line = format!("{HEAD}'json',\u{a0}'id', X'', '', '');");
        assert!(Statement::parse(&line).is_err());
        let line = format!("{HEAD}'json', 'id', X'é', '', '');");
        assert!(Statement::parse(&line).is_err());
    }

    #[test]
    fn bad_hex_is_refused() {
        let line = format!("{HEAD}'json', 'id', X'zz', '', '');");
        let refused = Statement::parse(&line).expect_err("bad hex");
        assert!(refused.contains("hex"), "{refused}");
    }
}
