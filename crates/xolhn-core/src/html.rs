use url::Url;

/// A styled inline run of text within a parsed Hacker News content block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub italic: bool,
    pub bold: bool,
    pub code: bool,
    pub link: Option<String>,
}

/// A block-level element extracted from Hacker News HTML content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentBlock {
    Paragraph(Vec<TextSpan>),
    Quote(Vec<TextSpan>),
    CodeBlock(String),
}

/// Validates and canonicalizes a URL, allowing only `http` and `https` schemes.
pub fn sanitize_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let parsed = Url::parse(trimmed).ok()?;
    match parsed.scheme() {
        "http" | "https" => Some(parsed.to_string()),
        _ => None,
    }
}

/// Parses the Hacker News HTML subset into styled content blocks without regex or panics.
pub fn parse_hn_html(html: &str) -> Vec<ContentBlock> {
    let mut parser = HtmlParser::new();
    parser.parse(html);
    parser.finish()
}

/// Extracts a single-line plain-text excerpt from HTML, truncated nicely at `max_chars` with `…`.
pub fn plain_text_excerpt(html: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let blocks = parse_hn_html(html);
    let mut combined = String::new();
    for block in blocks {
        match block {
            ContentBlock::Paragraph(spans) | ContentBlock::Quote(spans) => {
                for span in spans {
                    append_collapsed_whitespace(&mut combined, &span.text);
                }
            }
            ContentBlock::CodeBlock(code) => {
                append_collapsed_whitespace(&mut combined, &code);
            }
        }
        if !combined.is_empty() && !combined.ends_with(' ') {
            combined.push(' ');
        }
    }

    let collapsed = combined.trim();
    if collapsed.is_empty() {
        return String::new();
    }

    truncate_nicely(collapsed, max_chars)
}

fn append_collapsed_whitespace(out: &mut String, text: &str) {
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
}

fn truncate_nicely(text: &str, max_chars: usize) -> String {
    let total_chars = text.chars().count();
    if total_chars <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let budget = max_chars - 1;
    let cutoff_byte = text
        .char_indices()
        .nth(budget)
        .map_or(text.len(), |(idx, _)| idx);
    let prefix = &text[..cutoff_byte];

    let trimmed = if let Some(space_idx) = prefix.rfind(' ') {
        let chars_before_space = prefix[..space_idx].chars().count();
        if chars_before_space >= budget / 2 {
            prefix[..space_idx].trim_end()
        } else {
            prefix.trim_end()
        }
    } else {
        prefix.trim_end()
    };

    let mut result = String::with_capacity(trimmed.len() + '…'.len_utf8());
    result.push_str(trimmed);
    result.push('…');
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockMode {
    Paragraph,
    Quote,
    Pre,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InlineStyle {
    italic: bool,
    bold: bool,
    code: bool,
    link: Option<String>,
}

struct HtmlParser {
    blocks: Vec<ContentBlock>,
    mode: BlockMode,
    spans: Vec<TextSpan>,
    pre_buf: String,
    italic_depth: usize,
    bold_depth: usize,
    code_depth: usize,
    link_stack: Vec<Option<String>>,
}

impl HtmlParser {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            mode: BlockMode::Paragraph,
            spans: Vec::new(),
            pre_buf: String::new(),
            italic_depth: 0,
            bold_depth: 0,
            code_depth: 0,
            link_stack: Vec::new(),
        }
    }

    fn parse(&mut self, html: &str) {
        let bytes = html.as_bytes();
        let mut cursor = 0;

        while cursor < bytes.len() {
            let Some(rel_lt) = html[cursor..].find('<') else {
                self.push_text(&html[cursor..]);
                break;
            };

            let lt_idx = cursor + rel_lt;
            if lt_idx > cursor {
                self.push_text(&html[cursor..lt_idx]);
            }

            if !looks_like_tag_start(html, lt_idx) {
                self.push_text("<");
                cursor = lt_idx + 1;
                continue;
            }

            let Some(gt_idx) = find_tag_end(html, lt_idx) else {
                // Incomplete/unclosed tag at EOF: drop gracefully without panicking.
                break;
            };

            let raw_tag = &html[lt_idx + 1..gt_idx];
            self.handle_tag(raw_tag);
            cursor = gt_idx + 1;
        }
    }

    fn finish(mut self) -> Vec<ContentBlock> {
        match self.mode {
            BlockMode::Pre => self.flush_pre(),
            BlockMode::Paragraph | BlockMode::Quote => self.flush_block(),
        }
        self.blocks
    }

    fn current_style(&self) -> InlineStyle {
        InlineStyle {
            italic: self.italic_depth > 0,
            bold: self.bold_depth > 0,
            code: self.code_depth > 0,
            link: self.link_stack.last().cloned().flatten(),
        }
    }

    fn reset_inline(&mut self) {
        self.italic_depth = 0;
        self.bold_depth = 0;
        self.code_depth = 0;
        self.link_stack.clear();
    }

    fn push_text(&mut self, raw: &str) {
        let decoded = decode_entities(raw);
        if self.mode == BlockMode::Pre {
            self.pre_buf.push_str(&decoded);
            return;
        }

        let collapsed = collapse_ascii_whitespace(&decoded);
        let at_boundary = self
            .spans
            .last()
            .and_then(|span| span.text.chars().next_back())
            .is_none_or(|ch| ch == ' ' || ch == '\n');

        let slice = if at_boundary {
            collapsed.strip_prefix(' ').unwrap_or(&collapsed)
        } else {
            &collapsed
        };

        if !slice.is_empty() {
            let style = self.current_style();
            self.push_styled_text(slice, style);
        }
    }

    fn push_newline(&mut self) {
        if self.mode == BlockMode::Pre {
            self.pre_buf.push('\n');
            return;
        }
        let style = self.current_style();
        self.push_styled_text("\n", style);
    }

    fn push_styled_text(&mut self, text: &str, style: InlineStyle) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut()
            && last.italic == style.italic
            && last.bold == style.bold
            && last.code == style.code
            && last.link == style.link
        {
            last.text.push_str(text);
        } else {
            self.spans.push(TextSpan {
                text: text.to_string(),
                italic: style.italic,
                bold: style.bold,
                code: style.code,
                link: style.link,
            });
        }
    }

    fn handle_tag(&mut self, raw_tag: &str) {
        let trimmed = raw_tag.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') || trimmed.starts_with('?') {
            return;
        }

        let is_closing = trimmed.starts_with('/');
        let rest = if is_closing {
            trimmed[1..].trim_start()
        } else {
            trimmed
        };

        let name_end = rest
            .find(|ch: char| ch.is_ascii_whitespace() || ch == '/')
            .unwrap_or(rest.len());
        if name_end == 0 {
            return;
        }

        let tag_name = rest[..name_end].to_ascii_lowercase();
        let attrs = &rest[name_end..];

        if self.mode == BlockMode::Pre {
            if is_closing && tag_name == "pre" {
                self.flush_pre();
                self.mode = BlockMode::Paragraph;
                self.reset_inline();
            } else if !is_closing && tag_name == "br" {
                self.pre_buf.push('\n');
            }
            return;
        }

        match (is_closing, tag_name.as_str()) {
            (false, "p") => {
                if self.mode == BlockMode::Quote {
                    if !is_effectively_empty(&self.spans) {
                        let ends_with_nl = self
                            .spans
                            .last()
                            .is_some_and(|span| span.text.ends_with('\n'));
                        if !ends_with_nl {
                            self.push_newline();
                        }
                    }
                    self.reset_inline();
                } else {
                    self.flush_block();
                    self.reset_inline();
                }
            }
            (true, "p") => {
                if self.mode == BlockMode::Quote {
                    self.reset_inline();
                } else {
                    self.flush_block();
                    self.reset_inline();
                }
            }
            (false, "br") => {
                self.push_newline();
            }
            (false, "blockquote") => {
                self.flush_block();
                self.reset_inline();
                self.mode = BlockMode::Quote;
            }
            (true, "blockquote") => {
                if self.mode == BlockMode::Quote {
                    self.flush_block();
                    self.reset_inline();
                    self.mode = BlockMode::Paragraph;
                }
            }
            (false, "pre") => {
                self.flush_block();
                self.reset_inline();
                self.mode = BlockMode::Pre;
                self.pre_buf.clear();
            }
            (false, "i" | "em") => self.italic_depth += 1,
            (true, "i" | "em") => self.italic_depth = self.italic_depth.saturating_sub(1),
            (false, "b" | "strong") => self.bold_depth += 1,
            (true, "b" | "strong") => self.bold_depth = self.bold_depth.saturating_sub(1),
            (false, "code") => self.code_depth += 1,
            (true, "code") => self.code_depth = self.code_depth.saturating_sub(1),
            (false, "a") => {
                let href = extract_attr(attrs, "href").and_then(|raw| sanitize_url(&raw));
                self.link_stack.push(href);
            }
            (true, "a") => {
                self.link_stack.pop();
            }
            _ => {}
        }
    }

    fn flush_block(&mut self) {
        trim_trailing_spaces(&mut self.spans);
        if is_effectively_empty(&self.spans) {
            self.spans.clear();
            return;
        }

        let mut spans = std::mem::take(&mut self.spans);
        match self.mode {
            BlockMode::Quote => {
                self.blocks.push(ContentBlock::Quote(spans));
            }
            BlockMode::Paragraph => {
                if strip_leading_quote_marker(&mut spans) {
                    trim_trailing_spaces(&mut spans);
                    if !is_effectively_empty(&spans) {
                        self.blocks.push(ContentBlock::Quote(spans));
                    }
                } else {
                    self.blocks.push(ContentBlock::Paragraph(spans));
                }
            }
            BlockMode::Pre => {}
        }
    }

    fn flush_pre(&mut self) {
        let raw = std::mem::take(&mut self.pre_buf);
        if let Some(code) = normalize_code_block(&raw) {
            self.blocks.push(ContentBlock::CodeBlock(code));
        }
    }
}

fn looks_like_tag_start(html: &str, lt_idx: usize) -> bool {
    let Some(next) = html.as_bytes().get(lt_idx + 1).copied() else {
        return false;
    };
    next.is_ascii_alphabetic() || matches!(next, b'/' | b'!' | b'?')
}

fn find_tag_end(html: &str, lt_idx: usize) -> Option<usize> {
    if html[lt_idx..].starts_with("<!--") {
        let rel_end = html[lt_idx + 4..].find("-->")?;
        return Some(lt_idx + 4 + rel_end + 2);
    }

    let bytes = html.as_bytes();
    let next = *bytes.get(lt_idx + 1)?;
    if matches!(next, b'!' | b'?') {
        let rel_gt = html[lt_idx + 2..].find('>')?;
        return Some(lt_idx + 2 + rel_gt);
    }

    let mut pos = lt_idx + 1;
    let mut quote: Option<u8> = None;
    while pos < bytes.len() {
        let b = bytes[pos];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return Some(pos),
            None => {}
        }
        pos += 1;
    }
    None
}

fn extract_attr(attrs: &str, target: &str) -> Option<String> {
    let bytes = attrs.as_bytes();
    let mut pos = 0;

    while pos < bytes.len() {
        while pos < bytes.len() && (bytes[pos].is_ascii_whitespace() || bytes[pos] == b'/') {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        let name_start = pos;
        while pos < bytes.len()
            && !bytes[pos].is_ascii_whitespace()
            && bytes[pos] != b'='
            && bytes[pos] != b'/'
        {
            pos += 1;
        }
        let attr_name = &attrs[name_start..pos];

        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }

        let value = if pos < bytes.len() && bytes[pos] == b'=' {
            pos += 1;
            while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
                pos += 1;
            }
            if pos >= bytes.len() {
                ""
            } else if bytes[pos] == b'"' || bytes[pos] == b'\'' {
                let quote = bytes[pos];
                pos += 1;
                let val_start = pos;
                while pos < bytes.len() && bytes[pos] != quote {
                    pos += 1;
                }
                let slice = &attrs[val_start..pos];
                if pos < bytes.len() {
                    pos += 1;
                }
                slice
            } else {
                let val_start = pos;
                while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() && bytes[pos] != b'/' {
                    pos += 1;
                }
                &attrs[val_start..pos]
            }
        } else {
            ""
        };

        if attr_name.eq_ignore_ascii_case(target) {
            return Some(decode_entities(value));
        }
    }

    None
}

fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let decoded = if bytes[i] == b'&' {
            parse_entity_at(input, i)
        } else {
            None
        };
        if let Some((ch, end)) = decoded {
            out.push(ch);
            i = end;
            continue;
        }
        let ch = input[i..].chars().next().expect("i is within bounds");
        out.push(ch);
        i += ch.len_utf8();
    }

    out
}

fn parse_entity_at(input: &str, start: usize) -> Option<(char, usize)> {
    let bytes = input.as_bytes();
    let max_end = bytes.len().min(start + 16);
    let mut pos = start + 1;

    while pos < max_end {
        let b = bytes[pos];
        if b == b';' {
            let body = &input[start + 1..pos];
            let decoded = decode_entity_body(body)?;
            return Some((decoded, pos + 1));
        }
        if b == b'&' || b.is_ascii_whitespace() || b == b'<' {
            return None;
        }
        pos += 1;
    }

    None
}

fn decode_entity_body(body: &str) -> Option<char> {
    if let Some(hex) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        if hex.is_empty() {
            return None;
        }
        let code = u32::from_str_radix(hex, 16).ok()?;
        return valid_html_char(code);
    }

    if let Some(dec) = body.strip_prefix('#') {
        if dec.is_empty() {
            return None;
        }
        let code = dec.parse::<u32>().ok()?;
        return valid_html_char(code);
    }

    if body.eq_ignore_ascii_case("amp") {
        Some('&')
    } else if body.eq_ignore_ascii_case("lt") {
        Some('<')
    } else if body.eq_ignore_ascii_case("gt") {
        Some('>')
    } else if body.eq_ignore_ascii_case("quot") {
        Some('"')
    } else if body.eq_ignore_ascii_case("apos") {
        Some('\'')
    } else if body.eq_ignore_ascii_case("nbsp") {
        Some('\u{00A0}')
    } else {
        None
    }
}

fn valid_html_char(code: u32) -> Option<char> {
    if code == 0 {
        Some('\u{FFFD}')
    } else {
        char::from_u32(code)
    }
}

fn collapse_ascii_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = false;
    for ch in input.chars() {
        if ch.is_ascii_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

fn trim_trailing_spaces(spans: &mut Vec<TextSpan>) {
    while let Some(last) = spans.last_mut() {
        while last.text.ends_with(' ') {
            last.text.pop();
        }
        if last.text.is_empty() {
            spans.pop();
        } else {
            break;
        }
    }
}

fn is_effectively_empty(spans: &[TextSpan]) -> bool {
    spans
        .iter()
        .all(|span| span.text.chars().all(|ch| ch != '\n' && ch.is_whitespace()))
}

fn strip_leading_quote_marker(spans: &mut Vec<TextSpan>) -> bool {
    let Some(first) = spans.first_mut() else {
        return false;
    };

    if first.text.starts_with("> ") {
        first.text.drain(..2);
        if first.text.is_empty() {
            spans.remove(0);
        }
        return true;
    }

    if first.text == ">" && spans.get(1).is_some_and(|next| next.text.starts_with(' ')) {
        spans.remove(0);
        spans[0].text.drain(..1);
        if spans[0].text.is_empty() {
            spans.remove(0);
        }
        return true;
    }

    false
}

fn normalize_code_block(raw: &str) -> Option<String> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let start = lines.iter().position(|line| !line.trim().is_empty())?;
    let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
    Some(lines[start..=end].join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_span(text: &str) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            italic: false,
            bold: false,
            code: false,
            link: None,
        }
    }

    #[test]
    fn parses_paragraphs_and_hn_p_separators() {
        let html = "First paragraph<p>Second paragraph<p></p><p>   </p><p>Third paragraph</p>";
        assert_eq!(
            parse_hn_html(html),
            vec![
                ContentBlock::Paragraph(vec![plain_span("First paragraph")]),
                ContentBlock::Paragraph(vec![plain_span("Second paragraph")]),
                ContentBlock::Paragraph(vec![plain_span("Third paragraph")]),
            ]
        );
    }

    #[test]
    fn parses_algolia_entity_encoded_links_single_and_double_quotes() {
        let html = r#"See <a href="https:&#x2F;&#x2F;news.ycombinator.com&#x2F;item?id=123&amp;p=2" rel="nofollow">HN&#x27;s thread</a> and <a href='http://example.com/a&#x2F;b'>mirror</a>."#;
        assert_eq!(
            parse_hn_html(html),
            vec![ContentBlock::Paragraph(vec![
                plain_span("See "),
                TextSpan {
                    text: "HN's thread".to_string(),
                    italic: false,
                    bold: false,
                    code: false,
                    link: Some("https://news.ycombinator.com/item?id=123&p=2".to_string()),
                },
                plain_span(" and "),
                TextSpan {
                    text: "mirror".to_string(),
                    italic: false,
                    bold: false,
                    code: false,
                    link: Some("http://example.com/a/b".to_string()),
                },
                plain_span("."),
            ])]
        );
    }

    #[test]
    fn rejects_unsafe_url_schemes_in_sanitize_url_and_links() {
        assert_eq!(sanitize_url("javascript:alert(1)"), None);
        assert_eq!(sanitize_url("  JAVASCRIPT:alert(1) "), None);
        assert_eq!(
            sanitize_url("data:text/html,<script>alert(1)</script>"),
            None
        );
        assert_eq!(sanitize_url("file:///etc/passwd"), None);
        assert_eq!(sanitize_url(""), None);
        assert_eq!(sanitize_url("not a url"), None);
        assert_eq!(
            sanitize_url("  https://example.com/foo?a=1  "),
            Some("https://example.com/foo?a=1".to_string())
        );
        assert_eq!(
            sanitize_url("http://example.org/bar"),
            Some("http://example.org/bar".to_string())
        );

        let html = r#"Do not <a href="javascript:alert(1)">click here</a> please."#;
        assert_eq!(
            parse_hn_html(html),
            vec![ContentBlock::Paragraph(vec![plain_span(
                "Do not click here please."
            )])]
        );
    }

    #[test]
    fn parses_inline_formatting_and_nested_tags() {
        let html = "<i>italic <b>bold-italic</b> <code>code-italic</code></i> <em>em</em> <strong>strong</strong>";
        assert_eq!(
            parse_hn_html(html),
            vec![ContentBlock::Paragraph(vec![
                TextSpan {
                    text: "italic ".to_string(),
                    italic: true,
                    bold: false,
                    code: false,
                    link: None,
                },
                TextSpan {
                    text: "bold-italic".to_string(),
                    italic: true,
                    bold: true,
                    code: false,
                    link: None,
                },
                TextSpan {
                    text: " ".to_string(),
                    italic: true,
                    bold: false,
                    code: false,
                    link: None,
                },
                TextSpan {
                    text: "code-italic".to_string(),
                    italic: true,
                    bold: false,
                    code: true,
                    link: None,
                },
                plain_span(" "),
                TextSpan {
                    text: "em".to_string(),
                    italic: true,
                    bold: false,
                    code: false,
                    link: None,
                },
                plain_span(" "),
                TextSpan {
                    text: "strong".to_string(),
                    italic: false,
                    bold: true,
                    code: false,
                    link: None,
                },
            ])]
        );
    }

    #[test]
    fn merges_adjacent_identical_spans() {
        let html = "<i>one</i><i>two</i> <span>three</span> four";
        assert_eq!(
            parse_hn_html(html),
            vec![ContentBlock::Paragraph(vec![
                TextSpan {
                    text: "onetwo".to_string(),
                    italic: true,
                    bold: false,
                    code: false,
                    link: None,
                },
                plain_span(" three four"),
            ])]
        );
    }

    #[test]
    fn parses_pre_code_blocks_and_trims_blank_lines() {
        let html = "<p>Example:</p><pre><code>\n\n  fn main() {\n      println!(\"&lt;hi&gt;\");\n  }\n\n</code></pre><pre>\nplain\n</pre>";
        assert_eq!(
            parse_hn_html(html),
            vec![
                ContentBlock::Paragraph(vec![plain_span("Example:")]),
                ContentBlock::CodeBlock(
                    "  fn main() {\n      println!(\"<hi>\");\n  }".to_string()
                ),
                ContentBlock::CodeBlock("plain".to_string()),
            ]
        );
    }

    #[test]
    fn parses_blockquote_and_gt_quote_paragraphs() {
        let html = "<blockquote>Quoted <i>wisdom</i></blockquote><p>&gt; Reply <b>quote</b></p><p>&gt;50% faster</p>";
        assert_eq!(
            parse_hn_html(html),
            vec![
                ContentBlock::Quote(vec![
                    plain_span("Quoted "),
                    TextSpan {
                        text: "wisdom".to_string(),
                        italic: true,
                        bold: false,
                        code: false,
                        link: None,
                    },
                ]),
                ContentBlock::Quote(vec![
                    plain_span("Reply "),
                    TextSpan {
                        text: "quote".to_string(),
                        italic: false,
                        bold: true,
                        code: false,
                        link: None,
                    },
                ]),
                ContentBlock::Paragraph(vec![plain_span(">50% faster")]),
            ]
        );
    }

    #[test]
    fn parses_br_variants_in_paragraph_and_quote() {
        let html = "Line 1<br>Line 2<br/>Line 3<br />\nLine 4<blockquote>Q1<br />Q2</blockquote>";
        assert_eq!(
            parse_hn_html(html),
            vec![
                ContentBlock::Paragraph(vec![plain_span("Line 1\nLine 2\nLine 3\nLine 4")]),
                ContentBlock::Quote(vec![plain_span("Q1\nQ2")]),
            ]
        );
    }

    #[test]
    fn decodes_html_entities_safely() {
        let html = "AT&T &amp; &lt;tag&gt; &quot;hi&quot; &apos;ok&apos; &#39;39&#39; &#x27;x27&#x27; a&#x2F;b c&nbsp;d &#99999999;";
        assert_eq!(
            parse_hn_html(html),
            vec![ContentBlock::Paragraph(vec![plain_span(
                "AT&T & <tag> \"hi\" 'ok' '39' 'x27' a/b c\u{00A0}d &#99999999;"
            )])]
        );
    }

    #[test]
    fn handles_malformed_and_unknown_tags_without_panicking() {
        let html = "<p><i>unclosed italic</p></b></a><p>3 < 5 and <unknown attr=\"a > b\">ok</unknown></p><pre><code>unclosed pre\nline 2<bad";
        assert_eq!(
            parse_hn_html(html),
            vec![
                ContentBlock::Paragraph(vec![TextSpan {
                    text: "unclosed italic".to_string(),
                    italic: true,
                    bold: false,
                    code: false,
                    link: None,
                }]),
                ContentBlock::Paragraph(vec![plain_span("3 < 5 and ok")]),
                ContentBlock::CodeBlock("unclosed pre\nline 2".to_string()),
            ]
        );
    }

    #[test]
    fn plain_text_excerpt_collapses_and_truncates() {
        let html =
            "<p>Hello   <b>world</b>!</p>\n<p>This is a &quot;test&quot; &amp; more text.</p>";
        assert_eq!(
            plain_text_excerpt(html, 100),
            "Hello world! This is a \"test\" & more text."
        );
        assert_eq!(plain_text_excerpt(html, 15), "Hello world!…");
        assert_eq!(plain_text_excerpt("Supercalifragilistic", 10), "Supercali…");
        assert_eq!(plain_text_excerpt(html, 1), "…");
        assert_eq!(plain_text_excerpt(html, 0), "");
    }

    #[test]
    fn handles_empty_and_comment_only_input() {
        assert_eq!(parse_hn_html(""), Vec::<ContentBlock>::new());
        assert_eq!(
            parse_hn_html("   \n\t  <!-- comment -->  "),
            Vec::<ContentBlock>::new()
        );
    }
}
