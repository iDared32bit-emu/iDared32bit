/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableView`.
//!
//! Plain style only: grouped tables are laid out like plain ones. Row
//! animations aren't done; rows change straight away.

use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_index_path::new_index_path;
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_view::ui_table_view_cell::DEFAULT_ROW_HEIGHT;
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_send, msg_super, nil,
    objc_classes, release, retain, todo_objc_setter, ClassExports, NSZonePtr, SEL,
};
use crate::Environment;

type UITableViewStyle = NSInteger;
const UITableViewStylePlain: UITableViewStyle = 0;

type UITableViewCellSeparatorStyle = NSInteger;
const UITableViewCellSeparatorStyleNone: UITableViewCellSeparatorStyle = 0;
const UITableViewCellSeparatorStyleSingleLine: UITableViewCellSeparatorStyle = 1;

type UITableViewScrollPosition = NSInteger;
const UITableViewScrollPositionNone: UITableViewScrollPosition = 0;
const UITableViewScrollPositionTop: UITableViewScrollPosition = 1;
const UITableViewScrollPositionMiddle: UITableViewScrollPosition = 2;
const UITableViewScrollPositionBottom: UITableViewScrollPosition = 3;

type UITableViewRowAnimation = NSInteger;

/// A row: (section, row).
type Row = (NSInteger, NSInteger);

/// How far a touch can move and still be a tap.
const TAP_SLOP: CGFloat = 10.0;

struct Section {
    /// Where the section starts, including its header.
    y: CGFloat,
    header_height: CGFloat,
    /// `UIView*`, strong reference: the delegate's or a default one.
    header_view: id,
    /// Where each row starts, and its height.
    rows: Vec<(CGFloat, CGFloat)>,
}

pub struct UITableViewHostObject {
    superclass: super::UIScrollViewHostObject,
    /// UITableViewDataSource, weak reference
    data_source: id,
    style: UITableViewStyle,
    row_height: CGFloat,
    section_header_height: CGFloat,
    section_footer_height: CGFloat,
    separator_style: UITableViewCellSeparatorStyle,
    /// `UIColor*`, strong reference.
    separator_color: id,
    allows_selection: bool,
    /// Whether the data source has been asked for the rows since they last
    /// changed.
    loaded: bool,
    sections: Vec<Section>,
    content_height: CGFloat,
    /// The cells on screen. These are strong references.
    visible_cells: Vec<(Row, id)>,
    /// Cells waiting to be reused. These are strong references.
    reuse_pool: Vec<id>,
    /// Separator lines, reused on each layout. These are strong references.
    separators: Vec<id>,
    selected: Option<Row>,
    /// The row being touched, and where the touch began in the window.
    touched: Option<(Row, CGPoint)>,
}
impl_HostObject_with_superclass!(UITableViewHostObject);
impl Default for UITableViewHostObject {
    fn default() -> Self {
        UITableViewHostObject {
            superclass: Default::default(),
            data_source: nil,
            style: UITableViewStylePlain,
            row_height: DEFAULT_ROW_HEIGHT,
            section_header_height: 22.0,
            section_footer_height: 0.0,
            separator_style: UITableViewCellSeparatorStyleSingleLine,
            separator_color: nil,
            allows_selection: true,
            loaded: false,
            sections: Vec::new(),
            content_height: 0.0,
            visible_cells: Vec::new(),
            reuse_pool: Vec::new(),
            separators: Vec::new(),
            selected: None,
            touched: None,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UITableView: UIScrollView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; this setBackgroundColor:white];
    this
}
- (id)initWithFrame:(CGRect)frame style:(UITableViewStyle)style {
    let this: id = msg![env; this initWithFrame:frame];
    () = msg![env; this _touchHLE_setStyle:style];
    this
}
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UIStyle");
    let style: NSInteger = msg![env; coder decodeIntegerForKey:key];
    () = msg![env; this _touchHLE_setStyle:style];
    let key = get_static_str(env, "UISeparatorStyle");
    if msg![env; coder containsValueForKey:key] {
        let style: NSInteger = msg![env; coder decodeIntegerForKey:key];
        () = msg![env; this setSeparatorStyle:style];
    }
    for (key, setter) in [
        ("UIRowHeight", "setRowHeight:"),
        ("UISectionHeaderHeight", "setSectionHeaderHeight:"),
        ("UISectionFooterHeight", "setSectionFooterHeight:"),
    ] {
        let key = get_static_str(env, key);
        if msg![env; coder containsValueForKey:key] {
            let value: f32 = msg![env; coder decodeFloatForKey:key];
            let sel = selector(env, setter);
            () = msg_send(env, (this, sel, value as CGFloat));
        }
    }
    this
}
- (())_touchHLE_setStyle:(UITableViewStyle)style {
    if style != UITableViewStylePlain {
        log!("TODO: UITableView style {}, laid out like a plain table", style);
    }
    env.objc.borrow_mut::<UITableViewHostObject>(this).style = style;
}

- (())dealloc {
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    let separator_color = host.separator_color;
    let mut views: Vec<id> = std::mem::take(&mut host.reuse_pool);
    views.extend(std::mem::take(&mut host.separators));
    views.extend(host.visible_cells.drain(..).map(|(_, cell)| cell));
    views.extend(host.sections.drain(..).map(|section| section.header_view));
    for view in views {
        release(env, view);
    }
    release(env, separator_color);
    msg_super![env; this dealloc]
}

- (id)dataSource {
    env.objc.borrow::<UITableViewHostObject>(this).data_source
}
- (())setDataSource:(id)data_source {
    env.objc.borrow_mut::<UITableViewHostObject>(this).data_source = data_source;
    rows_changed(env, this);
}
- (())setDelegate:(id)delegate {
    () = msg_super![env; this setDelegate:delegate];
    rows_changed(env, this);
}

- (UITableViewStyle)style {
    env.objc.borrow::<UITableViewHostObject>(this).style
}

- (CGFloat)rowHeight {
    env.objc.borrow::<UITableViewHostObject>(this).row_height
}
- (())setRowHeight:(CGFloat)row_height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).row_height = row_height;
    rows_changed(env, this);
}
- (CGFloat)sectionHeaderHeight {
    env.objc.borrow::<UITableViewHostObject>(this).section_header_height
}
- (())setSectionHeaderHeight:(CGFloat)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).section_header_height = height;
    rows_changed(env, this);
}
- (CGFloat)sectionFooterHeight {
    env.objc.borrow::<UITableViewHostObject>(this).section_footer_height
}
- (())setSectionFooterHeight:(CGFloat)height {
    // Footers aren't shown yet (see reload()), so this changes nothing.
    env.objc.borrow_mut::<UITableViewHostObject>(this).section_footer_height = height;
}

- (UITableViewCellSeparatorStyle)separatorStyle {
    env.objc.borrow::<UITableViewHostObject>(this).separator_style
}
- (())setSeparatorStyle:(UITableViewCellSeparatorStyle)style {
    if style > UITableViewCellSeparatorStyleSingleLine {
        log!("TODO: UITableView separator style {}, using a single line", style);
    }
    env.objc.borrow_mut::<UITableViewHostObject>(this).separator_style = style;
    rows_changed(env, this);
}
- (id)separatorColor {
    let color = env.objc.borrow::<UITableViewHostObject>(this).separator_color;
    if color != nil {
        color
    } else {
        default_separator_color(env)
    }
}
- (())setSeparatorColor:(id)color { // UIColor*
    retain(env, color);
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    let old = std::mem::replace(&mut host.separator_color, color);
    release(env, old);
    () = msg![env; this setNeedsLayout];
}

- (bool)allowsSelection {
    env.objc.borrow::<UITableViewHostObject>(this).allows_selection
}
- (())setAllowsSelection:(bool)allows {
    env.objc.borrow_mut::<UITableViewHostObject>(this).allows_selection = allows;
}

- (())setTableHeaderView:(id)view {
    todo_objc_setter!(this, view);
}
- (())setTableFooterView:(id)view {
    todo_objc_setter!(this, view);
}
- (())setBackgroundView:(id)view {
    todo_objc_setter!(this, view);
}

- (())reloadData {
    reload(env, this);
}
- (())beginUpdates {}
- (())endUpdates {
    reload_keeping_selection(env, this);
}
- (())insertRowsAtIndexPaths:(id)index_paths // NSArray<NSIndexPath*>*
            withRowAnimation:(UITableViewRowAnimation)_animation {
    let rows = index_path_rows(env, index_paths);
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    if let Some(selected) = host.selected.as_mut() {
        for &(section, row) in &rows {
            if section == selected.0 && row <= selected.1 {
                selected.1 += 1;
            }
        }
    }
    reload_keeping_selection(env, this);
}
- (())deleteRowsAtIndexPaths:(id)index_paths // NSArray<NSIndexPath*>*
            withRowAnimation:(UITableViewRowAnimation)_animation {
    let rows = index_path_rows(env, index_paths);
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    if let Some((section, row)) = host.selected {
        if rows.contains(&(section, row)) {
            host.selected = None;
        } else {
            let above = rows.iter().filter(|r| r.0 == section && r.1 < row).count();
            host.selected = Some((section, row - above as NSInteger));
        }
    }
    reload_keeping_selection(env, this);
}

- (NSInteger)numberOfSections {
    load_if_needed(env, this);
    env.objc.borrow::<UITableViewHostObject>(this).sections.len() as NSInteger
}
- (NSInteger)numberOfRowsInSection:(NSInteger)section {
    load_if_needed(env, this);
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    host.sections[section as usize].rows.len() as NSInteger
}
- (CGRect)rectForRowAtIndexPath:(id)index_path { // NSIndexPath*
    load_if_needed(env, this);
    let row = index_path_row(env, index_path);
    row_rect(env, this, row)
}
- (id)indexPathForRowAtPoint:(CGPoint)point {
    load_if_needed(env, this);
    match row_at_point(env, this, point) {
        Some((section, row)) => new_index_path(env, &[section as NSUInteger, row as NSUInteger]),
        None => nil,
    }
}
- (id)cellForRowAtIndexPath:(id)index_path { // NSIndexPath*
    let row = index_path_row(env, index_path);
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    host.visible_cells
        .iter()
        .find(|&&(r, _)| r == row)
        .map_or(nil, |&(_, cell)| cell)
}
- (id)indexPathForCell:(id)cell { // UITableViewCell*
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    match host.visible_cells.iter().find(|&&(_, c)| c == cell) {
        Some(&((section, row), _)) => {
            new_index_path(env, &[section as NSUInteger, row as NSUInteger])
        }
        None => nil,
    }
}
- (id)visibleCells {
    let cells: Vec<id> = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .visible_cells
        .iter()
        .map(|&(_, cell)| cell)
        .collect();
    let array: id = msg_class![env; NSMutableArray array];
    for cell in cells {
        () = msg![env; array addObject:cell];
    }
    array
}

- (id)dequeueReusableCellWithIdentifier:(id)identifier { // NSString*
    if identifier == nil {
        return nil;
    }
    let pool = env.objc.borrow::<UITableViewHostObject>(this).reuse_pool.clone();
    for (i, cell) in pool.into_iter().enumerate() {
        let cell_identifier: id = msg![env; cell reuseIdentifier];
        if msg![env; identifier isEqual:cell_identifier] {
            env.objc.borrow_mut::<UITableViewHostObject>(this).reuse_pool.remove(i);
            () = msg![env; cell prepareForReuse];
            // The pool's reference goes to the caller.
            return autorelease(env, cell);
        }
    }
    nil
}

- (id)indexPathForSelectedRow {
    match env.objc.borrow::<UITableViewHostObject>(this).selected {
        Some((section, row)) => new_index_path(env, &[section as NSUInteger, row as NSUInteger]),
        None => nil,
    }
}
- (())selectRowAtIndexPath:(id)index_path // NSIndexPath*
                  animated:(bool)animated
            scrollPosition:(UITableViewScrollPosition)position {
    load_if_needed(env, this);
    if index_path == nil {
        set_selected(env, this, None);
        return;
    }
    let row = index_path_row(env, index_path);
    set_selected(env, this, Some(row));
    if position != UITableViewScrollPositionNone {
        () = msg![env; this scrollToRowAtIndexPath:index_path
                                   atScrollPosition:position
                                           animated:animated];
    }
}
- (())deselectRowAtIndexPath:(id)index_path // NSIndexPath*
                    animated:(bool)_animated {
    let row = index_path_row(env, index_path);
    if env.objc.borrow::<UITableViewHostObject>(this).selected == Some(row) {
        set_selected(env, this, None);
    }
}

// Scrolls straight there, even when animated.
- (())scrollToRowAtIndexPath:(id)index_path // NSIndexPath*
            atScrollPosition:(UITableViewScrollPosition)position
                    animated:(bool)animated {
    load_if_needed(env, this);
    let row = index_path_row(env, index_path);
    let rect = row_rect(env, this, row);
    let bounds: CGRect = msg![env; this bounds];
    let y = match position {
        UITableViewScrollPositionTop => rect.origin.y,
        UITableViewScrollPositionMiddle => {
            rect.origin.y - (bounds.size.height - rect.size.height) / 2.0
        }
        UITableViewScrollPositionBottom => {
            rect.origin.y + rect.size.height - bounds.size.height
        }
        _ => {
            () = msg![env; this scrollRectToVisible:rect animated:animated];
            return;
        }
    };
    let content_size: CGSize = msg![env; this contentSize];
    let max_y = (content_size.height - bounds.size.height).max(0.0);
    let offset = CGPoint { x: bounds.origin.x, y: y.clamp(0.0, max_y) };
    () = msg![env; this setContentOffset:offset];
}

- (())setContentOffset:(CGPoint)offset {
    () = msg_super![env; this setContentOffset:offset];
    () = msg![env; this setNeedsLayout];
}

- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    load_if_needed(env, this);
    layout_cells(env, this);
}

// Taps select rows. Touches on cells come here through the responder chain.
- (())touchesBegan:(id)touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    let start: CGPoint = msg![env; touch locationInView:nil];
    let Some(row) = row_at_point(env, this, point) else {
        return;
    };
    env.objc.borrow_mut::<UITableViewHostObject>(this).touched = Some((row, start));
    if env.objc.borrow::<UITableViewHostObject>(this).allows_selection {
        set_highlighted(env, this, row, true);
    }
}
- (())touchesMoved:(id)touches // NSSet* of UITouch*
         withEvent:(id)event { // UIEvent*
    if let Some((row, start)) = env.objc.borrow::<UITableViewHostObject>(this).touched {
        let touch: id = msg![env; touches anyObject];
        let point: CGPoint = msg![env; touch locationInView:nil];
        if (point.x - start.x).hypot(point.y - start.y) > TAP_SLOP {
            // It's a scroll, not a tap.
            env.objc.borrow_mut::<UITableViewHostObject>(this).touched = None;
            set_highlighted(env, this, row, false);
        }
    }
    () = msg_super![env; this touchesMoved:touches withEvent:event];
}
- (())touchesEnded:(id)_touches // NSSet* of UITouch*
         withEvent:(id)_event { // UIEvent*
    let Some((row, _)) = env.objc.borrow_mut::<UITableViewHostObject>(this).touched.take() else {
        return;
    };
    set_highlighted(env, this, row, false);
    if env.objc.borrow::<UITableViewHostObject>(this).allows_selection {
        select_by_user(env, this, row);
    }
}
- (())touchesCancelled:(id)_touches // NSSet* of UITouch*
             withEvent:(id)_event { // UIEvent*
    if let Some((row, _)) = env.objc.borrow_mut::<UITableViewHostObject>(this).touched.take() {
        set_highlighted(env, this, row, false);
    }
}

@end

};

fn selector(env: &mut Environment, name: &str) -> SEL {
    env.objc
        .register_host_selector(name.to_string(), &mut env.mem)
}

/// Whether `object` (a data source or delegate) has an optional method.
fn responds(env: &mut Environment, object: id, name: &str) -> bool {
    if object == nil {
        return false;
    }
    let sel = selector(env, name);
    msg![env; object respondsToSelector:sel]
}

fn index_path_row(env: &mut Environment, index_path: id) -> Row {
    let section: NSInteger = msg![env; index_path section];
    let row: NSInteger = msg![env; index_path row];
    (section, row)
}

fn index_path_rows(env: &mut Environment, index_paths: id) -> Vec<Row> {
    let count: NSUInteger = msg![env; index_paths count];
    (0..count)
        .map(|i| {
            let index_path: id = msg![env; index_paths objectAtIndex:i];
            index_path_row(env, index_path)
        })
        .collect()
}

fn default_separator_color(env: &mut Environment) -> id {
    msg_class![env; UIColor colorWithWhite:(0.88 as CGFloat) alpha:(1.0 as CGFloat)]
}

/// The rows need asking for again before the next layout.
fn rows_changed(env: &mut Environment, this: id) {
    env.objc.borrow_mut::<UITableViewHostObject>(this).loaded = false;
    () = msg![env; this setNeedsLayout];
}

fn load_if_needed(env: &mut Environment, this: id) {
    if !env.objc.borrow::<UITableViewHostObject>(this).loaded {
        reload_keeping_selection(env, this);
    }
}

/// `reloadData`: asks the data source for everything again. Like on a
/// device, this deselects the selected row.
fn reload(env: &mut Environment, this: id) {
    env.objc.borrow_mut::<UITableViewHostObject>(this).selected = None;
    reload_keeping_selection(env, this);
}

fn reload_keeping_selection(env: &mut Environment, this: id) {
    // The cells and headers on screen go, to be made again.
    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    host.loaded = true;
    let old_cells: Vec<id> = host.visible_cells.drain(..).map(|(_, cell)| cell).collect();
    let old_headers: Vec<id> = host
        .sections
        .drain(..)
        .map(|section| section.header_view)
        .collect();
    for cell in old_cells {
        recycle_cell(env, this, cell);
    }
    for header in old_headers {
        if header != nil {
            () = msg![env; header removeFromSuperview];
            release(env, header);
        }
    }

    let data_source: id = msg![env; this dataSource];
    let delegate: id = msg![env; this delegate];
    if data_source == nil {
        set_content_height(env, this, 0.0);
        return;
    }
    if responds(env, data_source, "tableView:titleForFooterInSection:")
        || responds(env, delegate, "tableView:viewForFooterInSection:")
    {
        log_once!("TODO: UITableView section footers aren't shown");
    }

    let section_count: NSInteger = if responds(env, data_source, "numberOfSectionsInTableView:") {
        msg![env; data_source numberOfSectionsInTableView:this]
    } else {
        1
    };
    let &UITableViewHostObject {
        row_height,
        section_header_height,
        ..
    } = env.objc.borrow(this);

    let mut sections = Vec::new();
    let mut y: CGFloat = 0.0;
    for section in 0..section_count {
        let section_y = y;
        let mut header_view: id = if responds(env, delegate, "tableView:viewForHeaderInSection:") {
            msg![env; delegate tableView:this viewForHeaderInSection:section]
        } else {
            nil
        };
        if header_view != nil {
            retain(env, header_view);
        } else if responds(env, data_source, "tableView:titleForHeaderInSection:") {
            let title: id = msg![env; data_source tableView:this titleForHeaderInSection:section];
            if title != nil {
                header_view = make_header_view(env, title);
            }
        }
        let header_height: CGFloat =
            if responds(env, delegate, "tableView:heightForHeaderInSection:") {
                msg![env; delegate tableView:this heightForHeaderInSection:section]
            } else if header_view != nil {
                section_header_height
            } else {
                0.0
            };
        y += header_height;

        let row_count: NSInteger =
            msg![env; data_source tableView:this numberOfRowsInSection:section];
        let custom_heights = responds(env, delegate, "tableView:heightForRowAtIndexPath:");
        let mut rows = Vec::new();
        for row in 0..row_count {
            let height = if custom_heights {
                let index_path = new_index_path(env, &[section as NSUInteger, row as NSUInteger]);
                msg![env; delegate tableView:this heightForRowAtIndexPath:index_path]
            } else {
                row_height
            };
            rows.push((y, height));
            y += height;
        }
        sections.push(Section {
            y: section_y,
            header_height,
            header_view,
            rows,
        });
    }

    let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
    // A selected row that no longer exists isn't selected.
    if let Some((section, row)) = host.selected {
        let exists = sections
            .get(section as usize)
            .is_some_and(|s| (row as usize) < s.rows.len());
        if !exists {
            host.selected = None;
        }
    }
    host.sections = sections;
    set_content_height(env, this, y);
    () = msg![env; this setNeedsLayout];
}

fn set_content_height(env: &mut Environment, this: id, height: CGFloat) {
    env.objc
        .borrow_mut::<UITableViewHostObject>(this)
        .content_height = height;
    let bounds: CGRect = msg![env; this bounds];
    let size = CGSize {
        width: bounds.size.width,
        height,
    };
    () = msg![env; this setContentSize:size];
    // Don't stay scrolled past the end.
    let max_y = (height - bounds.size.height).max(0.0);
    if bounds.origin.y > max_y {
        let offset = CGPoint {
            x: bounds.origin.x,
            y: max_y,
        };
        () = msg![env; this setContentOffset:offset];
    }
}

/// A plain section header: the title in white on grey.
fn make_header_view(env: &mut Environment, title: id) -> id {
    let view: id = msg_class![env; UIView new];
    let color: id = msg_class![env; UIColor colorWithRed:(0.6 as CGFloat)
                                                   green:(0.65 as CGFloat)
                                                    blue:(0.7 as CGFloat)
                                                   alpha:(0.95 as CGFloat)];
    () = msg![env; view setBackgroundColor:color];
    () = msg![env; view setUserInteractionEnabled:false];
    let label: id = msg_class![env; UILabel new];
    () = msg![env; label setText:title];
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:(18.0 as CGFloat)];
    () = msg![env; label setFont:font];
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; label setTextColor:white];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; label setBackgroundColor:clear];
    // The label fills the header, apart from a margin (see layout_cells).
    () = msg![env; label setTag:1i32];
    () = msg![env; view addSubview:label];
    release(env, label);
    view
}

/// Puts a cell that's no longer shown in the reuse pool, if it can be
/// reused.
fn recycle_cell(env: &mut Environment, this: id, cell: id) {
    () = msg![env; cell removeFromSuperview];
    let identifier: id = msg![env; cell reuseIdentifier];
    if identifier != nil {
        env.objc
            .borrow_mut::<UITableViewHostObject>(this)
            .reuse_pool
            .push(cell);
    } else {
        release(env, cell);
    }
}

fn row_rect(env: &mut Environment, this: id, (section, row): Row) -> CGRect {
    let bounds: CGRect = msg![env; this bounds];
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    let (y, height) = host.sections[section as usize].rows[row as usize];
    CGRect {
        origin: CGPoint { x: 0.0, y },
        size: CGSize {
            width: bounds.size.width,
            height,
        },
    }
}

/// The row at a point in the table's co-ordinates (which include the scroll
/// offset).
fn row_at_point(env: &mut Environment, this: id, point: CGPoint) -> Option<Row> {
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    for (s, section) in host.sections.iter().enumerate() {
        for (r, &(y, height)) in section.rows.iter().enumerate() {
            if point.y >= y && point.y < y + height {
                return Some((s as NSInteger, r as NSInteger));
            }
        }
    }
    None
}

fn visible_cell(env: &mut Environment, this: id, row: Row) -> Option<id> {
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    host.visible_cells
        .iter()
        .find(|&&(r, _)| r == row)
        .map(|&(_, cell)| cell)
}

fn set_highlighted(env: &mut Environment, this: id, row: Row, highlighted: bool) {
    if let Some(cell) = visible_cell(env, this, row) {
        () = msg![env; cell setHighlighted:highlighted];
    }
}

fn set_selected(env: &mut Environment, this: id, row: Option<Row>) {
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<UITableViewHostObject>(this).selected,
        row,
    );
    if let Some(cell) = old.and_then(|old| visible_cell(env, this, old)) {
        () = msg![env; cell setSelected:false animated:false];
    }
    if let Some(cell) = row.and_then(|row| visible_cell(env, this, row)) {
        () = msg![env; cell setSelected:true animated:false];
    }
}

/// A tap on a row: like on a device, the delegate is asked first, then told.
fn select_by_user(env: &mut Environment, this: id, (section, row): Row) {
    let delegate: id = msg![env; this delegate];
    let mut index_path = new_index_path(env, &[section as NSUInteger, row as NSUInteger]);
    if responds(env, delegate, "tableView:willSelectRowAtIndexPath:") {
        index_path = msg![env; delegate tableView:this willSelectRowAtIndexPath:index_path];
        if index_path == nil {
            return;
        }
    }
    let new_row = index_path_row(env, index_path);
    let old_row = env.objc.borrow::<UITableViewHostObject>(this).selected;
    if old_row == Some(new_row) {
        return;
    }
    set_selected(env, this, Some(new_row));
    if let Some((section, row)) = old_row {
        if responds(env, delegate, "tableView:didDeselectRowAtIndexPath:") {
            let old_path = new_index_path(env, &[section as NSUInteger, row as NSUInteger]);
            () = msg![env; delegate tableView:this didDeselectRowAtIndexPath:old_path];
        }
    }
    if responds(env, delegate, "tableView:didSelectRowAtIndexPath:") {
        () = msg![env; delegate tableView:this didSelectRowAtIndexPath:index_path];
    }
}

/// Shows the cells, headers and separators for the part of the table on
/// screen, asking the data source for cells as they come into view.
fn layout_cells(env: &mut Environment, this: id) {
    let bounds: CGRect = msg![env; this bounds];
    let width = bounds.size.width;
    let top = bounds.origin.y;
    let bottom = top + bounds.size.height;
    let &UITableViewHostObject {
        separator_style,
        row_height,
        content_height,
        selected,
        ..
    } = env.objc.borrow(this);
    let separator_height: CGFloat = if separator_style == UITableViewCellSeparatorStyleNone {
        0.0
    } else {
        1.0
    };

    let mut wanted: Vec<(Row, CGRect)> = Vec::new();
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    for (s, section) in host.sections.iter().enumerate() {
        for (r, &(y, height)) in section.rows.iter().enumerate() {
            if y + height > top && y < bottom {
                let frame = CGRect {
                    origin: CGPoint { x: 0.0, y },
                    size: CGSize {
                        width,
                        height: height - separator_height,
                    },
                };
                wanted.push(((s as NSInteger, r as NSInteger), frame));
            }
        }
    }

    // Cells that scrolled away can be reused for the ones coming into view.
    let old: Vec<(Row, id)> = std::mem::take(
        &mut env
            .objc
            .borrow_mut::<UITableViewHostObject>(this)
            .visible_cells,
    );
    let mut kept = Vec::new();
    for (row, cell) in old {
        if wanted.iter().any(|&(r, _)| r == row) {
            kept.push((row, cell));
        } else {
            recycle_cell(env, this, cell);
        }
    }
    env.objc
        .borrow_mut::<UITableViewHostObject>(this)
        .visible_cells = kept;

    let data_source: id = msg![env; this dataSource];
    let delegate: id = msg![env; this delegate];
    let mut separator_ys = Vec::new();
    for &((section, row), frame) in &wanted {
        let cell = if let Some(cell) = visible_cell(env, this, (section, row)) {
            cell
        } else {
            let index_path = new_index_path(env, &[section as NSUInteger, row as NSUInteger]);
            let cell: id = msg![env; data_source tableView:this cellForRowAtIndexPath:index_path];
            assert!(
                cell != nil,
                "tableView:cellForRowAtIndexPath: returned nil for {:?}",
                (section, row)
            );
            retain(env, cell);
            () = msg![env; cell setSelected:(selected == Some((section, row))) animated:false];
            if responds(
                env,
                delegate,
                "tableView:willDisplayCell:forRowAtIndexPath:",
            ) {
                () = msg![env; delegate tableView:this
                                  willDisplayCell:cell
                                forRowAtIndexPath:index_path];
            }
            () = msg![env; this addSubview:cell];
            env.objc
                .borrow_mut::<UITableViewHostObject>(this)
                .visible_cells
                .push(((section, row), cell));
            cell
        };
        () = msg![env; cell setFrame:frame];
        separator_ys.push(frame.origin.y + frame.size.height);
    }

    // Plain tables carry on the lines below the last row.
    if separator_height > 0.0 && row_height > 0.0 {
        let mut y = content_height + row_height;
        while y - separator_height < bottom {
            if y > top {
                separator_ys.push(y - separator_height);
            }
            y += row_height;
        }
    } else if separator_height == 0.0 {
        separator_ys.clear();
    }
    layout_separators(env, this, &separator_ys, width, separator_height);

    // Headers stay at the top while their section is on screen.
    let headers: Vec<(id, CGFloat, CGFloat, CGFloat)> = {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        let sections = &host.sections;
        sections
            .iter()
            .enumerate()
            .filter(|(_, section)| section.header_view != nil)
            .map(|(i, section)| {
                let end = sections.get(i + 1).map_or(content_height, |next| next.y);
                (section.header_view, section.y, section.header_height, end)
            })
            .collect()
    };
    for (view, y, height, end) in headers {
        let y = y.max(top).min(end - height);
        let frame = CGRect {
            origin: CGPoint { x: 0.0, y },
            size: CGSize { width, height },
        };
        () = msg![env; view setFrame:frame];
        let label: id = msg![env; view viewWithTag:1i32];
        if label != nil {
            let label_frame = CGRect {
                origin: CGPoint { x: 12.0, y: 0.0 },
                size: CGSize {
                    width: (width - 24.0).max(0.0),
                    height,
                },
            };
            () = msg![env; label setFrame:label_frame];
        }
        () = msg![env; this addSubview:view];
    }
}

fn layout_separators(
    env: &mut Environment,
    this: id,
    ys: &[CGFloat],
    width: CGFloat,
    height: CGFloat,
) {
    let color: id = msg![env; this separatorColor];
    while env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .separators
        .len()
        < ys.len()
    {
        let view: id = msg_class![env; UIView new];
        () = msg![env; view setUserInteractionEnabled:false];
        env.objc
            .borrow_mut::<UITableViewHostObject>(this)
            .separators
            .push(view);
    }
    let separators = env
        .objc
        .borrow::<UITableViewHostObject>(this)
        .separators
        .clone();
    for (i, view) in separators.into_iter().enumerate() {
        let Some(&y) = ys.get(i) else {
            () = msg![env; view setHidden:true];
            continue;
        };
        let frame = CGRect {
            origin: CGPoint { x: 0.0, y },
            size: CGSize { width, height },
        };
        () = msg![env; view setFrame:frame];
        () = msg![env; view setBackgroundColor:color];
        () = msg![env; view setHidden:false];
        () = msg![env; this addSubview:view];
    }
}
