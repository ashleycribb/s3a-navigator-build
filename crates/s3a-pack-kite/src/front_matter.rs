//! Minimal YAML front-matter reader for Scholar dossiers (SPEC §5.5, AT-188).
//!
//! Only single-line `key: value` scalars are accepted. The parser rejects
//! aliases and anchors (no alias expansion / "billion laughs"), block
//! scalars, flow collections, and anything deeper than the 128-level nesting
//! limit (F-14). A dossier whose front matter is missing, unparseable, or
//! not UTF-8 is omitted from the candidate list; this parser returns `None`
//! for all of those cases. It does not invent a numeric alias budget and
//! does not parse full YAML.

/// The four front-matter fields the host reads (SPEC §5.5 item 1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub source_url: Option<String>,
    pub drive_path: Option<String>,
    pub source_id: Option<String>,
}

/// Nesting limit for front-matter lines (F-14). Depth is leading-space pairs
/// plus one for a sequence marker (`- `).
pub const FRONT_MATTER_NESTING_LIMIT: usize = 128;

/// Parse front matter from raw bytes. `None` when the bytes are not valid
/// UTF-8 or the front matter is missing/unparseable.
pub fn parse_front_matter_bytes(bytes: &[u8]) -> Option<FrontMatter> {
    let text = std::str::from_utf8(bytes).ok()?;
    parse_front_matter(text)
}

/// Parse front matter from Markdown text. The text must start with a `---`
/// line, followed by `key: value` lines, closed by a `---` line. Anything
/// after the closing marker (the body) is ignored.
pub fn parse_front_matter(text: &str) -> Option<FrontMatter> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut fm = FrontMatter::default();
    for line in lines.by_ref() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.trim_end() == "---" {
            return Some(fm);
        }
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Nesting limit (F-14): leading-space pairs plus a sequence marker.
        let indent = line.len() - trimmed.len();
        let mut depth = indent / 2;
        if trimmed == "-" || trimmed.starts_with("- ") {
            depth += 1;
        }
        if depth > FRONT_MATTER_NESTING_LIMIT {
            return None;
        }
        let (key, value) = parse_mapping_line(trimmed)?;
        match key {
            "title" => fm.title = value,
            "source_url" => fm.source_url = value,
            "drive_path" => fm.drive_path = value,
            "source_id" => fm.source_id = value,
            _ => {}
        }
    }
    // EOF without a closing marker: unparseable.
    None
}

/// Parse one `key: value` line. Returns the key and the scalar value
/// (`None` for an empty / YAML-null value). `None` means the line is not a
/// valid single-line mapping entry.
fn parse_mapping_line(line: &str) -> Option<(&str, Option<String>)> {
    let colon = line.find(':')?;
    let key = &line[..colon];
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let rest = &line[colon + 1..];
    if rest.is_empty() {
        return Some((key, None));
    }
    // YAML requires a space after the colon for a mapping value; `key:value`
    // is a single plain scalar, not a mapping.
    let value = rest.strip_prefix(' ')?;
    let value = value.trim_end();
    if value.is_empty() {
        return Some((key, None));
    }
    if value == "~" || value == "null" || value == "Null" || value == "NULL" {
        return Some((key, None));
    }
    let scalar = parse_scalar(value)?;
    Some((key, Some(scalar)))
}

/// Parse a single-line scalar: single-quoted, double-quoted, or plain.
/// Aliases (`*x`), anchors (`&x`), block scalars (`|`, `>`), and flow
/// collections (`[`, `{`) are rejected as unparseable.
fn parse_scalar(value: &str) -> Option<String> {
    if let Some(first) = value.chars().next() {
        match first {
            '&' | '*' | '|' | '>' | '[' | '{' => return None,
            '\'' => return parse_single_quoted(value),
            '"' => return parse_double_quoted(value),
            _ => {}
        }
    }
    // Plain scalar: no `: ` (that would be a nested mapping), no anchor or
    // alias token, no comment-introducing ` #` kept in the value.
    if value.contains(": ") {
        return None;
    }
    for token in value.split_whitespace() {
        if (token.starts_with('&') || token.starts_with('*')) && token.len() > 1 {
            return None;
        }
    }
    let plain = match value.find(" #") {
        Some(idx) => &value[..idx],
        None => value,
    };
    let plain = plain.trim_end();
    if plain.is_empty() {
        return None;
    }
    Some(plain.to_string())
}

fn parse_single_quoted(value: &str) -> Option<String> {
    let inner = value.strip_prefix('\'')?;
    let inner = inner.strip_suffix('\'')?;
    // A single-quoted scalar must close on this line; an interior `''` is
    // an escaped quote. An odd trailing quote count means unclosed.
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next();
                out.push('\'');
            } else {
                return None;
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

fn parse_double_quoted(value: &str) -> Option<String> {
    let inner = value.strip_prefix('"')?;
    let inner = inner.strip_suffix('"')?;
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                _ => return None,
            }
        } else if c == '"' {
            return None;
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_fields() {
        let text = "---\ntitle: A Paper\nsource_url: https://example.org/p.pdf\ndrive_path: /x/y.pdf\nsource_id: SRC-0007\n---\n# Body\n";
        let fm = parse_front_matter(text).expect("parseable");
        assert_eq!(fm.title.as_deref(), Some("A Paper"));
        assert_eq!(fm.source_url.as_deref(), Some("https://example.org/p.pdf"));
        assert_eq!(fm.drive_path.as_deref(), Some("/x/y.pdf"));
        assert_eq!(fm.source_id.as_deref(), Some("SRC-0007"));
    }

    #[test]
    fn missing_front_matter_is_none() {
        assert!(parse_front_matter("# No front matter\n").is_none());
        assert!(parse_front_matter("").is_none());
    }

    #[test]
    fn unclosed_front_matter_is_none() {
        assert!(parse_front_matter("---\ntitle: x\n").is_none());
    }

    #[test]
    fn empty_front_matter_is_some_empty() {
        let fm = parse_front_matter("---\n---\nbody\n").expect("empty block parses");
        assert_eq!(fm, FrontMatter::default());
    }

    #[test]
    fn rejects_alias_and_anchor() {
        assert!(parse_front_matter("---\ntitle: *x\n---\n").is_none());
        assert!(parse_front_matter("---\ntitle: &a hello\n---\n").is_none());
        assert!(parse_front_matter("---\na: &anchor\n  b: c\n---\n").is_none());
        // A `&` inside a URL query string is not an anchor token.
        let fm = parse_front_matter("---\nsource_url: https://x.org/?a=1&b=2\n---\n")
            .expect("URL with & parses");
        assert_eq!(
            fm.source_url.as_deref(),
            Some("https://x.org/?a=1&b=2")
        );
    }

    #[test]
    fn rejects_block_scalar_and_flow() {
        assert!(parse_front_matter("---\ntitle: |\n  multi\n---\n").is_none());
        assert!(parse_front_matter("---\ntitle: [a, b]\n---\n").is_none());
    }

    #[test]
    fn rejects_deep_nesting() {
        let mut text = String::from("---\n");
        for _ in 0..300 {
            text.push_str("  ");
        }
        text.push_str("title: deep\n---\n");
        assert!(parse_front_matter(&text).is_none());
    }

    #[test]
    fn quoted_scalars() {
        let fm = parse_front_matter("---\ntitle: 'It''s'\nsource_url: \"https://x.org/a\\\"b\"\n---\n")
            .expect("quoted parses");
        assert_eq!(fm.title.as_deref(), Some("It's"));
        assert_eq!(fm.source_url.as_deref(), Some("https://x.org/a\"b"));
    }

    #[test]
    fn non_utf8_bytes_are_none() {
        assert!(parse_front_matter_bytes(b"---\ntitle: \xff\n---\n").is_none());
    }

    #[test]
    fn line_without_colon_is_unparseable() {
        assert!(parse_front_matter("---\njust a line\n---\n").is_none());
    }

    #[test]
    fn key_colon_without_space_is_unparseable() {
        assert!(parse_front_matter("---\ntitle:x\n---\n").is_none());
    }
}
