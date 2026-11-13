#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod account;
mod desk;
mod input;
mod manage;
mod markup;
mod rich;
mod site;
mod studio;
mod web;
mod writer;

use futures::StreamExt;
use input::{InputEvent, TextInput};
use gpui::{prelude::*, *};
use site::{Block, ChapterLink};
use std::{sync::Arc, time::Duration};
use studio::{Awaiting, FormState};

actions!(shub, [Back, NextChapter, PrevChapter, Bigger, Smaller, CycleTheme, ToggleSerif, ToggleWeb, FocusSearch, Quit]);

const SIDE_W: f32 = 232.;
const MODE_IN_MS: u64 = 360;
const MODE_OUT_MS: u64 = 360;
const MODE_MIN_MS: u64 = 1400;
const MODE_MAX_MS: u64 = 10_000;
const BAR_H: f32 = 56.;
const BANNER_H: f32 = 40.;
#[cfg(not(windows))]
const TITLE_INSET: f32 = 44.;
#[cfg(windows)]
const TITLE_INSET: f32 = 20.;
#[cfg(windows)]
const CHROME_H: f32 = 32.;
#[cfg(not(windows))]
const CHROME_H: f32 = 0.;
const SERIF: &str = "Charis SIL";
#[cfg(target_os = "macos")]
const MOD: &str = "⌘";
#[cfg(not(target_os = "macos"))]
const MOD: &str = "Ctrl+";
#[cfg(target_os = "macos")]
const UI_FONT: &str = ".SystemUIFont";
#[cfg(target_os = "windows")]
const UI_FONT: &str = "Segoe UI";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const UI_FONT: &str = "sans-serif";

fn load_fonts(cx: &mut App) {
    let fonts = [
        include_bytes!("../assets/fonts/CharisSIL-Regular.ttf").as_slice(),
        include_bytes!("../assets/fonts/CharisSIL-Italic.ttf"),
        include_bytes!("../assets/fonts/CharisSIL-Bold.ttf"),
        include_bytes!("../assets/fonts/CharisSIL-BoldItalic.ttf"),
        include_bytes!("../assets/fonts/Literata-Regular.ttf"),
        include_bytes!("../assets/fonts/Literata-Italic.ttf"),
        include_bytes!("../assets/fonts/Literata-Bold.ttf"),
        include_bytes!("../assets/fonts/Literata-BoldItalic.ttf"),
    ];
    if let Err(e) = cx.text_system().add_fonts(fonts.into_iter().map(std::borrow::Cow::Borrowed).collect()) {
        eprintln!("fonts: {e}");
    }
}

#[derive(Clone, Copy)]
struct Pal {
    name: &'static str,
    bg: u32,
    side: u32,
    surface: u32,
    line: u32,
    ink: u32,
    muted: u32,
    accent: u32,
    on_accent: u32,
    star: u32,
    sel: u32,
    mark: u32,
}

const THEMES: [Pal; 3] = [
    Pal { name: "Dark", bg: 0x121214, side: 0x1a1a1d, surface: 0x1e1e22, line: 0x2b2b30, ink: 0xf5f5f7, muted: 0x9a9aa2, accent: 0x4a9eff, on_accent: 0x04121f, star: 0xff5a5f, sel: 0x2a2a30, mark: 0x4a3d0c },
    Pal { name: "Sepia", bg: 0xf4ecd8, side: 0xece2c8, surface: 0xece2c8, line: 0xddd1b3, ink: 0x3b2f20, muted: 0x6b5a45, accent: 0x8a4b12, on_accent: 0xffffff, star: 0xc0392b, sel: 0xe0d5b8, mark: 0xf3d98a },
    Pal { name: "Light", bg: 0xffffff, side: 0xf5f5f7, surface: 0xf5f5f7, line: 0xe2e2e6, ink: 0x1d1d1f, muted: 0x66666c, accent: 0x0b66c3, on_accent: 0xffffff, star: 0xd93036, sel: 0xe6e6ea, mark: 0xffe9a8 },
];

enum Page {
    Home(site::Home),
    Series(site::Series, Vec<ChapterLink>),
    Search(String, Vec<site::Hit>, Option<String>),
    Chapter(site::Chapter, String),
    ReadingList(site::ReadingList),
    Alerts(Vec<site::Alert>),
    Dashboard(Vec<site::Work>),
    SeriesForm(Box<site::SeriesForm>),
    ChapterForm(String),
    Chapters(Box<studio::ChapterList>),
    Glossary(Box<manage::Glossary>),
    Related(Box<manage::Related>),
    Stats(Box<studio::Stats>),
    Settings(Box<site::Settings>),
    Today,
    Outline(Box<desk::OutlineView>),
    History(Box<desk::HistoryView>),
}

#[derive(PartialEq)]
enum Status {
    Ready,
    Loading,
    Challenge,
    Failed(SharedString),
}

struct Shell {
    web: web::Web,
    stack: Vec<Page>,
    want: Option<(String, bool)>,
    status: Status,
    browsing: bool,
    signing_in: bool,
    account: Option<site::Account>,
    _sign_in_poll: Task<()>,
    font: f32,
    toc: (String, Vec<ChapterLink>),
    newest: bool,
    rl_sel: Vec<String>,
    rl_confirm: Option<String>,
    rl_selecting: bool,
    theme: usize,
    serif: bool,
    scroll: ScrollHandle,
    shelf_scroll: Vec<(String, ScrollHandle)>,
    shelf_remeasure: bool,
    section_load: Option<String>,
    focus: FocusHandle,
    search: Entity<TextInput>,
    query: String,
    suggestions: Vec<site::Suggestion>,
    pick: Option<usize>,
    suggest_q: String,
    _suggest_timer: Task<()>,
    _events: Task<()>,
    _search_sub: Subscription,
    studio: bool,
    other_stack: Vec<Page>,
    mode_anim: Option<ModeAnim>,
    _mode_task: Task<()>,
    f_title: Entity<TextInput>,
    f_desc: Entity<TextInput>,
    f_fandom: Entity<TextInput>,
    f_tag: Entity<TextInput>,
    c_title: Entity<TextInput>,
    c_body: Entity<rich::RichEditor>,
    c_menu: Option<&'static str>,
    c_url: Entity<TextInput>,
    _form_subs: Vec<Subscription>,
    form_state: FormState,
    form_series: String,
    cover_path: Option<std::path::PathBuf>,
    crop: Option<studio::Crop>,
    alerts_pop: Option<Option<Vec<site::Alert>>>,
    acct_tab: usize,
    acct_in: account::AcctInputs,
    acct_pick: Option<&'static str>,
    blocked: Option<Vec<site::Blocked>>,
    avatar_new: Option<std::path::PathBuf>,
    publish_armed: bool,
    c_lossy: Vec<&'static str>,
    c_status: String,
    work_menu: Option<usize>,
    more_title: Option<String>,
    manage_in: manage::ManageInputs,
    w: desk::Desk,
    w_in: desk::DeskInputs,
    awaiting: Option<Awaiting>,
    after_load: Option<(String, Option<Awaiting>)>,
    web_at: String,
    live_script: Option<&'static str>,
    series_delete: Option<(String, String)>,
    delete_typed: String,
    delete_state: DeleteState,
    delete_input: Entity<TextInput>,
    _delete_sub: Subscription,
}

#[derive(Clone, Copy)]
struct ModeAnim {
    to_author: bool,
    started: std::time::Instant,
    out_at: Option<std::time::Instant>,
}

#[derive(PartialEq)]
enum DeleteState {
    Idle,
    Deleting,
    Failed,
}

impl Shell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (web, mut rx) = web::Web::new(window).expect("could not create the WebKit view");
        let events = cx.spawn(async move |this, cx| {
            while let Some(e) = rx.next().await {
                if this.update(cx, |s, cx| s.on_event(e, cx)).is_err() {
                    break;
                }
            }
        });
        let focus = cx.focus_handle();
        window.focus(&focus);
        let search = cx.new(|cx| TextInput::new("Search", cx));
        let search_sub = cx.subscribe_in(&search, window, |this, _, e: &InputEvent, window, cx| match e {
            InputEvent::Changed(q) => this.suggest_for(q.trim().to_string(), cx),
            InputEvent::Move(d) => {
                let n = this.suggestions.len() as isize;
                if n > 0 {
                    let i = this.pick.map_or(if *d > 0 { 0 } else { n - 1 }, |i| (i as isize + d).rem_euclid(n));
                    this.pick = Some(i as usize);
                    cx.notify();
                }
            }
            InputEvent::Submit(q) => {
                let url = match this.pick {
                    Some(i) => this.suggestions[i].url.clone(),
                    None if q.starts_with(site::BASE) => q.clone(),
                    None => {
                        this.query = q.clone();
                        site::search_url(q)
                    }
                };
                this.go_search(url, window, cx);
            }
            InputEvent::Cancel => {
                window.focus(&this.focus);
                this.suggest_for(String::new(), cx);
            }
        });
        let delete_input = cx.new(|cx| TextInput::new("Type the series title", cx));
        let delete_sub = cx.subscribe(&delete_input, |this, _, e: &InputEvent, cx| match e {
            InputEvent::Changed(t) => {
                this.delete_typed = t.clone();
                cx.notify();
            }
            InputEvent::Submit(_) => this.confirm_series_delete(cx),
            InputEvent::Cancel => this.close_series_delete(cx),
            InputEvent::Move(_) => {}
        });
        let f_title = cx.new(|cx| TextInput::new("Series title", cx).max_chars(150));
        let f_desc = cx.new(|cx| TextInput::multiline("What is it about?", cx).max_chars(4000));
        let f_fandom = cx.new(|cx| TextInput::multiline("Which series is it fanfiction of?", cx));
        let f_tag = cx.new(|cx| TextInput::new("Search tags", cx));
        let acct_in = account::AcctInputs::new(cx);
        let manage_in = manage::ManageInputs::new(cx);
        let w_in = desk::DeskInputs::new(cx);
        let c_title = cx.new(|cx| TextInput::new("Chapter title", cx).max_chars(150));
        let c_body = cx.new(|cx| rich::RichEditor::new("Start writing…", cx));
        let c_url = cx.new(|cx| TextInput::new("https://…", cx));
        let redraw = |_: &mut Self, _: Entity<TextInput>, e: &InputEvent, cx: &mut Context<Self>| {
            if matches!(e, InputEvent::Changed(_)) {
                cx.notify();
            }
        };
        let form_subs = vec![cx.subscribe(&f_tag, redraw), cx.subscribe(&f_title, redraw), cx.subscribe(&f_desc, redraw), cx.subscribe(&acct_in.about, redraw), cx.subscribe(&manage_in.desc, redraw)];
        let mut form_subs = form_subs;
        form_subs.push(cx.subscribe_in(&c_body, window, |this: &mut Self, _, e: &rich::RichEvent, window, cx| match e {
            rich::RichEvent::Changed => this.on_text_changed(cx),
            rich::RichEvent::Reveal(top, bottom) => this.reveal_caret(*top, *bottom, cx),
            rich::RichEvent::Escape if this.w.find.is_some() => this.toggle_find(window, cx),
            rich::RichEvent::Escape if this.w.focus => this.toggle_focus(window, cx),
            rich::RichEvent::Escape => {}
            rich::RichEvent::Find => this.toggle_find(window, cx),
            rich::RichEvent::ToggleFocus => this.toggle_focus(window, cx),
        }));
        form_subs.push(cx.subscribe(&c_title, |this: &mut Self, _, e: &InputEvent, cx| {
            if matches!(e, InputEvent::Changed(_)) {
                this.autosave_later(cx);
            }
        }));
        form_subs.push(cx.subscribe(&w_in.summary, |this: &mut Self, _, e: &InputEvent, cx| {
            if let InputEvent::Changed(t) = e {
                this.card_text(false, t.clone());
                cx.notify();
            }
        }));
        form_subs.push(cx.subscribe(&w_in.notes, |this: &mut Self, _, e: &InputEvent, cx| {
            if let InputEvent::Changed(t) = e {
                this.card_text(true, t.clone());
                cx.notify();
            }
        }));
        form_subs.push(cx.subscribe_in(&w_in.find, window, |this: &mut Self, _, e: &InputEvent, window, cx| match e {
            InputEvent::Changed(_) => {
                if let Some(f) = this.w.find.as_mut() {
                    f.cur = 0;
                }
                cx.notify();
            }
            InputEvent::Submit(_) => this.find_step(1, cx),
            InputEvent::Cancel => this.toggle_find(window, cx),
            _ => {}
        }));
        form_subs.push(cx.subscribe_in(&c_url, window, |this: &mut Self, _, e: &InputEvent, window, cx| match e {
            InputEvent::Submit(url) => this.apply_url(url.clone(), window, cx),
            InputEvent::Cancel => this.close_menu(window, cx),
            _ => {}
        }));
        let mut shell = Self {
            web,
            stack: vec![],
            want: None,
            status: Status::Loading,
            browsing: false,
            signing_in: false,
            account: None,
            _sign_in_poll: Task::ready(()),
            font: 19.,
            newest: true,
            rl_sel: vec![],
            rl_confirm: None,
            rl_selecting: false,
            toc: Default::default(),
            theme: 0,
            serif: true,
            scroll: ScrollHandle::new(),
            shelf_scroll: vec![],
            shelf_remeasure: false,
            section_load: None,
            focus,
            search,
            query: String::new(),
            suggestions: vec![],
            pick: None,
            suggest_q: String::new(),
            _suggest_timer: Task::ready(()),
            _events: events,
            _search_sub: search_sub,
            f_title,
            f_desc,
            f_fandom,
            f_tag,
            c_title,
            c_body,
            c_menu: None,
            c_url,
            _form_subs: form_subs,
            form_state: FormState::Idle,
            form_series: String::new(),
            cover_path: None,
            crop: None,
            alerts_pop: None,
            acct_tab: 0,
            acct_in,
            acct_pick: None,
            blocked: None,
            avatar_new: None,
            publish_armed: false,
            c_lossy: vec![],
            c_status: String::new(),
            work_menu: None,
            more_title: None,
            manage_in,
            w: desk::Desk::new(),
            w_in,
            awaiting: None,
            after_load: None,
            web_at: String::new(),
            live_script: None,
            studio: false,
            other_stack: vec![],
            mode_anim: None,
            _mode_task: Task::ready(()),
            series_delete: None,
            delete_typed: String::new(),
            delete_state: DeleteState::Idle,
            delete_input,
            _delete_sub: delete_sub,
        };
        let start = std::env::args().nth(1).filter(|u| u.starts_with(site::BASE)).unwrap_or(site::BASE.into());
        shell.open(start, false, cx);
        shell
    }

    fn suggest_for(&mut self, q: String, cx: &mut Context<Self>) {
        self.pick = None;
        self.suggest_q = q.clone();
        if q.chars().count() < 2 || q.starts_with(site::BASE) {
            self.suggestions.clear();
            self._suggest_timer = Task::ready(());
        } else {
            self._suggest_timer = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(300)).await;
                this.update(cx, |s, _| s.web.suggest(&q)).ok();
            });
        }
        cx.notify();
    }

    fn go_search(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        self.suggest_for(String::new(), cx);
        self.browsing = false;
        self.open(url, false, cx);
    }

    fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        self.browsing = false;
        self.want = None;
        self.signing_in = true;
        self.status = Status::Ready;
        self.web.load(&format!("{}login/", site::BASE));
        self._sign_in_poll = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let going = this.update(cx, |s, _| {
                if s.signing_in {
                    s.web.snapshot();
                }
                s.signing_in
            });
            if !matches!(going, Ok(true)) {
                break;
            }
        });
        cx.notify();
    }

    fn stop_sign_in(&mut self, cx: &mut Context<Self>) {
        self.signing_in = false;
        self._sign_in_poll = Task::ready(());
        self.stack.clear();
        self.open(site::BASE.into(), false, cx);
    }

    fn set_studio(&mut self, on: bool, cx: &mut Context<Self>) {
        if on == self.studio {
            return;
        }
        std::mem::swap(&mut self.stack, &mut self.other_stack);
        self.studio = on;
        self.browsing = false;
        self.want = None;
        self.section_load = None;
        self.status = Status::Ready;
        self.scroll.set_offset(point(px(0.), px(0.)));
        if self.stack.is_empty() {
            let url = if on { format!("{}dashboard/", site::BASE) } else { site::BASE.to_string() };
            self.open(url, false, cx);
        }
        cx.notify();
    }

    fn switch_mode(&mut self, cx: &mut Context<Self>) {
        if self.mode_anim.is_some() {
            return;
        }
        let to = !self.studio;
        self.browsing = false;
        self.mode_anim = Some(ModeAnim { to_author: to, started: std::time::Instant::now(), out_at: None });
        cx.notify();
        self._mode_task = cx.spawn(async move |this, cx| {
            let ms = Duration::from_millis;
            cx.background_executor().timer(ms(MODE_IN_MS)).await;
            this.update(cx, |this, cx| this.set_studio(to, cx)).ok();
            loop {
                cx.background_executor().timer(ms(50)).await;
                let Ok(done) = this.update(cx, |this, _| {
                    let e = this.mode_anim.map_or(ms(0), |a| a.started.elapsed());
                    (e >= ms(MODE_MIN_MS - MODE_OUT_MS) && this.status != Status::Loading) || e >= ms(MODE_MAX_MS)
                }) else {
                    return;
                };
                if done {
                    break;
                }
            }
            this.update(cx, |this, cx| {
                if let Some(a) = &mut this.mode_anim {
                    a.out_at = Some(std::time::Instant::now());
                }
                cx.notify();
            })
            .ok();
            cx.background_executor().timer(ms(MODE_OUT_MS + 20)).await;
            this.update(cx, |this, cx| {
                this.mode_anim = None;
                cx.notify();
            })
            .ok();
        });
    }

    fn mode_overlay(&self, window: &mut Window) -> Option<impl IntoElement> {
        let a = self.mode_anim?;
        window.request_animation_frame();
        let p = self.pal();
        let since = |t: std::time::Instant| t.elapsed().as_secs_f32() * 1000.;
        let e = since(a.started);
        let ramp = |from: f32, len: f32| ease_in_out(((e - from) / len).clamp(0., 1.));
        let out = a.out_at.map_or(0., |t| ease_in_out((since(t) / MODE_OUT_MS as f32).clamp(0., 1.)));
        let cover = ramp(0., MODE_IN_MS as f32) * (1. - out);
        let word = ramp(90., 300.) * (1. - ease_in_out((out * 1.4).min(1.)));
        let (first, second) = (ramp(200., 260.), ramp(480., 260.));
        let (shift, shown) = if a.to_author { (first, second) } else { (1. - second, 1. - first) };
        let waiting = a.out_at.is_none() && e > (MODE_MIN_MS - MODE_OUT_MS) as f32 && self.status == Status::Loading;
        Some(
            div()
                .id("mode-splash")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(28.))
                .bg(rgb(p.bg))
                .opacity(cover)
                .child(
                    div()
                        .relative()
                        .left(px(-shift * SPLASH_MARK_W / 2.))
                        .opacity(word)
                        .child(div().text_size(px(40.)).font_weight(FontWeight::BOLD).child("Scribble Hub"))
                        .child(
                            div()
                                .absolute()
                                .left(relative(1.))
                                .bottom(px(-(1. - shown) * 16.))
                                .pl(px(10.))
                                .opacity(shown)
                                .child(div().relative().left(px(-(1. - shift) * SPLASH_MARK_W / 2.)).child(author_mark(22., author(p)))),
                        ),
                )
                .child(div().h(px(10.)).when(waiting, |d| d.child(self.dots("mode-dot", 7.)))),
        )
    }

    fn sign_out(&mut self, cx: &mut Context<Self>) {
        if self.studio {
            self.set_studio(false, cx);
        }
        self.other_stack.clear();
        match self.account.as_ref().and_then(|a| a.logout.clone()) {
            Some(url) => {
                self.stack.clear();
                self.open(url, false, cx);
            }
            None => {
                self.web.load(&format!("{}wp-login.php?action=logout", site::BASE));
                self.browsing = true;
                cx.notify();
            }
        }
    }

    fn toggle_alerts(&mut self, cx: &mut Context<Self>) {
        if self.alerts_pop.take().is_none() {
            self.alerts_pop = Some(None);
            self.web.fetch_alerts();
        }
        cx.notify();
    }

    fn open(&mut self, url: String, replace: bool, cx: &mut Context<Self>) {
        self.web.load(&url);
        self.want = Some((url, replace));
        self.status = Status::Loading;
        cx.notify();
    }

    fn on_event(&mut self, e: web::Event, cx: &mut Context<Self>) {
        match e {
            web::Event::Finished => {
                if let Some((js, awaiting)) = self.after_load.take() {
                    self.web.eval(&js);
                    self.awaiting = awaiting;
                } else if self.want.is_some() || self.signing_in || self.section_load.is_some() || self.awaiting.is_some() {
                    self.web.snapshot()
                }
                if let Some(js) = self.live_script.take().filter(|_| self.browsing) {
                    self.web.eval(js);
                }
            }
            web::Event::Ajax { tag, text } => {
                if let Ok(dir) = std::env::var("SH_DUMP") {
                    std::fs::write(format!("{dir}/ajax-{tag}.txt"), &text).ok();
                }
                if !self.on_desk_ajax(&tag, &text, cx) && !self.on_acct_ajax(&tag, &text, cx) && !self.on_manage_ajax(&tag, &text, cx) {
                    self.on_ajax(&tag, &text, cx)
                }
            }
            web::Event::Saved { ok, msg } => self.on_saved(ok, msg, cx),
            web::Event::Page { url: at, title, html } => {
                if let Ok(dir) = std::env::var("SH_DUMP") {
                    let name = at.trim_end_matches('/').rsplit('/').next().unwrap_or("page").to_string();
                    std::fs::write(format!("{dir}/{name}.html"), &html).ok();
                }
                if let Some(account) = site::account(&html) {
                    self.account = account;
                }
                self.web_at = at.clone();
                if self.on_landed(&at, &html, cx) {
                    return;
                }
                if self.signing_in {
                    if self.account.is_some() && !at.contains("/login") {
                        self.stop_sign_in(cx);
                    }
                } else if title.starts_with("Just a moment") {
                    self.status = Status::Challenge;
                } else if title.contains("Attention Required") {
                    self.want = None;
                    self.section_load = None;
                    self.status = Status::Failed("Cloudflare refused this page. Open the site to see why.".into());
                } else if let Some(url) = self.section_load.take() {
                    let h = site::home(&html);
                    if h.shelves.is_empty() {
                        self.status = Status::Failed(format!("Couldn't read {url}").into());
                    } else if let Some(Page::Home(cur)) = self.stack.first_mut() {
                        cur.updates = h.updates;
                        cur.pages = h.pages;
                    }
                } else if let Some((url, replace)) = self.want.take() {
                    let page = if url.contains("/series-finder/") {
                        Some(Page::Search(format!("Results for “{}”", self.query), site::search(&html), None))
                    } else if url.contains("/series-ranking/") || url.contains("/latest-series/") {
                        let title = self.more_title.take().unwrap_or_else(|| if url.contains("/latest-series/") { "Latest Series".into() } else { "Series Ranking".into() });
                        Some(Page::Search(title, site::search(&html), Some(url.clone())))
                    } else if url.contains("/read/") {
                        Some(site::chapter(&html)).filter(|c| !c.blocks.is_empty()).map(|c| {
                            let id = series_id(&c.series_url);
                            if self.toc.0 != id {
                                self.toc = (id.clone(), vec![]);
                                self.web.fetch_toc(&id);
                            }
                            Page::Chapter(c, url.clone())
                        })
                    } else if url.contains("/series/") {
                        Some(site::series(&html)).filter(|s| !s.title.is_empty()).map(|s| {
                            self.toc = (s.id.clone(), vec![]);
                            self.web.fetch_toc(&s.id);
                            Page::Series(s, vec![])
                        })
                    } else if url.contains("/editseries/") && url.ends_with("#related") {
                        self.related_from(&url, &html).map(|r| Page::Related(Box::new(r)))
                    } else if url.contains("/add-series/") || url.contains("/editseries/") {
                        site::series_form(&html).map(|f| {
                            self.load_series_form(&f, cx);
                            Page::SeriesForm(Box::new(f))
                        })
                    } else if url.contains("/addchapter/") || url.contains("/editchapter/") {
                        html.contains("chapter-title").then(|| {
                            self.load_chapter_form(&url, &html, cx);
                            Page::ChapterForm(url.clone())
                        })
                    } else if url.contains("/dashboard/") {
                        site::dashboard(&html).map(Page::Dashboard)
                    } else if url.contains("/account/") {
                        site::settings(&html).map(|s| {
                            self.load_settings(&s, cx);
                            Page::Settings(Box::new(s))
                        })
                    } else if url.contains("/latest-alerts/") {
                        Some(Page::Alerts(site::alerts(&html)))
                    } else if url.contains("/reading-list/") {
                        self.rl_sel.clear();
                        self.rl_confirm = None;
                        self.rl_selecting = false;
                        let rl = site::reading_list(&html);
                        (!rl.lists.is_empty() || !rl.items.is_empty()).then_some(Page::ReadingList(rl))
                    } else {
                        Some(site::home(&html)).filter(|h| !h.shelves.is_empty()).map(Page::Home)
                    };
                    match page {
                        Some(page) => {
                            if replace {
                                self.stack.pop();
                            }
                            self.stack.push(page);
                            self.status = Status::Ready;
                            self.scroll.set_offset(point(px(0.), px(0.)));
                        }
                        None => self.status = Status::Failed(
                            if url.contains("/reading-list/") {
                                "Sign in to see your reading list.".into()
                            } else {
                                format!("Couldn't read {url}").into()
                            },
                        ),
                    }
                }
            }
            web::Event::Suggest { query, html } if query == self.suggest_q => {
                self.suggestions = site::suggestions(&html);
                self.pick = None;
            }
            web::Event::Suggest { .. } => {}
            web::Event::Toc { html } => {
                self.toc.1 = site::toc(&html);
                if let Some(Page::Series(_, chapters)) = self.stack.last_mut() {
                    *chapters = self.toc.1.clone();
                }
            }
            web::Event::Alerts { html } => {
                if let Some(pop) = self.alerts_pop.as_mut() {
                    *pop = Some(site::alerts(&html));
                } else if let Some(Page::Alerts(list)) = self.stack.last_mut() {
                    *list = site::alerts(&html);
                    self.status = Status::Ready;
                    self.scroll.set_offset(point(px(0.), px(0.)));
                }
            }
            web::Event::SeriesDeleted { id, ok } => {
                if ok {
                    if let Some(Page::Dashboard(works)) = self.stack.last_mut() {
                        works.retain(|w| w.id != id);
                    }
                    self.series_delete = None;
                    self.delete_state = DeleteState::Idle;
                } else {
                    self.delete_state = DeleteState::Failed;
                }
            }
            web::Event::RlDeleted { ok: true } => {}
            web::Event::RlDeleted { ok: false } => {
                if let Some(Page::ReadingList(rl)) = self.stack.last() {
                    if let Some((_, url)) = rl.lists.get(rl.active) {
                        let url = url.clone();
                        self.open(url, true, cx);
                    }
                }
            }
            web::Event::RlMore { html, next, more } => {
                if let Some(Page::ReadingList(rl)) = self.stack.last_mut() {
                    rl.items.extend(site::reading_list_rows(&html));
                    match (&mut rl.more, more) {
                        (Some(m), true) => m.offset = next,
                        (_, false) => rl.more = None,
                        (None, true) => {}
                    }
                    self.status = Status::Ready;
                }
            }
        }
        cx.notify();
    }

    fn chapter_link(&mut self, next: bool, cx: &mut Context<Self>) {
        if let Some(Page::Chapter(c, _)) = self.stack.last() {
            if let Some(url) = if next { c.next.clone() } else { c.prev.clone() } {
                self.open(url, true, cx);
            }
        }
    }

    fn rl_delete(&mut self, sids: Vec<String>, cx: &mut Context<Self>) {
        if let Some(Page::ReadingList(rl)) = self.stack.last_mut() {
            if let Some((to, nonce)) = &rl.delete {
                self.web.delete_rl(to, nonce, &sids);
                rl.items.retain(|i| !sids.contains(&i.sid));
            }
        }
        self.rl_sel.retain(|s| !sids.contains(s));
        self.rl_confirm = None;
        cx.notify();
    }

    fn check_box(&self, id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
        let p = self.pal();
        div()
            .id(id)
            .flex_none()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(FontWeight::BOLD)
            .border_1()
            .map(|d| if on { d.bg(rgb(p.accent)).border_color(rgb(p.accent)).text_color(rgb(p.on_accent)).child("✓") } else { d.border_color(rgb(p.muted)).hover(move |d| d.border_color(rgb(p.ink))) })
    }

    fn delete_button(&self, id: impl Into<ElementId>, key: &str, label: String, sids: Vec<String>, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.pal();
        let armed = self.rl_confirm.as_deref() == Some(key);
        let icon = label.is_empty() && !armed;
        let key = key.to_string();
        div()
            .id(id)
            .group("del")
            .flex_none()
            .flex()
            .items_center()
            .px(px(if icon { 8. } else { 12. }))
            .py(px(if icon { 7. } else { 5. }))
            .rounded(px(7.))
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .map(|d| if armed { d.bg(rgb(p.star)).text_color(rgb(0xffffff)) } else { d.text_color(rgb(p.muted)).hover(move |d| d.bg(rgb(p.sel)).text_color(rgb(p.star))) })
            .map(|d| {
                if icon {
                    d.child(svg().path("icons/trash.svg").size(px(17.)).text_color(rgb(p.muted)).group_hover("del", |s| s.text_color(rgb(p.star))))
                } else {
                    d.child(if armed { "Confirm".to_string() } else { label })
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.rl_confirm.as_deref() == Some(key.as_str()) {
                    this.rl_delete(sids.clone(), cx);
                } else {
                    this.rl_confirm = Some(key.clone());
                    cx.notify();
                }
            }))
    }

    fn rl_action_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        let n = self.rl_sel.len();
        if n == 0 || !matches!(self.stack.last(), Some(Page::ReadingList(_))) {
            return None;
        }
        let p = self.pal();
        Some(
            div().absolute().bottom(px(24.)).left_0().right_0().flex().justify_center().child(
                div()
                    .id("rl-actions")
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(52.))
                    .pl(px(20.))
                    .pr(px(10.))
                    .rounded(px(14.))
                    .bg(rgb(p.surface))
                    .border_1()
                    .border_color(rgb(p.line))
                    .shadow_lg()
                    .text_size(px(13.))
                    .child(div().mr_2().font_weight(FontWeight::SEMIBOLD).child(format!("{n} selected")))
                    .child(
                        div()
                            .id("rl-clear")
                            .px_3()
                            .py(px(5.))
                            .rounded(px(7.))
                            .text_size(px(12.))
                            .cursor_pointer()
                            .text_color(rgb(p.muted))
                            .hover(move |d| d.bg(rgb(p.sel)).text_color(rgb(p.ink)))
                            .child("Clear")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.rl_selecting = false;
                                this.rl_sel.clear();
                                this.rl_confirm = None;
                                cx.notify();
                            })),
                    )
                    .child(self.delete_button("rl-bulk", "", format!("Delete {n}"), self.rl_sel.clone(), cx)),
            ),
        )
    }

    fn open_live(&mut self, url: String, cx: &mut Context<Self>) {
        self.want = None;
        self.web_at.clear();
        self.web.load(&url);
        self.browsing = true;
        cx.notify();
    }

    fn toggle_web(&mut self, cx: &mut Context<Self>) {
        self.browsing = !self.browsing;
        self.web_at.clear();
        if !self.browsing && matches!(self.stack.last(), Some(Page::Dashboard(_))) {
            self.open(format!("{}dashboard/", site::BASE), true, cx);
        }
        cx.notify();
    }

    fn open_series_delete(&mut self, id: String, title: String, window: &mut Window, cx: &mut Context<Self>) {
        self.series_delete = Some((id, title));
        self.delete_typed.clear();
        self.delete_state = DeleteState::Idle;
        self.delete_input.update(cx, |i, cx| i.clear(cx));
        window.focus(&self.delete_input.read(cx).focus);
        cx.notify();
    }

    fn close_series_delete(&mut self, cx: &mut Context<Self>) {
        if self.delete_state != DeleteState::Deleting {
            self.series_delete = None;
            cx.notify();
        }
    }

    fn confirm_series_delete(&mut self, cx: &mut Context<Self>) {
        if let Some((id, title)) = &self.series_delete {
            if self.delete_typed == *title && self.delete_state != DeleteState::Deleting {
                self.web.delete_series(id);
                self.delete_state = DeleteState::Deleting;
                cx.notify();
            }
        }
    }

    fn go_back(&mut self, cx: &mut Context<Self>) {
        if self.signing_in {
            return self.stop_sign_in(cx);
        }
        if self.browsing {
            self.browsing = false;
        } else if self.want.take().is_none() && self.stack.len() > 1 {
            self.stack.pop();
        }
        self.status = Status::Ready;
        cx.notify();
    }

    fn reader_pref(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        f(self);
        cx.notify();
    }

    fn pal(&self) -> Pal {
        let p = THEMES[self.theme];
        if self.studio { author(p) } else { p }
    }

    fn go_home(&mut self, section: Option<usize>, cx: &mut Context<Self>) {
        self.browsing = false;
        self.want = None;
        self.status = Status::Ready;
        self.stack.truncate(1);
        if !matches!(self.stack.first(), Some(Page::Home(_))) {
            self.stack.clear();
            self.open(site::BASE.into(), false, cx);
        }
        match section {
            Some(i) => self.scroll.scroll_to_top_of_item(i),
            None => self.scroll.set_offset(point(px(0.), px(0.))),
        }
        cx.notify();
    }

    fn link(&self, id: impl Into<ElementId>, url: String, replace: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id(id)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| this.open(url.clone(), replace, cx)))
    }

    fn button(&self, id: &'static str, label: impl Into<SharedString>, on: impl Fn(&mut Self, &mut Context<Self>) + 'static, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.pal();
        div()
            .id(id)
            .h(px(32.))
            .px_3()
            .flex()
            .items_center()
            .rounded(px(8.))
            .text_color(rgb(p.ink))
            .cursor_pointer()
            .hover(move |s| s.bg(rgb(p.sel)))
            .child(label.into())
            .on_click(cx.listener(move |this, _, _, cx| on(this, cx)))
    }

    fn back_link(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let p = self.pal();
        (self.stack.len() > 1).then(|| {
            div()
                .id("back")
                .flex()
                .mb_4()
                .text_color(rgb(p.accent))
                .cursor_pointer()
                .hover(|s| s.underline())
                .child("‹  Back")
                .on_click(cx.listener(|this, _, _, cx| this.go_back(cx)))
        })
    }

    fn dots(&self, prefix: &'static str, size: f32) -> Div {
        let p = self.pal();
        div().flex().gap(px(8.)).children((0usize..3).map(move |i| {
            div().size(px(size)).rounded_full().bg(rgb(p.accent)).with_animation(
                (prefix, i),
                Animation::new(Duration::from_millis(1050)).repeat(),
                move |el, d| {
                    let phase = (d + i as f32 / 3.) % 1.;
                    let pulse = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
                    el.opacity(0.2 + 0.8 * pulse)
                },
            )
        }))
    }

    fn load_overlay(&self) -> impl IntoElement {
        let p = self.pal();
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .bg(rgb(p.bg))
            .child(self.dots("ldot", 10.))
            .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child("Loading…"))
    }

    fn suggest_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.pal();
        let rows = self.suggestions.iter().enumerate().map(|(i, s)| {
            let url = s.url.clone();
            let picked = self.pick == Some(i);
            let marks = s.marks.iter().map(|r| {
                (r.clone(), HighlightStyle { background_color: Some(rgb(p.mark).into()), font_weight: Some(FontWeight::BOLD), ..Default::default() })
            });
            div()
                .id(("sg", i))
                .flex()
                .items_center()
                .gap_3()
                .px_2()
                .py_1p5()
                .rounded(px(8.))
                .cursor_pointer()
                .when(picked, |d| d.bg(rgb(p.sel)))
                .hover(move |d| d.bg(rgb(p.sel)))
                .child(cover(&s.cover, 32., 46., 5., p))
                .child(div().flex_1().min_w_0().truncate().text_size(px(15.)).child(StyledText::new(s.title.clone()).with_highlights(marks)))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.go_search(url.clone(), window, cx)))
        });
        let all = site::search_url(&self.suggest_q);
        let q = self.suggest_q.clone();
        deferred(
            div()
                .id("suggest")
                .occlude()
                .absolute()
                .top(px(46.))
                .left_0()
                .w(px(360.))
                .max_h(px(560.))
                .overflow_y_scroll()
                .p(px(6.))
                .bg(rgb(p.surface))
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(p.line))
                .shadow_lg()
                .text_color(rgb(p.ink))
                .children(rows)
                .child(
                    div()
                        .id("sg-all")
                        .mt_1()
                        .px_2()
                        .py_3()
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .text_color(rgb(p.accent))
                        .cursor_pointer()
                        .hover(|d| d.underline())
                        .child(format!("See all results for “{q}”"))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            this.query = q.clone();
                            this.go_search(all.clone(), window, cx)
                        })),
                ),
        )
        .with_priority(1)
    }

    fn side_item(&self, id: impl Into<ElementId>, icon: &'static str, label: impl Into<SharedString>, selected: bool, muted: bool) -> Stateful<Div> {
        let p = self.pal();
        let color = rgb(if muted { p.muted } else { p.ink });
        div()
            .id(id)
            .h(px(36.))
            .px_3()
            .flex()
            .items_center()
            .rounded(px(8.))
            .cursor_pointer()
            .text_color(rgb(if muted { p.muted } else { p.ink }))
            .when(selected, |d| d.bg(rgb(p.sel)).font_weight(FontWeight::SEMIBOLD))
            .hover(move |s| s.bg(rgb(p.sel)))
            .gap(px(10.))
            .child(svg().path(icon).flex_none().size(px(17.)).text_color(color))
            .child(label.into())
    }

    fn title_bar(&self, reading: bool, window: &Window) -> Div {
        let p = self.pal();
        let button = |id: &'static str, glyph: &'static str, area: WindowControlArea, hover: u32, hover_ink: u32| {
            div()
                .id(id)
                .w(px(46.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .font_family("Segoe Fluent Icons")
                .text_size(px(10.))
                .text_color(rgb(p.muted))
                .window_control_area(area)
                .hover(move |s| s.bg(rgb(hover)).text_color(rgb(hover_ink)))
                .child(glyph)
        };
        let max = if window.is_maximized() { "\u{E923}" } else { "\u{E922}" };
        div()
            .h(px(CHROME_H))
            .w_full()
            .flex_none()
            .flex()
            .child(div().when(!reading, |d| d.w(px(SIDE_W)).bg(rgb(p.side)).border_r_1().border_color(rgb(p.line))).h_full().window_control_area(WindowControlArea::Drag))
            .child(div().flex_1().h_full().window_control_area(WindowControlArea::Drag))
            .child(button("win-min", "\u{E921}", WindowControlArea::Min, p.surface, p.ink))
            .child(button("win-max", max, WindowControlArea::Max, p.surface, p.ink))
            .child(button("win-close", "\u{E8BB}", WindowControlArea::Close, 0xc42b1c, 0xffffff))
    }

    fn sidebar(&self, show_suggest: bool, search_focused: bool, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let on_home = matches!(self.stack.last(), Some(Page::Home(_))) && !self.browsing;
        div()
            .w(px(SIDE_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .px(px(14.))
            .pt(px(TITLE_INSET))
            .pb_3()
            .bg(rgb(p.side))
            .border_r_1()
            .border_color(rgb(p.line))
            .child(
                div()
                    .id("logo")
                    .px(px(10.))
                    .pb_3()
                    .text_size(px(19.))
                    .font_weight(FontWeight::BOLD)
                    .cursor_pointer()
                    .flex()
                    .items_end()
                    .gap(px(5.))
                    .child("Scribble Hub")
                    .when(self.studio, |d| d.child(author_mark(13., p)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.studio {
                            this.stack.clear();
                            this.open(format!("{}dashboard/", site::BASE), false, cx);
                        } else {
                            this.go_home(None, cx)
                        }
                    })),
            )
            .when(self.studio, |d| {
                let on_dash = matches!(self.stack.last(), Some(Page::Dashboard(_))) && !self.browsing;
                d.child(self.side_item("nav-dash", "icons/chart.svg", "Your series", on_dash, false).on_click(cx.listener(|this, _, _, cx| {
                    if !matches!(this.stack.last(), Some(Page::Dashboard(_))) {
                        this.open(format!("{}dashboard/", site::BASE), false, cx);
                    }
                    this.browsing = false;
                    cx.notify();
                })))
                .child(self.side_item("nav-today", "icons/target.svg", "Today", matches!(self.stack.last(), Some(Page::Today)) && !self.browsing, false).on_click(cx.listener(|this, _, _, cx| this.open_today(cx))))
                .child(self.side_item("nav-add-series", "icons/plus.svg", "Add series", false, false).on_click(cx.listener(|this, _, _, cx| this.open_series_form(None, cx))))
            })
            .when(!self.studio, |d| d.child(
                div()
                    .relative()
                    .mb_3()
                    .child(
                        div()
                            .h(px(40.))
                            .px_3()
                            .flex()
                            .items_center()
                            .rounded(px(10.))
                            .bg(rgb(p.bg))
                            .border_1()
                            .border_color(rgb(if search_focused { p.accent } else { p.line }))
                            .child(self.search.clone()),
                    )
                    .when(show_suggest, |d| d.child(self.suggest_list(cx))),
            ))
            .when(!self.studio, |d| d.child(self.side_item("nav-home", "icons/home.svg", "Home", on_home, false).on_click(cx.listener(|this, _, _, cx| this.go_home(None, cx)))))
            .when(self.account.is_some() && !self.studio, |d| {
                d.child(
                    self.side_item("nav-reading-list", "icons/bookmark.svg", "Reading list", matches!(self.stack.last(), Some(Page::ReadingList(_))) && !self.browsing, false)
                        .on_click(cx.listener(|this, _, _, cx| this.open(format!("{}reading-list/", site::BASE), false, cx))),
                )
            })
            .child(div().flex_1())
            .child(self.account_rows(cx))
            .child(
                self.side_item("theme", if self.theme == 0 { "icons/sun.svg" } else { "icons/moon.svg" }, if self.theme == 0 { "Light mode" } else { "Dark mode" }, false, true)
                    .on_click(cx.listener(|this, _, _, cx| this.reader_pref(cx, |s| s.theme = if s.theme == 0 { 2 } else { 0 }))),
            )
            .child(
                self.side_item("web", "icons/globe.svg", if self.browsing { "Close website" } else { "Open website" }, self.browsing, true)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_web(cx))),
            )
    }

    fn reader_bar(&self, c: &site::Chapter, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let series_url = c.series_url.clone();
        let segment = || div().flex().gap(px(2.)).p(px(3.)).rounded(px(10.)).bg(rgb(p.surface));
        div()
            .h(px(BAR_H))
            .flex_none()
            .flex()
            .items_center()
            .gap_3()
            .pl(px(84.))
            .pr_5()
            .border_b_1()
            .border_color(rgb(p.line))
            .child(
                div()
                    .id("to-series")
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(p.accent))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(format!("‹  {}", c.series_title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.stack.len() > 1 { this.go_back(cx) } else { this.open(series_url.clone(), false, cx) }
                    })),
            )
            .child(div().flex_1())
            .child(
                segment()
                    .child(self.button("smaller", "A−", |s, cx| s.reader_pref(cx, |s| s.font = (s.font - 1.).max(13.)), cx))
                    .child(self.button("bigger", "A+", |s, cx| s.reader_pref(cx, |s| s.font = (s.font + 1.).min(32.)), cx))
                    .child(self.button("font", if self.serif { "Serif" } else { "Sans" }, |s, cx| s.reader_pref(cx, |s| s.serif = !s.serif), cx)),
            )
            .child(segment().children(THEMES.iter().enumerate().map(|(i, t)| {
                div()
                    .id(("theme", i))
                    .h(px(32.))
                    .px_3()
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(i == self.theme, |d| d.bg(rgb(p.sel)).font_weight(FontWeight::SEMIBOLD))
                    .hover(move |d| d.bg(rgb(p.sel)))
                    .child(t.name)
                    .on_click(cx.listener(move |this, _, _, cx| this.reader_pref(cx, |s| s.theme = i)))
            })))
    }

    fn account_rows(&self, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let rows = div().flex().flex_col().gap_1().mb_2().pb_2().border_b_1().border_color(rgb(p.line));
        if self.signing_in {
            return rows.child(self.side_item("sign-in", "icons/sign-in.svg", "Cancel sign in", true, false).on_click(cx.listener(|this, _, _, cx| this.stop_sign_in(cx))));
        }
        match &self.account {
            None => rows.child(self.side_item("sign-in", "icons/sign-in.svg", "Sign in", false, false).text_color(rgb(p.accent)).on_click(cx.listener(|this, _, _, cx| this.start_sign_in(cx)))),
            Some(a) => rows
                .child(
                    div()
                        .h(px(40.))
                        .pl_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("nav-settings")
                                .group("me")
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap_2()
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| this.open_settings(0, cx)))
                                .child(match &a.avatar {
                                    Some(src) => div().size(px(24.)).rounded_full().overflow_hidden().flex_none().child(img(SharedString::from(src.clone())).size_full().rounded_full()),
                                    None => div().size(px(24.)).rounded_full().flex_none().bg(rgb(p.sel)).flex().items_center().justify_center().text_size(px(12.)).font_weight(FontWeight::BOLD).child(a.name.chars().next().unwrap_or('?').to_uppercase().to_string()),
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .when(matches!(self.stack.last(), Some(Page::Settings(_))) && !self.browsing, |d| d.text_color(rgb(p.accent)))
                                        .group_hover("me", |d| d.text_color(rgb(p.accent)))
                                        .child(a.name.clone()),
                                ),
                        )
                        .child({
                            let on = self.alerts_pop.is_some() || (matches!(self.stack.last(), Some(Page::Alerts(_))) && !self.browsing);
                            div()
                                .id("nav-alerts")
                                .flex_none()
                                .mr(px(-4.))
                                .p(px(6.))
                                .rounded(px(8.))
                                .cursor_pointer()
                                .when(on, |d| d.bg(rgb(p.sel)))
                                .hover(move |d| d.bg(rgb(p.sel)))
                                .child(svg().path("icons/bell.svg").size(px(21.)).text_color(rgb(if on { p.ink } else { p.muted })))
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_alerts(cx)))
                        }),
                )
                .child(
                    self.side_item("mode", if self.studio { "icons/bookmark.svg" } else { "icons/pen.svg" }, if self.studio { "Switch to Reader" } else { "Switch to Author" }, false, false)
                        .on_click(cx.listener(|this, _, _, cx| this.switch_mode(cx))),
                )
                .child(self.side_item("sign-out", "icons/sign-out.svg", "Sign out", false, true).on_click(cx.listener(|this, _, _, cx| this.sign_out(cx)))),
        }
    }

    fn banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.pal();
        let (msg, bg): (SharedString, u32) = match &self.status {
            Status::Challenge => ("Scribble Hub wants to check you're human. Complete it below and reading continues.".into(), p.mark),
            Status::Failed(m) => (m.clone(), p.surface),
            _ if self.signing_in => ("Sign in on Scribble Hub's own page. The app never sees your password, and you stay signed in.".into(), p.surface),
            _ if self.browsing => ("Live website: the author dashboard, comments, or anything not in the app yet.".into(), p.surface),
            _ => return None,
        };
        Some(
            div()
                .h(px(BANNER_H))
                .flex_none()
                .flex()
                .items_center()
                .gap_3()
                .px_5()
                .bg(rgb(bg))
                .border_b_1()
                .border_color(rgb(p.line))
                .text_size(px(13.))
                .child(div().flex_1().truncate().child(msg))
                .when(matches!(self.status, Status::Failed(_)) && !self.browsing, |d| {
                    d.child(
                        div()
                            .id("open-site")
                            .text_color(rgb(p.accent))
                            .cursor_pointer()
                            .hover(|s| s.underline())
                            .child("Open website")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.browsing = true;
                                cx.notify();
                            })),
                    )
                }),
        )
    }

    fn home(&self, h: &site::Home, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let mut page = div().flex().flex_col().gap(px(40.)).px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.));
        for (si, sh) in h.shelves.iter().filter(|s| !s.cards.is_empty()).enumerate() {
            let (name, cards) = (&sh.name, &sh.cards);
            let handle = self.shelf_scroll[si].1.clone();
            let row = one_axis(div()).id(("shelf", si)).flex().gap(px(24.)).px_1().pt_1().pb_4().overflow_x_scroll().track_scroll(&handle)
                .on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                    let d = e.delta.pixel_delta(px(16.));
                    let (dx, dy) = (f32::from(d.x), f32::from(d.y));
                    if dx == 0. && dy == 0. {
                        return;
                    }
                    if dx.abs() > dy.abs() {
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    let h = &this.shelf_scroll[si].1;
                    let max = f32::from(h.max_offset().width);
                    let x = f32::from(h.offset().x);
                    if max <= 0. || (dy > 0. && x <= -(max - 1.)) || (dy < 0. && x >= -1.) {
                        return;
                    }
                    let nx = (x - dy).clamp(-max, 0.);
                    h.set_offset(point(px(nx), px(0.)));
                    cx.stop_propagation();
                    cx.notify();
                }))
                .children(cards.iter().enumerate().map(|(i, c)| {
                self.link(("card", i), c.url.clone(), false, cx)
                    .flex_none()
                    .w(px(112.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(div().mb_2().child(cover(&c.cover, 112., 164., 10., p)))
                    .when(c.rating > 0., |d| d.child(stars(c.rating, 11., p)))
                    .children(c.rank.clone().map(|r| div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.muted)).child(format!("No. {r}"))))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).line_height(relative(1.3)).line_clamp(3).child(c.title.clone()))
                    .rounded(px(10.))
                    .hover(move |s| s.bg(rgb(p.surface)))
            }));
            let off = f32::from(handle.offset().x);
            let max = f32::from(handle.max_offset().width);
            let arrow = |left: bool| {
                div()
                    .id(("sarrow", si * 2 + usize::from(!left)))
                    .absolute()
                    .top(px(66.))
                    .when(left, |d| d.left(px(2.)))
                    .when(!left, |d| d.right(px(2.)))
                    .size(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgb(p.bg))
                    .border_1()
                    .border_color(rgb(p.line))
                    .text_size(px(18.))
                    .text_color(rgb(p.ink))
                    .cursor_pointer()
                    .hover(move |d| d.bg(rgb(p.sel)))
                    .occlude()
                    .child(if left { "‹" } else { "›" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let h = &this.shelf_scroll[si].1;
                        let max = f32::from(h.max_offset().width);
                        let x0 = f32::from(h.offset().x);
                        let x = if left { (x0 + 700.).min(0.) } else { (x0 - 700.).max(-max) };
                        h.set_offset(point(px(x), px(0.)));
                        cx.notify();
                    }))
            };
            let row = div().relative().child(row)
                .when(max > 40. && off < -1., |d| d.child(arrow(true)))
                .when(max > 40. && off > -(max - 1.), |d| d.child(arrow(false)));
            let head = div().flex().items_center().justify_between().child(icon_heading(section_icon(&name), name.clone(), 28., p));
            let head = match &sh.view_more {
                Some(more) => head.child({
                    let (url, title) = (more.clone(), name.clone());
                    div()
                        .id(("vmore", si))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.more_title = Some(title.to_string());
                            this.open(url.clone(), false, cx)
                        }))
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(p.accent))
                        .hover(|s| s.underline())
                        .child("View More")
                }),
                None => head,
            };
            page = page.child(div().flex().flex_col().gap(px(14.)).child(head).child(row));
        }
        let loading = self.section_load.is_some();
        let updates = h.updates.iter().enumerate().map(|(i, u)| {
            div()
                .flex_grow()
                .flex_basis(px(360.))
                .min_w_0()
                .flex()
                .gap_3()
                .py(px(14.))
                .border_t_1()
                .border_color(rgb(p.line))
                .when(loading, |d| d.opacity(0.35))
                .child(self.link(("upc", i), u.url.clone(), false, cx).child(cover(&u.cover, 40., 58., 5., p)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .min_w_0()
                        .child(self.link(("upt", i), u.url.clone(), false, cx).text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).truncate().hover(|s| s.underline()).child(u.title.clone()))
                        .child(self.link(("upch", i), u.chapter_url.clone(), false, cx).text_color(rgb(p.accent)).truncate().hover(|s| s.underline()).child(u.chapter.clone()))
                        .child(div().text_size(px(12.)).text_color(rgb(p.muted)).truncate().child(format!("{} · {}", u.byline, u.genres.join(", ")))),
                )
        });
        let mut tail = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(icon_heading("icons/clock.svg", "Latest updates", 28., p))
                    .when(loading, |d| d.child(self.dots("sdot", 7.))),
            )
            .child(div().flex().flex_wrap().gap_x(px(40.)).children(updates));
        if !h.pages.is_empty() {
            let bar = div().flex().flex_wrap().justify_center().items_center().gap(px(4.)).pt(px(20.)).children(h.pages.iter().enumerate().map(|(i, pg)| {
                let label: SharedString = match pg.label.as_ref() {
                    "«" => "‹".into(),
                    "»" => "›".into(),
                    l => l.to_string().into(),
                };
                let arrow = label == "‹" || label == "›";
                let mut boxe = div()
                    .id(("pg", i))
                    .min_w(px(36.))
                    .h(px(36.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_size(px(if arrow { 20. } else { 14. }))
                    .when(arrow, |d| d.mx(px(8.)).bg(rgb(p.surface)))
                    .child(label);
                if pg.current {
                    boxe = boxe.bg(rgb(p.accent)).text_color(rgb(p.on_accent)).font_weight(FontWeight::SEMIBOLD);
                } else if let Some(url) = &pg.url {
                    let url = url.clone();
                    boxe = boxe
                        .cursor_pointer()
                        .text_color(rgb(p.ink))
                        .hover(move |d| d.bg(rgb(p.sel)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if this.section_load.as_deref() != Some(url.as_str()) {
                                this.section_load = Some(url.clone());
                                this.web.load(&url);
                                cx.notify();
                            }
                        }));
                } else {
                    boxe = boxe.text_color(rgb(p.muted));
                }
                boxe
            }));
            tail = tail.child(bar);
        }
        page.child(tail)
    }

    fn results(&self, title: &str, hits: &[site::Hit], paged: Option<&str>, area: f32, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let col_w = (area - 88.).clamp(320., 860.);
        let text_w = col_w - 88. - 22.;
        let count = match hits.len() {
            0 => "No series found".to_string(),
            n => format!("{n} series"),
        };
        let back = self.back_link(cx);
        let rows = hits.iter().enumerate().map(|(i, h)| {
            self.link(("hit", i), h.url.clone(), false, cx)
                .flex()
                .gap(px(22.))
                .py(px(22.))
                .border_t_1()
                .border_color(rgb(p.line))
                .rounded(px(10.))
                .hover(move |d| d.bg(rgb(p.surface)))
                .child(cover(&h.cover, 88., 128., 9., p))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .w(px(text_w))
                        .flex_none()
                        .child(div().w(px(text_w)).text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(h.title.clone()))
                        .child(div().flex().items_center().gap_2().child(stars(h.rating, 14., p)).child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(h.rating_label.clone())))
                        .child(div().w(px(text_w)).text_size(px(13.)).text_color(rgb(p.muted)).child(h.stats.join(" · ")))
                        .child(div().w(px(text_w)).line_height(relative(1.5)).line_clamp(2).child(h.blurb.clone()))
                        .child(div().w(px(text_w)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.accent)).child(h.genres.join(" · "))),
                )
        });
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .flex_none()
                .w(px(col_w))
                .flex()
                .flex_col()
                .children(back)
                .child(heading(title.to_string(), 28.))
                .when(paged.is_none(), |d| d.child(div().mt_2().mb_3().text_color(rgb(p.muted)).child(count)))
                .when(paged.is_some(), |d| d.child(div().h(px(16.))))
                .children(rows)
                .children(paged.map(|url| self.list_pager(url, hits.is_empty(), cx))),
        )
    }

    fn list_pager(&self, url: &str, empty: bool, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let n: u32 = url.split("pg=").nth(1).and_then(|s| s.split('&').next()).and_then(|s| s.parse().ok()).unwrap_or(1);
        let at = |pg: u32| {
            let base: String = url.split(['?', '#']).next().unwrap_or(url).to_string();
            let mut q: Vec<String> = url.split_once('?').map(|(_, q)| q.split('&').filter(|kv| !kv.starts_with("pg=") && !kv.is_empty()).map(str::to_string).collect()).unwrap_or_default();
            if pg > 1 {
                q.push(format!("pg={pg}"));
            }
            if q.is_empty() { base } else { format!("{base}?{}", q.join("&")) }
        };
        let step = |id: &'static str, label: &'static str, to: String, cx: &mut Context<Self>| {
            self.link(id, to, true, cx).h(px(38.)).px(px(16.)).flex().items_center().rounded(px(10.)).bg(rgb(p.surface)).font_weight(FontWeight::SEMIBOLD).hover(move |d| d.bg(rgb(p.sel))).child(label)
        };
        div()
            .mt(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .gap_4()
            .when(n > 1, |d| d.child(step("pg-prev", "← Previous", at(n - 1), cx)))
            .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("Page {n}")))
            .when(!empty, |d| d.child(step("pg-next", "Next →", at(n + 1), cx)))
    }

    fn series(&self, s: &site::Series, chapters: &[ChapterLink], cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let first = s.first_chapter.clone().or_else(|| chapters.first().map(|c| c.url.clone()));
        let latest = chapters.last().map(|c| c.url.clone());
        let cta = |id: &'static str, label: &'static str, url: String, bg: u32, fg: u32, cx: &mut Context<Self>| {
            self.link(id, url, false, cx)
                .h(px(46.))
                .px_6()
                .flex()
                .items_center()
                .rounded(px(12.))
                .bg(rgb(bg))
                .text_color(rgb(fg))
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .hover(|s| s.opacity(0.88))
                .child(label)
        };
        let header = div()
            .flex()
            .gap(px(36.))
            .child(cover(&s.cover, 180., 256., 12., p))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .min_w_0()
                    .child(heading(s.title.clone(), 40.))
                    .child(div().text_size(px(16.)).text_color(rgb(p.muted)).child(format!("by {}", s.author)))
                    .child(div().flex().items_center().gap_2().child(stars(s.rating, 18., p)).child(div().text_color(rgb(p.muted)).child(s.rating_label.clone())))
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .gap(px(10.))
                            .children(first.map(|url| cta("read", "Start reading", url, p.accent, p.on_accent, cx)))
                            .children(latest.map(|url| cta("latest", "Latest chapter", url, p.sel, p.ink, cx))),
                    ),
            );
        let tiles = div().flex().flex_wrap().gap_3().children(s.stats.iter().map(|stat| {
            let (value, label) = stat.split_once(' ').unwrap_or((stat.as_str(), ""));
            div()
                .flex_1()
                .min_w(px(130.))
                .px_4()
                .py(px(14.))
                .rounded(px(12.))
                .bg(rgb(p.surface))
                .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD).child(value.to_string()))
                .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(label.to_string()))
        }));
        let synopsis = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(heading("Synopsis", 24.))
            .children(s.synopsis.iter().map(|para| div().text_size(px(15.)).line_height(relative(1.65)).child(para.clone())))
            .child(div().mt_1().flex().flex_wrap().gap_2().children(s.genres.iter().map(|g| {
                div().px_3().py(px(6.)).rounded_full().bg(rgb(p.surface)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(g.clone())
            })));
        let mut ordered: Vec<_> = chapters.iter().enumerate().collect();
        if self.newest {
            ordered.reverse();
        }
        let toc = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(10.))
                    .flex()
                    .items_end()
                    .justify_between()
                    .child(heading(format!("Chapters ({})", chapters.len()), 24.))
                    .child(
                        div()
                            .id("order")
                            .px_2()
                            .py_1()
                            .rounded(px(6.))
                            .text_color(rgb(p.accent))
                            .cursor_pointer()
                            .hover(move |s| s.bg(rgb(p.sel)))
                            .child(if self.newest { "Newest first ↓" } else { "Oldest first ↑" })
                            .on_click(cx.listener(|this, _, _, cx| this.reader_pref(cx, |s| s.newest = !s.newest))),
                    ),
            )
            .when(chapters.is_empty(), |d| d.child(div().text_color(rgb(p.muted)).child("Loading chapters…")))
            .children(ordered.into_iter().map(|(i, c)| {
                self.link(("ch", i), c.url.clone(), false, cx)
                    .min_h(px(44.))
                    .py_2()
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(rgb(p.line))
                    .text_size(px(15.))
                    .hover(move |s| s.text_color(rgb(p.accent)))
                    .child(c.title.clone())
            }));
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(980.))
                .flex()
                .flex_col()
                .gap(px(32.))
                .children(self.back_link(cx))
                .child(header)
                .child(tiles)
                .child(div().flex().gap(px(48.)).child(synopsis).child(toc)),
        )
    }

    fn reading_list(&self, rl: &site::ReadingList, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let tabs = div().mt_2().mb_2().flex().flex_wrap().gap_2().children(rl.lists.iter().enumerate().map(|(i, (name, url))| {
            let selected = i == rl.active;
            self.link(("rltab", i), url.clone(), true, cx)
                .px_3()
                .py(px(7.))
                .rounded(px(8.))
                .text_size(px(13.))
                .when(selected, |d| d.bg(rgb(p.sel)).font_weight(FontWeight::SEMIBOLD))
                .hover(move |d| d.bg(rgb(p.sel)))
                .child(name.clone())
        }));
        let can_delete = rl.delete.is_some();
        let selecting = self.rl_selecting || !self.rl_sel.is_empty();
        let rows = rl.items.iter().enumerate().map(|(i, it)| {
            let sid = it.sid.clone();
            let catch_up_cid = it.latest.as_ref().map(|l| chapter_id(&l.url).to_string());
            let chapter_row = |kicker: &'static str, ch: &Option<site::RlChapter>, i: usize, cx: &mut Context<Self>| {
                ch.as_ref().map(|c| {
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .text_size(px(13.))
                        .child(div().flex_none().text_color(rgb(p.muted)).child(kicker))
                        .child(
                            self.link(("rlch", i), c.url.clone(), false, cx)
                                .min_w_0()
                                .truncate()
                                .text_color(rgb(p.accent))
                                .hover(|d| d.underline())
                                .child(c.title.clone()),
                        )
                })
            };
            let current = chapter_row("Current:", &it.current, i * 2, cx);
            let latest = chapter_row("Latest:", &it.latest, i * 2 + 1, cx);
            let (read, out) = (&it.progress.0, &it.progress.1);
            let title = if it.url.is_empty() {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .min_w_0()
                    .child(div().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).truncate().text_color(rgb(p.muted)).child(it.title.clone()))
                    .when(it.deleted, |d| {
                        d.child(div().flex_none().px(px(6.)).py(px(1.)).rounded(px(6.)).border_1().border_color(rgb(p.line)).text_size(px(11.)).text_color(rgb(p.muted)).child("Deleted"))
                    })
                    .into_any_element()
            } else {
                self.link(("rlt", i), it.url.clone(), false, cx)
                    .text_size(px(17.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate()
                    .hover(|d| d.underline())
                    .child(it.title.clone())
                    .into_any_element()
            };
            let cover_el = if it.url.is_empty() {
                cover(&it.cover, 56., 80., 8., p).into_any_element()
            } else {
                self.link(("rlc", i), it.url.clone(), false, cx).child(cover(&it.cover, 56., 80., 8., p)).into_any_element()
            };
            let tick = (can_delete && selecting).then(|| {
                let sid = it.sid.clone();
                div()
                    .flex()
                    .items_center()
                    .child(self.check_box(("rlsel", i), self.rl_sel.contains(&it.sid)).on_click(cx.listener(move |this, _, _, cx| {
                        match this.rl_sel.iter().position(|s| *s == sid) {
                            Some(at) => _ = this.rl_sel.remove(at),
                            None => this.rl_sel.push(sid.clone()),
                        }
                        this.rl_confirm = None;
                        cx.notify();
                    })))
            });

            let armed = self.rl_confirm.as_deref() == Some(it.sid.as_str());
            div()
                .id(("rli", i))
                .flex()
                .gap(px(18.))
                .py(px(16.))
                .border_t_1()
                .border_color(rgb(p.line))
                .group("rlrow")
                .px_3()
                .rounded(px(10.))
                .hover(move |d| d.bg(rgb(p.surface)))
                .children(tick)
                .child(cover_el)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .min_w_0()
                        .flex_1()
                        .child(title)
                        .when(!it.author.is_empty(), |d| {
                            d.child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(format!("by {}", it.author)))
                        })
                        .when(it.rating > 0., |d| d.child(stars(it.rating, 13., p)))
                        .children(current)
                        .children(latest.map(|l| {
                            l.when(it.unread > 0, |d| {
                                d.child(
                                    div()
                                        .flex_none()
                                        .px(px(6.))
                                        .py(px(1.))
                                        .rounded(px(6.))
                                        .bg(rgb(p.star))
                                        .text_color(rgb(0xffffff))
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!("{} new", it.unread)),
                                )
                            })
                        }))
                        .child(
                            div()
                                .mt_1()
                                .flex()
                                .items_center()
                                .gap_3()
                                .text_size(px(12.))
                                .text_color(rgb(p.muted))
                                .when(!read.is_empty() && !out.is_empty(), |d| d.child(div().child(format!("{read} / {out} chapters"))))
                                .when(it.unread > 0 && catch_up_cid.is_some(), |d| {
                                    let cid = catch_up_cid.clone().unwrap_or_default();
                                    d.child(
                                        div()
                                            .id(("catchup", i))
                                            .px_3()
                                            .py(px(4.))
                                            .rounded(px(7.))
                                            .bg(rgb(p.surface))
                                            .border_1()
                                            .border_color(rgb(p.line))
                                            .text_size(px(12.))
                                            .text_color(rgb(p.ink))
                                            .cursor_pointer()
                                            .hover(move |d| d.bg(rgb(p.sel)))
                                            .child("Catch up")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.web.mark_rl_read(&sid, &cid);
                                                if let Some(Page::ReadingList(rl)) = this.stack.last_mut() {
                                                    if let Some(item) = rl.items.iter_mut().find(|x| x.sid == sid) {
                                                        item.current = item.latest.clone();
                                                        item.unread = 0;
                                                        item.progress.0 = item.progress.1.clone();
                                                    }
                                                }
                                                cx.notify();
                                            })),
                                    )
                                }),
                        ),
                )
                .when(can_delete, |d| {
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .when(!armed, |d| d.opacity(0.).group_hover("rlrow", |s| s.opacity(1.)))
                            .child(self.delete_button(("rldel", i), &it.sid, String::new(), vec![it.sid.clone()], cx)),
                    )
                })
        });
        let rows: Vec<_> = rows.collect();
        let all = !rl.items.is_empty() && rl.items.iter().all(|i| self.rl_sel.contains(&i.sid));
        let ids: Vec<String> = rl.items.iter().map(|i| i.sid.clone()).collect();
        let quiet = |id: &'static str, icon: Option<&'static str>, label: String| {
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(6.))
                .px_3()
                .py(px(6.))
                .rounded(px(8.))
                .text_size(px(13.))
                .cursor_pointer()
                .text_color(rgb(p.muted))
                .hover(move |d| d.bg(rgb(p.sel)).text_color(rgb(p.ink)))
                .group(id)
                .children(icon.map(|i| svg().path(i).size(px(15.)).text_color(rgb(p.muted)).group_hover(id, move |s| s.text_color(rgb(p.ink)))))
                .child(label)
        };
        let controls = (can_delete && !rl.items.is_empty()).then(|| {
            if selecting {
                div()
                    .flex()
                    .gap_1()
                    .child(quiet("rl-all", None, if all { "Deselect all".into() } else { "Select all".into() }).on_click(cx.listener(move |this, _, _, cx| {
                        this.rl_sel = if all { vec![] } else { ids.clone() };
                        this.rl_selecting = true;
                        this.rl_confirm = None;
                        cx.notify();
                    })))
                    .child(quiet("rl-done", None, "Done".into()).text_color(rgb(p.accent)).font_weight(FontWeight::SEMIBOLD).on_click(cx.listener(|this, _, _, cx| {
                        this.rl_selecting = false;
                        this.rl_sel.clear();
                        this.rl_confirm = None;
                        cx.notify();
                    })))
            } else {
                div().child(
                    quiet("rl-select", Some("icons/select.svg"), "Select".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.rl_selecting = true;
                            cx.notify();
                        })),
                )
            }
        });
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(110.)).child(
            div()
                .max_w(px(860.))
                .flex()
                .flex_col()
                .children(self.back_link(cx))
                .child(heading("Reading List", 28.))
                .child(div().flex().items_center().justify_between().gap_3().child(tabs).children(controls))
                .when(rl.items.is_empty(), |d| d.child(div().py_6().text_color(rgb(p.muted)).child("This reading list is empty.")))
                .children(rows)
                .when(rl.more.is_some(), |d| {
                    d.child(
                        div()
                            .id("rl-more")
                            .mt_3()
                            .mx_auto()
                            .px_5()
                            .py_2()
                            .rounded(px(10.))
                            .bg(rgb(p.surface))
                            .text_size(px(13.))
                            .cursor_pointer()
                            .hover(move |d| d.bg(rgb(p.sel)))
                            .child("Load more")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(m) = this.stack.last().and_then(|p| match p {
                                    Page::ReadingList(rl) => rl.more.clone(),
                                    _ => None,
                                }) {
                                    this.status = Status::Loading;
                                    this.web.fetch_rl_more(&m);
                                    cx.notify();
                                }
                            })),
                    )
                }),
        )
    }

    fn alert_row(&self, i: usize, a: &site::Alert, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.pal();
        {
            let row = div().id(("alert", i)).flex().items_center().gap_3().py(px(12.)).border_t_1().border_color(rgb(p.line)).child(
                div()
                    .size(px(28.))
                    .flex_none()
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgb(p.surface))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(match &a.icon {
                        Some(src) => img(SharedString::from(src.clone())).size_full().object_fit(ObjectFit::Cover).into_any_element(),
                        None => div().size(px(8.)).rounded_full().bg(rgb(p.accent)).into_any_element(),
                    }),
            );
            let row = row
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.))
                    .child(div().text_size(px(14.)).line_height(relative(1.4)).child(a.text.clone()))
                    .when(!a.time.is_empty(), |d| d.child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(a.time.clone()))));
            let row = match &a.url {
                Some(url) => {
                    let url = url.clone();
                    row.on_click(cx.listener(move |this, _, _, cx| {
                        this.alerts_pop = None;
                        this.open(url.clone(), false, cx)
                    }))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(p.sel)))
                    .rounded(px(8.))
                }
                None => row,
            };
            row
        }
    }

    fn alerts_popover(&self, cx: &mut Context<Self>) -> Option<Div> {
        let pop = self.alerts_pop.as_ref()?;
        let p = self.pal();
        let link = |id: &'static str, label: &'static str| div().id(id).text_size(px(13.)).text_color(rgb(p.accent)).cursor_pointer().hover(|d| d.underline()).child(label);
        let body = match pop {
            None => div().px_4().py_4().text_size(px(13.)).text_color(rgb(p.muted)).child("Loading…"),
            Some(list) if list.is_empty() => div().px_4().py_4().text_size(px(14.)).text_color(rgb(p.muted)).child("You have no new alerts."),
            Some(list) => div().px_2().flex().flex_col().children(list.iter().take(6).enumerate().map(|(i, a)| self.alert_row(i, a, cx).px_2().text_size(px(13.)))),
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .child(div().id("alerts-away").absolute().inset_0().on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.alerts_pop = None;
                    cx.notify();
                })))
                .child(
                    div()
                        .id("alerts-pop")
                        .occlude()
                        .absolute()
                        .left(px(SIDE_W + 10.))
                        .bottom(px(16.))
                        .w(px(360.))
                        .rounded(px(12.))
                        .bg(rgb(p.surface))
                        .border_1()
                        .border_color(rgb(p.line))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .child(div().px_4().py_3().border_b_1().border_color(rgb(p.line)).text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("Alerts"))
                        .child(body)
                        .child(
                            div()
                                .px_4()
                                .py_3()
                                .border_t_1()
                                .border_color(rgb(p.line))
                                .flex()
                                .justify_between()
                                .child(link("alerts-all", "Show all").on_click(cx.listener(|this, _, _, cx| {
                                    this.alerts_pop = None;
                                    this.browsing = false;
                                    this.open(format!("{}latest-alerts/", site::BASE), false, cx);
                                })))
                                .child(link("alerts-prefs", "Alert preferences").on_click(cx.listener(|this, _, _, cx| this.open_settings(2, cx)))),
                        ),
                ),
        )
    }

    fn alerts(&self, list: &[site::Alert], cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let rows: Vec<_> = list.iter().enumerate().map(|(i, a)| self.alert_row(i, a, cx)).collect();
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(720.))
                .flex()
                .flex_col()
                .children(self.back_link(cx))
                .child(heading("Notifications", 28.))
                .when(list.is_empty(), |d| {
                    d.child(div().py_6().flex().flex_col().gap_2()
                        .child(div().text_color(rgb(p.muted)).child("No alerts yet.")))
                })
                .children(rows),
        )
    }

    fn action(&self, id: impl Into<ElementId>, icon: &'static str, label: &'static str, primary: bool) -> Stateful<Div> {
        let p = self.pal();
        let (fg, bg) = if primary { (p.on_accent, p.accent) } else { (p.ink, p.surface) };
        div()
            .id(id)
            .flex_none()
            .h(px(32.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .bg(rgb(bg))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(fg))
            .cursor_pointer()
            .hover(move |d| d.opacity(0.85))
            .child(svg().path(icon).size(px(15.)).text_color(rgb(fg)))
            .child(label)
    }

    fn dashboard(&self, works: &[site::Work], cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .child(heading("Your series", 28.))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(self.action("dash-sort", "icons/sort.svg", "Sort order", false).on_click(cx.listener(|this, _, _, cx| {
                        this.open_live(format!("{}dashboard/", site::BASE), cx);
                        this.live_script = Some("s_sort_order()");
                    })))
                    .child(self.action("dash-add", "icons/plus.svg", "Add series", true).on_click(cx.listener(|this, _, _, cx| this.open_series_form(None, cx)))),
            );
        let rows = works.iter().enumerate().map(|(i, w)| {
            let stats = div().flex().flex_wrap().gap_x(px(22.)).gap_y(px(6.)).children(w.stats.iter().filter(|(_, l)| l != "Chapters/Week").map(|(v, l)| {
                div()
                    .flex()
                    .flex_col()
                    .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(group_digits(v)))
                    .child(div().text_size(px(11.)).text_color(rgb(p.muted)).child(l.clone()))
            }));
            let (id, title) = (w.id.clone(), w.title.clone());
            div()
                .id(("work", i))
                .group("work")
                .flex()
                .gap(px(22.))
                .py(px(22.))
                .border_t_1()
                .border_color(rgb(p.line))
                .child(self.link(("wc", i), w.url.clone(), false, cx).flex_none().child(cover(&w.cover, 84., 120., 9., p)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(
                            div()
                                .flex()
                                .items_start()
                                .gap_3()
                                .child(self.link(("wt", i), w.url.clone(), false, cx).flex_1().min_w_0().child(heading(w.title.clone(), 20.)).hover(|d| d.underline()))
                                .child(
                                    div()
                                        .id(("wdel", i))
                                        .flex_none()
                                        .p(px(6.))
                                        .rounded(px(7.))
                                        .cursor_pointer()
                                        .opacity(0.)
                                        .group_hover("work", |s| s.opacity(1.))
                                        .hover(move |d| d.bg(rgb(p.sel)))
                                        .child(svg().path("icons/trash.svg").size(px(17.)).text_color(rgb(p.muted)))
                                        .on_click(cx.listener(move |this, _, window, cx| this.open_series_delete(id.clone(), title.clone(), window, cx))),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.))
                                .text_color(rgb(p.muted))
                                .when(!w.rating.is_empty(), |d| {
                                    d.child(stars(w.rating.parse().unwrap_or(0.), 13., p)).child(div().text_color(rgb(p.ink)).font_weight(FontWeight::SEMIBOLD).child(w.rating.clone())).child("·")
                                })
                                .child(div().min_w_0().truncate().child(if w.genres.is_empty() { format!("Updated {}", w.updated) } else { format!("Updated {} · {}", w.updated, w.genres.join(", ")) })),
                        )
                        .child(stats)
                        .when(!w.blurb.is_empty(), |d| d.child(div().text_size(px(13.)).line_height(px(20.)).text_color(rgb(p.muted)).line_clamp(2).child(w.blurb.clone())))
                        .child(
                            div()
                                .mt_1()
                                .flex()
                                .flex_wrap()
                                .gap_2()
                                .child(self.action(("wadd", i), "icons/plus.svg", "Add chapter", true).on_click({
                                    let w = w.clone();
                                    cx.listener(move |this, _, _, cx| this.open_chapter_form(&w, cx))
                                }))
                                .child(self.action(("wchap", i), "icons/list.svg", "Chapters", false).on_click({
                                    let w = w.clone();
                                    cx.listener(move |this, _, _, cx| this.open_chapters(&w, cx))
                                }))
                                .child(self.action(("wout", i), "icons/board.svg", "Outline", false).on_click({
                                    let w = w.clone();
                                    cx.listener(move |this, _, _, cx| this.open_outline(&w, cx))
                                }))
                                .child(self.action(("wedit", i), "icons/pen.svg", "Edit", false).on_click({
                                    let id = w.id.clone();
                                    cx.listener(move |this, _, _, cx| this.open_series_form(Some(&id), cx))
                                }))
                                .child(self.action(("wstat", i), "icons/chart.svg", "Stats", false).on_click({
                                    let w = w.clone();
                                    cx.listener(move |this, _, _, cx| this.open_stats(&w, cx))
                                }))
                                .child({
                                    let open = self.work_menu == Some(i);
                                    let (sid, title) = (w.id.clone(), w.title.clone());
                                    let (sid2, sid3) = (w.id.clone(), w.id.clone());
                                    let item = |id: &'static str, label: &'static str| {
                                        div().id(id).px(px(10.)).py(px(7.)).rounded(px(6.)).text_size(px(13.)).cursor_pointer().hover(move |d| d.bg(rgb(p.sel))).child(label)
                                    };
                                    div()
                                        .relative()
                                        .child(self.action(("wmore", i), "icons/more.svg", "More", false).on_click(cx.listener(move |this, _, _, cx| {
                                            this.work_menu = if this.work_menu == Some(i) { None } else { Some(i) };
                                            cx.notify();
                                        })))
                                        .when(open, |d| {
                                            d.child(deferred(
                                                div()
                                                    .id("wmore-menu")
                                                    .occlude()
                                                    .absolute()
                                                    .top(px(36.))
                                                    .left_0()
                                                    .w(px(190.))
                                                    .p(px(4.))
                                                    .bg(rgb(p.surface))
                                                    .rounded(px(10.))
                                                    .border_1()
                                                    .border_color(rgb(p.line))
                                                    .shadow_lg()
                                                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                                        this.work_menu = None;
                                                        cx.notify();
                                                    }))
                                                    .child(item("wm-gloss", "Glossary").on_click(cx.listener(move |this, _, _, cx| this.open_glossary(&sid, &title, cx))))
                                                    .child(item("wm-rel", "Related series").on_click(cx.listener(move |this, _, _, cx| this.open_related(&sid2, cx))))
                                                    .child(item("wm-site", "Open on Scribble Hub").on_click(cx.listener(move |this, _, _, cx| {
                                                        this.work_menu = None;
                                                        this.open_live(format!("{}editseries/{}/", site::BASE, sid3), cx);
                                                    }))),
                                            ))
                                        })
                                }),
                        ),
                )
        });
        let rows: Vec<_> = rows.collect();
        let list = div()
            .flex_1()
            .min_w(px(460.))
            .max_w(px(900.))
            .flex()
            .flex_col()
            .when(works.is_empty(), |d| d.child(div().py_6().text_color(rgb(p.muted)).child("No series yet. Add one to start writing.")))
            .children(rows);
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .px(px(44.))
            .pt(px(TITLE_INSET))
            .pb(px(56.))
            .child(header)
            .child(div().flex().flex_wrap().items_start().gap(px(28.)).child(list).children((!works.is_empty()).then(|| self.overview(works))))
    }

    fn overview(&self, works: &[site::Work]) -> Div {
        let p = self.pal();
        let num = |v: &str| v.chars().filter(char::is_ascii_digit).collect::<String>().parse::<u64>().unwrap_or(0);
        let stat = |w: &site::Work, label: &str| w.stats.iter().find(|(_, l)| l == label).map(|(v, _)| num(v)).unwrap_or(0);
        let total = |label: &str| works.iter().map(|w| stat(w, label)).sum::<u64>();
        let tile = |label: &'static str| {
            div()
                .flex_1()
                .min_w(px(100.))
                .p(px(14.))
                .rounded(px(10.))
                .bg(rgb(p.bg))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD).child(group_digits(&total(label).to_string())))
                .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(label))
        };
        let views = total("Views").max(1);
        let mut by_views: Vec<_> = works.iter().map(|w| (w.title.clone(), stat(w, "Views"))).collect();
        by_views.sort_by(|a, b| b.1.cmp(&a.1));
        let card = |title: &'static str| {
            div()
                .p(px(18.))
                .rounded(px(14.))
                .bg(rgb(p.surface))
                .border_1()
                .border_color(rgb(p.line))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
        };
        div()
            .flex_none()
            .w(px(300.))
            .mt(px(22.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                card(if works.len() == 1 { "Your series" } else { "All series" })
                    .child(div().flex().flex_wrap().gap_2().child(tile("Views")).child(tile("Readers")))
                    .child(div().flex().flex_wrap().gap_2().child(tile("Favorites")).child(tile("Chapters")))
                    .child(div().flex().flex_wrap().gap_2().child(tile("Words")).child(tile("Reviews"))),
            )
            .when(works.len() > 1, |d| {
                d.child(card("Share of views").children(by_views.into_iter().map(|(t, v)| {
                    let share = v as f32 / views as f32;
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex()
                                .justify_between()
                                .gap_3()
                                .text_size(px(12.))
                                .child(div().min_w_0().truncate().child(t))
                                .child(div().flex_none().text_color(rgb(p.muted)).child(format!("{:.0}%", share * 100.))),
                        )
                        .child(div().h(px(6.)).rounded_full().bg(rgb(p.sel)).child(div().h_full().w(relative(share)).rounded_full().bg(rgb(p.accent))))
                })))
            })
    }

    fn delete_dialog(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (_, title) = self.series_delete.as_ref()?;
        let p = self.pal();
        let matches = self.delete_typed == *title;
        let busy = self.delete_state == DeleteState::Deleting;
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x0000008c))
                .child(
                    div()
                        .id("del-dialog")
                        .occlude()
                        .w(px(440.))
                        .p(px(24.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .rounded(px(14.))
                        .bg(rgb(p.surface))
                        .border_1()
                        .border_color(rgb(p.line))
                        .shadow_lg()
                        .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child("Delete this series?"))
                        .child(div().text_size(px(13.)).line_height(px(20.)).text_color(rgb(p.muted)).child(format!(
                            "All chapters, comments and statistics of “{title}” will be deleted. This can't be undone."
                        )))
                        .child(div().text_size(px(13.)).child("Type the series title to confirm."))
                        .child(
                            div()
                                .h(px(40.))
                                .px_3()
                                .flex()
                                .items_center()
                                .rounded(px(9.))
                                .bg(rgb(p.bg))
                                .border_1()
                                .border_color(rgb(if matches { p.star } else { p.line }))
                                .child(self.delete_input.clone()),
                        )
                        .when(self.delete_state == DeleteState::Failed, |d| {
                            d.child(div().text_size(px(12.)).text_color(rgb(p.star)).child("Scribble Hub didn't delete it. Try again, or use the website."))
                        })
                        .child(
                            div()
                                .mt_2()
                                .flex()
                                .justify_end()
                                .gap_2()
                                .child(self.button("del-cancel", "Cancel", |this, cx| this.close_series_delete(cx), cx))
                                .child(
                                    div()
                                        .id("del-go")
                                        .h(px(32.))
                                        .px_3()
                                        .flex()
                                        .items_center()
                                        .rounded(px(8.))
                                        .bg(rgb(p.star))
                                        .text_color(rgb(0xffffff))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .when(!matches || busy, |d| d.opacity(0.4))
                                        .when(matches && !busy, |d| d.cursor_pointer())
                                        .child(if busy { "Deleting…" } else { "Delete forever" })
                                        .on_click(cx.listener(|this, _, _, cx| this.confirm_series_delete(cx))),
                                ),
                        ),
                ),
        )
    }

    fn nav(&self, id: &'static str, kicker: &'static str, label: SharedString, none: &'static str, url: Option<String>, end: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.pal();
        let card = |d: Stateful<Div>| {
            d.flex_1()
                .h(px(64.))
                .px(px(18.))
                .flex()
                .flex_col()
                .justify_center()
                .when(end, |d| d.items_end())
                .rounded(px(12.))
                .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(kicker))
        };
        match url {
            Some(url) => card(self.link(id, url, true, cx)).bg(rgb(p.surface)).hover(move |s| s.bg(rgb(p.sel))).child(div().font_weight(FontWeight::SEMIBOLD).child(label)),
            None => card(div().id(id)).border_1().border_color(rgb(p.line)).text_color(rgb(p.muted)).child(div().child(none)),
        }
    }

    fn chapter(&self, c: &site::Chapter, url: &str, width: f32, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let list = if self.toc.0 == series_id(&c.series_url) { &self.toc.1[..] } else { &[] };
        let at = list.iter().position(|l| chapter_id(&l.url) == chapter_id(url));
        let kicker = match at {
            Some(i) => format!("Chapter {} of {}", i + 1, list.len()),
            None => c.series_title.clone(),
        };
        let title_at = |i: Option<usize>| i.and_then(|i| list.get(i)).map(|l| SharedString::from(l.title.clone()));
        let prev_title = title_at(at.and_then(|i| i.checked_sub(1))).unwrap_or("Previous chapter".into());
        let next_title = title_at(at.map(|i| i + 1)).unwrap_or("Next chapter".into());
        let col_w = (width - 48.).min(680.);
        let text_w = col_w - 48.;
        let body = c.blocks.iter().map(|b| match b {
            Block::Rule => div().flex().justify_center().my_4().child(div().w(px(72.)).h(px(1.)).bg(rgb(p.muted))),
            Block::Text(t, runs) => div().w(px(text_w)).child(StyledText::new(t.clone()).with_highlights(runs.iter().map(|(r, e)| {
                (r.clone(), HighlightStyle {
                    font_weight: e.bold.then_some(FontWeight::BOLD),
                    font_style: e.italic.then_some(FontStyle::Italic),
                    ..Default::default()
                })
            }))),
        });
        div().w_full().flex().flex_col().items_center().child(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .w(px(col_w))
                .px_6()
                .pt(px(64.))
                .pb_12()
                .child(div().mb(px(10.)).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.muted)).child(kicker.to_uppercase()))
                .child(div().mb(px(40.)).child(heading(c.title.clone(), 38.)))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(self.font * 1.1))
                        .font_family(if self.serif { SERIF } else { UI_FONT })
                        .text_size(px(self.font))
                        .line_height(relative(1.75))
                        .children(body),
                )
                .child(
                    div()
                        .mt(px(64.))
                        .pt_6()
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .flex()
                        .gap_3()
                        .child(self.nav("prev", "Previous", prev_title, "This is the first chapter", c.prev.clone(), false, cx))
                        .child(self.nav("next", "Next", next_title, "This is the latest chapter", c.next.clone(), true, cx)),
                ),
        )
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.pal();
        let banner = self.banner(cx);
        let banner_h = if banner.is_some() { BANNER_H } else { 0. };
        let live = self.browsing || self.signing_in || self.status == Status::Challenge;
        if let Some(Page::Home(h)) = self.stack.first() {
            self.shelf_scroll = h
                .shelves
                .iter()
                .filter(|s| !s.cards.is_empty())
                .map(|s| {
                    let handle = self
                        .shelf_scroll
                        .iter()
                        .find(|(n, _)| *n == s.name)
                        .map(|(_, h)| h.clone())
                        .unwrap_or_else(ScrollHandle::new);
                    (s.name.clone(), handle)
                })
                .collect();
            let unmeasured = self
                .shelf_scroll
                .iter()
                .zip(h.shelves.iter().filter(|s| !s.cards.is_empty()))
                .any(|((_, hnd), _)| f32::from(hnd.max_offset().width) <= 0.);
            if unmeasured && !self.shelf_remeasure {
                self.shelf_remeasure = true;
                let shell = cx.entity().downgrade();
                window.on_next_frame(move |_, cx| {
                    if let Some(s) = shell.upgrade() {
                        s.update(cx, |_, cx| cx.notify());
                    }
                });
            }
        } else {
            self.shelf_remeasure = false;
        }
        let search_focused = self.search.focus_handle(cx).is_focused(window);
        let show_suggest = !live && !self.suggestions.is_empty() && search_focused;
        let viewport = window.viewport_size();
        let top = CHROME_H + banner_h;
        self.web.show(live.then(|| Bounds::new(point(px(SIDE_W), px(top)), size(viewport.width - px(SIDE_W), viewport.height - px(top)))));

        let content = match self.stack.last() {
            Some(Page::Home(h)) => self.home(h, cx),
            Some(Page::Series(s, ch)) => self.series(s, ch, cx),
            Some(Page::Search(t, hits, paged)) => self.results(t, hits, paged.as_deref(), f32::from(viewport.width) - SIDE_W, cx),
            Some(Page::Chapter(c, url)) => self.chapter(c, url, f32::from(viewport.width), cx),
            Some(Page::ReadingList(rl)) => self.reading_list(rl, cx),
            Some(Page::Alerts(a)) => self.alerts(a, cx),
            Some(Page::Dashboard(w)) => self.dashboard(w, cx),
            Some(Page::SeriesForm(f)) => self.series_form_page(f, cx),
            Some(Page::ChapterForm(_)) if self.w.focus => self.focus_page(cx),
            Some(Page::ChapterForm(url)) => self.chapter_form_page(url.contains("/editchapter/"), f32::from(viewport.width) - SIDE_W, cx),
            Some(Page::Today) => self.today_page(f32::from(viewport.width) - SIDE_W, cx),
            Some(Page::Outline(o)) => self.outline_page(o, cx),
            Some(Page::History(h)) => self.history_page(h, f32::from(viewport.width) - SIDE_W, cx),
            Some(Page::Chapters(l)) => self.chapters_page(l, cx),
            Some(Page::Glossary(g)) => self.glossary_page(g, cx),
            Some(Page::Related(r)) => self.related_page(r, cx),
            Some(Page::Stats(st)) => self.stats_page(st, cx),
            Some(Page::Settings(s)) => self.settings_page(s, cx),
            None => div().size_full().flex().items_center().justify_center().text_color(rgb(p.muted)).child("Opening Scribble Hub…"),
        };
        let page = one_axis(content).id("page").flex_1().min_h_0().overflow_y_scroll().track_scroll(&self.scroll);
        let bar = match self.stack.last() {
            Some(Page::Chapter(c, _)) if !live => Some(self.reader_bar(c, cx)),
            _ => None,
        };
        let reading = bar.is_some() || (self.w.focus && matches!(self.stack.last(), Some(Page::ChapterForm(_))));
        let loading = self.status == Status::Loading && !live;
        let column = div()
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .children(bar)
            .children(banner)
            .child(page)
            .children(self.rl_action_bar(cx))
            .children(self.delete_dialog(cx))
            .children(self.crop_dialog(cx))
            .when(loading, |d| d.child(self.load_overlay()));
        let root = div()
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                window.focus(&this.search.focus_handle(cx));
                this.search.update(cx, |s, cx| s.select_all_text(cx));
            }))
            .on_action(cx.listener(|this, _: &Back, _, cx| this.go_back(cx)))
            .on_action(cx.listener(|this, _: &NextChapter, _, cx| this.chapter_link(true, cx)))
            .on_action(cx.listener(|this, _: &PrevChapter, _, cx| this.chapter_link(false, cx)))
            .on_action(cx.listener(|this, _: &Bigger, _, cx| this.reader_pref(cx, |s| s.font = (s.font + 1.).min(32.))))
            .on_action(cx.listener(|this, _: &Smaller, _, cx| this.reader_pref(cx, |s| s.font = (s.font - 1.).max(13.))))
            .on_action(cx.listener(|this, _: &CycleTheme, _, cx| this.reader_pref(cx, |s| s.theme = (s.theme + 1) % THEMES.len())))
            .on_action(cx.listener(|this, _: &ToggleSerif, _, cx| this.reader_pref(cx, |s| s.serif = !s.serif)))
            .on_action(cx.listener(|this, _: &ToggleWeb, _, cx| this.toggle_web(cx)))
            .on_action(|_: &input::FocusNext, window, _| window.focus_next())
            .on_action(|_: &input::FocusPrev, window, _| window.focus_prev())
            .size_full()
            .flex()
            .bg(rgb(p.bg))
            .text_color(rgb(p.ink))
            .text_size(px(14.))
            .font_family(UI_FONT)
            .relative()
            .when(!reading, |d| d.child(self.sidebar(show_suggest, search_focused, cx)))
            .child(column)
            .children(self.alerts_popover(cx))
            .children(self.mode_overlay(window));
        if CHROME_H == 0. {
            return root.into_any_element();
        }
        div().size_full().flex().flex_col().bg(rgb(p.bg)).child(self.title_bar(reading, window)).child(root.flex_1().min_h_0()).into_any_element()
    }
}

fn author(mut p: Pal) -> Pal {
    let light = p.name != "Dark";
    p.accent = if light { 0xd7827e } else { 0xebbcba };
    p.on_accent = if light { 0xffffff } else { 0x191724 };
    p
}

const SPLASH_MARK_W: f32 = 10. + 70.;

fn author_mark(size: f32, p: Pal) -> Div {
    div().pb(px(size * 0.12)).text_size(px(size)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.accent)).child("Author")
}

fn group_digits(v: &str) -> String {
    if !v.chars().all(|c| c.is_ascii_digit()) {
        return v.to_string();
    }
    let mut out = String::new();
    for (i, c) in v.chars().enumerate() {
        if i > 0 && (v.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn series_id(url: &str) -> String {
    url.split("/series/").nth(1).unwrap_or_default().chars().take_while(char::is_ascii_digit).collect()
}

fn chapter_id(url: &str) -> &str {
    url.trim_end_matches('/').rsplit('/').next().unwrap_or_default()
}

fn one_axis<E: Styled>(mut e: E) -> E {
    e.style().restrict_scroll_to_axis = Some(true);
    e
}

fn section_icon(name: &str) -> &'static str {
    if name.contains("Trend") { "icons/flame.svg" } else { "icons/sparkle.svg" }
}

fn icon_heading(icon: &'static str, title: impl Into<SharedString>, size: f32, p: Pal) -> Div {
    div().flex().items_center().gap(px(size * 0.4)).child(svg().path(icon).flex_none().size(px(size * 0.8)).text_color(rgb(p.muted))).child(heading(title, size))
}

fn heading(title: impl Into<SharedString>, size: f32) -> Div {
    div().font_family(SERIF).text_size(px(size)).font_weight(FontWeight::BOLD).line_height(relative(1.15)).child(title.into())
}

fn cover(url: &str, w: f32, h: f32, radius: f32, p: Pal) -> Div {
    div()
        .flex_none()
        .w(px(w))
        .h(px(h))
        .rounded(px(radius))
        .overflow_hidden()
        .bg(rgb(p.surface))
        .shadow_md()
        .child(img(SharedString::from(url.to_string())).size_full().rounded(px(radius)).object_fit(ObjectFit::Cover))
}

fn stars(rating: f32, size: f32, p: Pal) -> Div {
    div().flex().text_size(px(size)).children((1..=5).map(|i| {
        let fill = rating - (i - 1) as f32;
        div()
            .text_color(rgb(if fill >= 0.25 { p.star } else { p.line }))
            .when(fill >= 0.25 && fill < 0.75, |d| d.opacity(0.55))
            .child("★")
    }))
}

struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        macro_rules! icons { ($($n:literal),*) => { match path { $(concat!("icons/", $n, ".svg") => Some(include_bytes!(concat!("../assets/icons/", $n, ".svg")).as_slice()),)* _ => None } } }
        Ok(icons!("home", "flame", "sparkle", "clock", "bookmark", "bell", "sign-in", "sign-out", "sun", "moon", "globe", "trash", "select", "pen", "plus", "chart", "sort", "list", "more", "target", "board", "timer", "search", "focus", "link", "image", "undo", "redo", "align-left", "align-center", "align-right", "align-justify").map(Into::into))
    }

    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

#[cfg(not(target_os = "macos"))]
fn dock_icon() {}

#[cfg(target_os = "macos")]
fn dock_icon() {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = MainThreadMarker::new() else { return };
    let data = NSData::with_bytes(include_bytes!("../assets/icon/icon.png"));
    if let Some(img) = NSImage::initWithData(NSImage::alloc(), &data) {
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&img)) };
    }
}

fn main() {
    #[cfg(windows)]
    unsafe {
        std::env::set_var("GPUI_DISABLE_DIRECT_COMPOSITION", "1")
    };
    Application::new().with_assets(Icons).with_http_client(Arc::new(web::Images)).run(|cx: &mut App| {
        input::bind_keys(cx);
        rich::bind_keys(cx);
        dock_icon();
        load_fonts(cx);
        let browse = Some("!TextInput");
        cx.bind_keys([
            KeyBinding::new("secondary-[", Back, None),
            KeyBinding::new("escape", Back, browse),
            KeyBinding::new("right", NextChapter, browse),
            KeyBinding::new("left", PrevChapter, browse),
            KeyBinding::new("t", CycleTheme, browse),
            KeyBinding::new("f", ToggleSerif, browse),
            KeyBinding::new("secondary-=", Bigger, None),
            KeyBinding::new("secondary--", Smaller, None),
            KeyBinding::new("secondary-l", ToggleWeb, None),
            KeyBinding::new("secondary-f", FocusSearch, None),
            KeyBinding::new("secondary-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![Menu { name: "Scribble Hub".into(), items: vec![MenuItem::action("Quit Scribble Hub", Quit)] }]);
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let bounds = Bounds::centered(None, size(px(1180.), px(820.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("Scribble Hub".into()), appears_transparent: cfg!(any(target_os = "macos", windows)), traffic_light_position: Some(point(px(16.), px(16.))) }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Shell::new(window, cx)),
        )
        .expect("open window");
        cx.activate(true);
    });
}
