/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIWebView`. Local pages are drawn by [html], which only knows simple
//! HTML. Web pages fail to load, as when offline.

mod html;

use crate::frameworks::core_graphics::cg_context::{CGContextFillRect, CGContextSetRGBFillColor};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{from_rust_string, get_static_str, to_rust_string};
use crate::frameworks::foundation::ns_url_connection::offline_error;
use crate::frameworks::foundation::{ns_data, ns_url, NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_font;
use crate::frameworks::uikit::ui_font::{font_with_style, size_with_font, FontFamily};
use crate::frameworks::uikit::ui_graphics::UIGraphicsGetCurrentContext;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr, SEL,
};
use crate::Environment;
use html::{Fragment, Measure, Page, Style};
use std::collections::HashMap;

type UIWebViewNavigationType = NSInteger;
const UIWebViewNavigationTypeLinkClicked: UIWebViewNavigationType = 0;
const UIWebViewNavigationTypeOther: UIWebViewNavigationType = 5;

/// A load waiting for the run loop, as on iOS.
enum Load {
    /// `NSURLRequest*`
    Request(id),
    /// The page and its base `NSURL*` (may be nil).
    Html(String, id),
}

#[derive(Default)]
pub struct UIWebViewHostObject {
    superclass: super::UIViewHostObject,
    /// Not retained.
    delegate: id,
    /// The view the page is drawn in, inside the scroll view. Not retained.
    document: id,
    pending: Option<Load>,
    loading: bool,
    /// The shown page's `NSURL*`, for links.
    base_url: id,
}
impl_HostObject_with_superclass!(UIWebViewHostObject);

#[derive(Default)]
pub struct DocumentHostObject {
    superclass: super::UIViewHostObject,
    /// Not retained.
    web_view: id,
    /// The fragments and the `UIFont*` each is drawn with.
    fragments: Vec<(Fragment, id)>,
    rules: Vec<(f32, f32, f32)>,
    /// All the fonts in `fragments`, retained.
    fonts: Vec<id>,
    /// Where a tap began, in window co-ordinates.
    touch_start: Option<CGPoint>,
}
impl_HostObject_with_superclass!(DocumentHostObject);

/// How far a touch can move and still be a tap.
const TAP_SLOP: CGFloat = 10.0;

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIWebView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIWebViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    add_scroll_view(env, this);
    this
}

// NSCoding implementation
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    add_scroll_view(env, this);
    this
}

- (())dealloc {
    let host = env.objc.borrow_mut::<UIWebViewHostObject>(this);
    let pending = host.pending.take();
    let base_url = std::mem::replace(&mut host.base_url, nil);
    release_load(env, pending);
    release(env, base_url);
    msg_super![env; this dealloc]
}

// Pages are laid out at their own size, as with this off.
- (())setScalesPageToFit:(bool)_scales {}

- (id)delegate {
    env.objc.borrow::<UIWebViewHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIWebViewHostObject>(this).delegate = delegate;
}

- (bool)isLoading {
    let host = env.objc.borrow::<UIWebViewHostObject>(this);
    host.loading || host.pending.is_some()
}
- (())stopLoading {
    let pending = env.objc.borrow_mut::<UIWebViewHostObject>(this).pending.take();
    release_load(env, pending);
}

- (())loadRequest:(id)request { // NSURLRequest*
    retain(env, request);
    start_load(env, this, Load::Request(request));
}
- (())loadData:(id)data // NSData*
      MIMEType:(id)mime_type // NSString*
textEncodingName:(id)encoding_name // NSString*
       baseURL:(id)base_url { // NSURL*
    let length: NSUInteger = msg![env; data length];
    let bytes = if length == 0 {
        Vec::new()
    } else {
        ns_data::to_rust_slice(env, data).to_vec()
    };
    let mime_type = to_rust_string(env, mime_type).to_ascii_lowercase();
    let encoding = if encoding_name == nil {
        String::new()
    } else {
        to_rust_string(env, encoding_name).to_ascii_lowercase()
    };
    let text = decode(&bytes, &encoding);
    let html = match mime_type.as_str() {
        "text/html" | "application/xhtml+xml" => text,
        "text/plain" => plain_text_html(&text),
        _ => {
            log!("TODO: UIWebView can't show {:?}, showing nothing", mime_type);
            String::new()
        }
    };
    retain(env, base_url);
    start_load(env, this, Load::Html(html, base_url));
}
- (())loadHTMLString:(id)string // NSString*
             baseURL:(id)base_url { // NSURL*
    let html = if string == nil {
        String::new()
    } else {
        to_rust_string(env, string).to_string()
    };
    retain(env, base_url);
    start_load(env, this, Load::Html(html, base_url));
}

- (())_touchHLE_load {
    if let Some(load) = env.objc.borrow_mut::<UIWebViewHostObject>(this).pending.take() {
        run_load(env, this, load);
    }
}

@end

// Keeps the page's view over the visible area as it scrolls: a long page in
// one view would need a texture bigger than the GPU allows.
@implementation _touchHLE_UIWebScrollView: UIScrollView

- (())setContentOffset:(CGPoint)offset {
    () = msg_super![env; this setContentOffset:offset];
    let web_view: id = msg![env; this superview];
    let class: id = msg_class![env; UIWebView class];
    let is_web_view: bool = msg![env; web_view isKindOfClass:class];
    if !is_web_view {
        return;
    }
    let document = env.objc.borrow::<UIWebViewHostObject>(web_view).document;
    let bounds: CGRect = msg![env; this bounds];
    () = msg![env; document setFrame:bounds];
    () = msg![env; document setNeedsDisplay];
}

@end

// The page's content, inside the web view's scroll view.
@implementation _touchHLE_UIWebDocumentView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<DocumentHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let fonts = std::mem::take(&mut env.objc.borrow_mut::<DocumentHostObject>(this).fonts);
    for font in fonts {
        release(env, font);
    }
    msg_super![env; this dealloc]
}

// The view only covers what's on screen (see `_touchHLE_UIWebScrollView`),
// so it draws the part of the page under it.
- (())drawRect:(CGRect)_rect {
    let context = UIGraphicsGetCurrentContext(env);
    let frame: CGRect = msg![env; this frame];
    let (top, bottom) = (frame.origin.y, frame.origin.y + frame.size.height);
    let host = env.objc.borrow::<DocumentHostObject>(this);
    let fragments: Vec<_> = host
        .fragments
        .iter()
        .filter(|(f, _)| f.y + f.height > top && f.y < bottom)
        .cloned()
        .collect();
    let rules = host.rules.clone();
    for (fragment, font) in fragments {
        let (r, g, b, a) = fragment.style.color;
        CGContextSetRGBFillColor(env, context, r, g, b, a);
        let (x, y) = (fragment.x - frame.origin.x, fragment.y - top);
        ui_font::draw_at_point(env, font, &fragment.text, CGPoint { x, y }, None);
        if fragment.style.underline {
            let ascender: CGFloat = msg![env; font ascender];
            let thickness = (fragment.style.size / 16.0).max(1.0);
            let rect = CGRect {
                origin: CGPoint { x, y: y + ascender + thickness },
                size: CGSize { width: fragment.width, height: thickness },
            };
            CGContextFillRect(env, context, rect);
        }
    }
    for (x, y, width) in rules.into_iter().filter(|r| r.1 >= top && r.1 < bottom) {
        CGContextSetRGBFillColor(env, context, 0.5, 0.5, 0.5, 1.0);
        let rect = CGRect {
            origin: CGPoint { x: x - frame.origin.x, y: y - top },
            size: CGSize { width, height: 1.0 },
        };
        CGContextFillRect(env, context, rect);
    }
}

// Drags fall through to the scroll view.
- (())touchesBegan:(id)touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:nil];
    env.objc.borrow_mut::<DocumentHostObject>(this).touch_start = Some(point);
}
- (())touchesEnded:(id)touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
    let Some(start) = env.objc.borrow_mut::<DocumentHostObject>(this).touch_start.take() else {
        return;
    };
    let touch: id = msg![env; touches anyObject];
    let end: CGPoint = msg![env; touch locationInView:nil];
    if (end.x - start.x).hypot(end.y - start.y) > TAP_SLOP {
        return;
    }
    let point: CGPoint = msg![env; touch locationInView:this];
    let frame: CGRect = msg![env; this frame];
    let point = CGPoint {
        x: point.x + frame.origin.x,
        y: point.y + frame.origin.y,
    };
    let host = env.objc.borrow::<DocumentHostObject>(this);
    let link = host.fragments.iter().find_map(|(f, _)| {
        let hit = point.x >= f.x - 2.0
            && point.x <= f.x + f.width + 2.0
            && point.y >= f.y - 2.0
            && point.y <= f.y + f.height + 2.0;
        f.style.link.clone().filter(|_| hit)
    });
    let web_view = host.web_view;
    if let Some(link) = link {
        open_link(env, web_view, &link);
    }
}
- (())touchesCancelled:(id)_touches // NSSet* of UITouch*
             withEvent:(id)_event { // UIEvent*
    env.objc.borrow_mut::<DocumentHostObject>(this).touch_start = None;
}

@end

};

/// The page is shown in a scroll view, the web view's first subview.
/// Some games reach it through -subviews to hide its shadows.
fn add_scroll_view(env: &mut Environment, web_view: id) {
    let bounds: CGRect = msg![env; web_view bounds];
    let scroll_view: id = msg_class![env; _touchHLE_UIWebScrollView alloc];
    let scroll_view: id = msg![env; scroll_view initWithFrame:bounds];
    () = msg![env; web_view addSubview:scroll_view];
    let document: id = msg_class![env; _touchHLE_UIWebDocumentView alloc];
    let document: id = msg![env; document initWithFrame:bounds];
    // Clear until a page with a background is shown.
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; document setBackgroundColor:clear];
    env.objc.borrow_mut::<DocumentHostObject>(document).web_view = web_view;
    () = msg![env; scroll_view addSubview:document];
    env.objc
        .borrow_mut::<UIWebViewHostObject>(web_view)
        .document = document;
    release(env, document);
    release(env, scroll_view);
}

fn release_load(env: &mut Environment, load: Option<Load>) {
    match load {
        Some(Load::Request(object) | Load::Html(_, object)) => release(env, object),
        None => (),
    }
}

/// Like iOS, loads on the run loop, replacing any load not started yet.
fn start_load(env: &mut Environment, web_view: id, load: Load) {
    let host = env.objc.borrow_mut::<UIWebViewHostObject>(web_view);
    let scheduled = host.pending.is_some();
    let old = host.pending.replace(load);
    release_load(env, old);
    if !scheduled {
        let sel = selector(env, "_touchHLE_load");
        () = msg![env; web_view performSelector:sel withObject:nil afterDelay:0.0];
    }
}

fn run_load(env: &mut Environment, web_view: id, load: Load) {
    // The request to ask the delegate about, and the page if it's given.
    let (request, html) = match load {
        Load::Request(request) => (request, None),
        Load::Html(html, base_url) => {
            // An empty base is about:blank too, as in WebKit.
            let empty = base_url == nil || {
                let string: id = msg![env; base_url absoluteString];
                to_rust_string(env, string).is_empty()
            };
            let url = if empty {
                let blank = get_static_str(env, "about:blank");
                msg_class![env; NSURL URLWithString:blank]
            } else {
                base_url
            };
            let request: id = msg_class![env; NSURLRequest requestWithURL:url];
            retain(env, request);
            release(env, base_url);
            (request, Some(html))
        }
    };
    if should_start(env, web_view, request, UIWebViewNavigationTypeOther) {
        let url: id = msg![env; request URL];
        load_url(env, web_view, url, html);
    }
    release(env, request);
}

/// Shows `html`, or the file `url` names, telling the delegate.
fn load_url(env: &mut Environment, web_view: id, url: id, html: Option<String>) {
    env.objc.borrow_mut::<UIWebViewHostObject>(web_view).loading = true;
    tell_delegate(env, web_view, "webViewDidStartLoad:");
    let is_file: bool = msg![env; url isFileURL];
    let html = html.or_else(|| is_file.then(|| read_file(env, url)));
    let Some(html) = html else {
        let url = msg![env; url description];
        log!(
            "UIWebView can't load {:?} without network access, failing as if offline",
            to_rust_string(env, url),
        );
        env.objc.borrow_mut::<UIWebViewHostObject>(web_view).loading = false;
        let delegate = env.objc.borrow::<UIWebViewHostObject>(web_view).delegate;
        if responds(env, delegate, "webView:didFailLoadWithError:") {
            let error = offline_error(env);
            () = msg![env; delegate webView:web_view didFailLoadWithError:error];
        }
        return;
    };
    retain(env, url);
    let host = env.objc.borrow_mut::<UIWebViewHostObject>(web_view);
    let old_base = std::mem::replace(&mut host.base_url, url);
    release(env, old_base);
    show_page(env, web_view, &html);
    env.objc.borrow_mut::<UIWebViewHostObject>(web_view).loading = false;
    tell_delegate(env, web_view, "webViewDidFinishLoad:");
}

/// A page's HTML from the game's files: missing files show nothing.
fn read_file(env: &mut Environment, url: id) -> String {
    let path = ns_url::to_rust_path(env, url);
    let Ok(bytes) = env.fs.read(&path) else {
        log!("UIWebView couldn't read {:?}, showing nothing", path);
        return String::new();
    };
    let text = decode(&bytes, "");
    if path.as_str().to_ascii_lowercase().ends_with(".txt") {
        plain_text_html(&text)
    } else {
        text
    }
}

/// Text in the page's encoding: UTF-8 unless it says Latin-1.
fn decode(bytes: &[u8], encoding: &str) -> String {
    match encoding {
        "iso-8859-1" | "latin1" | "windows-1252" => bytes.iter().map(|&b| b as char).collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn plain_text_html(text: &str) -> String {
    let text = text.replace('&', "&amp;").replace('<', "&lt;");
    format!("<pre>{}</pre>", text)
}

/// Lays out the page at the web view's width and shows it.
fn show_page(env: &mut Environment, web_view: id, html: &str) {
    let bounds: CGRect = msg![env; web_view bounds];
    let mut fonts = GuestFonts {
        env: &mut *env,
        fonts: HashMap::new(),
    };
    let page: Page = html::layout(html, bounds.size.width, &mut fonts);
    let fonts = fonts.fonts;
    if !page.unsupported.is_empty() {
        log!(
            "TODO: UIWebView page has {}, not shown",
            page.unsupported.join(", "),
        );
    }
    let fragments: Vec<_> = page
        .fragments
        .into_iter()
        .map(|f| {
            let font = fonts[&FontKey::of(&f.style)];
            (f, font)
        })
        .collect();

    let document = env.objc.borrow::<UIWebViewHostObject>(web_view).document;
    let host = env.objc.borrow_mut::<DocumentHostObject>(document);
    host.fragments = fragments;
    host.rules = page.rules;
    let old_fonts = std::mem::replace(&mut host.fonts, fonts.into_values().collect());
    for font in old_fonts {
        release(env, font);
    }

    let height = page.height.max(bounds.size.height);
    let content_size = CGSize {
        width: bounds.size.width,
        height,
    };
    let color: id = match page.background {
        Some((r, g, b, a)) => msg_class![env; UIColor colorWithRed:r green:g blue:b alpha:a],
        None => msg_class![env; UIColor whiteColor],
    };
    () = msg![env; document setBackgroundColor:color];
    () = msg![env; document setNeedsDisplay];
    let scroll_view: id = msg![env; document superview];
    () = msg![env; scroll_view setContentSize:content_size];
    // This also puts the page's view over the top of the page.
    () = msg![env; scroll_view setContentOffset:(CGPoint { x: 0.0, y: 0.0 })];
}

/// Asks the delegate whether to load a request.
fn should_start(
    env: &mut Environment,
    web_view: id,
    request: id,
    navigation_type: UIWebViewNavigationType,
) -> bool {
    let delegate = env.objc.borrow::<UIWebViewHostObject>(web_view).delegate;
    let name = "webView:shouldStartLoadWithRequest:navigationType:";
    if !responds(env, delegate, name) {
        return true;
    }
    msg![env; delegate webView:web_view
           shouldStartLoadWithRequest:request
                       navigationType:navigation_type]
}

/// Follows a tapped link, if the delegate allows it. Web pages fail as if
/// offline, and links iOS hands to other apps (mail, phone, App Store) are
/// opened the same way.
fn open_link(env: &mut Environment, web_view: id, href: &str) {
    if href.starts_with('#') {
        log!("TODO: UIWebView link to {:?} on the same page", href);
        return;
    }
    let base_url = env.objc.borrow::<UIWebViewHostObject>(web_view).base_url;
    let href_string = from_rust_string(env, href.to_string());
    let url: id = msg_class![env; NSURL URLWithString:href_string relativeToURL:base_url];
    release(env, href_string);
    let scheme: id = msg![env; url scheme];
    if scheme == nil {
        log!("UIWebView ignoring link {:?} with no scheme", href);
        return;
    }
    let scheme = to_rust_string(env, scheme).to_ascii_lowercase();
    let request: id = msg_class![env; NSURLRequest requestWithURL:url];
    if !should_start(env, web_view, request, UIWebViewNavigationTypeLinkClicked) {
        return;
    }
    if matches!(scheme.as_str(), "file" | "http" | "https") {
        () = msg![env; web_view loadRequest:request];
    } else {
        let app: id = msg_class![env; UIApplication sharedApplication];
        let _: bool = msg![env; app openURL:url];
    }
}

fn selector(env: &mut Environment, name: &str) -> SEL {
    env.objc
        .register_host_selector(name.to_string(), &mut env.mem)
}

fn responds(env: &mut Environment, object: id, name: &str) -> bool {
    if object == nil {
        return false;
    }
    let sel = selector(env, name);
    msg![env; object respondsToSelector:sel]
}

/// Sends `selector` (taking the web view) to the delegate, if it has it.
fn tell_delegate(env: &mut Environment, web_view: id, name: &str) {
    let delegate = env.objc.borrow::<UIWebViewHostObject>(web_view).delegate;
    if responds(env, delegate, name) {
        let sel = selector(env, name);
        () = crate::objc::msg_send(env, (delegate, sel, web_view));
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct FontKey(FontFamily, bool, bool, u32);
impl FontKey {
    fn of(style: &Style) -> FontKey {
        FontKey(style.family, style.bold, style.italic, style.size.to_bits())
    }
}

/// Measures with the same fonts the page is drawn with.
struct GuestFonts<'a> {
    env: &'a mut Environment,
    /// Retained.
    fonts: HashMap<FontKey, id>,
}
impl GuestFonts<'_> {
    fn font(&mut self, style: &Style) -> id {
        let key = FontKey::of(style);
        if let Some(&font) = self.fonts.get(&key) {
            return font;
        }
        let font = font_with_style(self.env, style.family, style.bold, style.italic, style.size);
        retain(self.env, font);
        self.fonts.insert(key, font);
        font
    }
}
impl Measure for GuestFonts<'_> {
    fn width(&mut self, style: &Style, text: &str) -> f32 {
        let font = self.font(style);
        size_with_font(self.env, font, text, None).width
    }
    fn metrics(&mut self, style: &Style) -> (f32, f32) {
        let font = self.font(style);
        let env = &mut *self.env;
        // Not -lineHeight: this UIFont's -leading is a whole line.
        let ascender: CGFloat = msg![env; font ascender];
        let descender: CGFloat = msg![env; font descender];
        (ascender, ascender - descender)
    }
}
