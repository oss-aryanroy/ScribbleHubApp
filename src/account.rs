use super::*;
use site::{Choice, Settings};

pub const TABS: [&str; 7] = ["Profile", "Privacy", "Alerts", "Author", "Email", "Password", "Blocked"];

pub struct AcctInputs {
    pub day: Entity<TextInput>,
    pub year: Entity<TextInput>,
    pub location: Entity<TextInput>,
    pub homepage: Entity<TextInput>,
    pub about: Entity<TextInput>,
    pub paypal: Entity<TextInput>,
    pub patreon: Entity<TextInput>,
    pub kofi: Entity<TextInput>,
    pub sstar: Entity<TextInput>,
    pub block: Entity<TextInput>,
}

impl AcctInputs {
    pub fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            day: cx.new(|cx| TextInput::new("Day", cx).max_chars(2)),
            year: cx.new(|cx| TextInput::new("Year", cx).max_chars(4)),
            location: cx.new(|cx| TextInput::new("Where you are", cx)),
            homepage: cx.new(|cx| TextInput::new("https://", cx)),
            about: cx.new(|cx| TextInput::multiline("Describe yourself", cx).max_chars(1000)),
            paypal: cx.new(|cx| TextInput::new("username", cx)),
            patreon: cx.new(|cx| TextInput::new("username", cx)),
            kofi: cx.new(|cx| TextInput::new("username", cx)),
            sstar: cx.new(|cx| TextInput::new("username", cx)),
            block: cx.new(|cx| TextInput::new("Username", cx)),
        }
    }
}

fn picked(c: &[Choice]) -> String {
    c.iter().find(|c| c.on).map(|c| c.id.clone()).unwrap_or_default()
}

impl Shell {
    pub(crate) fn open_settings(&mut self, tab: usize, cx: &mut Context<Self>) {
        self.alerts_pop = None;
        self.acct_tab = tab;
        self.form_state = FormState::Idle;
        if matches!(self.stack.last(), Some(Page::Settings(_))) && !self.browsing {
            return cx.notify();
        }
        self.browsing = false;
        self.open(format!("{}account/", site::BASE), false, cx);
    }

    pub(crate) fn load_settings(&mut self, s: &Settings, cx: &mut Context<Self>) {
        let a = &self.acct_in;
        for (input, v) in [(&a.day, &s.dob_day), (&a.year, &s.dob_year), (&a.location, &s.location), (&a.homepage, &s.homepage), (&a.about, &s.about), (&a.paypal, &s.paypal), (&a.patreon, &s.patreon), (&a.kofi, &s.kofi), (&a.sstar, &s.subscribestar)] {
            input.update(cx, |i, cx| i.set_text(v.clone(), cx));
        }
        self.blocked = None;
        self.acct_pick = None;
        self.avatar_new = None;
        if self.acct_tab == 6 {
            self.web.ajax("acct-blocked", "action=account_load_blocked");
        }
    }

    fn set_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        self.acct_tab = i;
        self.acct_pick = None;
        self.form_state = FormState::Idle;
        if i == 6 && self.blocked.is_none() {
            self.web.ajax("acct-blocked", "action=account_load_blocked");
        }
        cx.notify();
    }

    fn settings_mut(&mut self) -> Option<&mut Settings> {
        match self.stack.last_mut() {
            Some(Page::Settings(s)) => Some(s),
            _ => None,
        }
    }

    fn save_tab(&mut self, cx: &mut Context<Self>) {
        let Some(Page::Settings(s)) = self.stack.last() else { return };
        let a = &self.acct_in;
        let t = |e: &Entity<TextInput>| e.read(cx).text().to_string();
        let off = |list: Vec<&site::Toggle>| serde_json::to_string(&list.iter().filter(|t| !t.on).map(|t| t.key.as_str()).collect::<Vec<_>>()).unwrap_or_default();
        let (tag, body) = match self.acct_tab {
            0 => (
                "acct-main",
                site::form(&[
                    ("action", "wi_saveaccount_main"),
                    ("strTimeZone", &picked(&s.timezone)),
                    ("dob_month", &picked(&s.dob_month)),
                    ("dob_day", &t(&a.day)),
                    ("dob_year", &t(&a.year)),
                    ("user_gender", &picked(&s.gender)),
                    ("user_location", &t(&a.location)),
                    ("user_homepage", &t(&a.homepage)),
                    ("user_aboutme", &t(&a.about)),
                    ("userTimeFormat", &picked(&s.time_format)),
                    ("userdateFormat", "0"),
                ]),
            ),
            1 => ("acct-privacy", site::form(&[("action", "wi_saveaccount_privacy"), ("rl_move_list", &off(s.privacy.iter().collect()))])),
            2 => ("acct-alerts", site::form(&[("action", "wi_saveaccount_alerts"), ("rl_move_list", &off(s.alerts.iter().flat_map(|(_, l)| l).collect()))])),
            3 => (
                "acct-author",
                site::form(&[("action", "wi_saveaccount_author"), ("strPaypal", &t(&a.paypal)), ("strPatreon", &t(&a.patreon)), ("strKofi", &t(&a.kofi)), ("strSStar", &t(&a.sstar))]),
            ),
            _ => return,
        };
        self.form_state = FormState::Saving;
        self.web.ajax(tag, &body);
        cx.notify();
    }

    fn block_user(&mut self, cx: &mut Context<Self>) {
        let name = self.acct_in.block.read(cx).text().trim().to_string();
        if name.is_empty() {
            return;
        }
        self.form_state = FormState::Saving;
        self.web.ajax("acct-block", &site::form(&[("action", "wi_blockuser_username"), ("strBlockuser", &name)]));
        cx.notify();
    }

    fn unblock_user(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(list) = self.blocked.as_mut() {
            list.retain(|b| b.id != id);
        }
        self.web.ajax("acct-unblock", &site::form(&[("action", "wi_blockuser"), ("intAuthorID", &id), ("getBlockText", "Unblock User")]));
        cx.notify();
    }

    pub(crate) fn upload_avatar(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Ok(bytes) = std::fs::read(&path) else { return self.fail("Couldn't read that image.", cx) };
        self.avatar_new = Some(path);
        let js = format!(
            "{}var b=atob({}),u=new Uint8Array(b.length);for(var i=0;i<b.length;i++)u[i]=b.charCodeAt(i);\
             when(function(){{return window.jQuery&&document.getElementById('file')&&window.account_upload_avatar;}},function(){{\
             var dt=new DataTransfer();dt.items.add(new File([u],'avatar.jpg',{{type:'image/jpeg'}}));document.getElementById('file').files=dt.files;\
             var old=$('.avatarchange').attr('src');account_upload_avatar();\
             when(function(){{return $('.avatarchange').attr('src')!=old;}},function(){{post(true,'');}});}});",
            studio::HELPERS,
            serde_json::to_string(&studio::base64(&bytes)).unwrap_or_default(),
        );
        self.run_in(&format!("{}account/", site::BASE), js, None, cx);
    }

    pub(crate) fn on_acct_ajax(&mut self, tag: &str, text: &str, cx: &mut Context<Self>) -> bool {
        if !tag.starts_with("acct-") {
            return false;
        }
        let body = text.trim();
        let body = body.strip_suffix('0').unwrap_or(body).trim();
        let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
        let err = json.as_ref().and_then(|j| j["error"].as_str()).map(site::plain).filter(|e| !e.is_empty());
        let unreachable = text.is_empty();
        match tag {
            "acct-blocked" => self.blocked = Some(site::blocked(body)),
            "acct-block" => {
                self.form_state = match (unreachable, err) {
                    (true, _) => FormState::Error("Couldn't reach Scribble Hub.".into()),
                    (_, Some(e)) => FormState::Error(e.into()),
                    _ => {
                        self.acct_in.block.update(cx, |i, cx| i.clear(cx));
                        self.web.ajax("acct-blocked", "action=account_load_blocked");
                        FormState::Done("Blocked".into())
                    }
                }
            }
            "acct-unblock" => self.web.ajax("acct-blocked", "action=account_load_blocked"),
            "acct-author" if !body.is_empty() => self.form_state = FormState::Error(site::plain(body).into()),
            _ => {
                self.form_state = match (unreachable, err) {
                    (true, _) => FormState::Error("Couldn't reach Scribble Hub.".into()),
                    (_, Some(e)) => FormState::Error(e.into()),
                    _ => FormState::Done("Saved".into()),
                }
            }
        }
        cx.notify();
        true
    }

    fn select(&self, id: &'static str, list: &[Choice], which: fn(&mut Settings) -> &mut Vec<Choice>, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let open = self.acct_pick == Some(id);
        let current = list.iter().find(|c| c.on).map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or("—".into());
        let menu = open.then(|| {
            deferred(
                div()
                    .id((id, 9999usize))
                    .occlude()
                    .absolute()
                    .top(px(46.))
                    .left_0()
                    .min_w(px(240.))
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .p(px(4.))
                    .bg(rgb(p.surface))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgb(p.line))
                    .shadow_lg()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.acct_pick = None;
                        cx.notify();
                    }))
                    .children(list.iter().enumerate().map(|(i, c)| {
                        div()
                            .id((id, i))
                            .px(px(10.))
                            .py(px(6.))
                            .rounded(px(6.))
                            .text_size(px(13.))
                            .cursor_pointer()
                            .when(c.on, |d| d.bg(rgb(p.sel)).text_color(rgb(p.accent)).font_weight(FontWeight::SEMIBOLD))
                            .hover(move |d| d.bg(rgb(p.sel)))
                            .child(if c.name.is_empty() { "—".to_string() } else { c.name.clone() })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.settings_mut() {
                                    which(s).iter_mut().enumerate().for_each(|(j, c)| c.on = j == i);
                                }
                                this.acct_pick = None;
                                this.form_state = FormState::Idle;
                                cx.notify();
                            }))
                    })),
            )
        });
        div()
            .relative()
            .child(
                div()
                    .id(id)
                    .h(px(42.))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .rounded(px(10.))
                    .bg(rgb(p.bg))
                    .border_1()
                    .border_color(rgb(if open { p.accent } else { p.line }))
                    .cursor_pointer()
                    .child(div().min_w_0().truncate().child(current))
                    .child(div().text_color(rgb(p.muted)).child("▾"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.acct_pick = if this.acct_pick == Some(id) { None } else { Some(id) };
                        cx.notify();
                    })),
            )
            .children(menu)
    }

    fn choice_chips(&self, key: &'static str, list: &[Choice], which: fn(&mut Settings) -> &mut Vec<Choice>, cx: &mut Context<Self>) -> Div {
        div().flex().flex_wrap().gap_2().children(list.iter().enumerate().filter(|(_, c)| !c.name.is_empty()).map(|(i, c)| {
            self.chip((key, i), c.name.clone(), c.on).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(s) = this.settings_mut() {
                    which(s).iter_mut().enumerate().for_each(|(j, c)| c.on = j == i);
                }
                this.form_state = FormState::Idle;
                cx.notify();
            }))
        }))
    }

    pub(crate) fn labelled(&self, title: &'static str, hint: Option<String>, body: impl IntoElement) -> Div {
        div().flex().flex_col().gap_2().child(self.section(title, hint)).child(body)
    }

    fn save_footer(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.form_state == FormState::Saving;
        self.form_footer(self.big_button("acct-save", "Save", true, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.save_tab(cx) })), None)
    }

    pub(crate) fn settings_page(&self, s: &Settings, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let tab = self.acct_tab;
        let tabs = div().flex().flex_wrap().gap_1().border_b_1().border_color(rgb(p.line)).children(TABS.iter().enumerate().map(|(i, name)| {
            div()
                .id(("acct-tab", i))
                .px(px(14.))
                .py(px(10.))
                .mb(px(-1.))
                .text_size(px(14.))
                .cursor_pointer()
                .border_b_2()
                .map(|d| if i == tab { d.border_color(rgb(p.accent)).text_color(rgb(p.ink)).font_weight(FontWeight::SEMIBOLD) } else { d.border_color(rgba(0x00000000)).text_color(rgb(p.muted)).hover(move |d| d.text_color(rgb(p.ink))) })
                .child(*name)
                .on_click(cx.listener(move |this, _, _, cx| this.set_tab(i, cx)))
        }));
        let a = &self.acct_in;
        let body: Vec<AnyElement> = match tab {
            0 => {
                let avatar = match (&self.avatar_new, &s.avatar) {
                    (Some(path), _) => img(path.clone()).size_full().rounded_full().object_fit(ObjectFit::Cover).into_any_element(),
                    (None, Some(url)) => img(SharedString::from(url.clone())).size_full().rounded_full().object_fit(ObjectFit::Cover).into_any_element(),
                    _ => div().into_any_element(),
                };
                let about_len = a.about.read(cx).text().chars().count();
                let dob = div()
                    .flex()
                    .gap_2()
                    .child(div().w(px(150.)).child(self.select("acct-month", &s.dob_month, |s| &mut s.dob_month, cx)))
                    .child(div().w(px(72.)).child(self.field("acct-day", &a.day, 0., cx)))
                    .child(div().w(px(96.)).child(self.field("acct-year", &a.year, 0., cx)));
                vec![
                    self.card("Profile", None)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_4()
                                .child(div().size(px(64.)).flex_none().rounded_full().overflow_hidden().bg(rgb(p.sel)).child(avatar))
                                .child(div().flex_1().min_w_0().text_size(px(17.)).font_weight(FontWeight::SEMIBOLD).child(s.username.clone()))
                                .child(self.big_button("acct-avatar", "Change avatar", false, false).on_click(cx.listener(|this, _, _, cx| this.pick_image(true, cx)))),
                        )
                        .child(self.labelled("About", Some(format!("{} / 1,000", group_digits(&about_len.to_string()))), self.field("acct-about", &a.about, 120., cx)))
                        .child(self.labelled("Location", None, self.field("acct-location", &a.location, 0., cx)))
                        .child(self.labelled("Home page", None, self.field("acct-homepage", &a.homepage, 0., cx)))
                        .into_any_element(),
                    self.card("Personal", None)
                        .child(self.labelled("Date of birth", None, dob))
                        .child(self.labelled("Gender", None, self.choice_chips("acct-gender", &s.gender, |s| &mut s.gender, cx)))
                        .into_any_element(),
                    self.card("Time", None)
                        .child(self.labelled("Time zone", None, self.select("acct-tz", &s.timezone, |s| &mut s.timezone, cx)))
                        .child(self.labelled("Time format", None, self.choice_chips("acct-tf", &s.time_format, |s| &mut s.time_format, cx)))
                        .into_any_element(),
                    self.save_footer(cx).into_any_element(),
                ]
            }
            1 => vec![
                self.card("Privacy", None)
                    .children(s.privacy.iter().enumerate().map(|(i, t)| {
                        self.switch_row(("acct-priv", i), t.label.clone(), Some(t.hint.clone()), t.on).on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(s) = this.settings_mut() {
                                s.privacy[i].on = !s.privacy[i].on;
                            }
                            this.form_state = FormState::Idle;
                            cx.notify();
                        }))
                    }))
                    .into_any_element(),
                self.save_footer(cx).into_any_element(),
            ],
            2 => {
                let mut cards: Vec<AnyElement> = s
                .alerts
                .iter()
                .enumerate()
                .map(|(si, (head, list))| {
                    let head: &'static str = match head.as_str() {
                        "General" => "General",
                        "Profile" => "Profile",
                        "Authors" => "Authors",
                        _ => "Alerts",
                    };
                    self.card(head, None)
                        .children(list.iter().enumerate().map(|(i, t)| {
                            self.switch_row(("acct-alert", si * 100 + i), t.label.clone(), None, t.on).on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(s) = this.settings_mut() {
                                    s.alerts[si].1[i].on = !s.alerts[si].1[i].on;
                                }
                                this.form_state = FormState::Idle;
                                cx.notify();
                            }))
                        }))
                        .into_any_element()
                })
                .collect();
                cards.push(self.save_footer(cx).into_any_element());
                cards
            }
            3 => vec![
                self.card("Support links", None)
                    .child(self.labelled("PayPal", Some("paypal.me/".into()), self.field("acct-paypal", &a.paypal, 0., cx)))
                    .child(self.labelled("Patreon", Some("patreon.com/".into()), self.field("acct-patreon", &a.patreon, 0., cx)))
                    .child(self.labelled("Ko-fi", Some("ko-fi.com/".into()), self.field("acct-kofi", &a.kofi, 0., cx)))
                    .child(self.labelled("SubscribeStar", Some("subscribestar.com/".into()), self.field("acct-sstar", &a.sstar, 0., cx)))
                    .into_any_element(),
                self.save_footer(cx).into_any_element(),
            ],
            4 => vec![self
                .card("Email", None)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(div().flex_1().min_w_0().truncate().text_size(px(15.)).child(s.email.clone()))
                        .child(
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded_full()
                                .text_size(px(11.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .map(|d| if s.email_confirmed { d.bg(rgb(p.accent)).text_color(rgb(p.on_accent)) } else { d.bg(rgb(p.sel)).text_color(rgb(p.star)) })
                                .child(if s.email_confirmed { "Confirmed" } else { "Not confirmed" }),
                        ),
                )
                .child(div().flex().child(self.big_button("acct-email", "Change email", false, false).on_click(cx.listener(|this, _, _, cx| this.open_site_tab("$('#account_tab_5').prop('checked',true)", cx)))))
                .into_any_element()],
            5 => vec![self
                .card("Password", None)
                .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child("Changed on Scribble Hub's own page, so your password never passes through the app."))
                .child(div().flex().child(self.big_button("acct-password", "Change password", false, false).on_click(cx.listener(|this, _, _, cx| this.open_site_tab("$('#account_tab_6').prop('checked',true)", cx)))))
                .into_any_element()],
            _ => {
                let busy = self.form_state == FormState::Saving;
                let list: AnyElement = match &self.blocked {
                    None => div().text_size(px(13.)).text_color(rgb(p.muted)).child("Loading…").into_any_element(),
                    Some(l) if l.is_empty() => div().text_size(px(14.)).text_color(rgb(p.muted)).child("No one blocked.").into_any_element(),
                    Some(l) => div()
                        .flex()
                        .flex_col()
                        .children(l.iter().enumerate().map(|(i, b)| {
                            let id = b.id.clone();
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .py(px(8.))
                                .when(i > 0, |d| d.border_t_1().border_color(rgb(p.line)))
                                .child(div().flex_1().min_w_0().truncate().child(b.name.clone()))
                                .child(self.big_button(("acct-unblock", i), "Unblock", false, false).on_click(cx.listener(move |this, _, _, cx| this.unblock_user(id.clone(), cx))))
                        }))
                        .into_any_element(),
                };
                vec![
                    self.card("Blocked users", None).child(list).into_any_element(),
                    self.card("Block someone", None)
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(div().flex_1().child(self.field("acct-block", &a.block, 0., cx)))
                                .child(self.big_button("acct-block-go", "Block", true, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.block_user(cx) }))),
                        )
                        .children(match &self.form_state {
                            FormState::Error(m) => Some(div().text_size(px(13.)).text_color(rgb(p.star)).child(m.clone())),
                            FormState::Done(m) => Some(div().text_size(px(13.)).text_color(rgb(p.accent)).child(m.clone())),
                            _ => None,
                        })
                        .into_any_element(),
                ]
            }
        };
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(720.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(self.page_head("Settings", None, cx))
                .child(tabs)
                .children(body),
        )
    }

    fn open_site_tab(&mut self, script: &'static str, cx: &mut Context<Self>) {
        self.open_live(format!("{}account/", site::BASE), cx);
        self.live_script = Some(script);
    }
}
