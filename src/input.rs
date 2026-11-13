use gpui::{prelude::*, *};
use std::ops::Range;

actions!(
    text_input,
    [Backspace, Delete, Left, Right, SelectLeft, SelectRight, SelectAll, Home, End, Paste, Cut, Copy, Submit, Cancel, Up, Down, FocusNext, FocusPrev]
);

pub fn bind_keys(cx: &mut App) {
    let c = Some("TextInput");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, c),
        KeyBinding::new("delete", Delete, c),
        KeyBinding::new("left", Left, c),
        KeyBinding::new("right", Right, c),
        KeyBinding::new("shift-left", SelectLeft, c),
        KeyBinding::new("shift-right", SelectRight, c),
        KeyBinding::new("secondary-a", SelectAll, c),
        KeyBinding::new("secondary-v", Paste, c),
        KeyBinding::new("secondary-c", Copy, c),
        KeyBinding::new("secondary-x", Cut, c),
        KeyBinding::new("home", Home, c),
        KeyBinding::new("cmd-left", Home, c),
        KeyBinding::new("end", End, c),
        KeyBinding::new("cmd-right", End, c),
        KeyBinding::new("enter", Submit, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("up", Up, c),
        KeyBinding::new("down", Down, c),
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrev, None),
    ]);
}

pub enum InputEvent {
    Changed(String),
    Submit(String),
    Cancel,
    Move(isize),
}

pub struct TextInput {
    pub focus: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    selecting: bool,
    multiline: bool,
    last_lines: Vec<(usize, Pixels, WrappedLine)>,
    line_height: Pixels,
    max: Option<usize>,
}

impl EventEmitter<InputEvent> for TextInput {}

impl TextInput {
    pub fn new(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle().tab_stop(true),
            content: "".into(),
            placeholder: placeholder.into(),
            selected: 0..0,
            reversed: false,
            marked: None,
            last_layout: None,
            last_bounds: None,
            selecting: false,
            multiline: false,
            last_lines: vec![],
            line_height: px(20.),
            max: None,
        }
    }

    pub fn max_chars(mut self, n: usize) -> Self {
        self.max = Some(n);
        self
    }

    pub fn multiline(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
        Self { multiline: true, line_height: px(24.), ..Self::new(placeholder, cx) }
    }

    pub fn text(&self) -> &str {
        &self.content
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.content = text.into();
        let end = self.content.len();
        self.selected = end..end;
        self.marked = None;
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.content = "".into();
        self.selected = 0..0;
        self.marked = None;
        cx.notify();
    }

    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn cursor(&self) -> usize {
        if self.reversed { self.selected.start } else { self.selected.end }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = offset
        } else {
            self.selected.end = offset
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        cx.notify();
    }

    fn prev_boundary(&self, offset: usize) -> usize {
        self.content[..offset].char_indices().next_back().map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        self.content[offset..].chars().next().map_or(offset, |c| offset + c.len_utf8())
    }

    fn index_for_point(&self, position: Point<Pixels>) -> usize {
        let (Some(b), lh) = (self.last_bounds, self.line_height) else { return 0 };
        let y = position.y - b.top();
        let Some((start, top, line)) = self.last_lines.iter().rev().find(|(_, top, _)| *top <= y).or(self.last_lines.first()) else {
            return 0;
        };
        let local = point(position.x - b.left(), (y - *top).max(px(0.)));
        start + line.closest_index_for_position(local, lh).unwrap_or_else(|i| i).min(line.len())
    }

    fn point_for_index(&self, offset: usize) -> Option<Point<Pixels>> {
        let (start, top, line) = self.last_lines.iter().rev().find(|(start, _, _)| *start <= offset)?;
        let p = line.position_for_index((offset - start).min(line.len()), self.line_height)?;
        Some(point(p.x, p.y + *top))
    }

    fn vertical(&self, dir: f32) -> usize {
        let (Some(b), Some(p)) = (self.last_bounds, self.point_for_index(self.cursor())) else { return self.cursor() };
        let y = p.y + self.line_height * (0.5 + dir);
        if y < px(0.) {
            return 0;
        }
        self.index_for_point(point(b.left() + p.x, b.top() + y))
    }

    fn index_for_mouse(&self, position: Point<Pixels>) -> usize {
        if self.multiline {
            return self.index_for_point(position);
        }
        match (self.last_bounds, self.last_layout.as_ref()) {
            (Some(b), Some(line)) if !self.content.is_empty() => {
                if position.y < b.top() {
                    0
                } else if position.y > b.bottom() {
                    self.content.len()
                } else {
                    line.closest_index_for_x(position.x - b.left())
                }
            }
            _ => 0,
        }
    }

    fn from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = (0, 0);
        for ch in self.content.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn to_utf16(&self, offset: usize) -> usize {
        self.content[..offset].chars().map(char::len_utf16).sum()
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.from_utf16(r.start)..self.from_utf16(r.end)
    }

    fn copy_selection(&self, cx: &mut Context<Self>) {
        if !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(self.content[self.selected.clone()].to_string()));
        }
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(&mut self, r: Range<usize>, actual: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let range = self.range_from_utf16(&r);
        actual.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: self.range_to_utf16(&self.selected), reversed: self.reversed })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(&mut self, r: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        let range = r.map(|r| self.range_from_utf16(&r)).or(self.marked.clone()).unwrap_or(self.selected.clone());
        let mut text = if self.multiline { text.replace("\r\n", "\n") } else { text.replace('\n', " ") };
        if let Some(max) = self.max {
            let kept = self.content.chars().count() - self.content[range.clone()].chars().count();
            let room = max.saturating_sub(kept);
            if let Some((cut, _)) = text.char_indices().nth(room) {
                text.truncate(cut);
            }
        }
        self.content = format!("{}{}{}", &self.content[..range.start], text, &self.content[range.end..]).into();
        let end = range.start + text.len();
        self.selected = end..end;
        self.marked = None;
        cx.emit(InputEvent::Changed(self.content.to_string()));
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        new_selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = r.map(|r| self.range_from_utf16(&r)).or(self.marked.clone()).unwrap_or(self.selected.clone());
        self.content = format!("{}{}{}", &self.content[..range.start], text, &self.content[range.end..]).into();
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selected = new_selected
            .map(|s| self.range_from_utf16(&s))
            .map(|s| range.start + s.start..range.start + s.end)
            .unwrap_or_else(|| range.start + text.len()..range.start + text.len());
        cx.emit(InputEvent::Changed(self.content.to_string()));
        cx.notify();
    }

    fn bounds_for_range(&mut self, r: Range<usize>, bounds: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        if self.multiline {
            let range = self.range_from_utf16(&r);
            let p = self.point_for_index(range.start)?;
            return Some(Bounds::new(bounds.origin + p, size(px(2.), self.line_height)));
        }
        let line = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&r);
        Some(Bounds::from_corners(
            point(bounds.left() + line.x_for_index(range.start), bounds.top()),
            point(bounds.left() + line.x_for_index(range.end), bounds.bottom()),
        ))
    }

    fn character_index_for_point(&mut self, p: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        if self.multiline {
            return Some(self.to_utf16(self.index_for_point(p)));
        }
        let local = self.last_bounds?.localize(&p)?;
        let idx = self.last_layout.as_ref()?.index_for_x(local.x)?;
        Some(self.to_utf16(idx))
    }
}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TextInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("TextInput")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(|this, _: &Backspace, window, cx| {
                if this.selected.is_empty() {
                    this.select_to(this.prev_boundary(this.cursor()), cx)
                }
                this.replace_text_in_range(None, "", window, cx)
            }))
            .on_action(cx.listener(|this, _: &Delete, window, cx| {
                if this.selected.is_empty() {
                    this.select_to(this.next_boundary(this.cursor()), cx)
                }
                this.replace_text_in_range(None, "", window, cx)
            }))
            .on_action(cx.listener(|this, _: &Left, _, cx| {
                let to = if this.selected.is_empty() { this.prev_boundary(this.cursor()) } else { this.selected.start };
                this.move_to(to, cx)
            }))
            .on_action(cx.listener(|this, _: &Right, _, cx| {
                let to = if this.selected.is_empty() { this.next_boundary(this.cursor()) } else { this.selected.end };
                this.move_to(to, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectLeft, _, cx| this.select_to(this.prev_boundary(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &SelectRight, _, cx| this.select_to(this.next_boundary(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| this.select_all_text(cx)))
            .on_action(cx.listener(|this, _: &Home, _, cx| this.move_to(0, cx)))
            .on_action(cx.listener(|this, _: &End, _, cx| this.move_to(this.content.len(), cx)))
            .on_action(cx.listener(|this, _: &Paste, window, cx| {
                if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    this.replace_text_in_range(None, &text, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Copy, _, cx| this.copy_selection(cx)))
            .on_action(cx.listener(|this, _: &Cut, window, cx| {
                this.copy_selection(cx);
                this.replace_text_in_range(None, "", window, cx)
            }))
            .on_action(cx.listener(|this, _: &Submit, window, cx| {
                if this.multiline {
                    return this.replace_text_in_range(None, "\n", window, cx);
                }
                let q = this.content.trim().to_string();
                if !q.is_empty() {
                    cx.emit(InputEvent::Submit(q));
                }
            }))
            .on_action(cx.listener(|_, _: &Cancel, _, cx| cx.emit(InputEvent::Cancel)))
            .on_action(cx.listener(|this, _: &Up, _, cx| if this.multiline { this.move_to(this.vertical(-1.), cx) } else { cx.emit(InputEvent::Move(-1)) }))
            .on_action(cx.listener(|this, _: &Down, _, cx| if this.multiline { this.move_to(this.vertical(1.), cx) } else { cx.emit(InputEvent::Move(1)) }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                window.focus(&this.focus);
                this.selecting = true;
                let i = this.index_for_mouse(e.position);
                if e.modifiers.shift { this.select_to(i, cx) } else { this.move_to(i, cx) }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.selecting = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.selecting = false))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.selecting {
                    this.select_to(this.index_for_mouse(e.position), cx)
                }
            }))
            .w_full()
            .map(|d| if self.multiline {
                d.text_size(px(15.)).line_height(self.line_height).child(TextArea { input: cx.entity() }).into_any_element()
            } else {
                d.h(px(20.)).flex().items_center().text_size(px(14.)).line_height(px(20.)).child(TextLine { input: cx.entity() }).into_any_element()
            })
    }
}

struct TextLine {
    input: Entity<TextInput>,
}

struct Prepaint {
    line: ShapedLine,
    caret: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextLine {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for TextLine {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Prepaint {
        let input = self.input.read(cx);
        let style = window.text_style();
        let (text, color) = if input.content.is_empty() {
            (input.placeholder.clone(), style.color.opacity(0.5))
        } else {
            (input.content.clone(), style.color)
        };
        let run = TextRun { len: text.len(), font: style.font(), color, background_color: None, underline: None, strikethrough: None };
        let runs = match &input.marked {
            Some(m) => [
                TextRun { len: m.start, ..run.clone() },
                TextRun { len: m.end - m.start, underline: Some(UnderlineStyle { color: Some(color), thickness: px(1.), wavy: false }), ..run.clone() },
                TextRun { len: text.len() - m.end, ..run },
            ]
            .into_iter()
            .filter(|r| r.len > 0)
            .collect(),
            None => vec![run],
        };
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window.text_system().shape_line(text, font_size, &runs, None);
        let sel = &input.selected;
        let (selection, caret) = if sel.is_empty() {
            let x = bounds.left() + line.x_for_index(input.cursor());
            (None, Some(fill(Bounds::new(point(x, bounds.top()), size(px(1.5), bounds.size.height)), rgb(0x4a9eff))))
        } else {
            let quad = fill(
                Bounds::from_corners(
                    point(bounds.left() + line.x_for_index(sel.start), bounds.top()),
                    point(bounds.left() + line.x_for_index(sel.end), bounds.bottom()),
                ),
                rgba(0x4a9eff55),
            );
            (Some(quad), None)
        };
        Prepaint { line, caret, selection }
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), p: &mut Prepaint, window: &mut Window, cx: &mut App) {
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.input.clone()), cx);
        if let Some(s) = p.selection.take() {
            window.paint_quad(s);
        }
        p.line.paint(bounds.origin, window.line_height(), window, cx).ok();
        if focus.is_focused(window) {
            if let Some(c) = p.caret.take() {
                window.paint_quad(c);
            }
        }
        let line = p.line.clone();
        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

struct TextArea {
    input: Entity<TextInput>,
}

impl IntoElement for TextArea {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

fn area_runs(input: &TextInput, window: &Window) -> (SharedString, Vec<TextRun>, Pixels) {
    let style = window.text_style();
    let (text, color) = if input.content.is_empty() { (input.placeholder.clone(), style.color.opacity(0.5)) } else { (input.content.clone(), style.color) };
    let run = TextRun { len: text.len(), font: style.font(), color, background_color: None, underline: None, strikethrough: None };
    (text, vec![run], style.font_size.to_pixels(window.rem_size()))
}

impl Element for TextArea {
    type RequestLayoutState = ();
    type PrepaintState = Vec<(usize, Pixels, WrappedLine)>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, _: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let input = self.input.clone();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let input = input.read(cx);
            let (text, runs, font_size) = area_runs(input, window);
            let rows: usize = window.text_system().shape_text(text, font_size, &runs, width, None).map(|ls| ls.iter().map(|l| l.wrap_boundaries().len() + 1).sum()).unwrap_or(1);
            size(width.unwrap_or(px(0.)), input.line_height * rows.max(1) as f32)
        });
        (id, ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let (text, runs, font_size) = area_runs(input, window);
        let lh = input.line_height;
        let mut out = vec![];
        let (mut start, mut top) = (0, px(0.));
        for line in window.text_system().shape_text(text, font_size, &runs, Some(bounds.size.width), None).unwrap_or_default() {
            let rows = line.wrap_boundaries().len() + 1;
            let len = line.len();
            out.push((start, top, line));
            start += len + 1;
            top += lh * rows as f32;
        }
        out
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), lines: &mut Self::PrepaintState, window: &mut Window, cx: &mut App) {
        let focus = self.input.read(cx).focus.clone();
        window.handle_input(&focus, ElementInputHandler::new(bounds, self.input.clone()), cx);
        let lines = std::mem::take(lines);
        let (empty, sel, cursor, lh) = {
            let i = self.input.read(cx);
            (i.content.is_empty(), i.selected.clone(), i.cursor(), i.line_height)
        };
        self.input.update(cx, |input, _| {
            input.last_lines = lines.clone();
            input.last_bounds = Some(bounds);
        });
        let input = self.input.read(cx);
        let at = |o: usize| input.point_for_index(o).map(|p| bounds.origin + p);
        if !sel.is_empty() {
            if let (Some(a), Some(b)) = (at(sel.start), at(sel.end)) {
                let band = rgba(0x4a9eff55);
                let mut y = a.y;
                while y <= b.y {
                    let left = if y == a.y { a.x } else { bounds.left() };
                    let right = if y == b.y { b.x } else { bounds.right() };
                    if right > left {
                        window.paint_quad(fill(Bounds::from_corners(point(left, y), point(right, y + lh)), band));
                    }
                    y += lh;
                }
            }
        }
        let caret = if sel.is_empty() && focus.is_focused(window) { if empty { Some(bounds.origin) } else { at(cursor) } } else { None };
        for (_, top, line) in &lines {
            line.paint(point(bounds.left(), bounds.top() + *top), lh, TextAlign::Left, None, window, cx).ok();
        }
        if let Some(c) = caret {
            window.paint_quad(fill(Bounds::new(c, size(px(1.5), lh)), rgb(0x4a9eff)));
        }
    }
}
