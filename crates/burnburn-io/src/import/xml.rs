//! A minimal, bounded XML tokenizer for SVG: elements, attributes, comments, processing
//! instructions, CDATA and DOCTYPE (skipped). Only the five predefined entities and numeric
//! character references are decoded; no entity is ever expanded, so entity bombs cannot grow.
//! Text content is ignored (SVG geometry lives in attributes) unless the tokenizer is made
//! with [`Tokenizer::with_text`] (LBRN2 keeps transforms and vertex lists in text).

use super::ParseError;

pub(crate) struct Attr<'a> {
    pub name: &'a str,
    pub value: String,
}

pub(crate) enum Event<'a> {
    Start {
        name: &'a str,
        attrs: Vec<Attr<'a>>,
        self_closing: bool,
    },
    End {
        name: &'a str,
    },
    /// Non-blank character data, decoded (only from [`Tokenizer::with_text`]).
    Text {
        text: String,
    },
}

pub(crate) struct Tokenizer<'a> {
    s: &'a str,
    pos: usize,
    keep_text: bool,
}

fn malformed(at: usize, reason: &'static str) -> ParseError {
    ParseError::Malformed { at, reason }
}

fn is_name_end(b: u8) -> bool {
    b.is_ascii_whitespace() || matches!(b, b'/' | b'>' | b'=' | b'<')
}

impl<'a> Tokenizer<'a> {
    pub fn new(s: &'a str) -> Self {
        Tokenizer { s, pos: 0, keep_text: false }
    }

    /// A tokenizer that also reports text content.
    pub fn with_text(s: &'a str) -> Self {
        Tokenizer { s, pos: 0, keep_text: true }
    }

    fn bytes(&self) -> &'a [u8] {
        self.s.as_bytes()
    }

    fn rest(&self) -> &'a str {
        self.s.get(self.pos..).unwrap_or("")
    }

    /// Skips to just after `marker`, or fails.
    fn skip_past(&mut self, marker: &str, reason: &'static str) -> Result<(), ParseError> {
        match self.rest().find(marker) {
            Some(i) => {
                self.pos += i + marker.len();
                Ok(())
            }
            None => Err(malformed(self.pos, reason)),
        }
    }

    fn skip_ws(&mut self) {
        while self.bytes().get(self.pos).is_some_and(|b| b.is_ascii_whitespace()) {
            self.pos += 1;
        }
    }

    fn name(&mut self) -> Result<&'a str, ParseError> {
        let start = self.pos;
        while self.bytes().get(self.pos).is_some_and(|b| !is_name_end(*b)) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(malformed(start, "expected a name"));
        }
        // Name ends at an ASCII byte (or the end), so this is a char boundary.
        self.s.get(start..self.pos).ok_or(malformed(start, "invalid name"))
    }

    /// The next element event, or `None` at the end of the document.
    pub fn next_event(&mut self) -> Result<Option<Event<'a>>, ParseError> {
        loop {
            // Text is ignored, but an entity reference in it must still be well-formed.
            let Some(lt) = self.rest().find('<') else {
                decode(self.rest(), self.pos)?;
                return Ok(None);
            };
            let text = decode(self.rest().get(..lt).unwrap_or(""), self.pos)?;
            self.pos += lt;
            if self.keep_text && !text.trim().is_empty() {
                return Ok(Some(Event::Text { text }));
            }
            let rest = self.rest();
            if rest.starts_with("<?") {
                self.skip_past("?>", "unterminated processing instruction")?;
            } else if rest.starts_with("<!--") {
                self.skip_past("-->", "unterminated comment")?;
            } else if rest.starts_with("<![CDATA[") {
                self.skip_past("]]>", "unterminated CDATA")?;
            } else if rest.starts_with("<!") {
                // DOCTYPE (possibly with an internal subset); declarations are not interpreted.
                let close = rest.find('>').ok_or(malformed(self.pos, "unterminated declaration"))?;
                if let Some(open) = rest.find('[')
                    && open < close
                {
                    self.skip_past("]", "unterminated internal subset")?;
                    self.skip_past(">", "unterminated declaration")?;
                } else {
                    self.pos += close + 1;
                }
            } else if rest.starts_with("</") {
                self.pos += 2;
                let name = self.name()?;
                self.skip_ws();
                if self.bytes().get(self.pos) != Some(&b'>') {
                    return Err(malformed(self.pos, "expected '>'"));
                }
                self.pos += 1;
                return Ok(Some(Event::End { name }));
            } else {
                self.pos += 1;
                let name = self.name()?;
                let mut attrs = Vec::new();
                loop {
                    self.skip_ws();
                    match self.bytes().get(self.pos) {
                        Some(b'>') => {
                            self.pos += 1;
                            return Ok(Some(Event::Start { name, attrs, self_closing: false }));
                        }
                        Some(b'/') => {
                            if self.bytes().get(self.pos + 1) != Some(&b'>') {
                                return Err(malformed(self.pos, "expected '/>'"));
                            }
                            self.pos += 2;
                            return Ok(Some(Event::Start { name, attrs, self_closing: true }));
                        }
                        None => return Err(malformed(self.pos, "unterminated tag")),
                        _ => {
                            let an = self.name()?;
                            self.skip_ws();
                            if self.bytes().get(self.pos) != Some(&b'=') {
                                return Err(malformed(self.pos, "expected '='"));
                            }
                            self.pos += 1;
                            self.skip_ws();
                            let quote = *self.bytes().get(self.pos).ok_or(malformed(self.pos, "expected a quote"))?;
                            if quote != b'"' && quote != b'\'' {
                                return Err(malformed(self.pos, "attribute value must be quoted"));
                            }
                            self.pos += 1;
                            let start = self.pos;
                            let len = self
                                .rest()
                                .find(char::from(quote))
                                .ok_or(malformed(start, "unterminated attribute value"))?;
                            let raw = self.s.get(start..start + len).unwrap_or("");
                            if raw.contains('<') {
                                return Err(malformed(start, "'<' in attribute value"));
                            }
                            self.pos = start + len + 1;
                            attrs.push(Attr { name: an, value: decode(raw, start)? });
                        }
                    }
                }
            }
        }
    }
}

/// Decodes the predefined entities and numeric character references; anything else is an error.
fn decode(raw: &str, at: usize) -> Result<String, ParseError> {
    if !raw.contains('&') {
        return Ok(raw.to_owned());
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(rest.get(..i).unwrap_or(""));
        let after = rest.get(i + 1..).unwrap_or("");
        let end = after.find(';').ok_or(malformed(at, "unterminated entity reference"))?;
        let name = after.get(..end).unwrap_or("");
        let ch = match name {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(hex) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse::<u32>().ok()
                } else {
                    return Err(malformed(at, "unknown entity (entities are never expanded)"));
                };
                code.and_then(char::from_u32).ok_or(malformed(at, "invalid character reference"))?
            }
        };
        out.push(ch);
        rest = after.get(end + 1..).unwrap_or("");
    }
    out.push_str(rest);
    Ok(out)
}
