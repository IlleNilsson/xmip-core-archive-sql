//! One archived item as one ISO SQL `INSERT`, on one line, written and read
//! without a parser crate: the statement has a fixed shape, and reading it back
//! is a matter of two literal forms — a string with its quotes doubled, and a
//! hex string `X'...'` for the bytes. Both are ISO/IEC 9075, so the script
//! loads into any database an operator has. The string literal is
//! `codec::sql`'s, written and read there for every SQL crate.

use codec::char_reader::CharReader;
use codec::hex;
use codec::sql::Delimiter;

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
            Delimiter::STRING.quote(&self.data_type),
            Delimiter::STRING.quote(&self.identifier),
            hex::encode(&self.bytes),
            Delimiter::STRING.quote(&self.metadata),
            Delimiter::STRING.quote(&self.archived_at)
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
        let mut reader = CharReader::new(rest);
        let data_type = string(&mut reader)?;
        expect(&mut reader, ", ")?;
        let identifier = string(&mut reader)?;
        expect(&mut reader, ", ")?;
        let bytes = hex_literal(&mut reader)?;
        expect(&mut reader, ", ")?;
        let metadata = string(&mut reader)?;
        expect(&mut reader, ", ")?;
        let archived_at = string(&mut reader)?;
        expect(&mut reader, ");")?;
        if !reader.is_done() {
            return Err(format!("text after the statement: {:?}", reader.rest()));
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

/// Take `token` from the text after `VALUES (`, or refuse naming the column.
fn expect(reader: &mut CharReader<'_>, token: &str) -> Result<(), String> {
    if reader.eat_str(token) {
        Ok(())
    } else {
        Err(format!(
            "expected {token:?} at column {}",
            reader.column() + HEAD.len()
        ))
    }
}

/// A string literal: `'...'` with an embedded quote written twice.
fn string(reader: &mut CharReader<'_>) -> Result<String, String> {
    if reader.peek() != Some('\'') {
        return Err(format!(
            "expected a string literal at column {}",
            reader.column() + HEAD.len()
        ));
    }
    Delimiter::STRING
        .read(reader)
        .map_err(|error| error.message)
}

/// A hex string literal: `X'6b...'`.
fn hex_literal(reader: &mut CharReader<'_>) -> Result<Vec<u8>, String> {
    expect(reader, "X'")?;
    let digits = reader.take_while(|character| character != '\'');
    if !reader.eat('\'') {
        return Err("a hex literal is not terminated".to_string());
    }
    hex::decode(digits).map_err(|error| error.message)
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
