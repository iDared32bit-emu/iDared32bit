/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Lays out the simple HTML pages games show in a `UIWebView` (help, about,
//! credits). Not a browser: it knows text, paragraphs, headings, line breaks,
//! lists, `<pre>`, `<hr>`, `<font>`, bold, italic, underline, alignment,
//! `bgcolor` and links. CSS, images and scripts are ignored (and logged), and
//! tables' text flows as plain blocks.

use crate::frameworks::uikit::ui_font::FontFamily;

pub type Rgba = (f32, f32, f32, f32);

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Style {
    pub size: f32,
    pub family: FontFamily,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Rgba,
    /// The `href` of the link this text is in.
    pub link: Option<String>,
    align: Align,
    indent: f32,
    pre: bool,
}

/// A run of text in one style, at its top-left corner.
#[derive(Clone, PartialEq, Debug)]
pub struct Fragment {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub style: Style,
}

#[derive(Debug, Default)]
pub struct Page {
    /// `<body bgcolor>`, if set.
    pub background: Option<Rgba>,
    pub fragments: Vec<Fragment>,
    /// `<hr>`s, as `(x, y, width)`.
    pub rules: Vec<(f32, f32, f32)>,
    pub height: f32,
    /// Things the page has that aren't shown, for logging.
    pub unsupported: Vec<&'static str>,
}

/// Text measurement, from the fonts that will draw it.
pub trait Measure {
    fn width(&mut self, style: &Style, text: &str) -> f32;
    /// The ascent and line height.
    fn metrics(&mut self, style: &Style) -> (f32, f32);
}

/// WebKit's default page margin.
const MARGIN: f32 = 8.0;
/// `<font size>` 1 to 7, in pixels (points on iPhone OS).
const FONT_SIZES: [f32; 7] = [10.0, 13.0, 16.0, 18.0, 24.0, 32.0, 48.0];
/// `<h1>` to `<h6>`: size and margin (in ems).
const HEADINGS: [(f32, f32); 6] = [
    (32.0, 0.67),
    (24.0, 0.83),
    (18.72, 1.0),
    (16.0, 1.33),
    (13.28, 1.67),
    (10.72, 2.33),
];

pub fn layout(html: &str, width: f32, measure: &mut dyn Measure) -> Page {
    let mut layout = Layout {
        measure,
        width,
        page: Page::default(),
        base: Style {
            size: 16.0,
            family: FontFamily::Serif,
            bold: false,
            italic: false,
            underline: false,
            color: (0.0, 0.0, 0.0, 1.0),
            link: None,
            align: Align::Left,
            indent: 0.0,
            pre: false,
        },
        link_color: (0.0, 0.0, 238.0 / 255.0, 1.0),
        stack: Vec::new(),
        lists: Vec::new(),
        y: 0.0,
        margin: MARGIN,
        line: Vec::new(),
        line_x: 0.0,
        line_start: None,
        space: false,
        pre_start: false,
    };
    for token in tokenize(html) {
        match token {
            Token::Open(name, attrs) => layout.open(&name, &attrs),
            Token::Close(name) => layout.close(&name),
            Token::Text(text) => layout.text(&text),
        }
    }
    layout.finish_line();
    layout.page.height = layout.y + layout.margin.max(MARGIN);
    layout.page
}

struct Layout<'a> {
    measure: &'a mut dyn Measure,
    width: f32,
    page: Page,
    /// The page's style, from `<body>`.
    base: Style,
    link_color: Rgba,
    /// Open elements and their styles.
    stack: Vec<(String, Style)>,
    /// Open lists: the next number for `<ol>`, `None` for `<ul>`.
    lists: Vec<Option<u32>>,
    /// The top of the next line.
    y: f32,
    /// The collapsed vertical margin before the next line.
    margin: f32,
    line: Vec<Fragment>,
    line_x: f32,
    /// The alignment and indent of the line being filled.
    line_start: Option<(Align, f32)>,
    /// Whether a space comes before the next word.
    space: bool,
    /// A newline right after `<pre>` isn't shown.
    pre_start: bool,
}

impl Layout<'_> {
    fn style(&self) -> &Style {
        self.stack.last().map_or(&self.base, |(_, style)| style)
    }

    fn available(&self, indent: f32) -> f32 {
        (self.width - 2.0 * MARGIN - indent).max(1.0)
    }

    fn push_text(&mut self, text: &str, style: &Style, break_before: bool) {
        let width = self.measure.width(style, text);
        let mut space = if self.space && !self.line.is_empty() {
            self.measure.width(style, " ")
        } else {
            0.0
        };
        self.space = false;
        let available = self.available(style.indent);
        if break_before && !self.line.is_empty() && self.line_x + space + width > available {
            self.finish_line();
            space = 0.0;
        }
        self.line_start.get_or_insert((style.align, style.indent));
        let x = self.line_x + space;
        self.line_x = x + width;
        if let Some(last) = self.line.last_mut() {
            if last.style == *style && last.x + last.width == x - space {
                if space > 0.0 {
                    last.text.push(' ');
                }
                last.text.push_str(text);
                last.width = self.line_x - last.x;
                return;
            }
        }
        self.line.push(Fragment {
            x,
            y: 0.0,
            width,
            height: 0.0,
            text: text.to_string(),
            style: style.clone(),
        });
    }

    fn finish_line(&mut self) {
        let Some((align, indent)) = self.line_start.take() else {
            return;
        };
        let mut ascent: f32 = 0.0;
        let mut descent: f32 = 0.0;
        let mut metrics = Vec::new();
        for fragment in &self.line {
            let (a, height) = self.measure.metrics(&fragment.style);
            ascent = ascent.max(a);
            descent = descent.max(height - a);
            metrics.push((a, height));
        }
        self.y += self.margin;
        self.margin = 0.0;
        let free = self.available(indent) - self.line_x;
        let offset = MARGIN
            + indent
            + match align {
                Align::Left => 0.0,
                Align::Center => (free / 2.0).max(0.0),
                Align::Right => free.max(0.0),
            };
        for (mut fragment, (a, height)) in self.line.drain(..).zip(metrics) {
            fragment.x += offset;
            fragment.y = self.y + (ascent - a);
            fragment.height = height;
            self.page.fragments.push(fragment);
        }
        self.y += ascent + descent;
        self.line_x = 0.0;
        self.space = false;
    }

    fn line_break(&mut self) {
        if self.line.is_empty() {
            let (_, height) = self.measure.metrics(&self.style().clone());
            self.y += self.margin + height;
            self.margin = 0.0;
        } else {
            self.finish_line();
        }
    }

    fn block(&mut self, margin: f32) {
        self.finish_line();
        self.margin = self.margin.max(margin);
        self.space = false;
    }

    fn text(&mut self, text: &str) {
        let style = self.style().clone();
        if style.pre {
            let text = text.replace('\t', "        ");
            let mut text = text.as_str();
            if std::mem::take(&mut self.pre_start) {
                text = text.strip_prefix('\n').unwrap_or(text);
            }
            for (i, line) in text.split('\n').enumerate() {
                if i > 0 {
                    self.line_break();
                }
                if !line.is_empty() {
                    self.push_text(line, &style, false);
                }
            }
            return;
        }
        let is_space = |c: char| c.is_ascii_whitespace();
        if text.starts_with(is_space) {
            self.space = true;
        }
        for (i, word) in text.split(is_space).filter(|w| !w.is_empty()).enumerate() {
            if i > 0 {
                self.space = true;
            }
            self.push_text(word, &style, true);
        }
        if text.ends_with(is_space) {
            self.space = true;
        }
    }

    fn open(&mut self, name: &str, attrs: &[(String, String)]) {
        let attr = |key: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        // Elements that can't contain these end where they start.
        let ends_p = matches!(
            name,
            "p" | "div"
                | "ul"
                | "ol"
                | "pre"
                | "table"
                | "hr"
                | "blockquote"
                | "center"
                | "h1"
                | "h2"
                | "h3"
                | "h4"
                | "h5"
                | "h6"
        );
        if (ends_p && self.stack.last().is_some_and(|(n, _)| n == "p"))
            || (name == "li" && self.stack.last().is_some_and(|(n, _)| n == "li"))
        {
            let open = self.stack.last().unwrap().0.clone();
            self.close(&open);
        }

        let mut style = self.style().clone();
        match name {
            "br" => return self.line_break(),
            "hr" => {
                self.block(style.size / 2.0);
                self.y += self.margin;
                let width = self.available(style.indent);
                self.page.rules.push((MARGIN + style.indent, self.y, width));
                self.y += 2.0;
                self.margin = style.size / 2.0;
                return;
            }
            "img" => return self.unsupported("images"),
            "meta" | "link" | "base" | "input" | "col" | "area" | "param" | "wbr" => return,
            "body" => {
                if let Some(color) = attr("bgcolor").and_then(parse_color) {
                    self.page.background = Some(color);
                }
                if let Some(color) = attr("text").and_then(parse_color) {
                    self.base.color = color;
                }
                if let Some(color) = attr("link").and_then(parse_color) {
                    self.link_color = color;
                }
                return;
            }
            "font" => {
                if let Some(size) = attr("size") {
                    style.size = font_size(size, style.size);
                }
                if let Some(color) = attr("color").and_then(parse_color) {
                    style.color = color;
                }
                if let Some(face) = attr("face") {
                    style.family = font_family(face);
                }
            }
            "b" | "strong" | "th" => style.bold = true,
            "i" | "em" | "cite" | "var" | "dfn" | "address" => style.italic = true,
            "u" | "ins" => style.underline = true,
            "code" | "tt" | "kbd" | "samp" => style.family = FontFamily::Mono,
            "big" => style.size *= 1.2,
            "small" | "sub" | "sup" => style.size /= 1.2,
            "a" => {
                if let Some(href) = attr("href") {
                    style.link = Some(href.to_string());
                    style.underline = true;
                    style.color = self.link_color;
                }
            }
            "p" => self.block(style.size),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let (size, margin) = HEADINGS[usize::from(name.as_bytes()[1] - b'1')];
                style.size = size;
                style.bold = true;
                self.block(size * margin);
            }
            "pre" => {
                style.family = FontFamily::Mono;
                style.size = 13.0;
                style.pre = true;
                self.pre_start = true;
                self.block(style.size);
            }
            "ul" | "ol" => {
                self.block(if self.lists.is_empty() {
                    style.size
                } else {
                    0.0
                });
                self.lists.push((name == "ol").then_some(1));
                style.indent += 40.0;
            }
            "li" => {
                self.block(0.0);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        *n += 1;
                        format!("{}.", *n - 1)
                    }
                    _ => "•".to_string(),
                };
                let mut marker_style = style.clone();
                marker_style.link = None;
                marker_style.underline = false;
                let width = self.measure.width(&marker_style, &marker);
                self.line_start = Some((style.align, style.indent));
                self.line.push(Fragment {
                    x: -width - 6.0,
                    y: 0.0,
                    width,
                    height: 0.0,
                    text: marker,
                    style: marker_style,
                });
            }
            "blockquote" | "dd" => {
                self.block(if name == "dd" { 0.0 } else { style.size });
                style.indent += 40.0;
            }
            "div" | "center" | "dl" | "dt" | "tr" | "form" => self.block(0.0),
            "table" => {
                self.unsupported("tables");
                self.block(0.0);
            }
            "td" => self.space = true,
            _ => (),
        }
        if name == "center" {
            style.align = Align::Center;
        }
        if let Some(align) = attr("align") {
            style.align = match align.to_ascii_lowercase().as_str() {
                "center" => Align::Center,
                "right" => Align::Right,
                _ => Align::Left,
            };
        }
        if attr("style").is_some() {
            self.unsupported("CSS");
        }
        self.stack.push((name.to_string(), style));
    }

    fn close(&mut self, name: &str) {
        if name == "br" {
            return self.line_break();
        }
        let Some(i) = self.stack.iter().rposition(|(n, _)| n == name) else {
            return;
        };
        let style = self.stack[i].1.clone();
        self.stack.truncate(i);
        match name {
            "p" | "pre" | "blockquote" => self.block(style.size),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let (size, margin) = HEADINGS[usize::from(name.as_bytes()[1] - b'1')];
                self.block(size * margin);
            }
            "ul" | "ol" => {
                self.lists.pop();
                self.block(if self.lists.is_empty() {
                    style.size
                } else {
                    0.0
                });
            }
            "div" | "center" | "li" | "dl" | "dt" | "dd" | "tr" | "table" | "form" => {
                self.block(0.0)
            }
            "td" | "th" => self.space = true,
            _ => (),
        }
    }

    fn unsupported(&mut self, what: &'static str) {
        if !self.page.unsupported.contains(&what) {
            self.page.unsupported.push(what);
        }
    }
}

/// `<font size>`: 1 to 7, or relative to 3 with `+` or `-`.
fn font_size(size: &str, current: f32) -> f32 {
    let size = size.trim();
    let Ok(n) = size.parse::<i32>() else {
        return current;
    };
    let n = if size.starts_with(['+', '-']) {
        3 + n
    } else {
        n
    };
    FONT_SIZES[n.clamp(1, 7) as usize - 1]
}

/// The first family in a `face` list that's recognised.
fn font_family(face: &str) -> FontFamily {
    for name in face.split(',') {
        let name = name.trim().to_ascii_lowercase();
        if name.contains("courier") || name.contains("mono") {
            return FontFamily::Mono;
        }
        if name.contains("times") || name.contains("georgia") || name == "serif" {
            return FontFamily::Serif;
        }
        if !name.is_empty() {
            return FontFamily::Sans;
        }
    }
    FontFamily::Sans
}

fn parse_color(color: &str) -> Option<Rgba> {
    let color = color.trim().to_ascii_lowercase();
    let hex = match color.as_str() {
        "black" => "000000",
        "white" => "ffffff",
        "red" => "ff0000",
        "green" => "008000",
        "blue" => "0000ff",
        "yellow" => "ffff00",
        "gray" | "grey" => "808080",
        "silver" => "c0c0c0",
        "maroon" => "800000",
        "purple" => "800080",
        "fuchsia" | "magenta" => "ff00ff",
        "lime" => "00ff00",
        "olive" => "808000",
        "navy" => "000080",
        "teal" => "008080",
        "aqua" | "cyan" => "00ffff",
        "orange" => "ffa500",
        other => other.strip_prefix('#').unwrap_or(other),
    };
    let hex = if hex.len() == 3 {
        hex.chars().flat_map(|c| [c, c]).collect()
    } else {
        hex.to_string()
    };
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(&hex, 16).ok()?;
    let channel = |shift: u32| ((value >> shift) & 0xff) as f32 / 255.0;
    Some((channel(16), channel(8), channel(0), 1.0))
}

#[derive(Debug, PartialEq)]
enum Token {
    Open(String, Vec<(String, String)>),
    Close(String),
    Text(String),
}

fn tokenize(html: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |i| &after[i + 3..]);
            continue;
        }
        let is_tag = rest.starts_with('<')
            && rest[1..]
                .starts_with(|c: char| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?'));
        if !is_tag {
            // A '<' that doesn't start a tag is text.
            let end = rest[1..].find('<').map_or(rest.len(), |i| i + 1);
            tokens.push(Token::Text(decode_entities(&rest[..end])));
            rest = &rest[end..];
            continue;
        }
        let end = tag_end(rest).unwrap_or(rest.len());
        let inner = &rest[1..end.min(rest.len()).max(1)];
        rest = rest.get(end + 1..).unwrap_or("");
        if inner.starts_with(['!', '?']) {
            continue;
        }
        if let Some(name) = inner.strip_prefix('/') {
            tokens.push(Token::Close(name.trim().to_ascii_lowercase()));
            continue;
        }
        let (name, attrs) = parse_tag(inner);
        // Their contents aren't shown.
        if matches!(name.as_str(), "script" | "style" | "title" | "textarea") {
            let lower = rest.to_ascii_lowercase();
            let close = format!("</{}", name);
            rest = match lower.find(&close) {
                Some(i) => &rest[i..],
                None => "",
            };
        }
        tokens.push(Token::Open(name, attrs));
    }
    tokens
}

/// The index of the `>` ending the tag at the start of `s`, skipping quotes.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => (),
        }
    }
    None
}

fn parse_tag(inner: &str) -> (String, Vec<(String, String)>) {
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    let name_end = inner
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(inner.len());
    let name = inner[..name_end].to_ascii_lowercase();
    let mut attrs = Vec::new();
    let mut rest = inner[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest
            .find(|c: char| c.is_ascii_whitespace() || c == '=')
            .unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            let (raw, remaining) = match after.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let body = &after[1..];
                    let end = body.find(q).unwrap_or(body.len());
                    (&body[..end], body.get(end + 1..).unwrap_or(""))
                }
                _ => {
                    let end = after
                        .find(|c: char| c.is_ascii_whitespace())
                        .unwrap_or(after.len());
                    (&after[..end], &after[end..])
                }
            };
            value = decode_entities(raw);
            rest = remaining.trim_start();
        }
        if !key.is_empty() {
            attrs.push((key, value));
        }
    }
    (name, attrs)
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let entity = rest[1..]
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| Some((entity_char(&rest[1..end + 1])?, end + 2)));
        match entity {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity_char(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "bull" => '•',
        "middot" => '·',
        "deg" => '°',
        "eacute" => 'é',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is 10 wide; lines are the font size, ascent 80%.
    struct Fixed;
    impl Measure for Fixed {
        fn width(&mut self, _style: &Style, text: &str) -> f32 {
            text.chars().count() as f32 * 10.0
        }
        fn metrics(&mut self, style: &Style) -> (f32, f32) {
            (style.size * 0.8, style.size)
        }
    }

    fn texts(page: &Page) -> Vec<&str> {
        page.fragments.iter().map(|f| f.text.as_str()).collect()
    }

    #[test]
    fn wraps_and_collapses_spaces() {
        let page = layout("<p>aa  bb\n cc dd</p>", 96.0, &mut Fixed);
        // 80 wide for text: "aa bb cc" fits, "dd" wraps.
        assert_eq!(texts(&page), ["aa bb cc", "dd"]);
        assert_eq!((page.fragments[0].x, page.fragments[0].y), (8.0, 16.0));
        assert_eq!(page.fragments[1].y, 32.0);
        assert_eq!(page.height, 64.0);
    }

    #[test]
    fn styles_and_links() {
        let html = "<body bgcolor=\"black\"><font size=\"5\" face=\"Arial\" \
                    color=\"white\">Hi <a href=\"mailto:a@b.c\"><u>a@b.c</u></a>\
                    </font>&amp;<!-- <b>x</b> --></body>";
        let page = layout(html, 320.0, &mut Fixed);
        assert_eq!(page.background, Some((0.0, 0.0, 0.0, 1.0)));
        assert_eq!(texts(&page), ["Hi", "a@b.c", "&"]);
        let [hi, link, amp] = &page.fragments[..] else {
            unreachable!();
        };
        assert_eq!((hi.style.size, hi.style.family), (24.0, FontFamily::Sans));
        assert_eq!(hi.style.color, (1.0, 1.0, 1.0, 1.0));
        assert_eq!(link.style.link.as_deref(), Some("mailto:a@b.c"));
        assert!(link.style.underline);
        assert_eq!(link.x, 8.0 + 20.0 + 10.0);
        assert_eq!(amp.style.size, 16.0);
        // Same baseline: the smaller text is lower.
        assert!((amp.y - hi.y - (24.0 * 0.8 - 16.0 * 0.8)).abs() < 1e-4);
    }

    #[test]
    fn breaks_lists_and_skipped_content() {
        let html = "<head><title>T</title><style>p{}</style></head>\
                    a<br>b<ul><li>c<li>d</ul><pre>\n e  f\ng</pre>";
        let page = layout(html, 320.0, &mut Fixed);
        assert_eq!(texts(&page), ["a", "b", "•", "c", "•", "d", " e  f", "g"]);
        assert_eq!(page.fragments[3].x, 8.0 + 40.0);
        assert_eq!(page.fragments[6].style.family, FontFamily::Mono);
    }

    #[test]
    fn centered_and_unclosed_paragraphs() {
        let page = layout("<center>ab</center><p>x<p>y", 120.0, &mut Fixed);
        assert_eq!(page.fragments[0].x, 8.0 + (104.0 - 20.0) / 2.0);
        assert_eq!(page.fragments[2].y - page.fragments[1].y, 32.0);
    }
}
