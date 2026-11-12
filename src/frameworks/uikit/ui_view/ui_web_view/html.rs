/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Lays out the simple HTML pages games show in a `UIWebView` (help, about,
//! credits). Not a browser: it knows text, paragraphs, headings, line breaks,
//! lists, `<pre>`, `<hr>`, `<font>`, bold, italic, underline, alignment,
//! `bgcolor` and links, and CSS for those (tag, class and id selectors, with
//! descendants). Other CSS, images and scripts are ignored (and logged), and
//! tables' text flows as plain blocks.

use crate::frameworks::uikit::ui_font::FontFamily;

pub type Rgba = (f32, f32, f32, f32);

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
    /// Lines that wrap are stretched to both edges.
    Justify,
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
    pub unsupported: Vec<String>,
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
        rules: Vec::new(),
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
            Token::Css(css) => layout.stylesheet(&css),
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
    stack: Vec<(Element, Style)>,
    /// The page's `<style>` rules, in order.
    rules: Vec<Rule>,
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
            self.end_line(true);
            space = 0.0;
        }
        self.line_start.get_or_insert((style.align, style.indent));
        let x = self.line_x + space;
        self.line_x = x + width;
        if let Some(last) = self.line.last_mut() {
            // Justified words are moved apart, so each is kept on its own.
            if last.style == *style
                && last.x + last.width == x - space
                && style.align != Align::Justify
            {
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
        self.end_line(false);
    }

    /// `wrapped`: whether the line ends because the next word didn't fit.
    fn end_line(&mut self, wrapped: bool) {
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
                Align::Left | Align::Justify => 0.0,
                Align::Center => (free / 2.0).max(0.0),
                Align::Right => free.max(0.0),
            };
        // The words after a space, which a justified line moves along.
        let mut end = None;
        let spaced: Vec<bool> = self
            .line
            .iter()
            .map(|f| {
                let spaced = end.is_some_and(|end| f.x > end);
                // Not after a list's marker.
                end = (f.x >= 0.0).then_some(f.x + f.width);
                spaced
            })
            .collect();
        let gaps = spaced.iter().filter(|&&s| s).count();
        let extra = if wrapped && align == Align::Justify && free > 0.0 && gaps > 0 {
            free / gaps as f32
        } else {
            0.0
        };
        let mut shift = 0.0;
        let line = self.line.drain(..).zip(metrics).zip(spaced);
        for ((mut fragment, (a, height)), spaced) in line {
            if spaced {
                shift += extra;
            }
            fragment.x += offset + shift;
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
        if (ends_p && self.stack.last().is_some_and(|(e, _)| e.name == "p"))
            || (name == "li" && self.stack.last().is_some_and(|(e, _)| e.name == "li"))
        {
            let open = self.stack.last().unwrap().0.name.clone();
            self.close(&open);
        }

        let mut style = self.style().clone();
        // The block's vertical margin, in ems of its font size (after CSS).
        let mut margin = None;
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
            "link" => {
                if attr("rel").is_some_and(|r| r.eq_ignore_ascii_case("stylesheet")) {
                    self.unsupported("linked CSS");
                }
                return;
            }
            "meta" | "base" | "input" | "col" | "area" | "param" | "wbr" => return,
            "body" => {
                if let Some(color) = attr("bgcolor").and_then(parse_color) {
                    self.page.background = Some(color);
                }
                if let Some(color) = attr("text").and_then(parse_color) {
                    style.color = color;
                }
                if let Some(color) = attr("link").and_then(parse_color) {
                    self.link_color = color;
                }
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
            "p" => margin = Some(1.0),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let (size, ems) = HEADINGS[usize::from(name.as_bytes()[1] - b'1')];
                style.size = size;
                style.bold = true;
                margin = Some(ems);
            }
            "pre" => {
                style.family = FontFamily::Mono;
                style.size = 13.0;
                style.pre = true;
                margin = Some(1.0);
            }
            "ul" | "ol" => {
                margin = Some(if self.lists.is_empty() { 1.0 } else { 0.0 });
                style.indent += 40.0;
            }
            "blockquote" | "dd" => {
                margin = Some(if name == "dd" { 0.0 } else { 1.0 });
                style.indent += 40.0;
            }
            "li" | "div" | "center" | "dl" | "dt" | "tr" | "form" | "table" => margin = Some(0.0),
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
                "justify" => Align::Justify,
                _ => Align::Left,
            };
        }
        let element = Element::new(name, attrs);
        self.cascade(&element, attr("style"), &mut style);
        if let Some(ems) = margin {
            self.block(style.size * ems);
        }
        match name {
            "pre" => self.pre_start = true,
            "ul" | "ol" => self.lists.push((name == "ol").then_some(1)),
            "li" => {
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
            "table" => self.unsupported("tables"),
            _ => (),
        }
        self.stack.push((element, style));
    }

    fn close(&mut self, name: &str) {
        if name == "br" {
            return self.line_break();
        }
        let Some(i) = self.stack.iter().rposition(|(e, _)| e.name == name) else {
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

    fn unsupported(&mut self, what: &str) {
        if !self.page.unsupported.iter().any(|w| w == what) {
            self.page.unsupported.push(what.to_string());
        }
    }

    /// Adds a `<style>`'s rules.
    fn stylesheet(&mut self, css: &str) {
        let css = strip_comments(css).replace("<!--", "").replace("-->", "");
        let mut rest = css.as_str();
        while let Some(open) = rest.find('{') {
            // The matching '}', after any nested blocks.
            let mut depth = 0;
            let close = rest[open..]
                .find(|c| {
                    match c {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => (),
                    }
                    depth == 0
                })
                .map_or(rest.len(), |i| open + i);
            // Statements like `@import "a.css";` can come first.
            let mut statements: Vec<&str> = rest[..open].split(';').collect();
            let prelude = statements.pop().unwrap().trim();
            let body = rest.get(open + 1..close).unwrap_or("");
            rest = rest.get(close + 1..).unwrap_or("");
            if statements.iter().any(|s| s.trim().starts_with('@')) {
                self.unsupported("CSS @-rules");
            }
            if prelude.starts_with('@') {
                self.unsupported("CSS @-rules");
                continue;
            }
            let declarations = parse_declarations(body);
            for selector in prelude.split(',') {
                match parse_selector(selector) {
                    Some((selector, specificity)) => self.rules.push(Rule {
                        selector,
                        specificity,
                        declarations: declarations.clone(),
                    }),
                    None => self.unsupported("CSS selectors"),
                }
            }
        }
    }

    /// Applies the page's CSS for an element to its style.
    fn cascade(&mut self, element: &Element, inline: Option<&str>, style: &mut Style) {
        let ancestors: Vec<&Element> = self.stack.iter().map(|(e, _)| e).collect();
        let mut declarations: Vec<_> = self
            .rules
            .iter()
            .enumerate()
            .filter(|(_, rule)| rule.matches(element, &ancestors))
            .flat_map(|(order, rule)| {
                rule.declarations
                    .iter()
                    .map(move |d| ((d.important, rule.specificity, order), d.clone()))
            })
            .collect();
        let inline = inline.map(parse_declarations).unwrap_or_default();
        declarations.extend(
            inline
                .into_iter()
                .map(|d| ((d.important, (1, 0, 0, 0), usize::MAX), d)),
        );
        declarations.sort_by_key(|(key, _)| *key);
        let parent = self.style().size;
        for (_, declaration) in declarations {
            self.apply(element, &declaration, parent, style);
        }
    }

    fn apply(&mut self, element: &Element, d: &Declaration, parent: f32, style: &mut Style) {
        let value = d.value.to_ascii_lowercase();
        match d.name.as_str() {
            "color" => {
                if let Some(color) = parse_color(&value) {
                    style.color = color;
                }
            }
            "background" | "background-color" => {
                if value.contains("url(") {
                    self.unsupported("CSS background images");
                }
                let color =
                    parse_color(&value).or_else(|| value.split_whitespace().find_map(parse_color));
                match color {
                    Some(color) if matches!(element.name.as_str(), "html" | "body") => {
                        self.page.background = Some(color);
                    }
                    Some(_) => self.unsupported("CSS backgrounds"),
                    None => (),
                }
            }
            "font-family" => style.family = font_family(&value.replace(['"', '\''], "")),
            "font-size" => {
                if let Some(size) = css_font_size(&value, parent) {
                    style.size = size;
                }
            }
            "font-weight" => {
                if let Some(bold) = css_bold(&value) {
                    style.bold = bold;
                }
            }
            "font-style" => match value.as_str() {
                "italic" | "oblique" => style.italic = true,
                "normal" => style.italic = false,
                _ => (),
            },
            "font" => css_font(&value, parent, style),
            "text-decoration" => {
                if value.contains("underline") {
                    style.underline = true;
                } else if value == "none" {
                    style.underline = false;
                }
                if value.contains("line-through") || value.contains("overline") {
                    self.unsupported("CSS text-decoration");
                }
            }
            "text-align" => match value.as_str() {
                "left" | "start" | "-webkit-left" => style.align = Align::Left,
                "justify" => style.align = Align::Justify,
                "center" | "-webkit-center" => style.align = Align::Center,
                "right" | "end" | "-webkit-right" => style.align = Align::Right,
                _ => (),
            },
            // Nothing to show for these.
            "-webkit-user-select"
            | "-webkit-touch-callout"
            | "-webkit-tap-highlight-color"
            | "-webkit-text-size-adjust"
            | "cursor" => (),
            name => self.unsupported(&format!("CSS {}", name)),
        }
    }
}

/// An element, as CSS selectors see it.
struct Element {
    name: String,
    id: Option<String>,
    classes: Vec<String>,
    /// An `<a>` with an `href`.
    link: bool,
}

impl Element {
    fn new(name: &str, attrs: &[(String, String)]) -> Element {
        let attr = |key: &str| {
            attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
        };
        Element {
            name: name.to_string(),
            id: attr("id").map(str::to_string),
            classes: attr("class")
                .map(|c| c.split_whitespace().map(str::to_string).collect())
                .unwrap_or_default(),
            link: name == "a" && attr("href").is_some(),
        }
    }
}

/// Inline, ids, classes and tags.
type Specificity = (u32, u32, u32, u32);

#[derive(Clone)]
struct Declaration {
    name: String,
    value: String,
    important: bool,
}

/// A `<style>` rule, one for each of its selectors.
struct Rule {
    selector: Vec<Part>,
    specificity: Specificity,
    declarations: Vec<Declaration>,
}

impl Rule {
    fn matches(&self, element: &Element, ancestors: &[&Element]) -> bool {
        let (subject, rest) = self.selector.split_last().unwrap();
        subject.matches(element) && matches_ancestors(rest, subject.child, ancestors)
    }
}

/// A part of a selector, like `p.note`.
struct Part {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    /// `:link` or `:visited`.
    link: bool,
    /// `:hover`, `:active` or `:focus`, which nothing is.
    never: bool,
    /// Whether it must be a child (`>`) of the part before it, not just a
    /// descendant.
    child: bool,
}

impl Part {
    fn matches(&self, element: &Element) -> bool {
        !self.never
            && self.tag.as_ref().is_none_or(|tag| *tag == element.name)
            && self
                .id
                .as_ref()
                .is_none_or(|id| element.id.as_ref() == Some(id))
            && self.classes.iter().all(|c| element.classes.contains(c))
            && (!self.link || element.link)
    }
}

/// Whether `parts`, the ones before the part that matched, match the
/// element's ancestors (innermost last). `child`: whether the last of them
/// must be the parent.
fn matches_ancestors(parts: &[Part], child: bool, ancestors: &[&Element]) -> bool {
    let Some((last, rest)) = parts.split_last() else {
        return true;
    };
    let start = if child {
        ancestors.len().saturating_sub(1)
    } else {
        0
    };
    (start..ancestors.len())
        .rev()
        .any(|i| last.matches(ancestors[i]) && matches_ancestors(rest, last.child, &ancestors[..i]))
}

/// A selector made of tags, classes, ids and `:link`, joined by descendant
/// or child combinators. `None` for anything else.
fn parse_selector(selector: &str) -> Option<(Vec<Part>, Specificity)> {
    let mut parts = Vec::new();
    let mut specificity = (0, 0, 0, 0);
    let mut child = false;
    for word in selector.replace('>', " > ").split_whitespace() {
        if word == ">" {
            child = true;
            continue;
        }
        if word.contains(['+', '~', '[']) {
            return None;
        }
        let mut part = Part {
            tag: None,
            id: None,
            classes: Vec::new(),
            link: false,
            never: false,
            child: std::mem::take(&mut child),
        };
        let end = word.find(['.', '#', ':']).unwrap_or(word.len());
        if !matches!(&word[..end], "" | "*") {
            part.tag = Some(word[..end].to_ascii_lowercase());
            specificity.3 += 1;
        }
        let mut rest = &word[end..];
        while let Some(kind) = rest.chars().next() {
            let body = &rest[1..];
            let end = body.find(['.', '#', ':']).unwrap_or(body.len());
            let name = &body[..end];
            rest = &body[end..];
            match kind {
                '.' => {
                    part.classes.push(name.to_string());
                    specificity.2 += 1;
                }
                '#' => {
                    part.id = Some(name.to_string());
                    specificity.1 += 1;
                }
                _ => {
                    match name.to_ascii_lowercase().as_str() {
                        "link" | "visited" => part.link = true,
                        "hover" | "active" | "focus" => part.never = true,
                        _ => return None,
                    }
                    specificity.2 += 1;
                }
            }
        }
        parts.push(part);
    }
    if child || parts.is_empty() {
        return None;
    }
    Some((parts, specificity))
}

fn parse_declarations(css: &str) -> Vec<Declaration> {
    css.split(';')
        .filter_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            let value = value.trim();
            let (value, important) = match value.split_once('!') {
                Some((v, flag)) if flag.trim().eq_ignore_ascii_case("important") => {
                    (v.trim(), true)
                }
                _ => (value, false),
            };
            Some(Declaration {
                name: name.trim().to_ascii_lowercase(),
                value: value.to_string(),
                important,
            })
        })
        .collect()
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(i) = rest.find("/*") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        rest = after.find("*/").map_or("", |j| &after[j + 2..]);
    }
    out.push_str(rest);
    out
}

/// A CSS `font-size`, given the parent's.
fn css_font_size(size: &str, parent: f32) -> Option<f32> {
    Some(match size {
        "xx-small" => 9.0,
        "x-small" => 10.0,
        "small" => 13.0,
        "medium" => 16.0,
        "large" => 18.0,
        "x-large" => 24.0,
        "xx-large" => 32.0,
        "smaller" => parent / 1.2,
        "larger" => parent * 1.2,
        _ => css_length(size, parent).filter(|&size| size > 0.0)?,
    })
}

/// A CSS length in pixels (points on iPhone OS), with `em` as given.
fn css_length(length: &str, em: f32) -> Option<f32> {
    let end = length
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+')))
        .unwrap_or(length.len());
    let n: f32 = length[..end].parse().ok()?;
    Some(match &length[end..] {
        // Old pages leave the unit out, which WebKit's quirks mode allows.
        "px" | "" => n,
        "pt" => n * 4.0 / 3.0,
        "pc" => n * 16.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        "em" => n * em,
        "ex" => n * em / 2.0,
        "%" => n * em / 100.0,
        "rem" => n * 16.0,
        _ => return None,
    })
}

fn css_bold(weight: &str) -> Option<bool> {
    match weight {
        "bold" | "bolder" => Some(true),
        "normal" | "lighter" => Some(false),
        n => n.parse::<u32>().ok().map(|n| n >= 600),
    }
}

/// The `font` shorthand: style and weight, size (and line height), family.
fn css_font(font: &str, parent: f32, style: &mut Style) {
    let words: Vec<&str> = font.split_whitespace().collect();
    // Weights like 700 have no unit, sizes do.
    let size = words.iter().enumerate().find_map(|(i, word)| {
        let size = word.split('/').next().unwrap();
        if size.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((i, css_font_size(size, parent)?))
    });
    let Some((i, size)) = size.filter(|&(i, _)| i + 1 < words.len()) else {
        return;
    };
    style.italic = false;
    style.bold = false;
    for word in &words[..i] {
        match *word {
            "italic" | "oblique" => style.italic = true,
            word => style.bold = css_bold(word).unwrap_or(style.bold),
        }
    }
    style.size = size;
    style.family = font_family(&words[i + 1..].join(" ").replace(['"', '\''], ""));
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
    let rgb = color
        .strip_prefix("rgb(")
        .or_else(|| color.strip_prefix("rgba("))
        .and_then(|args| args.strip_suffix(')'));
    if let Some(args) = rgb {
        let channels: Vec<f32> = args
            .split(',')
            .map(|c| match c.trim().strip_suffix('%') {
                Some(percent) => percent.parse().map(|p: f32| p * 2.55),
                None => c.trim().parse(),
            })
            .collect::<Result<_, _>>()
            .ok()?;
        let channel = |c: f32| c.clamp(0.0, 255.0) / 255.0;
        return match channels[..] {
            [r, g, b] => Some((channel(r), channel(g), channel(b), 1.0)),
            [r, g, b, a] => Some((channel(r), channel(g), channel(b), a.clamp(0.0, 1.0))),
            _ => None,
        };
    }
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
    /// A `<style>`'s contents.
    Css(String),
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
            let end = lower.find(&close).unwrap_or(rest.len());
            let media = attrs.iter().find(|(k, _)| k == "media");
            if name == "style" && !media.is_some_and(|(_, m)| m.contains("print")) {
                tokens.push(Token::Open(name, attrs));
                tokens.push(Token::Css(rest[..end].to_string()));
                rest = &rest[end..];
                continue;
            }
            rest = &rest[end..];
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

    #[test]
    fn justified() {
        let html = "<p align=\"justify\">aa <b>bb</b> cc dd ee<br>ff gg</p>";
        let page = layout(html, 100.0, &mut Fixed);
        assert_eq!(texts(&page), ["aa", "bb", "cc", "dd", "ee", "ff", "gg"]);
        let x: Vec<f32> = page.fragments.iter().map(|f| f.x).collect();
        // "aa bb cc" wraps, so it's stretched to 84 wide; the line before
        // the <br> and the last line aren't.
        assert_eq!(x, [8.0, 40.0, 72.0, 8.0, 38.0, 8.0, 38.0]);
    }

    #[test]
    fn css() {
        let html = "<style><!-- body { -webkit-user-select: none; }\n\
                    /* x */ .s2 { font-family: Verdana, sans-serif; \
                    color: #FFFFFF; font-size: small; }\n\
                    .s5 { color: #FF0000; font-weight: bold }\n\
                    div p, a:link { color: rgb(0, 255, 0) !important; \
                    text-decoration: none } p#x { color: red } --></style>\
                    <p class=\"s2\">a <span class=\"s5\">b</span></p>\
                    <div><p class=\"s2\" id=\"x\" style=\"font: italic \
                    bold 12pt Courier\">c <a href=\"h\">d</a></p></div>";
        let page = layout(html, 320.0, &mut Fixed);
        assert_eq!(texts(&page), ["a", "b", "c", "d"]);
        let [a, b, c, d] = &page.fragments[..] else {
            unreachable!();
        };
        assert_eq!((a.style.size, a.style.family), (13.0, FontFamily::Sans));
        assert_eq!(a.style.color, (1.0, 1.0, 1.0, 1.0));
        // A paragraph's margin is in its own font size.
        assert_eq!(a.y, 13.0);
        assert_eq!(b.style.color, (1.0, 0.0, 0.0, 1.0));
        assert!(b.style.bold && !a.style.bold);
        assert_eq!((c.style.size, c.style.family), (16.0, FontFamily::Mono));
        assert!(c.style.italic && c.style.bold);
        assert_eq!(c.style.color, (0.0, 1.0, 0.0, 1.0));
        assert!(!d.style.underline);
        assert!(page.unsupported.is_empty(), "{:?}", page.unsupported);
    }
}
