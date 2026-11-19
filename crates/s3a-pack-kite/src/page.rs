//! `render_page` (§5.1, DD-58, DD-91, DD-77, DD-118, DD-14) and the page
//! parsers (step 8c).
//!
//! - Markdown is parsed with pulldown-cmark (DD-13) with default options
//!   (no tables — tables are AT-233 / Step 12). Raw HTML events are
//!   omitted (§5.5 "raw HTML disabled or escaped"); images render their
//!   alt text only and are never loaded (DD-77 / DD-118).
//! - HTML is parsed by a small static tag scanner in this module (no
//!   html5ever in the tree). `<script>`, `<style>`, `<title>`, and `<head>`
//!   contents are skipped entirely, comments are skipped, and no
//!   subresource of any kind is loaded (nothing is loaded by construction).
//! - F-14: a page whose HTML or Markdown exceeds the implementation
//!   nesting limit (128 levels) is a page error, `KiteError` code
//!   `nesting_too_deep`. The SPEC names no code for this error, so the code
//!   is implementation-named (logged in the step-8c test log).
//! - Whitespace inside a block is collapsed to single spaces (including
//!   inside `<pre>` — pre formatting is not preserved in 8c), so every
//!   parsed line is exactly one visual line.
//! - Non-UTF-8 page bytes are rendered lossy (`from_utf8_lossy`); the
//!   SPEC names no rule for `render_page` here.
//! - No Blitz layout is wired in this tree (DD-13 names Blitz); layout is
//!   one line per block, top-down, 20 px per line.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::open::Handle;
use crate::render::{finish_frame, pixmap_size, push_lines, push_run, RenderFrame, SemanticNode, TextRun};
use crate::types::Kind;
use crate::KiteError;

/// F-14 implementation nesting limit (SPEC: "no numeric nesting-depth
/// default is invented; the bound is an implementation limit"). Applies to
/// Markdown block nesting and HTML open-element depth.
pub(crate) const MAX_NESTING: usize = 128;

fn nesting_error() -> KiteError {
    KiteError::new(
        "nesting_too_deep",
        "page nesting exceeds the implementation limit (128)",
    )
}

/// One visual line of page text, with the semantic nodes that sit on it.
#[derive(Debug, Clone)]
pub(crate) struct PageLine {
    pub text: String,
    pub heading: bool,
    pub links: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PageContent {
    pub lines: Vec<PageLine>,
}

/// Strip a leading YAML front-matter block (`---` ... `---`, delimiter
/// lines) for display; its keys never appear as page text. The block is
/// not parsed (no YAML parser in the pack). No closing delimiter means
/// the text is not front matter and is returned whole.
pub(crate) fn strip_front_matter(text: &str) -> &str {
    let rest = match text.strip_prefix("---") {
        Some(r) => r,
        None => return text,
    };
    let rest = match rest.strip_prefix("\r\n").or_else(|| rest.strip_prefix('\n')) {
        Some(r) => r,
        None => return text,
    };
    let mut pos = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end_matches(['\n', '\r']) == "---" {
            return &rest[pos + line.len()..];
        }
        pos += line.len();
    }
    text
}

/// Parse Markdown into lines. Each block's inline text is coalesced into a
/// single run so headings are contiguous substrings (AT-30).
pub(crate) fn parse_markdown(text: &str) -> Result<PageContent, KiteError> {
    let text = strip_front_matter(text);
    let parser = Parser::new_ext(text, Options::empty());
    let mut content = PageContent::default();
    let mut buf = String::new();
    let mut block_heading = false;
    let mut block_links: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut depth: usize = 0;
    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { .. } | Tag::Paragraph | Tag::CodeBlock(_) => {
                    in_block = true;
                    block_heading = matches!(tag, Tag::Heading { .. });
                    buf.clear();
                    block_links.clear();
                }
                Tag::BlockQuote(_) | Tag::List(_) | Tag::Item => {
                    depth += 1;
                    if depth > MAX_NESTING {
                        return Err(nesting_error());
                    }
                }
                Tag::Link { dest_url, .. } => {
                    if in_block {
                        // AT-206 (bytes-opened handle): the href is the raw
                        // attribute with the fragment stripped; it is not
                        // resolved against a parent directory (there is none).
                        block_links.push(strip_fragment(&dest_url));
                    }
                }
                // Raw HTML, images, footnotes: the image's alt text flows
                // as Text events and is kept; the destination is never
                // loaded and never painted.
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(..) | TagEnd::Paragraph | TagEnd::CodeBlock => {
                    if in_block {
                        let text = std::mem::take(&mut buf);
                        if !text.trim().is_empty() {
                            content.lines.push(PageLine {
                                text,
                                heading: block_heading,
                                links: std::mem::take(&mut block_links),
                            });
                        }
                        in_block = false;
                    }
                }
                TagEnd::BlockQuote(..) | TagEnd::List(..) | TagEnd::Item => {
                    depth = depth.saturating_sub(1);
                }
                _ => {}
            },
            Event::Text(t) | Event::Code(t) => {
                if in_block {
                    buf.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_block {
                    buf.push(' ');
                }
            }
            // Html / InlineHtml: omitted (§5.5). FootnoteReference,
            // TaskListMarker, Rule: no text.
            _ => {}
        }
    }
    if in_block && !buf.trim().is_empty() {
        content.lines.push(PageLine {
            text: buf,
            heading: block_heading,
            links: block_links,
        });
    }
    Ok(content)
}

fn strip_fragment(href: &str) -> String {
    match href.find('#') {
        Some(i) => href[..i].to_string(),
        None => href.to_string(),
    }
}

fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
            | "source" | "track" | "wbr"
    )
}

fn is_block(name: &str) -> bool {
    matches!(
        name,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "li" | "div" | "pre" | "blockquote"
            | "ul" | "ol" | "section" | "article" | "header" | "footer" | "main" | "nav"
            | "table" | "tr" | "td" | "th" | "figure" | "figcaption"
    )
}

fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// Skip-set elements: their contents are never page text (DD-44/AT-33).
fn is_skip(name: &str) -> bool {
    matches!(name, "script" | "style" | "title" | "head")
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 1..];
        let Some(semi) = after.find(';') else {
            out.push('&');
            rest = after;
            break;
        };
        if semi > 10 {
            out.push('&');
            rest = after;
            continue;
        }
        let entity = &after[..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse runs of whitespace to single spaces (and trim).
fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_ws = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_ws {
                out.push(' ');
            }
            last_ws = true;
        } else {
            out.push(c);
            last_ws = false;
        }
    }
    out.trim_end().to_string()
}

/// Parse static HTML into lines. No script runs, no subresources load.
pub(crate) fn parse_html(text: &str) -> Result<PageContent, KiteError> {
    let mut content = PageContent::default();
    let mut buf = String::new();
    let mut block_links: Vec<String> = Vec::new();
    let mut block_heading = false;
    let mut in_block = false;
    let mut stack: Vec<String> = Vec::new();
    let mut skip_depth: Option<usize> = None;
    let flush = |content: &mut PageContent,
                     buf: &mut String,
                     block_links: &mut Vec<String>,
                     block_heading: &mut bool,
                     in_block: &mut bool| {
        if *in_block {
            let text = collapse_ws(buf);
            if !text.is_empty() {
                content.lines.push(PageLine {
                    text,
                    heading: *block_heading,
                    links: std::mem::take(block_links),
                });
            }
        }
        buf.clear();
        block_links.clear();
        *block_heading = false;
        *in_block = false;
    };
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if text[i..].starts_with("<!--") {
                match text[i..].find("-->") {
                    Some(end) => {
                        i += end + 3;
                        continue;
                    }
                    None => break,
                }
            }
            let Some(end) = text[i..].find('>') else {
                break;
            };
            let raw = text[i + 1..i + end].trim();
            i += end + 1;
            if raw.is_empty() || raw.starts_with('!') || raw.starts_with('?') {
                continue;
            }
            let closing = raw.starts_with('/');
            let name_raw = if closing { &raw[1..] } else { raw };
            let name_end = name_raw
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(name_raw.len());
            let name = name_raw[..name_end].to_ascii_lowercase();
            if name.is_empty() {
                continue;
            }
            if closing {
                if let Some(pos) = stack.iter().rposition(|n| n == &name) {
                    stack.truncate(pos);
                }
                if skip_depth.map_or(false, |d| stack.len() < d) {
                    skip_depth = None;
                }
                if is_block(&name) && skip_depth.is_none() {
                    flush(&mut content, &mut buf, &mut block_links, &mut block_heading, &mut in_block);
                }
                continue;
            }
            let self_closing = raw.ends_with('/') || is_void(&name);
            if is_skip(&name) {
                // Raw-text elements (script/style/title) and head: their
                // contents are never page text. For script/style/title,
                // jump to the close tag so `<` inside JS/CSS/text is not
                // misread as markup.
                if !self_closing {
                    stack.push(name.clone());
                    if stack.len() > MAX_NESTING {
                        return Err(nesting_error());
                    }
                    skip_depth = Some(stack.len());
                    if matches!(name.as_str(), "script" | "style" | "title") {
                        let close = format!("</{name}");
                        let lower = text[i..].to_ascii_lowercase();
                        if let Some(pos) = lower.find(&close) {
                            i += pos;
                        } else {
                            break;
                        }
                    }
                }
                continue;
            }
            if skip_depth.is_some() {
                if !self_closing {
                    stack.push(name);
                    if stack.len() > MAX_NESTING {
                        return Err(nesting_error());
                    }
                }
                continue;
            }
            if name == "br" || name == "hr" {
                flush(&mut content, &mut buf, &mut block_links, &mut block_heading, &mut in_block);
                continue;
            }
            if name == "img" {
                // Alt text only, never loaded (DD-77/DD-118).
                if let Some(alt) = attr_value(name_raw, "alt") {
                    let alt = collapse_ws(&decode_entities(&alt));
                    if !alt.is_empty() {
                        if !in_block {
                            in_block = true;
                        }
                        if !buf.is_empty() {
                            buf.push(' ');
                        }
                        buf.push_str(&alt);
                    }
                }
                continue;
            }
            if name == "a" {
                if let Some(href) = attr_value(name_raw, "href") {
                    if in_block {
                        block_links.push(strip_fragment(&href));
                    } else {
                        // A link outside any block starts an implicit block.
                        in_block = true;
                        block_links.push(strip_fragment(&href));
                    }
                }
                if !self_closing {
                    stack.push(name);
                    if stack.len() > MAX_NESTING {
                        return Err(nesting_error());
                    }
                }
                continue;
            }
            if is_block(&name) {
                flush(&mut content, &mut buf, &mut block_links, &mut block_heading, &mut in_block);
                in_block = true;
                block_heading = is_heading(&name);
                if !self_closing {
                    stack.push(name);
                    if stack.len() > MAX_NESTING {
                        return Err(nesting_error());
                    }
                }
                continue;
            }
            if !self_closing {
                stack.push(name);
                if stack.len() > MAX_NESTING {
                    return Err(nesting_error());
                }
            }
        } else {
            let next = text[i..].find('<').map(|j| i + j).unwrap_or(text.len());
            let chunk = &text[i..next];
            i = next;
            if skip_depth.is_some() {
                continue;
            }
            let decoded = decode_entities(chunk);
            if in_block {
                buf.push_str(&decoded);
            } else if !decoded.trim().is_empty() {
                in_block = true;
                block_heading = false;
                buf.push_str(decoded.trim_start());
            }
        }
    }
    flush(&mut content, &mut buf, &mut block_links, &mut block_heading, &mut in_block);
    Ok(content)
}

/// Read a quoted attribute value (`name="..."` or `name='...'`) from a tag's
/// raw attribute text. Unquoted values are not supported (fixtures use
/// quotes); returns None when absent.
fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut search_from = 0;
    loop {
        let pos = lower[search_from..].find(name)?;
        let start = search_from + pos;
        // The name must start at a boundary (space, tab, newline, or the
        // start of the attribute text).
        let boundary = start == 0
            || matches!(
                attrs.as_bytes().get(start - 1),
                Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r')
            );
        let after = &attrs[start + name.len()..];
        let after = after.trim_start();
        if boundary && after.starts_with('=') {
            let value = after[1..].trim_start();
            let quote = value.chars().next()?;
            if quote == '"' || quote == '\'' {
                let end = value[1..].find(quote)?;
                return Some(value[1..1 + end].to_string());
            }
            return None;
        }
        search_from = start + name.len();
    }
}

/// `render_page` (§5.1, DD-58): the wiki/HTML/`other` view. `scroll_y` is
/// applied to run positions; `select_enabled` paints the Select control
/// only when `paper_identity` is also present (DD-91, AT-144).
pub fn render_page(
    handle: &Handle,
    width: f64,
    height: f64,
    scroll_y: f64,
    device_pixel_ratio: f64,
    select_enabled: bool,
) -> Result<RenderFrame, KiteError> {
    match handle.kind_enum() {
        Kind::HyperTile | Kind::Edge | Kind::Unsupported => {
            return Err(KiteError::new(
                "view_mismatch",
                "render_page is not defined for an archive kind",
            ));
        }
        Kind::Wiki | Kind::Html | Kind::Other => {}
    }
    let (pix_w, pix_h) = pixmap_size(handle, width, height, device_pixel_ratio)?;
    let text = String::from_utf8_lossy(handle.bytes()).into_owned();
    let content = match handle.kind_enum() {
        Kind::Wiki => parse_markdown(&text)?,
        Kind::Html => parse_html(&text)?,
        Kind::Other => PageContent {
            lines: vec![PageLine {
                text: "no text conversion in archive".to_string(),
                heading: false,
                links: Vec::new(),
            }],
        },
        _ => unreachable!(),
    };
    let mut runs: Vec<TextRun> = Vec::new();
    let mut semantics: Vec<SemanticNode> = Vec::new();
    let mut y = 16i32 - scroll_y.round() as i32;
    for line in &content.lines {
        let line_y = y;
        let line_w = (line.text.len() as u32).saturating_mul(8).max(8);
        push_lines(&mut runs, &line.text, &mut y);
        if line.heading {
            semantics.push(SemanticNode {
                role: "heading".into(),
                label: line.text.clone(),
                x: 16,
                y: line_y,
                width: line_w,
                height: 20,
                href: None,
                paper_id: None,
                floor: None,
                opacity: None,
            });
        }
        for href in &line.links {
            semantics.push(SemanticNode {
                role: "link".into(),
                label: href.clone(),
                x: 16,
                y: line_y,
                width: line_w,
                height: 20,
                href: Some(href.clone()),
                paper_id: None,
                floor: None,
                opacity: None,
            });
        }
    }
    // Select control (AT-144): top-right, only when enabled AND a paper
    // identity exists. `open` of bytes yields paper_identity None, so the
    // control is never painted in 8c; the positive case needs open_path
    // (Step 10, AT-101).
    if select_enabled {
        if let Some(paper_id) = handle.paper_identity() {
            let sx = width as i32 - 136;
            semantics.push(SemanticNode {
                role: "select".into(),
                label: "Select".into(),
                x: sx,
                y: 16,
                width: 120,
                height: 32,
                href: None,
                paper_id: Some(paper_id.to_string()),
                floor: None,
                opacity: None,
            });
            push_run(&mut runs, "Select", sx, 16, 120, 32);
        }
    }
    let frame = finish_frame(pix_w, pix_h, runs, semantics, device_pixel_ratio);
    handle.store_frame("page", None, frame.clone());
    Ok(frame)
}
