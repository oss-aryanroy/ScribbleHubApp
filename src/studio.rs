use super::*;
use scraper::Html;
use site::{Choice, SeriesForm};

const FANFIC: &str = "38";

pub struct Stats {
    pub sid: String,
    pub title: String,
    pub totals: Vec<(String, String)>,
    pub kind: &'static str,
    pub range: &'static str,
    pub chart: Vec<(String, f64)>,
    pub loading: bool,
    pub refs: (Vec<(String, String)>, Vec<(String, String)>),
}

pub struct ChapterList {
    pub sid: String,
    pub title: String,
    pub rows: Vec<site::ChapterRow>,
    pub loading: bool,
    pub error: Option<String>,
    pub order_changed: bool,
    pub confirm_delete: Option<String>,
}

#[derive(Clone, PartialEq)]
pub enum FormState {
    Idle,
    Saving,
    Done(SharedString),
    Error(SharedString),
}

#[derive(Clone, Copy, PartialEq)]
pub enum Awaiting {
    SeriesAdded,
    Chapter { publish: bool },
    ChapterSaved,
}

impl Shell {
    pub(crate) fn open_series_form(&mut self, sid: Option<&str>, cx: &mut Context<Self>) {
        self.form_state = FormState::Idle;
        self.cover_path = None;
        let url = match sid {
            Some(id) => format!("{}editseries/{id}/", site::BASE),
            None => format!("{}add-series/", site::BASE),
        };
        self.browsing = false;
        self.open(url, false, cx);
    }

    pub(crate) fn open_chapter_form(&mut self, w: &site::Work, cx: &mut Context<Self>) {
        self.form_state = FormState::Idle;
        self.publish_armed = false;
        self.form_series = w.title.clone();
        self.browsing = false;
        self.open(format!("{}addchapter/{}/", site::BASE, w.id), false, cx);
    }

    pub(crate) fn open_stats(&mut self, w: &site::Work, cx: &mut Context<Self>) {
        self.browsing = false;
        self.want = None;
        self.stack.push(Page::Stats(Box::new(Stats {
            sid: w.id.clone(),
            title: w.title.clone(),
            totals: w.stats.clone(),
            kind: "views",
            range: "days",
            chart: vec![],
            loading: true,
            refs: (vec![], vec![]),
        })));
        self.status = Status::Ready;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.fetch_chart(cx);
        self.web.ajax("refs", &format!("action=wi_referrers_stat&mypostid={}&index=0&option=", digits(&w.id)));
        cx.notify();
    }

    fn fetch_chart(&mut self, cx: &mut Context<Self>) {
        if let Some(Page::Stats(st)) = self.stack.last_mut() {
            st.loading = true;
            let readers = if st.kind == "readers" { "gained" } else { "" };
            self.web.ajax(
                "chart",
                &format!("action=wi_newstats_getstats&mypostid={}&strChartType={}&readers_type={readers}&strChart_option={}&ismobile=", digits(&st.sid), st.kind, st.range),
            );
        }
        cx.notify();
    }

    pub(crate) fn load_series_form(&mut self, f: &SeriesForm, cx: &mut Context<Self>) {
        let (t, d, fd) = (f.title.clone(), f.description.clone(), f.fandom.clone());
        self.f_title.update(cx, |i, cx| i.set_text(t, cx));
        self.f_desc.update(cx, |i, cx| i.set_text(d, cx));
        self.f_fandom.update(cx, |i, cx| i.set_text(fd, cx));
        self.f_tag.update(cx, |i, cx| i.clear(cx));
    }

    pub(crate) fn load_chapter_form(&mut self, url: &str, html: &str, cx: &mut Context<Self>) {
        self.c_lossy.clear();
        let doc = Html::parse_document(html);
        let pick = |q: &str| doc.select(&scraper::Selector::parse(q).unwrap()).next();
        let title = pick("#chapter-title").and_then(|e| e.value().attr("value")).unwrap_or_default().to_string();
        let saved = pick("#edit_mycontent_chapter").map(|e| e.text().collect::<String>()).unwrap_or_default();
        let editing = html.contains("/editchapter/") || pick("#ac_post_status option[value='published']").is_some();
        self.c_status = if editing { pick("#ac_post_status option[selected]").and_then(|o| o.value().attr("value")).unwrap_or_default().to_string() } else { String::new() };
        if editing && !matches!(self.c_status.as_str(), "published" | "draft") {
            self.c_lossy.push("a scheduled or unusual status");
        }
        let lost = self.c_body.update(cx, |e, cx| e.set_html(&saved, cx));
        self.c_lossy.splice(0..0, lost);
        self.c_title.update(cx, |i, cx| i.set_text(title.clone(), cx));
        self.c_menu = None;
        self.desk_chapter_loaded(url, html, &saved, &title, cx);
    }

    pub(crate) fn open_chapters(&mut self, w: &site::Work, cx: &mut Context<Self>) {
        self.form_state = FormState::Idle;
        self.browsing = false;
        self.want = None;
        self.stack.push(Page::Chapters(Box::new(ChapterList { sid: w.id.clone(), title: w.title.clone(), rows: vec![], loading: true, error: None, order_changed: false, confirm_delete: None })));
        self.status = Status::Ready;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.fetch_chapters(&w.id.clone());
        cx.notify();
    }

    fn fetch_chapters(&self, sid: &str) {
        self.web.ajax("chapters", &site::form(&[("action", "wi_loadchapters"), ("intPostID", &digits(sid)), ("toc_batch", "initial")]));
    }

    pub(crate) fn open_edit_chapter(&mut self, cid: &str, cx: &mut Context<Self>) {
        if let Some(Page::Chapters(l)) = self.stack.last() {
            self.form_series = l.title.clone();
        }
        self.form_state = FormState::Idle;
        self.publish_armed = false;
        self.browsing = false;
        self.open(format!("{}editchapter/{cid}/", site::BASE), false, cx);
    }

    pub(crate) fn on_ajax(&mut self, tag: &str, text: &str, cx: &mut Context<Self>) {
        if tag.starts_with("chapters") {
            let sid = match self.stack.last() {
                Some(Page::Chapters(l)) => l.sid.clone(),
                _ => return,
            };
            let json: Option<serde_json::Value> = serde_json::from_str(text.trim().strip_suffix('0').unwrap_or(text.trim())).ok();
            let data = json.as_ref().filter(|j| j["success"].as_bool() == Some(true)).map(|j| &j["data"]);
            let ids = |k: &str| data.and_then(|d| d[k].as_array()).map(|a| a.iter().map(|v| v.to_string().trim_matches('"').to_string()).collect::<Vec<_>>()).unwrap_or_default();
            let (all, loaded) = (ids("chapter_ids"), ids("loaded_ids"));
            let more = data.map(|d| site::chapter_rows(d["html"].as_str().unwrap_or_default()));
            if let Some(Page::Chapters(l)) = self.stack.last_mut() {
                match more {
                    Some(rows) => {
                        if tag == "chapters" {
                            l.rows = rows;
                        } else {
                            l.rows.extend(rows);
                        }
                        l.loading = false;
                        if tag == "chapters" && loaded.len() < all.len() {
                            l.loading = true;
                            self.web.ajax(
                                "chapters-more",
                                &site::form(&[("action", "wi_loadchapters"), ("intPostID", &digits(&sid)), ("toc_batch", "remaining"), ("chapter_ids", &all.join(",")), ("loaded_ids", &loaded.join(","))]),
                            );
                        }
                    }
                    None => {
                        l.loading = false;
                        l.error = Some(json.as_ref().and_then(|j| j["data"]["message"].as_str()).unwrap_or("Couldn't load the chapters.").to_string());
                    }
                }
            }
            return cx.notify();
        }
        if let Some(Page::Stats(st)) = self.stack.last_mut() {
            match tag {
                "chart" => {
                    st.chart = site::stats_chart(text);
                    st.loading = false;
                }
                "refs" => st.refs = site::referrers(text),
                _ => {}
            }
        }
        cx.notify();
    }

    pub(crate) fn on_saved(&mut self, ok: bool, msg: String, cx: &mut Context<Self>) {
        self.awaiting = None;
        self.form_state = if ok { FormState::Done("Saved".into()) } else { FormState::Error(msg.into()) };
        cx.notify();
    }

    pub(crate) fn on_landed(&mut self, at: &str, html: &str, cx: &mut Context<Self>) -> bool {
        let Some(a) = self.awaiting.take() else { return false };
        let ok = match a {
            Awaiting::SeriesAdded => !at.contains("/add-series"),
            Awaiting::Chapter { .. } | Awaiting::ChapterSaved => at.contains("editchapter"),
        };
        if ok {
            if !matches!(a, Awaiting::SeriesAdded) {
                self.desk_saved_to_site();
            }
            self.form_state = FormState::Done(match a {
                Awaiting::SeriesAdded => "Series created".into(),
                Awaiting::Chapter { publish: true } => "Chapter published".into(),
                Awaiting::Chapter { publish: false } => "Draft saved".into(),
                Awaiting::ChapterSaved => "Chapter saved".into(),
            });
            while self.stack.len() > 1 && !matches!(self.stack.last(), Some(Page::Dashboard(_)) | Some(Page::Chapters(_))) {
                self.stack.pop();
            }
            if let Some(Page::Chapters(l)) = self.stack.last_mut() {
                l.loading = true;
                let sid = l.sid.clone();
                self.fetch_chapters(&sid);
            } else {
                let replace = matches!(self.stack.last(), Some(Page::Dashboard(_)));
                self.open(format!("{}dashboard/", site::BASE), replace, cx);
            }
        } else {
            let err = Html::parse_document(html)
                .select(&scraper::Selector::parse(".edit_saveerror, .error_list_add").unwrap())
                .map(|e| e.text().collect::<String>().trim().to_string())
                .find(|t| !t.is_empty())
                .unwrap_or_else(|| "Scribble Hub didn't accept it. Check the fields and try again.".into());
            self.form_state = FormState::Error(err.into());
        }
        cx.notify();
        true
    }

    pub(crate) fn run_in(&mut self, page_url: &str, js: String, awaiting: Option<Awaiting>, cx: &mut Context<Self>) {
        self.form_state = FormState::Saving;
        let path = page_url.trim_start_matches(site::BASE);
        if self.web_at.contains(path) && !path.is_empty() {
            self.web.eval(&js);
            self.awaiting = awaiting;
        } else {
            self.web.load(page_url);
            self.after_load = Some((js, awaiting));
        }
        cx.notify();
    }

    pub(crate) fn save_series(&mut self, cx: &mut Context<Self>) {
        let Some(Page::SeriesForm(f)) = self.stack.last() else { return };
        let ids = |cs: &[Choice]| cs.iter().filter(|c| c.on).map(|c| c.id.clone()).collect::<Vec<_>>();
        let data = serde_json::json!({
            "title": self.f_title.read(cx).text().trim(),
            "description": self.f_desc.read(cx).text().trim(),
            "genres": ids(&f.genres),
            "main": f.main_genre,
            "fandom": self.f_fandom.read(cx).text(),
            "tags": f.picked_tags,
            "copyright": f.copyright.iter().find(|c| c.on).map(|c| c.id.clone()).unwrap_or_default(),
            "mature": f.mature,
            "warnings": ids(&f.warnings),
            "status": f.status.iter().find(|c| c.on).map(|c| c.id.clone()),
        });
        let adding = f.sid.is_none();
        let cover = if adding {
            match self.cover_path.as_ref().map(std::fs::read) {
                Some(Ok(bytes)) => {
                    let name = self.cover_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().to_string()).unwrap_or("cover.jpg".into());
                    let mime = if name.to_lowercase().ends_with(".png") { "image/png" } else if name.to_lowercase().ends_with(".gif") { "image/gif" } else { "image/jpeg" };
                    format!(
                        "var b=atob({}),u=new Uint8Array(b.length);for(var i=0;i<b.length;i++)u[i]=b.charCodeAt(i);\
                         var dt=new DataTransfer();dt.items.add(new File([u],{},{{type:{}}}));document.getElementById('file').files=dt.files;",
                        serde_json::to_string(&base64(&bytes)).unwrap_or_default(),
                        serde_json::to_string(&name).unwrap_or_default(),
                        serde_json::to_string(mime).unwrap_or_default(),
                    )
                }
                Some(Err(_)) => return self.fail("Couldn't read that cover image.", cx),
                None => String::new(),
            }
        } else {
            String::new()
        };
        let (url, call, awaiting) = if adding {
            (format!("{}add-series/", site::BASE), "addseries(document.querySelector('[onclick^=\"addseries\"]'));watch_errors();", Some(Awaiting::SeriesAdded))
        } else {
            let sid = f.sid.clone().unwrap_or_default();
            (format!("{}editseries/{sid}/", site::BASE), "watch_ajax('wi_savefic_saveedits');savefic_edits(document.querySelector('[onclick^=\"savefic_edits\"]'));watch_errors();", None)
        };
        let js = format!(
            "{HELPERS}when(()=>window.jQuery&&$('#edittitle').length,function(){{var d={data};\
             $('#edittitle').val(d.title);$('#editdescription').val(d.description);\
             $('.altchk-g').each(function(){{var id=$(this).attr('gmeid')||this.id.replace('gen','');var on=d.genres.indexOf(id)>=0;$(this).attr('chkme',on?'include':'off').prop('checked',on);}});\
             try{{reload_select_box_gen();}}catch(e){{}}\
             if(d.main&&!$('#main_genre option[value=\"'+d.main+'\"]').length)$('#main_genre').append($('<option>').val(d.main));\
             $('#main_genre').val(d.main);$('#editfandom').val(d.fandom);\
             $('.tagsalledit').val(d.tags).trigger('change');$('#copyrightselect').val(d.copyright);\
             $('#toggle-rating').prop('checked',d.mature);try{{$('#toggle-rating').bootstrapToggle(d.mature?'on':'off');}}catch(e){{}}\
             $('.es_mature_content').each(function(){{$(this).attr('value',d.warnings.indexOf($(this).attr('ctid'))>=0?'1':'0');}});\
             if(d.status)$('#fic_storystatus').val(d.status);{cover}{call}}});",
        );
        self.run_in(&url, js, awaiting, cx);
    }

    pub(crate) fn save_chapter(&mut self, publish: bool, cx: &mut Context<Self>) {
        let Some(Page::ChapterForm(url)) = self.stack.last() else { return };
        let url = url.clone();
        if !self.c_lossy.is_empty() {
            return self.fail("This chapter has formatting the app can't keep; edit it on Scribble Hub.", cx);
        }
        let title = self.c_title.read(cx).text().trim().to_string();
        let body = self.c_body.read(cx).html();
        if title.is_empty() || body.is_empty() {
            return self.fail("A chapter needs a title and some text.", cx);
        }
        let editing = url.contains("/editchapter/");
        let live = editing && self.c_status == "published";
        let call = if live {
            "var b=$('.meshbtn.publish').filter(function(){return $(this).text().trim()=='Update';})[0];\
             if(!b)return post(false,'Scribble Hub’s page has no Update button; nothing was saved.');publish_check(b);"
        } else if publish {
            "var b=document.getElementById('pub_chp_btn');if(!b)return post(false,'Scribble Hub’s page has no Publish button; nothing was saved.');publish_check(b);"
        } else {
            "var b=document.getElementById('save_draft_preview');if(!b)return post(false,'Scribble Hub’s page has no Save Draft button; nothing was saved.');ea_save(b);"
        };
        let js = format!(
            "{HELPERS}when(()=>window.jQuery&&$('#chapter-title').length&&($('#mbeditor').attr('value')=='1'||(window.tinyMCE&&tinyMCE.get('edit_mycontent_chapter'))),function(){{\
             $('#chapter-title').val({t});var h={b};\
             if($('#mbeditor').attr('value')=='1')$('#edit_mycontent_chapter').trumbowyg('html',h);else tinyMCE.get('edit_mycontent_chapter').setContent(h);\
             {call}watch_errors();}});",
            t = serde_json::to_string(&title).unwrap_or_default(),
            b = serde_json::to_string(&body).unwrap_or_default(),
        );
        self.publish_armed = false;
        self.run_in(&url, js, Some(if live { Awaiting::ChapterSaved } else { Awaiting::Chapter { publish } }), cx);
    }

    pub(crate) fn fail(&mut self, msg: &str, cx: &mut Context<Self>) {
        self.form_state = FormState::Error(msg.to_string().into());
        cx.notify();
    }

    pub(crate) fn field(&self, id: &'static str, input: &Entity<TextInput>, min_h: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.pal();
        let focus = input.read(cx).focus.clone();
        div()
            .id(id)
            .min_h(px(min_h))
            .px_3()
            .py(px(10.))
            .rounded(px(10.))
            .bg(rgb(p.bg))
            .border_1()
            .border_color(rgb(p.line))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, window, _| {
                window.focus(&focus);
                window.prevent_default();
            }))
            .child(input.clone())
    }

    pub(crate) fn chip(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, on: bool) -> Stateful<Div> {
        let p = self.pal();
        div()
            .id(id)
            .px(px(12.))
            .py(px(6.))
            .rounded_full()
            .text_size(px(13.))
            .cursor_pointer()
            .border_1()
            .map(|d| if on { d.bg(rgb(p.accent)).border_color(rgb(p.accent)).text_color(rgb(p.on_accent)).font_weight(FontWeight::SEMIBOLD) } else { d.bg(rgb(p.bg)).border_color(rgb(p.line)).text_color(rgb(p.ink)).hover(move |d| d.border_color(rgb(p.muted))) })
            .child(label.into())
    }

    pub(crate) fn card(&self, title: &'static str, count: Option<String>) -> Div {
        let p = self.pal();
        div()
            .p(px(22.))
            .rounded(px(14.))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .children(count.map(|c| div().text_size(px(13.)).text_color(rgb(p.muted)).child(c))),
            )
    }

    pub(crate) fn switch_row(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, hint: Option<String>, on: bool) -> Stateful<Div> {
        let p = self.pal();
        div()
            .id(id)
            .flex()
            .items_center()
            .gap_4()
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).child(label.into()))
                    .children(hint.filter(|h| !h.is_empty()).map(|h| div().text_size(px(12.)).text_color(rgb(p.muted)).child(h))),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(40.))
                    .h(px(24.))
                    .p(px(3.))
                    .rounded_full()
                    .flex()
                    .when(on, |d| d.justify_end())
                    .bg(rgb(if on { p.accent } else { p.sel }))
                    .child(div().size(px(18.)).rounded_full().bg(rgb(if on { p.on_accent } else { p.muted }))),
            )
    }

    pub(crate) fn section(&self, title: &'static str, hint: Option<String>) -> Div {
        let p = self.pal();
        div()
            .flex()
            .items_baseline()
            .gap_2()
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .children(hint.map(|h| div().text_size(px(12.)).text_color(rgb(p.muted)).child(h)))
    }

    pub(crate) fn form_footer(&self, primary: Stateful<Div>, extra: Option<Stateful<Div>>) -> Div {
        let p = self.pal();
        let (msg, color) = match &self.form_state {
            FormState::Idle => (None, p.muted),
            FormState::Saving => (Some("Saving…".into()), p.muted),
            FormState::Done(m) => (Some(m.clone()), p.accent),
            FormState::Error(m) => (Some(m.clone()), p.star),
        };
        div()
            .mt_2()
            .pt(px(18.))
            .border_t_1()
            .border_color(rgb(p.line))
            .flex()
            .items_center()
            .gap_3()
            .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(rgb(color)).children(msg))
            .children(extra)
            .child(primary)
    }

    pub(crate) fn big_button(&self, id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool, busy: bool) -> Stateful<Div> {
        let p = self.pal();
        div()
            .id(id)
            .h(px(40.))
            .px(px(20.))
            .flex()
            .items_center()
            .rounded(px(10.))
            .text_size(px(14.))
            .font_weight(FontWeight::SEMIBOLD)
            .map(|d| if primary { d.bg(rgb(p.accent)).text_color(rgb(p.on_accent)) } else { d.bg(rgb(p.surface)).text_color(rgb(p.ink)) })
            .when(busy, |d| d.opacity(0.5))
            .when(!busy, |d| d.cursor_pointer().hover(|d| d.opacity(0.88)))
            .child(label.into())
    }

    pub(crate) fn page_head(&self, title: &'static str, sub: Option<String>, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(self.back_link(cx))
            .child(heading(title, 28.))
            .children(sub.map(|s| div().text_size(px(14.)).text_color(rgb(p.muted)).child(s)))
    }

    pub(crate) fn series_form_page(&self, f: &SeriesForm, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let adding = f.sid.is_none();
        let picked = f.genres.iter().filter(|g| g.on).count();
        let fanfic = f.genres.iter().any(|g| g.id == FANFIC && g.on);
        let toggle = |which: fn(&mut SeriesForm) -> &mut Vec<Choice>, i: usize, radio: bool| {
            cx.listener(move |this: &mut Shell, _: &ClickEvent, _, cx| {
                if let Some(Page::SeriesForm(f)) = this.stack.last_mut() {
                    let list = which(f);
                    if radio {
                        list.iter_mut().enumerate().for_each(|(j, c)| c.on = j == i);
                    } else {
                        list[i].on = !list[i].on;
                    }
                    if !f.genres.iter().any(|g| g.on && g.id == f.main_genre) {
                        f.main_genre = f.genres.iter().find(|g| g.on).map(|g| g.id.clone()).unwrap_or_default();
                    }
                }
                this.form_state = FormState::Idle;
                cx.notify();
            })
        };
        let genres = div().flex().flex_wrap().gap_2().children(f.genres.iter().enumerate().map(|(i, g)| self.chip(("g", i), g.name.clone(), g.on).on_click(toggle(|f| &mut f.genres, i, false))));
        let mains = div().flex().flex_wrap().gap_2().children(f.genres.iter().enumerate().filter(|(_, g)| g.on).map(|(i, g)| {
            let id = g.id.clone();
            self.chip(("mg", i), g.name.clone(), g.id == f.main_genre).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(Page::SeriesForm(f)) = this.stack.last_mut() {
                    f.main_genre = id.clone();
                }
                cx.notify();
            }))
        }));
        let q = self.f_tag.read(cx).text().trim().to_lowercase();
        let name_of = |id: &str| f.tags.iter().find(|(t, _)| t == id).map(|(_, n)| n.clone()).unwrap_or(id.to_string());
        let picked_tags = div().flex().flex_wrap().gap_2().children(f.picked_tags.iter().enumerate().map(|(i, id)| {
            let id = id.clone();
            self.chip(("pt", i), format!("{}  ×", name_of(&id)), true).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(Page::SeriesForm(f)) = this.stack.last_mut() {
                    f.picked_tags.retain(|t| *t != id);
                }
                cx.notify();
            }))
        }));
        let matches: Vec<_> = if q.is_empty() {
            vec![]
        } else {
            f.tags.iter().filter(|(id, n)| !f.picked_tags.contains(id) && n.to_lowercase().contains(&q)).take(24).cloned().collect()
        };
        let full = f.picked_tags.len() >= 25;
        let tag_hits = div().flex().flex_wrap().gap_2().children(matches.into_iter().enumerate().map(|(i, (id, n))| {
            self.chip(("tm", i), format!("+ {n}"), false).when(full, |d| d.opacity(0.4)).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(Page::SeriesForm(f)) = this.stack.last_mut() {
                    if f.picked_tags.len() < 25 && !f.picked_tags.contains(&id) {
                        f.picked_tags.push(id.clone());
                    }
                }
                this.f_tag.update(cx, |i, cx| i.clear(cx));
                cx.notify();
            }))
        }));
        let copyright = div().flex().flex_wrap().gap_2().children(f.copyright.iter().enumerate().map(|(i, c)| self.chip(("cr", i), c.name.clone(), c.on).on_click(toggle(|f| &mut f.copyright, i, true))));
        let warnings = div().flex().flex_wrap().gap_2().children(f.warnings.iter().enumerate().map(|(i, c)| self.chip(("wn", i), c.name.clone(), c.on).on_click(toggle(|f| &mut f.warnings, i, false))));
        let status = div().flex().flex_wrap().gap_2().children(f.status.iter().enumerate().map(|(i, c)| self.chip(("st", i), c.name.clone(), c.on).on_click(toggle(|f| &mut f.status, i, true))));
        let mature = f.mature;
        let busy = self.form_state == FormState::Saving;
        let label = if adding { "Create series" } else { "Save changes" };
        let mut saves = ["series-save", "series-save-top"].map(|id| Some(self.big_button(id, label, true, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.save_series(cx) }))));
        let title = self.f_title.read(cx).text().to_string();
        let desc = self.f_desc.read(cx).text().to_string();
        let desc_len = desc.chars().count();

        let basics = self
            .card("The basics", None)
            .child(div().flex().flex_col().gap_2().child(self.section("Title", None)).child(self.field("f-title", &self.f_title, 0., cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(self.section("Description", Some(format!("{} / 4,000", group_digits(&desc_len.to_string())))))
                    .child(self.field("f-desc", &self.f_desc, 180., cx)),
            );
        let genre_card = self
            .card("Genres", Some(format!("{picked} of 9")))
            .child(genres)
            .when(picked > 0, |d| d.child(div().flex().flex_col().gap(px(10.)).child(self.section("Main genre", None)).child(mains)))
            .when(fanfic, |d| d.child(div().flex().flex_col().gap_2().child(self.section("Fandom", Some("one per line, up to five".into()))).child(self.field("f-fandom", &self.f_fandom, 72., cx))));
        let tag_card = self
            .card("Tags", Some(format!("{} of 25", f.picked_tags.len())))
            .when(!f.picked_tags.is_empty(), |d| d.child(picked_tags))
            .child(self.field("f-tag", &self.f_tag, 0., cx))
            .child(tag_hits);
        let details = self
            .card("Details", None)
            .child(div().flex().flex_col().gap(px(10.)).child(self.section("Copyright", None)).child(copyright))
            .child(div().h(px(1.)).bg(rgb(p.line)))
            .child(self.switch_row("mature", "Mature content", None, mature).on_click(cx.listener(|this, _, _, cx| {
                if let Some(Page::SeriesForm(f)) = this.stack.last_mut() {
                    f.mature = !f.mature;
                }
                cx.notify();
            })))
            .when(mature, |d| d.child(warnings))
            .when(!f.status.is_empty(), |d| d.child(div().h(px(1.)).bg(rgb(p.line))).child(div().flex().flex_col().gap(px(10.)).child(self.section("Story status", None)).child(status)));
        let form = div().flex_1().min_w(px(420.)).max_w(px(720.)).flex().flex_col().gap(px(18.)).child(basics).child(genre_card).child(tag_card).child(details).child(self.form_footer(saves[0].take().unwrap(), None));

        let existing = f.sid.as_ref().and_then(|sid| {
            self.stack.iter().find_map(|pg| match pg {
                Page::Dashboard(ws) => ws.iter().find(|w| &w.id == sid).map(|w| w.cover.clone()),
                _ => None,
            })
        });
        let (cw, ch) = (148., 212.);
        let art = match (&self.cover_path, existing) {
            (Some(path), _) => div().w(px(cw)).h(px(ch)).rounded(px(10.)).overflow_hidden().shadow_md().child(img(path.clone()).size_full().object_fit(ObjectFit::Cover)),
            (None, Some(url)) if !url.is_empty() => cover(&url, cw, ch, 10., p),
            _ => div()
                .w(px(cw))
                .h(px(ch))
                .rounded(px(10.))
                .border_1()
                .border_dashed()
                .border_color(rgb(p.muted))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(rgb(p.muted))
                .child(svg().path("icons/plus.svg").size(px(22.)).text_color(rgb(p.muted)))
                .child(div().text_size(px(13.)).child(if adding { "Add a cover" } else { "No cover" })),
        };
        let art = div()
            .id("cover-pick")
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(art)
            .when(adding, |d| {
                d.cursor_pointer()
                    .hover(|d| d.opacity(0.85))
                    .child(div().text_size(px(12.)).text_color(rgb(p.accent)).child(if self.cover_path.is_some() { "Change cover" } else { "Choose image" }))
                    .on_click(cx.listener(|this, _, _, cx| this.pick_cover(cx)))
            });
        let main_name = f.genres.iter().find(|g| g.id == f.main_genre).map(|g| g.name.clone());
        let others: Vec<_> = f.genres.iter().filter(|g| g.on && g.id != f.main_genre).map(|g| g.name.clone()).collect();
        let author = self.account.as_ref().map(|a| a.name.clone()).unwrap_or_default();
        let blank = |s: &str, empty: &'static str| if s.trim().is_empty() { (SharedString::from(empty), p.muted) } else { (SharedString::from(s.trim().to_string()), p.ink) };
        let (t, tc) = blank(&title, "Untitled series");
        let (d, dc) = blank(&desc, "Your description shows here.");
        let preview = div()
            .rounded(px(14.))
            .bg(rgb(p.surface))
            .border_1()
            .border_color(rgb(p.line))
            .overflow_hidden()
            .child(div().py(px(26.)).rounded_t(px(13.)).bg(rgb(p.sel)).flex().justify_center().child(art))
            .child(
                div()
                    .p(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(heading(t, 21.).text_color(rgb(tc)))
                    .when(!author.is_empty(), |el| el.child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("by {author}"))))
                    .when(main_name.is_some() || !others.is_empty(), |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(6.))
                                .children(main_name.map(|n| div().px(px(8.)).py(px(2.)).rounded_full().bg(rgb(p.accent)).text_color(rgb(p.on_accent)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).child(n)))
                                .children(others.into_iter().map(|n| div().px(px(8.)).py(px(2.)).rounded_full().border_1().border_color(rgb(p.line)).text_size(px(11.)).text_color(rgb(p.muted)).child(n))),
                        )
                    })
                    .child(div().text_size(px(13.)).line_height(px(20.)).text_color(rgb(dc)).line_clamp(5).child(d))
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .text_size(px(12.))
                            .text_color(rgb(p.muted))
                            .child(format!("{} tags", f.picked_tags.len()))
                            .when(mature, |el| el.child("· Mature"))
                            .children(f.status.iter().find(|s| s.on).map(|s| format!("· {}", s.name))),
                    ),
            )
            .child(div().px(px(20.)).pb(px(20.)).flex().flex_col().child(saves[1].take().unwrap().justify_center()));
        let aside = div()
            .flex_none()
            .w(px(300.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(preview)
            .when(adding, |d| d.child(div().text_size(px(12.)).line_height(px(18.)).text_color(rgb(p.muted)).child("Covers: JPG, PNG or GIF under 2 MB. Scribble Hub reviews them before they go live.")));

        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .px(px(44.))
            .pt(px(TITLE_INSET))
            .pb(px(56.))
            .child(self.page_head(if adding { "New series" } else { "Edit details" }, (!adding).then(|| f.title.clone()), cx))
            .child(div().flex().flex_wrap().items_start().gap(px(28.)).child(form).child(aside))
    }

    fn pick_cover(&mut self, cx: &mut Context<Self>) {
        self.pick_image(false, cx);
    }

    pub(crate) fn pick_image(&mut self, avatar: bool, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(if avatar { "Choose avatar".into() } else { "Choose cover".into() }) });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut picked))) = paths.await {
                if let Some(path) = picked.pop() {
                    let dims = image::image_dimensions(&path);
                    this.update(cx, |this, cx| match dims {
                        Ok((w, h)) => {
                            this.crop = Some(Crop::new(path, w as f32, h as f32, avatar));
                            cx.notify();
                        }
                        Err(_) => this.fail("Couldn't open that image.", cx),
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    pub(crate) fn crop_dialog(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let c = self.crop.as_ref()?;
        let p = self.pal();
        let s = (CROP_W / c.iw).min(CROP_H / c.ih);
        let (dw, dh) = (c.iw * s, c.ih * s);
        let (x, y, w, h) = (c.x * s, c.y * s, c.w * s, c.h() * s);
        let shade = |l: f32, t: f32, wd: f32, ht: f32| div().absolute().left(px(l)).top(px(t)).w(px(wd.max(0.))).h(px(ht.max(0.))).bg(rgba(0x000000a8));
        let start = |resize: bool| {
            cx.listener(move |this: &mut Shell, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Shell>| {
                if let Some(c) = this.crop.as_mut() {
                    c.drag = Some(Drag { at: e.position, x: c.x, y: c.y, w: c.w, resize });
                }
                window.prevent_default();
                cx.stop_propagation();
            })
        };
        let frame = div()
            .relative()
            .w(px(dw))
            .h(px(dh))
            .overflow_hidden()
            .child(img(c.src.clone()).size_full())
            .child(shade(0., 0., dw, y))
            .child(shade(0., y + h, dw, dh - y - h))
            .child(shade(0., y, x, h))
            .child(shade(x + w, y, dw - x - w, h))
            .child(
                div()
                    .id("crop-box")
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(w))
                    .h(px(h))
                    .border_2()
                    .border_color(rgb(0xffffff))
                    .cursor(CursorStyle::OpenHand)
                    .on_mouse_down(MouseButton::Left, start(false))
                    .child(
                        div()
                            .id("crop-corner")
                            .absolute()
                            .right(px(-8.))
                            .bottom(px(-8.))
                            .size(px(16.))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .border_2()
                            .border_color(rgb(p.accent))
                            .cursor(CursorStyle::ResizeUpLeftDownRight)
                            .on_mouse_down(MouseButton::Left, start(true)),
                    ),
            );
        let cancel = self.big_button("crop-cancel", "Cancel", false, false).on_click(cx.listener(|this, _, _, cx| {
            this.crop = None;
            cx.notify();
        }));
        let done = self.big_button("crop-use", if c.avatar { "Use avatar" } else { "Use cover" }, true, false).on_click(cx.listener(|this, _, _, cx| this.apply_crop(cx)));
        Some(
            div()
                .id("crop-dialog")
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x0000008c))
                .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                    if let Some(c) = this.crop.as_mut() {
                        if let Some(d) = c.drag {
                            let s = (CROP_W / c.iw).min(CROP_H / c.ih);
                            c.drag_to(&d, f32::from(e.position.x - d.at.x) / s, f32::from(e.position.y - d.at.y) / s);
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| {
                    if let Some(c) = this.crop.as_mut() {
                        c.drag = None;
                    }
                }))
                .child(
                    div()
                        .p(px(24.))
                        .rounded(px(16.))
                        .bg(rgb(p.surface))
                        .border_1()
                        .border_color(rgb(p.line))
                        .shadow_lg()
                        .flex()
                        .flex_col()
                        .gap(px(18.))
                        .child(div().text_size(px(18.)).font_weight(FontWeight::SEMIBOLD).child(if c.avatar { "Crop avatar" } else { "Crop cover" }))
                        .child(div().w(px(CROP_W)).h(px(CROP_H)).flex().items_center().justify_center().child(frame))
                        .child(div().flex().justify_end().gap_2().child(cancel).child(done)),
                ),
        )
    }

    fn apply_crop(&mut self, cx: &mut Context<Self>) {
        let Some(c) = self.crop.take() else { return };
        let out = std::env::temp_dir().join(format!("sh-cover-{}.jpg", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));
        let made = image::open(&c.src).map_err(|e| e.to_string()).and_then(|im| {
            let mut im = im.crop_imm(c.x.round() as u32, c.y.round() as u32, c.w.round().max(1.) as u32, c.h().round().max(1.) as u32);
            let (tw, th) = if c.avatar { (400, 400) } else { (500, 700) };
            if im.width() > tw {
                im = im.resize_exact(tw, th, image::imageops::FilterType::Lanczos3);
            }
            image::DynamicImage::ImageRgb8(im.to_rgb8()).save(&out).map_err(|e| e.to_string())
        });
        match made {
            Ok(()) if c.avatar => self.upload_avatar(out, cx),
            Ok(()) => {
                self.cover_path = Some(out);
                cx.notify();
            }
            Err(_) => self.fail("Couldn't crop that image.", cx),
        }
    }

    pub(crate) fn chapter_form_page(&self, editing: bool, area: f32, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let content = area - 88.;
        let side_by_side = content >= 460. + 28. + 300.;
        let form_w = if side_by_side { (content - 328.).min(780.) } else { content.min(780.) }.max(320.);
        let text_w = form_w - 2. - 80.;
        let (hl, analysis) = self.editor_highlights(cx);
        self.c_body.update(cx, |e, _| {
            e.pal = p;
            e.width = text_w;
            e.hl = hl;
        });
        let (words, chars, paras) = {
            let d = self.c_body.read(cx).doc();
            let paras = d.paras.iter().zip(d.lines()).filter(|(p, r)| !p.kind.atom() && !d.text[r.clone()].trim().is_empty()).count();
            (d.word_count(), d.text.chars().filter(|c| !c.is_whitespace() && *c != rich::OBJ).count(), paras)
        };
        let lossy = !self.c_lossy.is_empty();
        let busy = self.form_state == FormState::Saving || lossy;
        let armed = self.publish_armed;
        let live = editing && self.c_status == "published";
        let publish = self
            .big_button("ch-publish", if live { "Save changes" } else if armed { "Publish now?" } else { "Publish" }, true, busy)
            .on_click(cx.listener(move |this, _, _, cx| {
                if busy {
                } else if live || this.publish_armed {
                    this.save_chapter(true, cx);
                } else {
                    this.publish_armed = true;
                    cx.notify();
                }
            }));
        let draft = self.big_button("ch-draft", "Save draft", false, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.save_chapter(false, cx) }));

        let focus = self.c_body.read(cx).focus.clone();
        let page = div()
            .id("c-body")
            .min_h(px(560.))
            .px(px(40.))
            .py(px(32.))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, cx.listener(move |_, _, window, _| {
                window.focus(&focus);
                window.prevent_default();
            }))
            .child(self.c_body.clone());
        let editor = div().rounded(px(12.)).border_1().border_color(rgb(p.line)).bg(rgb(p.bg)).child(self.chapter_toolbar(cx)).child(page);
        let warn = lossy.then(|| {
            let list = match self.c_lossy.as_slice() {
                [one] => one.to_string(),
                [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
                [] => String::new(),
            };
            let url = match self.stack.last() {
                Some(Page::ChapterForm(u)) => u.clone(),
                _ => String::new(),
            };
            div()
                .p(px(16.))
                .rounded(px(12.))
                .border_1()
                .border_color(rgb(p.star))
                .bg(rgb(p.surface))
                .flex()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("Saving here is off for this chapter"))
                        .child(div().text_size(px(13.)).line_height(px(19.)).text_color(rgb(p.muted)).child(format!("It has {list}, which the app's editor can't keep. Saving from here would change them on the live chapter."))),
                )
                .child(self.big_button("ch-onsite", "Edit on Scribble Hub", false, false).flex_none().on_click(cx.listener(move |this, _, _, cx| this.open_live(url.clone(), cx))))
        });
        let form = div()
            .flex_none()
            .w(px(form_w))
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(warn)
            .children(self.editor_banners(text_w, cx))
            .child(self.field("c-title", &self.c_title, 0., cx).font_family(SERIF))
            .child(editor)
            .child(self.form_footer(publish, (!live).then_some(draft)));

        let mins = (words as f32 / 230.).ceil() as usize;
        let stat = |v: String, l: &'static str| {
            div()
                .flex_1()
                .min_w(px(110.))
                .p(px(12.))
                .rounded(px(10.))
                .bg(rgb(p.bg))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(18.)).font_weight(FontWeight::BOLD).child(v))
                .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(l))
        };
        let numbers = self
            .card("This chapter", None)
            .child(div().flex().gap_2().child(stat(group_digits(&words.to_string()), "Words")).child(stat(if words == 0 { "0 min".into() } else { format!("{mins} min") }, "Reading time")))
            .child(div().flex().gap_2().child(stat(group_digits(&chars.to_string()), "Characters")).child(stat(paras.to_string(), "Paragraphs")));
        let aside = div().flex_none().w(px(300.)).flex().flex_col().gap(px(14.)).child(numbers).children(self.editor_aside(analysis, cx));

        div()
            .flex()
            .flex_col()
            .gap(px(20.))
            .px(px(44.))
            .pt(px(TITLE_INSET))
            .pb(px(56.))
            .child(self.page_head(if editing { "Edit chapter" } else { "New chapter" }, Some(self.form_series.clone()), cx))
            .child(div().flex().flex_wrap().items_start().gap(px(28.)).child(form).child(aside))
    }

    fn ed_act(&self, f: impl Fn(&mut rich::RichEditor, &mut Context<rich::RichEditor>) + 'static, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let focus = self.c_body.read(cx).focus.clone();
        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            window.prevent_default();
            this.c_menu = None;
            this.c_body.update(cx, |e, cx| f(e, cx));
            window.focus(&focus);
            cx.notify();
        })
    }

    fn menu_toggle(&self, name: &'static str, was_open: bool, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            window.prevent_default();
            this.c_menu = (!was_open).then_some(name);
            if !was_open && matches!(name, "link" | "image") {
                let current = if name == "link" { this.c_body.read(cx).link_at().map(|l| l.0.to_string()) } else { None };
                this.c_url.update(cx, |i, cx| i.set_text(current.unwrap_or_default(), cx));
                window.focus(&this.c_url.read(cx).focus.clone());
            }
            cx.notify();
        })
    }

    pub(crate) fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.c_menu = None;
        window.focus(&self.c_body.read(cx).focus.clone());
        cx.notify();
    }

    pub(crate) fn apply_url(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        let url = url.trim();
        if !url.is_empty() {
            let url: SharedString = if url.contains("://") || url.starts_with("mailto:") || url.starts_with('/') || url.starts_with('#') { url.to_string() } else { format!("https://{url}") }.into();
            match self.c_menu {
                Some("link") => self.c_body.update(cx, |e, cx| e.set_link(Some(url), cx)),
                Some("image") => self.c_body.update(cx, |e, cx| e.insert_block(rich::Kind::Image { src: url, html: None }, cx)),
                _ => {}
            }
        }
        self.c_url.update(cx, |i, cx| i.clear(cx));
        self.close_menu(window, cx);
    }

    pub(crate) fn reveal(&mut self, top: Pixels, bottom: Pixels, cx: &mut Context<Self>) {
        let b = self.scroll.bounds();
        let pad = px(32.);
        let mut off = self.scroll.offset();
        if bottom > b.bottom() - pad {
            off.y -= bottom - (b.bottom() - pad);
        } else if top < b.top() + pad {
            off.y += b.top() + pad - top;
        } else {
            return;
        }
        off.y = off.y.min(px(0.));
        self.scroll.set_offset(off);
        cx.notify();
    }

    fn chapter_toolbar(&self, cx: &mut Context<Self>) -> Div {
        use rich::{Align, Flag, Kind};
        let p = self.pal();
        let (mark, para, (can_undo, can_redo), link) = {
            let e = self.c_body.read(cx);
            let (m, para) = e.current();
            (m, para, e.can_undo(), e.link_at().map(|l| l.0))
        };
        let open = self.c_menu;
        let btn = move |id: &'static str, tip: &'static str, on: bool, enabled: bool, label: AnyElement| {
            div()
                .id(id)
                .h(px(32.))
                .min_w(px(32.))
                .px(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(3.))
                .rounded(px(7.))
                .text_size(px(15.))
                .text_color(rgb(if on { p.accent } else { p.ink }))
                .when(on, |d| d.bg(rgb(p.sel)))
                .when(enabled, |d| d.cursor_pointer().hover(move |d| d.bg(rgb(p.sel))))
                .when(!enabled, |d| d.opacity(0.4))
                .tooltip(move |_, cx| cx.new(|_| Tip(tip)).into())
                .child(label)
        };
        let icon = move |name: &'static str, on: bool| svg().path(SharedString::from(format!("icons/{name}.svg"))).size(px(17.)).flex_none().text_color(rgb(if on { p.accent } else { p.ink })).into_any_element();
        let glyph = |s: &'static str| div().child(s);
        let caret = move || div().text_size(px(10.)).text_color(rgb(p.muted)).child("\u{25BE}");
        let sep = || div().w(px(1.)).h(px(18.)).mx(px(4.)).bg(rgb(p.line));
        let panel = |id: &'static str, w: f32, cx: &mut Context<Self>| {
            div()
                .id(id)
                .occlude()
                .absolute()
                .top(px(38.))
                .left_0()
                .w(px(w))
                .p(px(6.))
                .bg(rgb(p.surface))
                .rounded(px(10.))
                .border_1()
                .border_color(rgb(p.line))
                .shadow_lg()
                .flex()
                .flex_col()
                .gap(px(2.))
                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                    this.c_menu = None;
                    cx.notify();
                }))
        };
        let item = move |id: SharedString, label: AnyElement, on: bool| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(6.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(rgb(if on { p.accent } else { p.ink }))
                .when(on, |d| d.bg(rgb(p.sel)))
                .hover(move |d| d.bg(rgb(p.sel)))
                .child(label)
        };
        let menu = |button: Stateful<Div>, body: Option<Stateful<Div>>| div().relative().child(button).children(body.map(|b| deferred(b).with_priority(1)));

        let block_name = match para.kind {
            Kind::H(1) => "Heading 1",
            Kind::H(2) => "Heading 2",
            Kind::H(_) => "Heading 3",
            Kind::Quote => "Quote",
            Kind::Bullet => "Bulleted list",
            Kind::Number => "Numbered list",
            _ => "Paragraph",
        };
        let block_body = (open == Some("block")).then(|| {
            let mut b = panel("m-block", 180., cx);
            for (i, (label, kind, size)) in [("Paragraph", Kind::P, 14.), ("Heading 1", Kind::H(1), 20.), ("Heading 2", Kind::H(2), 17.), ("Heading 3", Kind::H(3), 15.)].into_iter().enumerate() {
                let on = para.kind == kind || (i == 3 && matches!(para.kind, Kind::H(n) if n >= 3));
                let k = kind.clone();
                let el = div().font_family(SERIF).text_size(px(size)).when(i > 0, |d| d.font_weight(FontWeight::BOLD)).child(label).into_any_element();
                b = b.child(item(format!("b{i}").into(), el, on).on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| if k == Kind::P { e.toggle_kind(Kind::P, cx) } else { e.toggle_kind(k.clone(), cx) }, cx)));
            }
            b
        });
        let block = menu(
            btn("t-block", "Text style", false, true, div().flex().items_center().gap(px(6.)).w(px(94.)).justify_between().text_size(px(13.)).child(block_name).child(caret()).into_any_element())
                .on_mouse_down(MouseButton::Left, self.menu_toggle("block", open == Some("block"), cx)),
            block_body,
        );

        let flag = |id: &'static str, tip: &'static str, f: Flag, label: Div, cx: &mut Context<Self>| btn(id, tip, mark.get(f), true, label.into_any_element()).on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| e.toggle(f, cx), cx));
        let bold = flag("t-b", "Bold  \u{2318}B", Flag::Bold, glyph("B").font_weight(FontWeight::BOLD), cx);
        let italic = flag("t-i", "Italic  \u{2318}I", Flag::Italic, glyph("I").italic().font_family(SERIF), cx);
        let under = flag("t-u", "Underline  \u{2318}U", Flag::Underline, glyph("U").underline(), cx);
        let strike = flag("t-s", "Strikethrough", Flag::Strike, glyph("S").line_through(), cx);
        let sup = flag("t-sup", "Superscript", Flag::Sup, div().flex().items_start().child("x").child(div().text_size(px(9.)).child("2")), cx);

        const SWATCHES: [u32; 20] = [
            0xbfedd2, 0xfbeeb8, 0xf8cac6, 0xeccafa, 0xc2e0f4, 0x2dc26b, 0xf1c40f, 0xe03e2d, 0xb96ad9, 0x3598db, 0x169179, 0xe67e23, 0xba372a, 0x843fa1, 0x236fa1, 0xecf0f1, 0xced4d9, 0x95a5a6, 0x7e8c8d, 0x34495e,
        ];
        let swatches = |name: &'static str, current: Option<u32>, cx: &mut Context<Self>| {
            let bg = name == "bg";
            let mut grid = div().flex().flex_wrap().gap(px(6.)).p(px(4.));
            for (i, c) in SWATCHES.into_iter().enumerate() {
                let on = current == Some(c);
                grid = grid.child(
                    div()
                        .id((name, i))
                        .size(px(22.))
                        .rounded(px(5.))
                        .bg(rgb(c))
                        .cursor_pointer()
                        .border_2()
                        .border_color(rgb(if on { p.accent } else { p.surface }))
                        .hover(move |d| d.border_color(rgb(p.muted)))
                        .on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| if bg { e.set_bg(Some(c), cx) } else { e.set_color(Some(c), cx) }, cx)),
                );
            }
            panel(if bg { "m-bg" } else { "m-color" }, 166., cx)
                .child(grid)
                .child(item(format!("{name}-none").into(), div().child(if bg { "No highlight" } else { "Default colour" }).into_any_element(), current.is_none()).on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| if bg { e.set_bg(None, cx) } else { e.set_color(None, cx) }, cx)))
        };
        let color_body = (open == Some("color")).then(|| swatches("color", mark.color, cx));
        let color = menu(
            btn("t-color", "Text colour", false, true, div().flex().flex_col().items_center().child(div().font_weight(FontWeight::BOLD).line_height(px(16.)).child("A")).child(div().w(px(16.)).h(px(3.)).rounded(px(1.)).bg(rgb(mark.color.unwrap_or(p.ink)))).into_any_element())
                .on_mouse_down(MouseButton::Left, self.menu_toggle("color", open == Some("color"), cx)),
            color_body,
        );
        let bg_body = (open == Some("bg")).then(|| swatches("bg", mark.bg, cx));
        let highlight = menu(
            btn("t-bg", "Highlight", false, true, div().px(px(3.)).rounded(px(3.)).bg(rgb(mark.bg.unwrap_or(0xf1c40f))).text_color(rgb(0x1d1d1f)).font_weight(FontWeight::BOLD).line_height(px(18.)).child("H").into_any_element())
                .on_mouse_down(MouseButton::Left, self.menu_toggle("bg", open == Some("bg"), cx)),
            bg_body,
        );
        let sizes: [(&str, Option<&'static str>); 5] = [("Small", Some("10pt")), ("Normal", None), ("Large", Some("14pt")), ("Larger", Some("18pt")), ("Huge", Some("24pt"))];
        let size_body = (open == Some("size")).then(|| {
            let mut b = panel("m-size", 150., cx);
            for (i, (label, z)) in sizes.into_iter().enumerate() {
                let on = mark.size.as_ref().map(|s| s.as_ref()) == z;
                let shown = [12., 14., 16., 18., 21.][i];
                b = b.child(item(format!("z{i}").into(), div().text_size(px(shown)).child(label).into_any_element(), on).on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| e.set_size(z, cx), cx)));
            }
            b
        });
        let size = menu(
            btn("t-size", "Text size", mark.size.is_some(), true, div().flex().items_end().child(div().text_size(px(12.)).child("A")).child(div().text_size(px(17.)).child("A")).into_any_element()).on_mouse_down(MouseButton::Left, self.menu_toggle("size", open == Some("size"), cx)),
            size_body,
        );

        let aligns = [(Align::Left, "align-left", "Left"), (Align::Center, "align-center", "Centre"), (Align::Right, "align-right", "Right"), (Align::Justify, "align-justify", "Justify")];
        let current_icon = aligns.iter().find(|a| a.0 == para.align).map_or("align-left", |a| a.1);
        let align_body = (open == Some("align")).then(|| {
            let mut b = panel("m-align", 150., cx);
            for (i, (a, ic, label)) in aligns.into_iter().enumerate() {
                let on = para.align == a;
                b = b.child(item(format!("a{i}").into(), div().flex().items_center().gap_2().child(icon(ic, on)).child(label).into_any_element(), on).on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| e.set_align(a, cx), cx)));
            }
            b
        });
        let align = menu(
            btn("t-align", "Alignment", para.align != Align::Left, true, div().flex().items_center().gap(px(3.)).child(icon(current_icon, para.align != Align::Left)).child(caret()).into_any_element())
                .on_mouse_down(MouseButton::Left, self.menu_toggle("align", open == Some("align"), cx)),
            align_body,
        );

        let kind_btn = |id: &'static str, tip: &'static str, k: Kind, label: Div, cx: &mut Context<Self>| {
            let on = para.kind == k;
            btn(id, tip, on, true, label.into_any_element()).on_mouse_down(MouseButton::Left, self.ed_act(move |e, cx| e.toggle_kind(k.clone(), cx), cx))
        };
        let bullets = kind_btn("t-ul", "Bulleted list  (type \u{201C}- \u{201D})", Kind::Bullet, glyph("\u{2022}").text_size(px(22.)), cx);
        let numbers = kind_btn("t-ol", "Numbered list  (type \u{201C}1. \u{201D})", Kind::Number, glyph("1.").text_size(px(13.)).font_weight(FontWeight::SEMIBOLD), cx);
        let quote = kind_btn("t-q", "Quote  (type \u{201C}> \u{201D})", Kind::Quote, glyph("\u{201C}").font_family(SERIF).text_size(px(24.)), cx);
        let rule = btn("t-hr", "Scene break  (type *** and Enter)", false, true, glyph("\u{2042}").into_any_element()).on_mouse_down(MouseButton::Left, self.ed_act(|e, cx| e.insert_block(Kind::Rule, cx), cx));

        let url_panel = |name: &'static str, title: &'static str, cx: &mut Context<Self>| {
            let apply = self.big_button(("url-apply", 0usize), if name == "image" { "Insert" } else { "Apply" }, true, false).on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                window.prevent_default();
                let url = this.c_url.read(cx).text().to_string();
                this.apply_url(url, window, cx);
            }));
            let remove = (name == "link" && link.is_some()).then(|| {
                self.big_button(("url-remove", 0usize), "Remove", false, false).on_mouse_down(MouseButton::Left, self.ed_act(|e, cx| e.set_link(None, cx), cx))
            });
            panel(if name == "image" { "m-image" } else { "m-link" }, 340., cx)
                .p(px(12.))
                .gap(px(10.))
                .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.muted)).child(title))
                .child(self.field("c-url", &self.c_url, 0., cx))
                .child(div().flex().justify_end().gap_2().children(remove).child(apply))
        };
        let link_body = (open == Some("link")).then(|| url_panel("link", "Link address", cx));
        let link_btn = menu(btn("t-link", "Link", link.is_some(), true, icon("link", link.is_some())).on_mouse_down(MouseButton::Left, self.menu_toggle("link", open == Some("link"), cx)), link_body);
        let image_body = (open == Some("image")).then(|| url_panel("image", "Image address", cx));
        let image = menu(btn("t-img", "Image", false, true, icon("image", false)).on_mouse_down(MouseButton::Left, self.menu_toggle("image", open == Some("image"), cx)), image_body);

        let find = btn("t-find", "Find & replace  \u{2318}F", self.w.find.is_some(), true, icon("search", self.w.find.is_some())).on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
            window.prevent_default();
            this.toggle_find(window, cx);
        }));
        let focus_btn = btn("t-focus", "Focus mode  \u{21E7}\u{2318}F", false, true, icon("focus", false)).on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
            window.prevent_default();
            this.toggle_focus(window, cx);
        }));

        let undo = btn("t-undo", "Undo  \u{2318}Z", false, can_undo, icon("undo", false)).when(can_undo, |d| d.on_mouse_down(MouseButton::Left, self.ed_act(|e, cx| e.undo(cx), cx)));
        let redo = btn("t-redo", "Redo  \u{21E7}\u{2318}Z", false, can_redo, icon("redo", false)).when(can_redo, |d| d.on_mouse_down(MouseButton::Left, self.ed_act(|e, cx| e.redo(cx), cx)));

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(2.))
            .px(px(8.))
            .py(px(6.))
            .border_b_1()
            .border_color(rgb(p.line))
            .child(block)
            .child(sep())
            .child(bold)
            .child(italic)
            .child(under)
            .child(strike)
            .child(sup)
            .child(sep())
            .child(color)
            .child(highlight)
            .child(size)
            .child(sep())
            .child(align)
            .child(bullets)
            .child(numbers)
            .child(quote)
            .child(sep())
            .child(link_btn)
            .child(image)
            .child(rule)
            .child(div().flex_1())
            .child(find)
            .child(focus_btn)
            .child(undo)
            .child(redo)
    }

    pub(crate) fn chapters_page(&self, l: &ChapterList, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let sid = l.sid.clone();
        let title = l.title.clone();
        let add = self.big_button("chs-add", "Add chapter", true, false).on_click(cx.listener(move |this, _, _, cx| {
            let w = site::Work { id: sid.clone(), title: title.clone(), ..Default::default() };
            this.open_chapter_form(&w, cx)
        }));
        let n = l.rows.len();
        let rows = l.rows.iter().enumerate().map(|(i, r)| {
            let cid = r.id.clone();
            let del_id = r.id.clone();
            let armed = l.confirm_delete.as_deref() == Some(r.id.as_str());
            let drag = manage::ChapterDrag { ix: i, title: r.title.clone(), bg: p.surface, fg: p.ink };
            div()
                .id(("chs-row", i))
                .group("chrow")
                .flex()
                .items_center()
                .gap_3()
                .px(px(10.))
                .py(px(12.))
                .rounded(px(10.))
                .border_t_2()
                .border_color(rgba(0x00000000))
                .cursor_pointer()
                .hover(move |d| d.bg(rgb(p.surface)))
                .drag_over::<manage::ChapterDrag>(move |s, _, _, _| s.border_color(rgb(p.accent)))
                .on_drop(cx.listener(move |this, d: &manage::ChapterDrag, _, cx| this.move_chapter(d.ix, i, cx)))
                .child(
                    div()
                        .id(("chs-grip", i))
                        .flex_none()
                        .px(px(6.))
                        .py(px(4.))
                        .rounded(px(6.))
                        .text_size(px(16.))
                        .text_color(rgb(p.muted))
                        .cursor(CursorStyle::OpenHand)
                        .hover(move |d| d.bg(rgb(p.sel)).text_color(rgb(p.ink)))
                        .tooltip(|_, cx| cx.new(|_| Tip("Drag to reorder")).into())
                        .child("\u{2261}")
                        .on_drag(drag, |d, _, _, cx| cx.new(|_| d.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().truncate().font_family(SERIF).text_size(px(16.)).child(r.title.clone()))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(12.))
                                .text_color(rgb(p.muted))
                                .when(!r.status.is_empty(), |d| {
                                    let published = r.status == "Published";
                                    d.child(div().px(px(7.)).py(px(1.)).rounded_full().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).map(|d| if published { d.bg(rgb(p.sel)).text_color(rgb(p.muted)) } else { d.bg(rgb(p.accent)).text_color(rgb(p.on_accent)) }).child(r.status.clone()))
                                })
                                .when(!r.date.is_empty(), |d| d.child(r.date.clone()))
                                .when(!r.stats.0.is_empty(), |d| d.child(format!("· {} views · {} hearts · {} comments", r.stats.0, r.stats.1, r.stats.2))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(13.))
                        .text_color(rgb(p.accent))
                        .opacity(0.)
                        .group_hover("chrow", |s| s.opacity(1.))
                        .child(svg().path("icons/pen.svg").size(px(14.)).text_color(rgb(p.accent)))
                        .child("Edit"),
                )
                .child(
                    div()
                        .id(("chs-del", i))
                        .flex_none()
                        .px(px(8.))
                        .py(px(4.))
                        .rounded(px(7.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(12.))
                        .map(|d| if armed { d.bg(rgb(p.star)).text_color(rgb(p.on_accent)).child("Delete chapter?") } else { d.opacity(0.).group_hover("chrow", |s| s.opacity(1.)).hover(move |d| d.bg(rgb(p.sel))).child(svg().path("icons/trash.svg").size(px(15.)).text_color(rgb(p.muted))) })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.delete_chapter(del_id.clone(), cx)
                        })),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.open_edit_chapter(&cid, cx)))
        });
        let body = if let Some(e) = &l.error {
            div().py_4().text_color(rgb(p.star)).child(e.clone())
        } else if l.rows.is_empty() && l.loading {
            div().py_4().text_color(rgb(p.muted)).child("Loading chapters…")
        } else if l.rows.is_empty() {
            div().py_4().text_color(rgb(p.muted)).child("No chapters yet.")
        } else {
            div().flex().flex_col().children(rows).when(l.loading, |d| d.child(div().px(px(14.)).py_3().text_size(px(13.)).text_color(rgb(p.muted)).child("Loading the rest…")))
        };
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(820.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(
                    div()
                        .flex()
                        .items_end()
                        .justify_between()
                        .gap_4()
                        .child(self.page_head("Chapters", Some(if n > 0 { format!("{} · {n} chapters", l.title) } else { l.title.clone() }), cx))
                        .child(add),
                )
                .child(div().h(px(1.)).bg(rgb(p.line)))
                .when(l.order_changed || matches!(self.form_state, FormState::Saving | FormState::Error(_)) || self.form_state == FormState::Done("Order saved".into()), |d| {
                    let busy = self.form_state == FormState::Saving;
                    d.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px(px(14.))
                            .py(px(10.))
                            .rounded(px(10.))
                            .bg(rgb(p.surface))
                            .child(div().flex_1().text_size(px(13.)).text_color(rgb(match &self.form_state {
                                FormState::Error(_) => p.star,
                                _ => p.muted,
                            })).child(match &self.form_state {
                                FormState::Error(e) => e.clone(),
                                FormState::Done(m) if !l.order_changed => m.clone(),
                                _ => "New order not saved yet".into(),
                            }))
                            .when(l.order_changed, |d| d.child(self.big_button("chs-sort", "Save order", true, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.save_chapter_order(cx) })))),
                    )
                })
                .child(body),
        )
    }

    pub(crate) fn stats_page(&self, st: &Stats, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let tiles = div().flex().flex_wrap().gap(px(10.)).children(st.totals.iter().filter(|(_, l)| l != "Chapters/Week").map(|(v, l)| {
            div()
                .flex_grow()
                .flex_basis(px(120.))
                .px(px(16.))
                .py(px(14.))
                .rounded(px(12.))
                .bg(rgb(p.surface))
                .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD).child(group_digits(v)))
                .child(div().text_size(px(12.)).text_color(rgb(p.muted)).child(l.clone()))
        }));
        let seg = |id: &'static str, items: &[(&'static str, &'static str)], current: &'static str, set: fn(&mut Stats, &'static str)| {
            div().flex().gap(px(2.)).p(px(3.)).rounded(px(10.)).bg(rgb(p.surface)).children(items.iter().map(|&(key, label)| {
                div()
                    .id((id, key.len() * 31 + label.len()))
                    .px(px(12.))
                    .py(px(5.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .when(key == current, |d| d.bg(rgb(p.sel)).font_weight(FontWeight::SEMIBOLD))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(Page::Stats(st)) = this.stack.last_mut() {
                            set(st, key);
                        }
                        this.fetch_chart(cx);
                    }))
            }))
        };
        let kinds = seg("kind", &[("views", "Views"), ("readers", "Readers"), ("comments", "Comments"), ("favorites", "Favorites")], st.kind, |s, k| s.kind = k);
        let ranges = seg("range", &[("days", "Days"), ("weeks", "Weeks"), ("months", "Months"), ("years", "Years")], st.range, |s, r| s.range = r);
        let max = st.chart.iter().map(|(_, v)| *v).fold(0., f64::max);
        let total: f64 = st.chart.iter().map(|(_, v)| *v).sum();
        let n = st.chart.len().max(1);
        let every = n.div_ceil(8);
        const H: f32 = 200.;
        let bars = div().h(px(H + 22.)).flex().items_end().gap(px(4.)).children(st.chart.iter().enumerate().map(|(i, (label, v))| {
            let h = if max > 0. { (*v / max) as f32 * H } else { 0. };
            div()
                .id(("bar", i))
                .group("bar")
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .child(div().text_size(px(10.)).text_color(rgb(p.muted)).opacity(0.).group_hover("bar", |s| s.opacity(1.)).child(group_digits(&format!("{v}"))))
                .child(div().w_full().h(px(h.max(2.))).rounded_t(px(4.)).bg(rgb(p.accent)).opacity(if *v > 0. { 0.9 } else { 0.25 }))
                .child(div().h(px(14.)).text_size(px(10.)).text_color(rgb(p.muted)).truncate().child(if i % every == 0 { label.clone() } else { String::new() }))
        }));
        let chart = div()
            .p(px(20.))
            .rounded(px(14.))
            .bg(rgb(p.surface))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap_2()
                    .child(div().text_size(px(22.)).font_weight(FontWeight::BOLD).child(group_digits(&format!("{total}"))))
                    .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(format!("{} in this range", st.kind))),
            )
            .child(if st.loading {
                div().h(px(H + 22.)).flex().items_center().justify_center().child(self.dots("sdots", 7.))
            } else if st.chart.is_empty() {
                div().h(px(H + 22.)).flex().items_center().justify_center().text_color(rgb(p.muted)).child("No numbers for this range yet.")
            } else {
                bars
            });
        let table = |title: &'static str, rows: &[(String, String)]| {
            div()
                .flex_1()
                .min_w(px(260.))
                .flex()
                .flex_col()
                .child(div().pb_2().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .when(rows.is_empty(), |d| d.child(div().py_2().text_size(px(13.)).text_color(rgb(p.muted)).child("None today.")))
                .children(rows.iter().map(|(r, v)| {
                    div()
                        .flex()
                        .gap_3()
                        .py(px(8.))
                        .border_t_1()
                        .border_color(rgb(p.line))
                        .text_size(px(13.))
                        .child(div().flex_1().min_w_0().truncate().child(r.clone()))
                        .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).child(v.clone()))
                }))
        };
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(900.))
                .flex()
                .flex_col()
                .gap(px(20.))
                .child(self.page_head("Stats", Some(st.title.clone()), cx))
                .child(tiles)
                .child(div().flex().flex_wrap().justify_between().gap_3().child(kinds).child(ranges))
                .child(chart)
                .child(div().flex().flex_col().gap_2().child(self.section("Referrers today", None)).child(div().flex().flex_wrap().gap(px(32.)).child(table("Scribble Hub", &st.refs.0)).child(table("Web", &st.refs.1)))),
        )
    }
}

pub(crate) const HELPERS: &str = "function post(ok,msg){window.ipc.postMessage(JSON.stringify({kind:'saved',ok:ok,msg:msg||''}));}\
function when(ready,go){var n=0;(function t(){if(ready())return go();if(++n>150)return post(false,'The Scribble Hub page didn\\'t finish loading.');setTimeout(t,100);})();}\
function watch_errors(){setTimeout(function(){var e=$('.edit_saveerror:visible, .error_msg_add:visible').first().text().trim();if(e)post(false,e.replace(/\\s+/g,' '));},600);}\
function watch_ajax(action){var h=function(ev,x,o){if(String(o.data||'').indexOf(action)<0)return;$(document).off('ajaxComplete',h);post(x.status==200,x.status==200?'':'Scribble Hub refused the save ('+x.status+').');};$(document).on('ajaxComplete',h);}";

const CROP_W: f32 = 520.;
const CROP_H: f32 = 460.;

pub struct Crop {
    src: std::path::PathBuf,
    ratio: f32,
    pub avatar: bool,
    iw: f32,
    ih: f32,
    x: f32,
    y: f32,
    w: f32,
    drag: Option<Drag>,
}

#[derive(Clone, Copy)]
struct Drag {
    at: Point<Pixels>,
    x: f32,
    y: f32,
    w: f32,
    resize: bool,
}

impl Crop {
    pub fn new(src: std::path::PathBuf, iw: f32, ih: f32, avatar: bool) -> Self {
        let ratio = if avatar { 1. } else { 5. / 7. };
        let w = iw.min(ih * ratio);
        let h = w / ratio;
        Self { src, ratio, avatar, iw, ih, x: (iw - w) / 2., y: (ih - h) / 2., w, drag: None }
    }

    fn h(&self) -> f32 {
        self.w / self.ratio
    }

    fn drag_to(&mut self, d: &Drag, dx: f32, dy: f32) {
        if d.resize {
            let most = (self.iw - d.x).min((self.ih - d.y) * self.ratio);
            let dy = dy * self.ratio;
            let grow = if dx.abs() > dy.abs() { dx } else { dy };
            self.w = (d.w + grow).clamp(most.min(40.), most);
        } else {
            self.x = (d.x + dx).clamp(0., self.iw - self.w);
            self.y = (d.y + dy).clamp(0., self.ih - self.h());
        }
    }
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

pub(crate) struct Tip(pub &'static str);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px_2().py_1().rounded(px(6.)).bg(rgb(0x2a2a30)).text_color(rgb(0xf5f5f7)).text_size(px(12.)).child(self.0)
    }
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { T[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{base64, Crop, Drag};

    #[test]
    fn crop_stays_inside() {
        let mut c = Crop::new("x".into(), 2000., 1400., false);
        assert_eq!((c.w, c.h(), c.x, c.y), (1000., 1400., 500., 0.));
        let d = Drag { at: Default::default(), x: c.x, y: c.y, w: c.w, resize: true };
        c.drag_to(&d, -500., 0.);
        assert_eq!(c.w, 500.);
        let d = Drag { resize: false, w: c.w, ..d };
        c.drag_to(&d, 9999., 9999.);
        assert_eq!((c.x, c.y), (1500., 700.));
        let d = Drag { resize: true, x: c.x, y: c.y, ..d };
        c.drag_to(&d, 9999., 0.);
        assert_eq!(c.w, 500.);
        c.drag_to(&d, -9999., 0.);
        assert_eq!(c.w, 40.);
    }

    #[test]
    fn base64_matches_known() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
