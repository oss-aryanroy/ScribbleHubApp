use super::*;
use crate::rich::Hl;
use crate::writer::{self, Card, Draft, Look, Snap, SnapKind};
use scraper::Html;
use std::collections::HashMap;
use std::ops::Range;

pub struct Running {
    pub start: i64,
    pub mins: u32,
    pub words: i64,
}

#[derive(Default)]
pub struct Find {
    pub all: bool,
    pub case: bool,
    pub whole: bool,
    pub cur: usize,
    pub texts: Vec<(String, String, String)>,
    pub queue: Vec<(String, String)>,
    pub fetching: bool,
}

pub enum Pending {
    Select(String),
    ReplaceAll(String, String),
    Title(String),
}

pub struct Desk {
    pub data: writer::Data,
    pub session: i64,
    base: Option<usize>,
    pub focus: bool,
    pub aa: bool,
    pub sprint: Option<Running>,
    pub sprint_mins: u32,
    _tick: Task<()>,
    _save: Task<()>,
    _data: Task<()>,
    pub key: String,
    pub sid: String,
    pub url: String,
    pub saved_at: Option<i64>,
    last_auto: i64,
    pub offer: Option<Draft>,
    pub hints: [bool; 4],
    pub find: Option<Find>,
    pub card: Option<(String, String)>,
    pub chapters: HashMap<String, Vec<site::ChapterRow>>,
    pub editing_goals: bool,
    pub editing_deadline: bool,
    pub pending: Option<Pending>,
    pub waiting: Vec<Draft>,
}

impl Desk {
    pub fn new() -> Self {
        Desk {
            data: writer::Data::load(),
            session: 0,
            base: None,
            focus: false,
            aa: false,
            sprint: None,
            sprint_mins: 20,
            _tick: Task::ready(()),
            _save: Task::ready(()),
            _data: Task::ready(()),
            key: String::new(),
            sid: String::new(),
            url: String::new(),
            saved_at: None,
            last_auto: 0,
            offer: None,
            hints: [false; 4],
            find: None,
            card: None,
            chapters: HashMap::new(),
            editing_goals: false,
            editing_deadline: false,
            pending: None,
            waiting: Draft::waiting(),
        }
    }
}

pub struct DeskInputs {
    pub daily: Entity<TextInput>,
    pub per_session: Entity<TextInput>,
    pub dl_label: Entity<TextInput>,
    pub dl_words: Entity<TextInput>,
    pub dl_date: Entity<TextInput>,
    pub summary: Entity<TextInput>,
    pub notes: Entity<TextInput>,
    pub idea: Entity<TextInput>,
    pub find: Entity<TextInput>,
    pub replace: Entity<TextInput>,
    pub snap_name: Entity<TextInput>,
}

impl DeskInputs {
    pub fn new(cx: &mut Context<Shell>) -> Self {
        DeskInputs {
            daily: cx.new(|cx| TextInput::new("1000", cx).max_chars(6)),
            per_session: cx.new(|cx| TextInput::new("500", cx).max_chars(6)),
            dl_label: cx.new(|cx| TextInput::new("Volume 2 draft", cx).max_chars(60)),
            dl_words: cx.new(|cx| TextInput::new("50000", cx).max_chars(7)),
            dl_date: cx.new(|cx| TextInput::new("2027-03-01", cx).max_chars(10)),
            summary: cx.new(|cx| TextInput::multiline("What happens in this chapter?", cx).max_chars(600)),
            notes: cx.new(|cx| TextInput::multiline("Notes to self", cx).max_chars(4000)),
            idea: cx.new(|cx| TextInput::new("A chapter idea…", cx).max_chars(150)),
            find: cx.new(|cx| TextInput::new("Find", cx)),
            replace: cx.new(|cx| TextInput::new("Replace with", cx)),
            snap_name: cx.new(|cx| TextInput::new("Name this version", cx).max_chars(60)),
        }
    }
}

pub struct OutlineView {
    pub sid: String,
    pub title: String,
    pub loading: bool,
}

pub struct HistoryView {
    pub title: String,
    pub snaps: Vec<Snap>,
    pub sel: Option<usize>,
}

pub struct CardDrag {
    pub key: String,
    pub title: String,
    pub bg: u32,
    pub fg: u32,
}

impl Render for CardDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px_4().py_2().rounded(px(10.)).bg(rgb(self.bg)).text_color(rgb(self.fg)).shadow_lg().font_family(SERIF).text_size(px(15.)).child(self.title.clone())
    }
}

fn ring(d: f32, w: f32, frac: f32, color: u32, track: u32) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, _, window, _| {
            let c = b.center();
            let r = d / 2. - w / 2.;
            let arc = |from: f32, to: f32| {
                let steps = (((to - from) / std::f32::consts::TAU) * 96.).ceil().max(2.) as usize;
                (0..=steps).map(|i| from + (to - from) * i as f32 / steps as f32).map(|a| point(c.x + px(r * a.cos()), c.y + px(r * a.sin()))).collect::<Vec<_>>()
            };
            let top = -std::f32::consts::FRAC_PI_2;
            let mut track_path = PathBuilder::stroke(px(w));
            track_path.add_polygon(&arc(0., std::f32::consts::TAU), true);
            if let Ok(path) = track_path.build() {
                window.paint_path(path, rgb(track));
            }
            let frac = frac.clamp(0., 1.);
            if frac > 0.001 {
                let mut p = PathBuilder::stroke(px(w));
                p.add_polygon(&arc(top, top + std::f32::consts::TAU * frac), false);
                if let Ok(path) = p.build() {
                    window.paint_path(path, rgb(color));
                }
            }
        },
    )
    .flex_none()
    .size(px(d))
}

fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f32;
    let m = |s: u32| ((ch(a, s) + (ch(b, s) - ch(a, s)) * t).round() as u32) << s;
    m(16) | m(8) | m(0)
}

fn num(input: &Entity<TextInput>, cx: &App) -> Option<u32> {
    input.read(cx).text().trim().replace(',', "").parse().ok()
}

pub fn draft_key(url: &str) -> String {
    let id = |seg: &str| url.split(seg).nth(1).map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>()).unwrap_or_default();
    if url.contains("/editchapter/") { format!("c{}", id("/editchapter/")) } else { format!("new-{}", id("/addchapter/")) }
}

impl Shell {
    pub(crate) fn on_text_changed(&mut self, cx: &mut Context<Self>) {
        let n = self.c_body.read(cx).doc().word_count();
        if let Some(prev) = self.w.base {
            let delta = n as i64 - prev as i64;
            if delta != 0 {
                self.w.data.add_words(writer::today(), delta);
                self.w.session += delta;
                if let Some(s) = self.w.sprint.as_mut() {
                    s.words += delta;
                }
                self.save_data_later(cx);
            }
        }
        self.w.base = Some(n);
        self.autosave_later(cx);
        cx.notify();
    }

    pub(crate) fn save_data_later(&mut self, cx: &mut Context<Self>) {
        self.w._data = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            this.update(cx, |s, _| s.w.data.save()).ok();
        });
    }

    pub(crate) fn autosave_later(&mut self, cx: &mut Context<Self>) {
        if self.w.key.is_empty() {
            return;
        }
        self.w._save = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |s, cx| s.autosave(cx)).ok();
        });
    }

    fn autosave(&mut self, cx: &mut Context<Self>) {
        if self.w.key.is_empty() || !matches!(self.stack.last(), Some(Page::ChapterForm(_))) {
            return;
        }
        let (html, words) = {
            let e = self.c_body.read(cx);
            (e.html(), e.doc().word_count())
        };
        let title = self.c_title.read(cx).text().to_string();
        let d = Draft { key: self.w.key.clone(), title: title.clone(), html: html.clone(), url: self.w.url.clone(), sid: self.w.sid.clone(), series: self.form_series.clone(), updated: writer::now(), uploaded: false, words };
        d.save();
        self.w.saved_at = Some(d.updated);
        if d.updated - self.w.last_auto >= 300 && writer::add_snap(&self.w.key, Snap { at: d.updated, kind: SnapKind::Auto, name: String::new(), title, html, words }) {
            self.w.last_auto = d.updated;
        }
        self.w.waiting = Draft::waiting();
        cx.notify();
    }

    pub(crate) fn desk_chapter_loaded(&mut self, url: &str, html: &str, saved: &str, title: &str, cx: &mut Context<Self>) {
        self.w.key = draft_key(url);
        self.w.focus = false;
        self.w.find = None;
        self.w.url = url.to_string();
        self.w.sid = if url.contains("/addchapter/") {
            url.split("/addchapter/").nth(1).unwrap_or_default().chars().take_while(char::is_ascii_digit).collect()
        } else {
            Html::parse_document(html).select(&scraper::Selector::parse("#ficid").unwrap()).next().and_then(|e| e.value().attr("value")).unwrap_or_default().to_string()
        };
        self.w.base = Some(self.c_body.read(cx).doc().word_count());
        self.w.saved_at = None;
        self.w.last_auto = writer::now();
        let site_html = self.c_body.read(cx).html();
        self.w.offer = Draft::load(&self.w.key).filter(|d| !d.uploaded && (d.html != site_html || d.title != title) && !d.html.is_empty());
        if !saved.trim().is_empty() {
            let words = self.c_body.read(cx).doc().word_count();
            writer::add_snap(&self.w.key, Snap { at: writer::now(), kind: SnapKind::Site, name: String::new(), title: title.to_string(), html: saved.to_string(), words });
        }
        if let Some(cid) = self.w.key.strip_prefix('c').map(String::from) {
            let sid = self.w.sid.clone();
            self.select_card(&sid, &cid, cx);
        }
        match self.w.pending.take() {
            Some(Pending::Select(needle)) => {
                let f = self.w.find.get_or_insert_with(Find::default);
                let (case, whole) = (f.case, f.whole);
                let text = self.c_body.read(cx).doc().text.clone();
                if let Some(r) = writer::find_all(&text, &needle, case, whole).into_iter().next() {
                    self.c_body.update(cx, |e, cx| e.select(r, cx));
                }
            }
            Some(Pending::ReplaceAll(needle, with)) => {
                let f = self.w.find.get_or_insert_with(Find::default);
                let (case, whole) = (f.case, f.whole);
                let text = self.c_body.read(cx).doc().text.clone();
                let hits = writer::find_all(&text, &needle, case, whole);
                self.c_body.update(cx, |e, cx| e.replace_ranges(&hits, &with, cx));
                self.form_state = FormState::Done(format!("Replaced {} — check it, then save", hits.len()).into());
            }
            Some(Pending::Title(t)) => self.c_title.update(cx, |i, cx| i.set_text(t, cx)),
            None => {}
        }
        self.apply_look(cx);
    }

    pub(crate) fn desk_saved_to_site(&mut self) {
        if let Some(mut d) = Draft::load(&self.w.key) {
            d.uploaded = true;
            d.save();
            writer::add_snap(&self.w.key, Snap { at: writer::now(), kind: SnapKind::Site, name: String::new(), title: d.title.clone(), html: d.html.clone(), words: d.words });
        }
        self.w.waiting = Draft::waiting();
    }

    pub(crate) fn restore_draft(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.w.offer.take() else { return };
        self.c_title.update(cx, |i, cx| i.set_text(d.title.clone(), cx));
        self.c_body.update(cx, |e, cx| e.load_undoable(&d.html, cx));
        self.w.base = Some(self.c_body.read(cx).doc().word_count());
        cx.notify();
    }

    pub(crate) fn discard_draft(&mut self, cx: &mut Context<Self>) {
        if let Some(mut d) = self.w.offer.take() {
            writer::add_snap(&d.key, Snap { at: d.updated, kind: SnapKind::Named, name: "Discarded local copy".into(), title: d.title.clone(), html: d.html.clone(), words: d.words });
            d.uploaded = true;
            d.save();
        }
        self.w.waiting = Draft::waiting();
        cx.notify();
    }

    pub(crate) fn apply_look(&self, cx: &mut Context<Self>) {
        let l = self.w.data.look.clone();
        let focus = self.w.focus;
        self.c_body.update(cx, |e, cx| {
            e.family = l.family().into();
            e.size = l.size;
            e.spacing = l.line();
            e.spotlight = focus && l.highlight;
            if focus {
                e.width = l.width;
            }
            cx.notify();
        });
    }

    fn set_look(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Look)) {
        f(&mut self.w.data.look);
        self.w.data.save();
        self.apply_look(cx);
        cx.notify();
    }

    pub(crate) fn toggle_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.stack.last(), Some(Page::ChapterForm(_))) {
            return;
        }
        self.w.focus = !self.w.focus;
        self.w.aa = false;
        self.apply_look(cx);
        window.focus(&self.c_body.read(cx).focus.clone());
        cx.notify();
    }

    pub(crate) fn reveal_caret(&mut self, top: Pixels, bottom: Pixels, cx: &mut Context<Self>) {
        if !(self.w.focus && self.w.data.look.typewriter) {
            return self.reveal(top, bottom, cx);
        }
        let b = self.scroll.bounds();
        let mid = (top + bottom) / 2.;
        let want = b.top() + b.size.height * 0.45;
        if (mid - want).abs() < px(2.) {
            return;
        }
        let mut off = self.scroll.offset();
        off.y = (off.y - (mid - want)).min(px(0.));
        self.scroll.set_offset(off);
        cx.notify();
    }

    fn goal_pill(&self) -> Div {
        let p = self.pal();
        let today = self.w.data.words_on(writer::today());
        let goal = self.w.data.daily_goal.max(1);
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .pl(px(6.))
            .pr(px(14.))
            .rounded_full()
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .text_size(px(13.))
            .child(ring(26., 4., today as f32 / goal as f32, p.accent, p.line))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(group_digits(&today.to_string())))
            .child(div().text_color(rgb(p.muted)).child(format!("/ {} today", group_digits(&goal.to_string()))))
    }

    fn sprint_pill(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let p = self.pal();
        let s = self.w.sprint.as_ref()?;
        let left = (s.mins as i64 * 60 - (writer::now() - s.start)).max(0);
        Some(
            div()
                .id("sprint-pill")
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(36.))
                .px(px(14.))
                .rounded_full()
                .bg(rgb(p.surface))
                .border_1()
                .border_color(rgb(p.line))
                .text_size(px(13.))
                .cursor_pointer()
                .child(svg().path("icons/timer.svg").size(px(15.)).text_color(rgb(p.accent)))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("{}:{:02}", left / 60, left % 60)))
                .child(div().text_color(rgb(p.accent)).font_weight(FontWeight::SEMIBOLD).child(format!("{:+}", s.words)))
                .tooltip(|_, cx| cx.new(|_| studio::Tip("Stop sprint")).into())
                .on_click(cx.listener(|this, _, _, cx| this.stop_sprint(cx))),
        )
    }

    pub(crate) fn focus_page(&self, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let hl: Vec<_> = self.matches(cx).into_iter().map(|r| (r, Hl::Back(0x6b5a1e))).collect();
        self.c_body.update(cx, |e, _| e.hl = hl);
        let title = self.c_title.read(cx).text().to_string();
        let words = self.c_body.read(cx).doc().word_count();
        let look = self.w.data.look.clone();
        let focus = self.c_body.read(cx).focus.clone();
        let exit = div()
            .id("focus-exit")
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(36.))
            .px(px(12.))
            .rounded(px(9.))
            .border_1()
            .border_color(rgb(p.line))
            .text_size(px(13.))
            .text_color(rgb(p.muted))
            .cursor_pointer()
            .hover(move |d| d.text_color(rgb(p.ink)))
            .child(div().text_size(px(11.)).px(px(5.)).rounded(px(4.)).border_1().border_color(rgb(p.line)).child("esc"))
            .child("Exit focus")
            .on_click(cx.listener(|this, _, window, cx| this.toggle_focus(window, cx)));
        let aa = div()
            .id("focus-aa")
            .h(px(36.))
            .min_w(px(44.))
            .px(px(12.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(9.))
            .border_1()
            .border_color(rgb(if self.w.aa { p.accent } else { p.line }))
            .text_color(rgb(if self.w.aa { p.accent } else { p.ink }))
            .font_family(SERIF)
            .text_size(px(15.))
            .cursor_pointer()
            .child("Aa")
            .tooltip(|_, cx| cx.new(|_| studio::Tip("Type settings")).into())
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                window.prevent_default();
                this.w.aa = !this.w.aa;
                cx.notify();
            }));
        let bar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .px(px(24.))
            .pt(px(TITLE_INSET - 12.))
            .pb(px(14.))
            .child(exit)
            .child(div().flex_1().min_w_0().text_center().text_size(px(13.)).text_color(rgb(p.muted)).truncate().child(if title.trim().is_empty() { "Untitled chapter".to_string() } else { title }))
            .children(self.sprint_pill(cx))
            .child(self.goal_pill())
            .child(div().relative().child(aa).children(self.w.aa.then(|| deferred(self.aa_panel(cx)).with_priority(1))));
        let saved = match (self.w.saved_at, self.form_state == FormState::Saving) {
            (_, true) => "Saving to Scribble Hub…".to_string(),
            (Some(t), _) => format!("Saved on this Mac · {}", writer::when(t)),
            (None, _) => "No changes yet".to_string(),
        };
        let page = div()
            .id("focus-page")
            .flex()
            .justify_center()
            .px(px(24.))
            .pt(px(24.))
            .pb(px(420.))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, window, _| {
                window.focus(&focus);
                window.prevent_default();
            }))
            .child(div().w(px(look.width)).child(self.c_body.clone()));
        div()
            .flex()
            .flex_col()
            .min_h_full()
            .child(bar)
            .child(page)
            .child(
                div()
                    .flex()
                    .gap_4()
                    .px(px(24.))
                    .py(px(12.))
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child(div().flex().items_center().gap(px(6.)).child(div().size(px(7.)).rounded_full().bg(rgb(if self.w.saved_at.is_some() { 0x5ac48a } else { p.line }))).child(saved))
                    .child(div().flex_1())
                    .child(format!("{} words · {} or esc to leave", group_digits(&words.to_string()), if cfg!(target_os = "macos") { "⇧⌘F" } else { "Ctrl+Shift+F" })),
            )
    }

    fn aa_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.pal();
        let l = self.w.data.look.clone();
        let label = |s: &'static str| div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.muted)).child(s);
        let opt = move |id: (&'static str, usize), text: &'static str, on: bool| {
            div()
                .id(id)
                .flex_1()
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(if on { p.accent } else { p.line }))
                .bg(rgb(if on { p.sel } else { p.bg }))
                .text_size(px(13.))
                .cursor_pointer()
                .child(text)
        };
        let fonts = writer::FONTS.iter().enumerate().map(|(i, (name, fam))| {
            opt(("aa-font", i), name, l.font as usize == i).font_family(*fam).text_size(px(15.)).on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                window.prevent_default();
                this.set_look(cx, |l| l.font = i as u8);
            }))
        }).collect::<Vec<_>>();
        let spacings = writer::SPACINGS.iter().enumerate().map(|(i, (name, _))| {
            opt(("aa-sp", i), name, l.spacing as usize == i).on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                window.prevent_default();
                this.set_look(cx, |l| l.spacing = i as u8);
            }))
        }).collect::<Vec<_>>();
        let stepper = |id: &'static str, value: String, minus: fn(&mut Look), plus: fn(&mut Look), cx: &mut Context<Self>| {
            let btn = |sub: usize, t: &'static str, f: fn(&mut Look), cx: &mut Context<Self>| {
                opt((id, sub), t, false).flex_none().w(px(40.)).on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    window.prevent_default();
                    this.set_look(cx, f);
                }))
            };
            div().flex().items_center().gap_2().child(btn(0, "−", minus, cx)).child(div().flex_1().text_center().text_size(px(13.)).child(value)).child(btn(1, "+", plus, cx))
        };
        let toggle = |id: &'static str, title: &'static str, hint: &'static str, on: bool, f: fn(&mut Look), cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap_3()
                .min_h(px(40.))
                .cursor_pointer()
                .child(div().flex_1().flex().flex_col().child(div().text_size(px(13.)).child(title)).child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(hint)))
                .child(div().flex_none().w(px(38.)).h(px(22.)).rounded_full().bg(rgb(if on { p.accent } else { p.line })).relative().child(div().absolute().top(px(3.)).left(px(if on { 19. } else { 3. })).size(px(16.)).rounded_full().bg(rgb(if on { p.on_accent } else { p.muted }))))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                    window.prevent_default();
                    this.set_look(cx, f);
                }))
        };
        div()
            .id("aa-panel")
            .occlude()
            .absolute()
            .top(px(44.))
            .right_0()
            .w(px(320.))
            .p(px(18.))
            .rounded(px(14.))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap(px(14.))
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.w.aa = false;
                cx.notify();
            }))
            .child(div().flex().flex_col().gap_2().child(label("Font")).child(div().flex().gap(px(6.)).children(fonts)))
            .child(div().flex().flex_col().gap_2().child(label("Size")).child(stepper("aa-size", format!("{}", l.size), |l| l.size = (l.size - 1.).max(13.), |l| l.size = (l.size + 1.).min(28.), cx)))
            .child(div().flex().flex_col().gap_2().child(label("Page width")).child(stepper("aa-w", format!("{} px", l.width), |l| l.width = (l.width - 40.).max(480.), |l| l.width = (l.width + 40.).min(1080.), cx)))
            .child(div().flex().flex_col().gap_2().child(label("Line spacing")).child(div().flex().gap(px(6.)).children(spacings)))
            .child(div().h(px(1.)).bg(rgb(p.line)))
            .child(toggle("aa-hl", "Highlight paragraph", "The rest of the page fades", l.highlight, |l| l.highlight = !l.highlight, cx))
            .child(toggle("aa-tw", "Typewriter scrolling", "Keep the line you type at mid-screen", l.typewriter, |l| l.typewriter = !l.typewriter, cx))
    }

    pub(crate) fn start_sprint(&mut self, mins: u32, cx: &mut Context<Self>) {
        self.w.sprint = Some(Running { start: writer::now(), mins, words: 0 });
        self.w._tick = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let done = this
                .update(cx, |s, cx| {
                    let over = s.w.sprint.as_ref().is_none_or(|r| writer::now() - r.start >= r.mins as i64 * 60);
                    if over {
                        s.stop_sprint(cx);
                    }
                    cx.notify();
                    over
                })
                .unwrap_or(true);
            if done {
                break;
            }
        });
        cx.notify();
    }

    pub(crate) fn stop_sprint(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.w.sprint.take() {
            let mins = (((writer::now() - r.start) as f32 / 60.).round() as u32).clamp(1, r.mins);
            self.w.data.sprints.push(writer::Sprint { at: r.start, mins, words: r.words });
            self.w.data.save();
            self.form_state = FormState::Done(format!("Sprint done: {} words in {mins} min", r.words).into());
        }
        self.w._tick = Task::ready(());
        cx.notify();
    }

    pub(crate) fn open_today(&mut self, cx: &mut Context<Self>) {
        self.browsing = false;
        self.want = None;
        self.w.waiting = Draft::waiting();
        if !matches!(self.stack.last(), Some(Page::Today)) {
            self.stack.push(Page::Today);
        }
        self.status = Status::Ready;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub(crate) fn today_page(&self, area: f32, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let d = &self.w.data;
        let today = writer::today();
        let words = d.words_on(today);

        let goals = if self.w.editing_goals {
            let save = self.big_button("goals-save", "Save goals", true, false).on_click(cx.listener(|this, _, _, cx| {
                if let Some(n) = num(&this.w_in.daily, cx).filter(|n| *n > 0) {
                    this.w.data.daily_goal = n;
                }
                if let Some(n) = num(&this.w_in.per_session, cx).filter(|n| *n > 0) {
                    this.w.data.session_goal = n;
                }
                this.w.editing_goals = false;
                this.w.data.save();
                cx.notify();
            }));
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(self.labelled("Words a day", None, self.field("g-daily", &self.w_in.daily, 0., cx)))
                .child(self.labelled("Words a session", None, self.field("g-session", &self.w_in.per_session, 0., cx)))
                .child(save)
                .into_any_element()
        } else {
            let edit = div()
                .id("goals-edit")
                .px(px(10.))
                .py(px(5.))
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(p.line))
                .text_size(px(12.))
                .text_color(rgb(p.muted))
                .cursor_pointer()
                .hover(move |d| d.text_color(rgb(p.ink)))
                .child("Change goals")
                .on_click(cx.listener(|this, _, _, cx| {
                    let (a, b) = (this.w.data.daily_goal.to_string(), this.w.data.session_goal.to_string());
                    this.w_in.daily.update(cx, |i, cx| i.set_text(a, cx));
                    this.w_in.per_session.update(cx, |i, cx| i.set_text(b, cx));
                    this.w.editing_goals = true;
                    cx.notify();
                }));
            div()
                .flex()
                .items_center()
                .gap(px(22.))
                .child(
                    div()
                        .relative()
                        .size(px(132.))
                        .flex_none()
                        .child(ring(132., 12., words as f32 / d.daily_goal.max(1) as f32, p.accent, p.line))
                        .child(div().absolute().top(px(20.)).left(px(20.)).child(ring(92., 8., self.w.session.max(0) as f32 / d.session_goal.max(1) as f32, 0x4a9eff, p.line))),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(div().child(div().text_size(px(28.)).font_weight(FontWeight::BOLD).child(group_digits(&words.to_string()))).child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("of {} today", group_digits(&d.daily_goal.to_string())))))
                        .child(
                            div()
                                .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD).text_color(rgb(0x4a9eff)).child(group_digits(&self.w.session.max(0).to_string())))
                                .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("of {} this session", group_digits(&d.session_goal.to_string())))),
                        )
                        .child(edit),
                )
                .into_any_element()
        };
        let goals = self.card("Goals", None).flex_1().min_w(px(300.)).child(goals);

        let (cur, best) = d.streak(today);
        let monday = today - writer::weekday(today) as i64;
        let week = (0..7).map(|i| {
            let day = monday + i;
            let w = d.words_on(day);
            let bg = if day > today { p.bg } else if w >= writer::STREAK_MIN { p.accent } else { p.line };
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(div().w_full().h(px(32.)).rounded(px(8.)).bg(rgb(bg)).when(day == today, |d| d.border_2().border_color(rgb(p.ink))))
                .child(div().text_size(px(11.)).text_color(rgb(p.muted)).child(["M", "T", "W", "T", "F", "S", "S"][i as usize]))
        });
        let streak = self
            .card("Streak", None)
            .flex_1()
            .min_w(px(280.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(div().size(px(44.)).rounded(px(12.)).bg(rgb(p.sel)).flex().items_center().justify_center().child(svg().path("icons/flame.svg").size(px(22.)).text_color(rgb(p.accent))))
                    .child(div().child(div().text_size(px(26.)).font_weight(FontWeight::BOLD).child(if cur == 1 { "1 day".to_string() } else { format!("{cur} days") })).child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("Best: {best}")))),
            )
            .child(div().flex().gap(px(6.)).children(week))
            .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(format!("A day counts from {} words.", writer::STREAK_MIN)));

        let deadline = self.deadline_card(today, cx);

        let weeks = 26;
        let start = monday - (weeks - 1) * 7;
        let total: i64 = (start..=today).map(|x| d.words_on(x)).sum();
        let active = (start..=today).filter(|x| d.words_on(*x) >= writer::STREAK_MIN).count();
        let level = |w: i64| match w {
            0 => p.line,
            w if w < writer::STREAK_MIN => mix(p.line, p.accent, 0.2),
            w if w < 800 => mix(p.line, p.accent, 0.45),
            w if w < 2000 => mix(p.line, p.accent, 0.72),
            _ => p.accent,
        };
        let cols = (0..weeks).map(|wk| {
            div().flex().flex_col().gap(px(4.)).children((0..7).map(move |dy| {
                let day = start + wk * 7 + dy;
                div().size(px(14.)).rounded(px(3.)).bg(rgb(if day > today { p.bg } else { level(d.words_on(day)) }))
            }))
        });
        let heat = self
            .card("Days written", Some(format!("{active} days · {} words in 6 months", group_digits(&total.to_string()))))
            .flex_1()
            .min_w(px(480.))
            .child(div().flex().gap(px(4.)).children(cols))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .text_color(rgb(p.muted))
                    .child("Less")
                    .children([p.line, mix(p.line, p.accent, 0.2), mix(p.line, p.accent, 0.45), mix(p.line, p.accent, 0.72), p.accent].map(|c| div().size(px(12.)).rounded(px(3.)).bg(rgb(c))))
                    .child("More"),
            );

        let pick = self.w.sprint_mins;
        let mins = [15u32, 20, 30].map(|m| {
            self.chip(("sp-min", m as usize), format!("{m} min"), pick == m).on_click(cx.listener(move |this, _, _, cx| {
                this.w.sprint_mins = m;
                cx.notify();
            }))
        });
        let running = self.w.sprint.as_ref().map(|r| {
            let left = (r.mins as i64 * 60 - (writer::now() - r.start)).max(0);
            (left, r.words)
        });
        let go = match running {
            Some((left, w)) => self.big_button("sp-stop", format!("Stop · {}:{:02} left · {:+} words", left / 60, left % 60, w), false, false).on_click(cx.listener(|this, _, _, cx| this.stop_sprint(cx))),
            None => self.big_button("sp-go", "Start sprint", true, false).on_click(cx.listener(move |this, _, _, cx| this.start_sprint(pick, cx))),
        };
        let best_sprint = d.sprints.iter().map(|s| s.words).max().unwrap_or(0);
        let recent = d.sprints.iter().rev().take(4).map(|s| {
            div()
                .flex()
                .justify_between()
                .text_size(px(13.))
                .child(format!("{} · {} min", writer::when(s.at), s.mins))
                .child(div().text_color(rgb(p.muted)).child(format!("{} words", group_digits(&s.words.to_string()))))
        });
        let sprint = self
            .card("Sprint", (best_sprint > 0).then(|| format!("best {best_sprint}")))
            .flex_1()
            .min_w(px(280.))
            .child(div().flex().gap_2().children(mins))
            .child(go)
            .children(recent)
            .when(d.sprints.is_empty(), |c| c.child(div().text_size(px(13.)).text_color(rgb(p.muted)).child("Write as much as you can before the timer ends. Words count in any chapter.")));

        let waiting = (!self.w.waiting.is_empty()).then(|| {
            let rows = self.w.waiting.iter().enumerate().map(|(i, dr)| {
                let url = dr.url.clone();
                let series = dr.series.clone();
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().font_family(SERIF).font_weight(FontWeight::BOLD).truncate().child(if dr.title.trim().is_empty() { "Untitled chapter".to_string() } else { dr.title.clone() }))
                            .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(format!("{} · {} words · saved {}", dr.series, group_digits(&dr.words.to_string()), writer::when(dr.updated)))),
                    )
                    .child(self.big_button(("wait-open", i), "Open", false, false).on_click(cx.listener(move |this, _, _, cx| {
                        this.form_series = series.clone();
                        this.form_state = FormState::Idle;
                        this.publish_armed = false;
                        this.browsing = false;
                        this.open(url.clone(), false, cx);
                    })))
            }).collect::<Vec<_>>();
            self.card("On this Mac, not on Scribble Hub yet", Some(format!("{}", self.w.waiting.len()))).w_full().children(rows)
        });

        let _ = area;
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(44.))
            .pt(px(TITLE_INSET))
            .pb(px(56.))
            .child(self.page_head("Today", Some(writer::long_date(today)), cx))
            .children(waiting)
            .child(div().flex().flex_wrap().gap(px(18.)).child(goals).child(streak).child(deadline))
            .child(div().flex().flex_wrap().gap(px(18.)).child(heat).child(sprint))
    }

    fn deadline_card(&self, today: i64, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let card = self.card("Deadline", None).flex_1().min_w(px(300.));
        if self.w.editing_deadline || self.w.data.deadline.is_none() {
            let set = self.big_button("dl-set", "Set deadline", true, false).on_click(cx.listener(move |this, _, _, cx| {
                let label = this.w_in.dl_label.read(cx).text().trim().to_string();
                let by = writer::parse_day(this.w_in.dl_date.read(cx).text());
                match (num(&this.w_in.dl_words, cx).filter(|n| *n > 0), by.filter(|b| *b >= writer::today())) {
                    (Some(words), Some(by)) => {
                        let start = this.w.data.deadline.as_ref().map_or(writer::today(), |d| d.start);
                        this.w.data.deadline = Some(writer::Deadline { label: if label.is_empty() { "Deadline".into() } else { label }, words, start, by });
                        this.w.editing_deadline = false;
                        this.w.data.save();
                        this.form_state = FormState::Idle;
                    }
                    _ => this.form_state = FormState::Error("A deadline needs a word count and a future date (YYYY-MM-DD).".into()),
                }
                cx.notify();
            }));
            let clear = self.w.data.deadline.is_some().then(|| {
                self.big_button("dl-clear", "Remove", false, false).on_click(cx.listener(|this, _, _, cx| {
                    this.w.data.deadline = None;
                    this.w.editing_deadline = false;
                    this.w.data.save();
                    cx.notify();
                }))
            });
            return card
                .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child("Set a target and the app works out the words a day."))
                .child(self.labelled("What", None, self.field("dl-label", &self.w_in.dl_label, 0., cx)))
                .child(div().flex().gap_3().child(self.labelled("Words", None, self.field("dl-words", &self.w_in.dl_words, 0., cx)).flex_1()).child(self.labelled("By (YYYY-MM-DD)", None, self.field("dl-date", &self.w_in.dl_date, 0., cx)).flex_1()))
                .child(div().flex().gap_2().child(set).children(clear));
        }
        let dl = self.w.data.deadline.clone().unwrap_or_default();
        let (written, left, per_day) = writer::pace(&dl, &self.w.data, today);
        let frac = written as f32 / dl.words.max(1) as f32;
        let tile = |v: String, l: &'static str, hot: bool| {
            div().flex_1().p(px(10.)).rounded(px(10.)).bg(rgb(p.bg)).child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).when(hot, |d| d.text_color(rgb(p.accent))).child(v)).child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(l))
        };
        let verdict = if written >= dl.words as i64 {
            "Done. Well written.".to_string()
        } else if left == 0 {
            "The date has passed.".to_string()
        } else if per_day <= self.w.data.daily_goal as i64 {
            "On pace: your daily goal covers it.".to_string()
        } else {
            format!("Behind your daily goal by {} a day.", group_digits(&(per_day - self.w.data.daily_goal as i64).to_string()))
        };
        let edit = div()
            .id("dl-edit")
            .px(px(10.))
            .py(px(5.))
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(p.line))
            .text_size(px(12.))
            .text_color(rgb(p.muted))
            .cursor_pointer()
            .child("Edit")
            .on_click(cx.listener(move |this, _, _, cx| {
                let d = this.w.data.deadline.clone().unwrap_or_default();
                this.w_in.dl_label.update(cx, |i, cx| i.set_text(d.label.clone(), cx));
                this.w_in.dl_words.update(cx, |i, cx| i.set_text(d.words.to_string(), cx));
                this.w_in.dl_date.update(cx, |i, cx| i.set_text(writer::key(d.by), cx));
                this.w.editing_deadline = true;
                cx.notify();
            }));
        card.child(
            div()
                .flex()
                .items_baseline()
                .gap_2()
                .child(div().flex_1().font_family(SERIF).text_size(px(18.)).line_height(px(24.)).child(format!("{} — {} words by {}", dl.label, group_digits(&dl.words.to_string()), writer::short_date(dl.by))))
                .child(edit),
        )
        .child(div().h(px(10.)).rounded_full().bg(rgb(p.line)).child(div().h_full().w(relative(frac.clamp(0., 1.))).rounded_full().bg(rgb(p.accent))))
        .child(div().flex().gap_2().child(tile(group_digits(&written.to_string()), "written", false)).child(tile(left.to_string(), "days left", false)).child(tile(group_digits(&per_day.to_string()), "words a day", true)))
        .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(verdict))
    }

    pub(crate) fn open_history(&mut self, cx: &mut Context<Self>) {
        if self.w.key.is_empty() {
            return;
        }
        self.autosave(cx);
        let snaps = writer::snaps(&self.w.key);
        let sel = snaps.len().checked_sub(1);
        let title = self.c_title.read(cx).text().to_string();
        self.stack.push(Page::History(Box::new(HistoryView { title, snaps, sel })));
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub(crate) fn history_page(&self, h: &HistoryView, area: f32, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let now_text = self.c_body.read(cx).doc().text.clone();
        let now_words = self.c_body.read(cx).doc().word_count();
        let list_w = 280.;
        let cmp_w = (area - 88. - list_w - 24.).max(480.);
        let col_w = (cmp_w - 2. - 40. * 2.) / 2.;
        let kind_label = |s: &Snap| match s.kind {
            SnapKind::Auto => format!("{} · autosaved", writer::when(s.at)),
            SnapKind::Named => format!("{} · named", writer::when(s.at)),
            SnapKind::Site => format!("{} · on Scribble Hub", writer::when(s.at)),
        };
        let mut list = div().flex_none().w(px(list_w)).flex().flex_col().gap(px(4.)).child(
            div()
                .px(px(12.))
                .py(px(10.))
                .rounded(px(10.))
                .border_1()
                .border_color(rgb(p.line))
                .bg(rgb(p.surface))
                .child(div().font_weight(FontWeight::SEMIBOLD).child("Now"))
                .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(format!("{} words · in the editor", group_digits(&now_words.to_string())))),
        );
        for (i, s) in h.snaps.iter().enumerate().rev() {
            let on = h.sel == Some(i);
            let name = if s.name.is_empty() { kind_label(s) } else { format!("“{}”", s.name) };
            let sub = if s.name.is_empty() { format!("{} words", group_digits(&s.words.to_string())) } else { format!("{} · {} words", kind_label(s), group_digits(&s.words.to_string())) };
            list = list.child(
                div()
                    .id(("snap", i))
                    .px(px(12.))
                    .py(px(10.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgb(if on { p.accent } else { p.bg }))
                    .when(on, |d| d.bg(rgb(p.sel)))
                    .cursor_pointer()
                    .hover(move |d| d.bg(rgb(p.surface)))
                    .child(div().font_weight(if on { FontWeight::SEMIBOLD } else { FontWeight::NORMAL }).child(name))
                    .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(sub))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(Page::History(h)) = this.stack.last_mut() {
                            h.sel = Some(i);
                        }
                        cx.notify();
                    })),
            );
        }
        list = list.child(div().px(px(12.)).pt(px(8.)).text_size(px(12.)).line_height(px(17.)).text_color(rgb(p.muted)).child("Kept on this Mac: a version every 5 minutes of writing (the last 40), each one Scribble Hub had, and the ones you name."));

        let snap = h.sel.and_then(|i| h.snaps.get(i));
        let cell = move |parts: &[writer::Part], old: bool| {
            let mut text = String::new();
            let mut hls = vec![];
            for part in parts {
                let (t, style) = match part {
                    writer::Part::Same(t) => (t, None),
                    writer::Part::Gone(t) => (t, Some(HighlightStyle { background_color: Some(rgb(0x3d1d21).into()), color: Some(rgb(0xffb3b6).into()), strikethrough: Some(StrikethroughStyle { thickness: px(1.), color: None }), ..Default::default() })),
                    writer::Part::New(t) => (t, Some(HighlightStyle { background_color: Some(rgb(0x13304f).into()), color: Some(rgb(0xb9dcff).into()), ..Default::default() })),
                };
                if let Some(st) = style {
                    hls.push((text.len()..text.len() + t.len(), st));
                }
                text.push_str(t);
            }
            let _ = old;
            div().w(px(col_w)).font_family(SERIF).text_size(px(15.)).line_height(px(25.)).child(StyledText::new(text).with_highlights(hls))
        };
        let compare = match snap {
            None => div().p(px(24.)).text_color(rgb(p.muted)).child("No earlier versions yet. They appear as you write."),
            Some(s) => {
                let old_text = rich::Doc::from_html(&s.html).0.text;
                let rows = writer::trim_rows(writer::compare(&old_text, &now_text), 1);
                let (gone, added) = rows.iter().filter_map(|r| r.as_ref().ok()).fold((0, 0), |(g, a), r| {
                    (g + r.old.iter().filter(|x| matches!(x, writer::Part::Gone(_))).count(), a + r.new.iter().filter(|x| matches!(x, writer::Part::New(_))).count())
                });
                let mut body = div().flex().flex_col();
                if rows.iter().all(|r| r.is_err()) {
                    body = body.child(div().p(px(24.)).text_color(rgb(p.muted)).child("Same text as now."));
                }
                for r in rows {
                    body = body.child(match r {
                        Ok(row) => div().flex().border_b_1().border_color(rgb(p.line)).child(div().p(px(20.)).border_r_1().border_color(rgb(p.line)).child(cell(&row.old, true))).child(div().p(px(20.)).child(cell(&row.new, false))),
                        Err(n) => div().px(px(20.)).py(px(8.)).border_b_1().border_color(rgb(p.line)).text_size(px(12.)).text_color(rgb(p.muted)).child(if n == 1 { "1 unchanged paragraph".to_string() } else { format!("{n} unchanged paragraphs") }),
                    });
                }
                div()
                    .child(
                        div()
                            .flex()
                            .border_b_1()
                            .border_color(rgb(p.line))
                            .text_size(px(13.))
                            .child(div().w(px(cmp_w / 2.)).px(px(20.)).py(px(12.)).border_r_1().border_color(rgb(p.line)).font_weight(FontWeight::SEMIBOLD).child(if s.name.is_empty() { kind_label(s) } else { s.name.clone() }))
                            .child(div().flex_1().px(px(20.)).py(px(12.)).flex().justify_between().child(div().font_weight(FontWeight::SEMIBOLD).child("Now")).child(div().text_color(rgb(p.muted)).child(format!("{gone} removed · {added} added")))),
                    )
                    .child(body)
            }
        };
        let restore = snap.map(|s| {
            let (html, title) = (s.html.clone(), s.title.clone());
            self.big_button("h-restore", "Restore this version", true, false).on_click(cx.listener(move |this, _, _, cx| {
                this.autosave(cx);
                let key = this.w.key.clone();
                let (cur_html, cur_title, words) = (this.c_body.read(cx).html(), this.c_title.read(cx).text().to_string(), this.c_body.read(cx).doc().word_count());
                writer::add_snap(&key, Snap { at: writer::now(), kind: SnapKind::Named, name: "Before restoring".into(), title: cur_title, html: cur_html, words });
                this.c_body.update(cx, |e, cx| e.load_undoable(&html, cx));
                this.w.base = Some(this.c_body.read(cx).doc().word_count());
                this.c_title.update(cx, |i, cx| i.set_text(title.clone(), cx));
                this.stack.pop();
                this.form_state = FormState::Done(format!("Version restored — undo with {MOD}Z").into());
                cx.notify();
            }))
        });
        let name = self.big_button("h-name", "Save as named version", false, false).on_click(cx.listener(|this, _, _, cx| {
            let name = this.w_in.snap_name.read(cx).text().trim().to_string();
            let key = this.w.key.clone();
            let (html, title, words) = (this.c_body.read(cx).html(), this.c_title.read(cx).text().to_string(), this.c_body.read(cx).doc().word_count());
            writer::add_snap(&key, Snap { at: writer::now(), kind: SnapKind::Named, name: if name.is_empty() { "Named version".into() } else { name }, title, html, words });
            this.w_in.snap_name.update(cx, |i, cx| i.clear(cx));
            if let Some(Page::History(h)) = this.stack.last_mut() {
                h.snaps = writer::snaps(&key);
                h.sel = h.snaps.len().checked_sub(1);
            }
            cx.notify();
        }));
        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(44.))
            .pt(px(TITLE_INSET))
            .pb(px(56.))
            .child(self.page_head("Version history", Some(if h.title.trim().is_empty() { "Untitled chapter".into() } else { h.title.clone() }), cx))
            .child(div().flex().flex_wrap().items_center().gap_2().child(div().w(px(260.)).child(self.field("h-name-in", &self.w_in.snap_name, 0., cx))).child(name).child(div().flex_1()).children(restore))
            .child(div().flex().items_start().gap(px(24.)).child(list).child(div().w(px(cmp_w)).rounded(px(14.)).border_1().border_color(rgb(p.line)).bg(rgb(p.surface)).overflow_hidden().child(compare)))
    }

    pub(crate) fn open_outline(&mut self, w: &site::Work, cx: &mut Context<Self>) {
        self.browsing = false;
        self.want = None;
        self.work_menu = None;
        self.stack.push(Page::Outline(Box::new(OutlineView { sid: w.id.clone(), title: w.title.clone(), loading: true })));
        self.status = Status::Ready;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.fetch_series_chapters(&w.id);
        cx.notify();
    }

    fn fetch_series_chapters(&self, sid: &str) {
        let id: String = sid.chars().filter(char::is_ascii_digit).collect();
        self.web.ajax(&format!("w-chs:{id}"), &site::form(&[("action", "wi_loadchapters"), ("intPostID", &id), ("toc_batch", "initial")]));
    }

    pub(crate) fn on_desk_ajax(&mut self, tag: &str, text: &str, cx: &mut Context<Self>) -> bool {
        if let Some(rest) = tag.strip_prefix("w-chs") {
            let more = rest.starts_with("-more");
            let sid = rest.trim_start_matches("-more").trim_start_matches(':').to_string();
            let json: Option<serde_json::Value> = serde_json::from_str(text.trim().strip_suffix('0').unwrap_or(text.trim())).ok();
            let data = json.as_ref().filter(|j| j["success"].as_bool() == Some(true)).map(|j| &j["data"]);
            let ids = |k: &str| data.and_then(|d| d[k].as_array()).map(|a| a.iter().map(|v| v.to_string().trim_matches('"').to_string()).collect::<Vec<_>>()).unwrap_or_default();
            let rows = data.map(|d| site::chapter_rows(d["html"].as_str().unwrap_or_default())).unwrap_or_default();
            let (all, loaded) = (ids("chapter_ids"), ids("loaded_ids"));
            let list = self.w.chapters.entry(sid.clone()).or_default();
            if more { list.extend(rows) } else { *list = rows }
            let complete = more || loaded.len() >= all.len();
            if !complete {
                self.web.ajax(&format!("w-chs-more:{sid}"), &site::form(&[("action", "wi_loadchapters"), ("intPostID", &sid), ("toc_batch", "remaining"), ("chapter_ids", &all.join(",")), ("loaded_ids", &loaded.join(","))]));
            } else {
                if let Some(Page::Outline(o)) = self.stack.last_mut() {
                    o.loading = false;
                }
                self.queue_find_fetch(&sid);
            }
            cx.notify();
            return true;
        }
        if let Some(cid) = tag.strip_prefix("w-ch:") {
            let doc = Html::parse_document(text);
            let pick = |q: &str| doc.select(&scraper::Selector::parse(q).unwrap()).next();
            let title = pick("#chapter-title").and_then(|e| e.value().attr("value")).unwrap_or_default().to_string();
            let body = pick("#edit_mycontent_chapter").map(|e| e.text().collect::<String>()).unwrap_or_default();
            let plain = rich::Doc::from_html(&body).0.text;
            if let Some(f) = self.w.find.as_mut() {
                f.texts.push((cid.to_string(), title, plain));
                f.fetching = false;
            }
            self.fetch_next_chapter();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn outline_page(&self, o: &OutlineView, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let empty = writer::Outline::default();
        let ol = self.w.data.outlines.get(&o.sid).unwrap_or(&empty);
        let mut rows = self.w.chapters.get(&o.sid).cloned().unwrap_or_default();
        rows.reverse();
        let sel = self.w.card.as_ref().filter(|(s, _)| *s == o.sid).map(|(_, k)| k.clone());
        let mut cards: Vec<(String, String, String, Card)> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut c = ol.cards.get(&r.id).cloned().unwrap_or_default();
                if !ol.cards.contains_key(&r.id) {
                    c.status = if r.status.to_lowercase().contains("draft") { 1 } else { 4 };
                }
                (r.id.clone(), format!("Chapter {}", i + 1), r.title.clone(), c)
            })
            .collect();
        for k in &ol.ideas {
            if let Some(c) = ol.cards.get(k) {
                cards.push((k.clone(), "Idea".into(), c.title.clone(), c.clone()));
            }
        }
        let sid = o.sid.clone();
        let beats = writer::TEMPLATES[(ol.template as usize).min(2)].1;
        let columns = writer::STATUSES.iter().enumerate().map(|(st, name)| {
            let in_col: Vec<_> = cards.iter().filter(|c| c.3.status as usize == st).cloned().collect();
            let count = in_col.len();
            let dot = [p.muted, 0xf1c40f, 0x4a9eff, 0x5ac48a, p.line][st];
            let sid = sid.clone();
            let cards_el = in_col.into_iter().map(|(key, label, title, c)| {
                let on = sel.as_deref() == Some(key.as_str());
                let (k2, sid2) = (key.clone(), sid.clone());
                let drag = CardDrag { key: key.clone(), title: title.clone(), bg: p.surface, fg: p.ink };
                let beat = c.beat.and_then(|b| beats.get(b as usize)).map(|b| b.to_string());
                div()
                    .id(SharedString::from(format!("card-{key}")))
                    .p(px(12.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgb(if on { p.accent } else { p.line }))
                    .bg(rgb(if on { p.sel } else { p.surface }))
                    .flex()
                    .flex_col()
                    .gap(px(5.))
                    .cursor_pointer()
                    .on_drag(drag, |d: &CardDrag, _, _, cx| cx.new(|_| CardDrag { key: d.key.clone(), title: d.title.clone(), bg: d.bg, fg: d.fg }))
                    .child(div().flex().justify_between().text_size(px(11.)).text_color(rgb(p.muted)).child(label).children(beat.map(|b| div().text_color(rgb(p.accent)).child(b))))
                    .child(div().w(px(164.)).font_family(SERIF).font_weight(FontWeight::BOLD).text_size(px(15.)).line_height(px(20.)).child(title))
                    .when(!c.summary.trim().is_empty(), |d| d.child(div().w(px(164.)).text_size(px(13.)).line_height(px(18.)).text_color(rgb(p.muted)).child(c.summary.clone())))
                    .when(!c.notes.trim().is_empty(), |d| d.child(div().text_size(px(12.)).text_color(rgb(0xf1c40f)).child("● note")))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_card(&sid2, &k2, cx)))
            }).collect::<Vec<_>>();
            let sid3 = sid.clone();
            div()
                .id(("col", st))
                .flex_none()
                .w(px(210.))
                .min_h(px(360.))
                .p(px(10.))
                .rounded(px(14.))
                .bg(rgb(p.side))
                .border_1()
                .border_color(rgb(p.line))
                .flex()
                .flex_col()
                .gap(px(8.))
                .drag_over::<CardDrag>(move |s, _, _, _| s.border_color(rgb(p.accent)))
                .on_drop(cx.listener(move |this, d: &CardDrag, _, cx| {
                    this.card_mut(&sid3, &d.key, |c| c.status = st as u8);
                    cx.notify();
                }))
                .child(div().flex().items_center().gap_2().px(px(4.)).child(div().size(px(8.)).rounded_full().bg(rgb(dot))).child(div().font_weight(FontWeight::SEMIBOLD).child(*name)).child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(count.to_string())))
                .children(cards_el)
        }).collect::<Vec<_>>();

        let templates = writer::TEMPLATES.iter().enumerate().map(|(i, (name, _))| {
            let sid = sid.clone();
            self.chip(("tpl", i), *name, ol.template as usize == i).on_click(cx.listener(move |this, _, _, cx| {
                this.w.data.outline(&sid).template = i as u8;
                this.w.data.save();
                cx.notify();
            }))
        }).collect::<Vec<_>>();
        let beat_row = beats.iter().enumerate().map(|(b, name)| {
            let pinned: Vec<String> = cards.iter().filter(|c| c.3.beat == Some(b as u8)).map(|c| c.1.replace("Chapter ", "Ch ")).collect();
            div()
                .flex_1()
                .min_w(px(90.))
                .p(px(8.))
                .rounded(px(8.))
                .bg(rgb(if pinned.is_empty() { p.bg } else { p.sel }))
                .child(div().text_size(px(11.)).text_color(rgb(p.muted)).child(name.to_string()))
                .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(if pinned.is_empty() { "—".to_string() } else { pinned.join(", ") }))
        });
        let beats_card = div()
            .p(px(16.))
            .rounded(px(14.))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().flex().flex_wrap().items_center().gap_2().child(div().font_weight(FontWeight::SEMIBOLD).mr_2().child("Story beats")).children(templates))
            .when(!beats.is_empty(), |d| d.child(div().flex().flex_wrap().gap(px(6.)).children(beat_row)));

        let panel = sel.as_ref().and_then(|k| cards.iter().find(|c| &c.0 == k)).map(|(key, label, title, c)| {
            let statuses = writer::STATUSES.iter().enumerate().map(|(st, name)| {
                let (sid, key) = (sid.clone(), key.clone());
                self.chip(("st", st), *name, c.status as usize == st).on_click(cx.listener(move |this, _, _, cx| {
                    this.card_mut(&sid, &key, |c| c.status = st as u8);
                    cx.notify();
                }))
            }).collect::<Vec<_>>();
            let beat_chips = beats.iter().enumerate().map(|(b, name)| {
                let (sid, key) = (sid.clone(), key.clone());
                let on = c.beat == Some(b as u8);
                self.chip(("bt", b), name.to_string(), on).on_click(cx.listener(move |this, _, _, cx| {
                    this.card_mut(&sid, &key, |c| c.beat = if on { None } else { Some(b as u8) });
                    cx.notify();
                }))
            }).collect::<Vec<_>>();
            let is_idea = key.starts_with("idea-");
            let (key2, title2, sid2) = (key.clone(), title.clone(), sid.clone());
            let series_title = o.title.clone();
            let open = self.big_button("card-open", if is_idea { "Start writing" } else { "Open chapter" }, true, false).on_click(cx.listener(move |this, _, _, cx| {
                this.form_series = series_title.clone();
                if is_idea {
                    this.w.pending = Some(Pending::Title(title2.clone()));
                    let w = site::Work { id: sid2.clone(), title: series_title.clone(), ..Default::default() };
                    this.open_chapter_form(&w, cx);
                } else {
                    this.form_state = FormState::Idle;
                    this.publish_armed = false;
                    this.browsing = false;
                    this.open(format!("{}editchapter/{key2}/", site::BASE), false, cx);
                }
            }));
            let (sid4, key4) = (sid.clone(), key.clone());
            let delete = is_idea.then(|| {
                self.big_button("card-del", "Delete idea", false, false).on_click(cx.listener(move |this, _, _, cx| {
                    let ol = this.w.data.outline(&sid4);
                    ol.cards.remove(&key4);
                    ol.ideas.retain(|k| *k != key4);
                    this.w.card = None;
                    this.w.data.save();
                    cx.notify();
                }))
            });
            div()
                .p(px(18.))
                .rounded(px(16.))
                .bg(rgb(p.surface))
                .border_1()
                .border_color(rgb(p.accent))
                .flex()
                .flex_wrap()
                .gap(px(18.))
                .child(
                    div()
                        .w(px(300.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(div().child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(label.clone())).child(div().w(px(300.)).font_family(SERIF).text_size(px(19.)).font_weight(FontWeight::BOLD).child(title.clone())))
                        .child(div().flex().flex_wrap().gap(px(6.)).children(statuses))
                        .when(!beats.is_empty(), |d| d.child(self.labelled("Beat", None, div().flex().flex_wrap().gap(px(6.)).children(beat_chips))))
                        .child(div().flex().gap_2().child(open).children(delete)),
                )
                .child(div().w(px(300.)).child(self.labelled("Summary", None, self.field("c-sum", &self.w_in.summary, 70., cx))))
                .child(div().w(px(300.)).child(self.labelled("Notes to self", None, self.field("c-notes", &self.w_in.notes, 100., cx).bg(rgb(mix(p.bg, 0xf1c40f, 0.06))))))
        });

        let sid5 = sid.clone();
        let add = self.big_button("idea-add", "Add idea", true, false).on_click(cx.listener(move |this, _, _, cx| {
            let title = this.w_in.idea.read(cx).text().trim().to_string();
            if title.is_empty() {
                return;
            }
            let ol = this.w.data.outline(&sid5);
            ol.next_idea += 1;
            let key = format!("idea-{}", ol.next_idea);
            ol.cards.insert(key.clone(), Card { title, ..Default::default() });
            ol.ideas.push(key.clone());
            this.w.data.save();
            this.w_in.idea.update(cx, |i, cx| i.clear(cx));
            this.select_card(&sid5, &key, cx);
        }));
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .px(px(44.))
            .pt(px(TITLE_INSET))
            .pb(px(56.))
            .child(self.page_head("Outline", Some(o.title.clone()), cx))
            .child(div().flex().items_center().gap_2().child(div().w(px(320.)).child(self.field("idea-in", &self.w_in.idea, 0., cx))).child(add).child(div().flex_1()).when(o.loading, |d| d.child(div().text_size(px(13.)).text_color(rgb(p.muted)).child("Loading chapters…"))))
            .child(beats_card)
            .children(panel)
            .child(div().id("board").w_full().overflow_x_scroll().child(div().flex().items_start().gap(px(12.)).children(columns)))
    }

    fn card_mut(&mut self, sid: &str, key: &str, f: impl FnOnce(&mut Card)) {
        let ol = self.w.data.outline(sid);
        f(ol.cards.entry(key.to_string()).or_default());
        self.w.data.save();
    }

    pub(crate) fn select_card(&mut self, sid: &str, key: &str, cx: &mut Context<Self>) {
        let c = self.w.data.outlines.get(sid).and_then(|o| o.cards.get(key)).cloned().unwrap_or_default();
        self.w.card = Some((sid.to_string(), key.to_string()));
        self.w_in.summary.update(cx, |i, cx| i.set_text(c.summary, cx));
        self.w_in.notes.update(cx, |i, cx| i.set_text(c.notes, cx));
        cx.notify();
    }

    pub(crate) fn card_text(&mut self, notes: bool, text: String) {
        let Some((sid, key)) = self.w.card.clone() else { return };
        let ol = self.w.data.outline(&sid);
        let c = ol.cards.entry(key).or_default();
        if notes { c.notes = text } else { c.summary = text }
        self.w.data.save();
    }

    pub(crate) fn toggle_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.w.find.is_some() {
            self.w.find = None;
            window.focus(&self.c_body.read(cx).focus.clone());
        } else {
            self.w.find = Some(Find::default());
            let sel = self.c_body.read(cx).selection();
            let picked = self.c_body.read(cx).doc().text.get(sel).unwrap_or_default().to_string();
            if !picked.is_empty() && !picked.contains('\n') {
                self.w_in.find.update(cx, |i, cx| i.set_text(picked, cx));
            }
            window.focus(&self.w_in.find.read(cx).focus.clone());
        }
        cx.notify();
    }

    fn matches(&self, cx: &App) -> Vec<Range<usize>> {
        let Some(f) = self.w.find.as_ref() else { return vec![] };
        let needle = self.w_in.find.read(cx).text().to_string();
        writer::find_all(&self.c_body.read(cx).doc().text, &needle, f.case, f.whole)
    }

    pub(crate) fn find_step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let hits = self.matches(cx);
        let Some(f) = self.w.find.as_mut() else { return };
        if hits.is_empty() {
            return;
        }
        f.cur = ((f.cur as isize + dir).rem_euclid(hits.len() as isize)) as usize;
        let r = hits[f.cur].clone();
        self.c_body.update(cx, |e, cx| e.select(r, cx));
        cx.notify();
    }

    fn replace_one(&mut self, cx: &mut Context<Self>) {
        let hits = self.matches(cx);
        let with = self.w_in.replace.read(cx).text().to_string();
        let sel = self.c_body.read(cx).selection();
        let cur = self.w.find.as_ref().map_or(0, |f| f.cur);
        let target = hits.iter().find(|h| **h == sel).cloned().or_else(|| hits.get(cur).cloned());
        if let Some(r) = target {
            self.c_body.update(cx, |e, cx| e.replace_ranges(&[r], &with, cx));
            self.find_step(0, cx);
        }
    }

    fn replace_all(&mut self, cx: &mut Context<Self>) {
        let hits = self.matches(cx);
        let with = self.w_in.replace.read(cx).text().to_string();
        let n = hits.len();
        self.c_body.update(cx, |e, cx| e.replace_ranges(&hits, &with, cx));
        self.form_state = FormState::Done(format!("Replaced {n} in this chapter").into());
        cx.notify();
    }

    fn search_all(&mut self, cx: &mut Context<Self>) {
        let sid = self.w.sid.clone();
        let Some(f) = self.w.find.as_mut() else { return };
        f.all = !f.all;
        if !f.all || sid.is_empty() {
            return cx.notify();
        }
        if f.texts.is_empty() {
            if self.w.chapters.contains_key(&sid) {
                self.queue_find_fetch(&sid);
            } else {
                self.fetch_series_chapters(&sid);
            }
        }
        cx.notify();
    }

    fn queue_find_fetch(&mut self, sid: &str) {
        let here = self.w.key.strip_prefix('c').unwrap_or_default().to_string();
        let rows = self.w.chapters.get(sid).cloned().unwrap_or_default();
        let Some(f) = self.w.find.as_mut().filter(|f| f.all && f.texts.is_empty() && f.queue.is_empty()) else { return };
        f.queue = rows.into_iter().filter(|r| r.id != here).map(|r| (r.id, r.title)).collect();
        self.fetch_next_chapter();
    }

    fn fetch_next_chapter(&mut self) {
        let Some(f) = self.w.find.as_mut() else { return };
        if f.fetching {
            return;
        }
        if let Some((cid, _)) = (!f.queue.is_empty()).then(|| f.queue.remove(0)) {
            f.fetching = true;
            self.web.get(&format!("w-ch:{cid}"), &format!("{}editchapter/{cid}/", site::BASE));
        }
    }

    fn find_panel(&self, text_w: f32, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.pal();
        let f = self.w.find.as_ref()?;
        let needle = self.w_in.find.read(cx).text().to_string();
        let hits = self.matches(cx);
        let toggle = |id: &'static str, label: &'static str, tip: &'static str, on: bool| {
            div()
                .id(id)
                .h(px(36.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(rgb(if on { p.accent } else { p.line }))
                .bg(rgb(if on { p.sel } else { p.bg }))
                .text_size(px(13.))
                .cursor_pointer()
                .child(label)
                .tooltip(move |_, cx| cx.new(|_| studio::Tip(tip)).into())
        };
        let count = if needle.is_empty() { String::new() } else if hits.is_empty() { "No matches".into() } else { format!("{} of {}", f.cur.min(hits.len() - 1) + 1, hits.len()) };
        let _ = text_w;
        let mut panel = div()
            .p(px(14.))
            .rounded(px(12.))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(self.field("f-find", &self.w_in.find, 0., cx)))
                    .child(div().flex_1().min_w_0().child(self.field("f-repl", &self.w_in.replace, 0., cx)))
                    .child(toggle("f-case", "Aa", "Match case", f.case).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.w.find.as_mut() {
                            f.case = !f.case;
                        }
                        cx.notify();
                    })))
                    .child(toggle("f-whole", "Word", "Whole words only", f.whole).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(f) = this.w.find.as_mut() {
                            f.whole = !f.whole;
                        }
                        cx.notify();
                    })))
                    .child(toggle("f-close", "\u{2715}", "Close (esc)", false).on_click(cx.listener(|this, _, window, cx| this.toggle_find(window, cx)))),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(div().min_w(px(80.)).text_size(px(13.)).text_color(rgb(p.muted)).child(count))
                    .child(toggle("f-prev", "\u{2191}", "Previous match", false).on_click(cx.listener(|this, _, _, cx| this.find_step(-1, cx))))
                    .child(toggle("f-next", "\u{2193}", "Next match (Enter)", false).on_click(cx.listener(|this, _, _, cx| this.find_step(1, cx))))
                    .child(toggle("f-one", "Replace", "Replace this match", false).on_click(cx.listener(|this, _, _, cx| this.replace_one(cx))))
                    .child(toggle("f-allr", "Replace all", "Replace every match in this chapter", false).on_click(cx.listener(|this, _, _, cx| this.replace_all(cx))))
                    .child(div().flex_1())
                    .child(toggle("f-all", "All chapters", "Also search every other chapter of this series", f.all).on_click(cx.listener(|this, _, _, cx| this.search_all(cx)))),
            );
        if f.all && !needle.is_empty() {
            let total = f.texts.len() + f.queue.len() + usize::from(f.fetching);
            let found: Vec<_> = f
                .texts
                .iter()
                .filter_map(|(cid, title, text)| {
                    let rs = writer::find_all(text, &needle, f.case, f.whole);
                    let first = rs.first()?.clone();
                    let a = text[..first.start].char_indices().rev().take(40).last().map_or(0, |(i, _)| i);
                    let b = first.end + text[first.end..].char_indices().take(40).last().map_or(0, |(i, c)| i + c.len_utf8());
                    let snippet = text[a..b].replace('\n', " ");
                    Some((cid.clone(), title.clone(), rs.len(), snippet))
                })
                .collect();
            let n_hits: usize = found.iter().map(|x| x.2).sum();
            let with = self.w_in.replace.read(cx).text().to_string();
            let rows = found.into_iter().enumerate().map(|(i, (cid, title, n, snippet))| {
                let (c1, c2, nd1, nd2, w2) = (cid.clone(), cid.clone(), needle.clone(), needle.clone(), with.clone());
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py(px(6.))
                    .border_t_1()
                    .border_color(rgb(p.line))
                    .child(
                        div()
                            .w(px((text_w - 260.).max(200.)))
                            .child(div().flex().gap_2().text_size(px(12.)).text_color(rgb(p.muted)).child(title).child(format!("· {n}")))
                            .child(div().font_family(SERIF).text_size(px(14.)).truncate().child(format!("…{snippet}…"))),
                    )
                    .child(self.big_button(("f-go", i), "Open", false, false).on_click(cx.listener(move |this, _, _, cx| {
                        this.w.pending = Some(Pending::Select(nd1.clone()));
                        this.open_edit_chapter(&c1, cx);
                    })))
                    .child(self.big_button(("f-rep", i), "Open & replace", false, false).on_click(cx.listener(move |this, _, _, cx| {
                        this.w.pending = Some(Pending::ReplaceAll(nd2.clone(), w2.clone()));
                        this.open_edit_chapter(&c2, cx);
                    })))
            }).collect::<Vec<_>>();
            let status = if f.texts.len() < total { format!("Searching chapters… {} of {}", f.texts.len(), total) } else { format!("{n_hits} more in {} other chapters", f.texts.iter().filter(|t| !writer::find_all(&t.2, &needle, f.case, f.whole).is_empty()).count()) };
            panel = panel
                .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(status))
                .children(rows)
                .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child("Other chapters open with the change made, unsaved, so you can check each one before saving."));
        }
        Some(panel)
    }

    pub(crate) fn editor_highlights(&self, cx: &mut Context<Self>) -> (Vec<(Range<usize>, Hl)>, Option<writer::Analysis>) {
        let text = self.c_body.read(cx).doc().text.clone();
        let any = self.w.hints.iter().any(|h| *h);
        let a = writer::analyze(&text);
        let mut hl: Vec<(Range<usize>, Hl)> = vec![];
        if any {
            let [rep, adv, pas, long] = self.w.hints;
            if long {
                hl.extend(a.long.iter().map(|r| (r.clone(), Hl::Under(0x9b59b6))));
            }
            if rep {
                hl.extend(a.repeats.iter().flat_map(|(_, rs)| rs.iter().map(|r| (r.clone(), Hl::Under(0xe67e23)))));
            }
            if adv {
                hl.extend(a.adverbs.iter().map(|r| (r.clone(), Hl::Under(0x4a9eff))));
            }
            if pas {
                hl.extend(a.passive.iter().map(|r| (r.clone(), Hl::Under(0x2dc26b))));
            }
        }
        if self.w.find.is_some() {
            hl.extend(self.matches(cx).into_iter().map(|r| (r, Hl::Back(0x6b5a1e))));
        }
        hl.sort_by_key(|(r, _)| r.start);
        (hl, Some(a))
    }

    pub(crate) fn editor_banners(&self, text_w: f32, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.pal();
        let mut out = vec![];
        if let Some(d) = &self.w.offer {
            out.push(
                div()
                    .p(px(16.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgb(p.accent))
                    .bg(rgb(p.surface))
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("This Mac has newer writing for this chapter"))
                            .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("Saved {} · {} words, not on Scribble Hub yet.", writer::when(d.updated), group_digits(&d.words.to_string())))),
                    )
                    .child(self.big_button("draft-discard", "Keep the site's", false, false).on_click(cx.listener(|this, _, _, cx| this.discard_draft(cx))))
                    .child(self.big_button("draft-restore", "Restore mine", true, false).on_click(cx.listener(|this, _, _, cx| this.restore_draft(cx))))
                    .into_any_element(),
            );
        }
        if let Some(f) = self.find_panel(text_w, cx) {
            out.push(f.into_any_element());
        }
        out
    }

    pub(crate) fn editor_aside(&self, a: Option<writer::Analysis>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = self.pal();
        let mut out = vec![];
        let saved = match self.w.saved_at {
            Some(t) => format!("Saved on this Mac · {}", writer::when(t)),
            None => "Saves to this Mac as you write".into(),
        };
        let today = self.w.data.words_on(writer::today());
        let goal = self.w.data.daily_goal.max(1);
        let btn = |id: &'static str, label: &'static str| {
            div().id(id).px(px(10.)).py(px(6.)).rounded(px(8.)).border_1().border_color(rgb(p.line)).text_size(px(12.)).cursor_pointer().hover(move |d| d.bg(rgb(p.sel))).child(label)
        };
        out.push(
            self.card("Today", None)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(ring(52., 6., today as f32 / goal as f32, p.accent, p.line))
                        .child(div().child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child(group_digits(&today.to_string()))).child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(format!("of {} words", group_digits(&goal.to_string()))))),
                )
                .child(div().flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(rgb(p.muted)).child(div().size(px(7.)).rounded_full().bg(rgb(if self.w.saved_at.is_some() { 0x5ac48a } else { p.line }))).child(saved))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(btn("as-focus", "Focus mode").on_click(cx.listener(|this, _, window, cx| this.toggle_focus(window, cx))))
                        .child(btn("as-hist", "History").on_click(cx.listener(|this, _, _, cx| this.open_history(cx))))
                        .children(self.w.sprint.is_none().then(|| {
                            let m = self.w.sprint_mins;
                            btn("as-sprint", "Sprint").on_click(cx.listener(move |this, _, _, cx| this.start_sprint(m, cx)))
                        }))
                        .children(self.sprint_pill(cx)),
                )
                .into_any_element(),
        );
        if self.w.key.starts_with('c') && !self.w.sid.is_empty() {
            out.push(self.card("Notes", None).child(self.field("ed-notes", &self.w_in.notes, 90., cx).bg(rgb(mix(p.bg, 0xf1c40f, 0.06)))).into_any_element());
        }
        if let Some(a) = a {
            let stat = |v: String, l: &'static str| div().flex_1().min_w(px(110.)).p(px(10.)).rounded(px(10.)).bg(rgb(p.bg)).child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child(v)).child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(l));
            let per = if a.sentences == 0 { 0. } else { a.words as f32 / a.sentences as f32 };
            let level = match a.grade {
                g if g < 6. => "Easy",
                g if g < 10. => "Plain",
                g if g < 14. => "Dense",
                _ => "Hard",
            };
            let counts = [a.repeats.iter().map(|r| r.1.len()).sum::<usize>(), a.adverbs.len(), a.passive.len(), a.long.len()];
            let names = ["Repeats", "Adverbs", "Passive", "Long sentences"];
            let colors = [0xe67e23, 0x4a9eff, 0x2dc26b, 0x9b59b6];
            let chips = (0..4).map(|i| {
                let on = self.w.hints[i];
                div()
                    .id(("hint", i))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(10.))
                    .py(px(6.))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(if on { colors[i] } else { p.line }))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .child(div().size(px(8.)).rounded_full().bg(rgb(colors[i])))
                    .child(format!("{} · {}", names[i], counts[i]))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.w.hints[i] = !this.w.hints[i];
                        cx.notify();
                    }))
            }).collect::<Vec<_>>();
            let top = a.repeats.first().map(|(w, rs)| format!("“{w}” ×{}", rs.len())).unwrap_or("—".into());
            out.push(
                self.card("Writing", None)
                    .child(div().flex().flex_wrap().gap_2().child(stat(format!("{per:.1}"), "words a sentence")).child(stat(format!("{:.0}%", a.dialogue * 100.), "dialogue")))
                    .child(div().flex().flex_wrap().gap_2().child(stat(format!("{level}"), "reading level")).child(stat(top, "most repeated")))
                    .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child("Show in the text:"))
                    .child(div().flex().flex_wrap().gap(px(6.)).children(chips))
                    .into_any_element(),
            );
        }
        out
    }
}
