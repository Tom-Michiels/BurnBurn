//! Frozen data rows for personalisation: CSV-like table → frozen, hashed rows → resolved text per
//! instance. A frozen set carries its own time value, so reopening never re-personalises.

use burnburn_types::{CanonicalWriter, ContentHash, HashDomain};

/// Input bounds (untrusted files).
const MAX_ROWS: usize = 100_000;
const MAX_COLUMNS: usize = 256;
const MAX_FIELD_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTable {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CsvError {
    Empty,
    UnterminatedQuote {
        line: usize,
    },
    /// A row with another number of fields than the header.
    Ragged {
        line: usize,
        expected: usize,
        found: usize,
    },
    DuplicateHeader {
        name: String,
    },
    EmptyHeader {
        column: usize,
    },
    TooLarge,
    /// The bytes are not UTF-8; `valid_up_to` is the length of the valid prefix.
    NotUtf8 {
        valid_up_to: usize,
    },
}

impl DataTable {
    /// As [`DataTable::parse_csv`] for file bytes: UTF-8 only, a leading byte-order mark is dropped.
    pub fn from_utf8(bytes: &[u8], delimiter: char) -> Result<DataTable, CsvError> {
        const BOM: &[u8] = b"\xEF\xBB\xBF";
        let (body, skipped) = match bytes.strip_prefix(BOM) {
            Some(rest) => (rest, BOM.len()),
            None => (bytes, 0),
        };
        let text =
            std::str::from_utf8(body).map_err(|e| CsvError::NotUtf8 { valid_up_to: skipped + e.valid_up_to() })?;
        DataTable::parse_csv(text, delimiter)
    }

    /// RFC 4180-style: `delimiter`-separated, `"` quoting with `""` escapes, LF or CRLF lines,
    /// first row = header. Bounded (rows, columns, field size).
    pub fn parse_csv(text: &str, delimiter: char) -> Result<DataTable, CsvError> {
        let mut records: Vec<(usize, Vec<String>)> = Vec::new();
        let mut record = Vec::new();
        let mut field = String::new();
        let mut quoted = false;
        let mut field_started_quoted = false;
        let mut line = 1;
        let mut record_line = 1;
        let mut chars = text.chars().peekable();
        let mut any = false;
        while let Some(ch) = chars.next() {
            any = true;
            if field.len() > MAX_FIELD_BYTES {
                return Err(CsvError::TooLarge);
            }
            if quoted {
                match ch {
                    '"' if chars.peek() == Some(&'"') => {
                        chars.next();
                        field.push('"');
                    }
                    '"' => quoted = false,
                    '\n' => {
                        line += 1;
                        field.push(ch);
                    }
                    _ => field.push(ch),
                }
                continue;
            }
            match ch {
                '"' if field.is_empty() && !field_started_quoted => {
                    quoted = true;
                    field_started_quoted = true;
                }
                c if c == delimiter => {
                    record.push(std::mem::take(&mut field));
                    field_started_quoted = false;
                    if record.len() > MAX_COLUMNS {
                        return Err(CsvError::TooLarge);
                    }
                }
                '\r' if chars.peek() == Some(&'\n') => {}
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    field_started_quoted = false;
                    records.push((record_line, std::mem::take(&mut record)));
                    if records.len() > MAX_ROWS {
                        return Err(CsvError::TooLarge);
                    }
                    line += 1;
                    record_line = line;
                }
                _ => field.push(ch),
            }
        }
        if quoted {
            return Err(CsvError::UnterminatedQuote { line: record_line });
        }
        if !field.is_empty() || !record.is_empty() || field_started_quoted {
            record.push(field);
            records.push((record_line, record));
        }
        if !any || records.is_empty() {
            return Err(CsvError::Empty);
        }
        let mut iter = records.into_iter();
        let (_, headers) = iter.next().ok_or(CsvError::Empty)?;
        for (i, h) in headers.iter().enumerate() {
            if h.trim().is_empty() {
                return Err(CsvError::EmptyHeader { column: i });
            }
            if headers[..i].contains(h) {
                return Err(CsvError::DuplicateHeader { name: h.clone() });
            }
        }
        let mut rows = Vec::new();
        for (l, r) in iter {
            if r.len() != headers.len() {
                return Err(CsvError::Ragged { line: l, expected: headers.len(), found: r.len() });
            }
            rows.push(r);
        }
        Ok(DataTable { headers, rows })
    }
}

/// Rows frozen for production: table + the time value that `{datum}` resolves to + a content
/// hash. Instances bind to this hash; re-importing changed data gives a different hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenRows {
    pub table: DataTable,
    pub frozen_at: String,
    pub hash: ContentHash,
}

impl FrozenRows {
    /// `frozen_at` is the time value chosen at freeze time (injected by the caller, never read
    /// from a clock here), so the same frozen set always resolves the same.
    pub fn freeze(table: DataTable, frozen_at: &str) -> FrozenRows {
        let mut w = CanonicalWriter::new();
        w.tag("burnburn-frozen-rows");
        w.str(frozen_at);
        w.u64(table.headers.len() as u64);
        for h in &table.headers {
            w.str(h);
        }
        w.u64(table.rows.len() as u64);
        for r in &table.rows {
            for c in r {
                w.str(c);
            }
        }
        // Only strings and integers: finishing cannot meet a non-finite number.
        let hash = ContentHash::of(HashDomain::Asset, &w.finish().unwrap_or_default());
        FrozenRows { table, frozen_at: frozen_at.to_owned(), hash }
    }
}

/// What an instance stores: which frozen set and which row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowBinding {
    pub rows: ContentHash,
    pub row: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRow {
    pub text: String,
    /// `first_sequence + row`.
    pub sequence: u64,
    /// Placeholders without a column, or whose cell is empty (shown in the row preview).
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RowError {
    RowOutOfRange {
        row: usize,
        rows: usize,
    },
    /// `{` without `}` (or a lone `}`) at byte `at`.
    BadTemplate {
        at: usize,
    },
    /// The binding refers to another frozen set (data changed since it was frozen).
    StaleRows,
}

/// Resolves `{column}`, `{#}` (sequence number) and `{datum}` (frozen time); `{{`/`}}` escape.
pub fn resolve(template: &str, rows: &FrozenRows, row: usize, first_sequence: u64) -> Result<ResolvedRow, RowError> {
    let cells = rows.table.rows.get(row).ok_or(RowError::RowOutOfRange { row, rows: rows.table.rows.len() })?;
    let sequence = first_sequence.saturating_add(row as u64);
    let mut text = String::new();
    let mut missing = Vec::new();
    let mut it = template.char_indices().peekable();
    while let Some((i, ch)) = it.next() {
        match ch {
            '{' if it.peek().map(|p| p.1) == Some('{') => {
                it.next();
                text.push('{');
            }
            '}' if it.peek().map(|p| p.1) == Some('}') => {
                it.next();
                text.push('}');
            }
            '}' => return Err(RowError::BadTemplate { at: i }),
            '{' => {
                let rest = template.get(i + 1..).unwrap_or("");
                let end = rest.find('}').ok_or(RowError::BadTemplate { at: i })?;
                let name = rest.get(..end).unwrap_or("");
                if name.contains('{') {
                    return Err(RowError::BadTemplate { at: i });
                }
                for _ in 0..name.chars().count() + 1 {
                    it.next();
                }
                match name {
                    "#" => text.push_str(&sequence.to_string()),
                    "datum" => text.push_str(&rows.frozen_at),
                    _ => match rows.table.headers.iter().position(|h| h == name).and_then(|c| cells.get(c)) {
                        Some(v) if !v.is_empty() => text.push_str(v),
                        _ => missing.push(name.to_owned()),
                    },
                }
            }
            c => text.push(c),
        }
    }
    Ok(ResolvedRow { text, sequence, missing })
}

/// As [`resolve`], but only against the frozen set the instance was bound to.
pub fn resolve_bound(
    template: &str,
    rows: &FrozenRows,
    binding: RowBinding,
    first_sequence: u64,
) -> Result<ResolvedRow, RowError> {
    if rows.hash != binding.rows {
        return Err(RowError::StaleRows);
    }
    resolve(template, rows, binding.row, first_sequence)
}
