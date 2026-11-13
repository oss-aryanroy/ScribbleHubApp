use super::*;
use site::Choice;
use studio::FormState;

#[derive(Clone, Debug, PartialEq)]
pub struct GlossItem {
    pub cat: String,
    pub order: String,
    pub name: String,
}

pub struct Glossary {
    pub sid: String,
    pub title: String,
    pub cats: Vec<Choice>,
    pub items: Vec<GlossItem>,
    pub loading: bool,
    pub form: Option<Option<(String, String)>>,
    pub confirm_delete: Option<(String, String)>,
}

pub struct Related {
    pub sid: String,
    pub title: String,
    pub rows: Vec<(String, String, String)>,
    pub series: Vec<Choice>,
    pub relations: Vec<Choice>,
}

pub struct ManageInputs {
    pub name: Entity<TextInput>,
    pub desc: Entity<TextInput>,
    pub new_cat: Entity<TextInput>,
}

impl ManageInputs {
    pub fn new(cx: &mut Context<Shell>) -> Self {
        Self {
            name: cx.new(|cx| TextInput::new("Name", cx).max_chars(200)),
            desc: cx.new(|cx| TextInput::multiline("Who or what it is", cx).max_chars(6000)),
            new_cat: cx.new(|cx| TextInput::new("New category", cx).max_chars(99)),
        }
    }
}

#[derive(Clone)]
pub struct ChapterDrag {
    pub ix: usize,
    pub title: String,
    pub bg: u32,
    pub fg: u32,
}

impl Render for ChapterDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().px_4().py_2().rounded(px(10.)).bg(rgb(self.bg)).text_color(rgb(self.fg)).shadow_lg().font_family(SERIF).text_size(px(15.)).child(self.title.clone())
    }
}

fn ajax_body(text: &str) -> &str {
    let t = text.trim();
    t.strip_suffix('0').unwrap_or(t)
}

pub fn gloss_items(html: &str) -> Vec<GlossItem> {
    let frag = scraper::Html::parse_fragment(html);
    let li = scraper::Selector::parse("li[category][orderid]").unwrap();
    frag.root_element()
        .select(&li)
        .map(|e| {
            let a = |k: &str| e.value().attr(k).unwrap_or_default().to_string();
            let name = Some(a("item")).filter(|n| !n.is_empty()).unwrap_or_else(|| e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "));
            GlossItem { cat: a("category"), order: a("orderid"), name }
        })
        .collect()
}

pub fn options(html: &str) -> Vec<Choice> {
    let frag = scraper::Html::parse_fragment(html);
    let o = scraper::Selector::parse("option").unwrap();
    frag.root_element()
        .select(&o)
        .map(|e| Choice { id: e.value().attr("value").unwrap_or_default().to_string(), name: e.text().collect::<String>().trim().to_string(), on: e.value().attr("selected").is_some() })
        .collect()
}

pub fn related_rows(html: &str) -> Vec<(String, String, String)> {
    let frag = scraper::Html::parse_fragment(&format!("<table>{html}</table>"));
    let tr = scraper::Selector::parse(".rs_sid").unwrap();
    let td = scraper::Selector::parse("td").unwrap();
    frag.root_element()
        .select(&tr)
        .filter_map(|r| {
            let id = r.value().classes().find(|c| c.chars().all(|ch| ch.is_ascii_digit()))?.to_string();
            let cells: Vec<String> = r.select(&td).map(|c| c.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")).collect();
            Some((id, cells.first().cloned().unwrap_or_default(), cells.get(1).cloned().unwrap_or_default()))
        })
        .collect()
}

impl Shell {
    pub(crate) fn move_chapter(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if let Some(Page::Chapters(l)) = self.stack.last_mut() {
            if from < l.rows.len() && to < l.rows.len() && from != to {
                let r = l.rows.remove(from);
                l.rows.insert(to, r);
                l.order_changed = true;
                self.form_state = FormState::Idle;
            }
        }
        cx.notify();
    }

    pub(crate) fn save_chapter_order(&mut self, cx: &mut Context<Self>) {
        let Some(Page::Chapters(l)) = self.stack.last() else { return };
        let ids = l.rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>().join(",");
        self.web.ajax("m-sort", &site::form(&[("action", "wi_savechpsort"), ("mypostid", &l.sid), ("strInclude", &ids), ("toc_batch_sort", "1")]));
        self.form_state = FormState::Saving;
        cx.notify();
    }

    pub(crate) fn delete_chapter(&mut self, cid: String, cx: &mut Context<Self>) {
        let Some(Page::Chapters(l)) = self.stack.last_mut() else { return };
        if l.confirm_delete.as_deref() != Some(&cid) {
            l.confirm_delete = Some(cid);
            return cx.notify();
        }
        l.confirm_delete = None;
        l.rows.retain(|r| r.id != cid);
        self.web.ajax("m-chdel", &site::form(&[("action", "wi_deletechapter"), ("strCID", &cid)]));
        cx.notify();
    }

    pub(crate) fn open_glossary(&mut self, sid: &str, title: &str, cx: &mut Context<Self>) {
        self.work_menu = None;
        self.browsing = false;
        self.want = None;
        self.form_state = FormState::Idle;
        self.stack.push(Page::Glossary(Box::new(Glossary { sid: sid.into(), title: title.into(), cats: vec![], items: vec![], loading: true, form: None, confirm_delete: None })));
        self.status = Status::Ready;
        self.scroll.set_offset(point(px(0.), px(0.)));
        self.reload_glossary(sid);
        cx.notify();
    }

    fn reload_glossary(&self, sid: &str) {
        self.web.ajax("m-gcats", &site::form(&[("action", "wi_getsrtcategories"), ("intPostID", sid)]));
        self.web.ajax("m-glist", &site::form(&[("action", "wi_reloadglossary"), ("strPostID", sid)]));
    }

    fn glossary_mut(&mut self) -> Option<&mut Glossary> {
        match self.stack.last_mut() {
            Some(Page::Glossary(g)) => Some(g),
            _ => None,
        }
    }

    fn open_gloss_form(&mut self, key: Option<(String, String)>, cx: &mut Context<Self>) {
        let m = &self.manage_in;
        for i in [&m.name, &m.desc] {
            i.update(cx, |i, cx| i.clear(cx));
        }
        let sid = match self.glossary_mut() {
            Some(g) => {
                g.form = Some(key.clone());
                if let Some((cat, _)) = &key {
                    g.cats.iter_mut().for_each(|c| c.on = &c.id == cat);
                }
                g.sid.clone()
            }
            None => return,
        };
        if let Some((cat, order)) = key {
            self.web.ajax("m-gload", &site::form(&[("action", "wi_loadglossary"), ("strPostID", &sid), ("strCategoryID", &cat), ("strOrderID", &order)]));
        }
        self.form_state = FormState::Idle;
        cx.notify();
    }

    fn save_gloss(&mut self, cx: &mut Context<Self>) {
        let name = self.manage_in.name.read(cx).text().trim().to_string();
        let desc = markup::to_html(self.manage_in.desc.read(cx).text());
        let Some(Page::Glossary(g)) = self.stack.last() else { return };
        let cat = g.cats.iter().find(|c| c.on && c.id != "0").map(|c| c.id.clone());
        let (Some(cat), false, false) = (cat, name.is_empty(), desc.is_empty()) else {
            return self.fail("A glossary entry needs a name, a category and a description.", cx);
        };
        let body = match g.form.clone().flatten() {
            Some((pc, po)) => site::form(&[("action", "wi_saveglossaryitem"), ("glossary_name", &name), ("glossary_desc", &desc), ("glossary_category", &cat), ("strPostID", &g.sid), ("strPrevious_CategoryID", &pc), ("strPrevious_strOrderID", &po)]),
            None => site::form(&[("action", "wi_addglossary_two"), ("glossary_name", &name), ("glossary_desc", &desc), ("glossary_category", &cat), ("strPostID", &g.sid)]),
        };
        self.web.ajax("m-gsave", &body);
        self.form_state = FormState::Saving;
        cx.notify();
    }

    fn delete_gloss(&mut self, key: (String, String), cx: &mut Context<Self>) {
        let Some(g) = self.glossary_mut() else { return };
        if g.confirm_delete.as_ref() != Some(&key) {
            g.confirm_delete = Some(key);
            return cx.notify();
        }
        g.confirm_delete = None;
        g.items.retain(|i| (i.cat.clone(), i.order.clone()) != key);
        let sid = g.sid.clone();
        self.web.ajax("m-gdel", &site::form(&[("action", "wi_deleteglossary"), ("strPostID", &sid), ("strCategoryID", &key.0), ("strOrderID", &key.1)]));
        cx.notify();
    }

    fn add_category(&mut self, cx: &mut Context<Self>) {
        let name = self.manage_in.new_cat.read(cx).text().trim().to_string();
        let Some(Page::Glossary(g)) = self.stack.last() else { return };
        if name.is_empty() {
            return;
        }
        self.web.ajax("m-gcat", &site::form(&[("action", "wi_addglossary_category"), ("intPostID", &g.sid), ("strCategory", &name)]));
        cx.notify();
    }

    pub(crate) fn open_related(&mut self, sid: &str, cx: &mut Context<Self>) {
        self.work_menu = None;
        self.form_state = FormState::Idle;
        self.browsing = false;
        self.open(format!("{}editseries/{sid}/#related", site::BASE), false, cx);
    }

    pub(crate) fn related_from(&self, url: &str, html: &str) -> Option<Related> {
        let doc = scraper::Html::parse_document(html);
        let one = |q: &str| doc.select(&scraper::Selector::parse(q).unwrap()).next();
        let opts = |q: &str| one(q).map(|e| options(&e.inner_html())).unwrap_or_default();
        let mut relations = opts("#rsl_set");
        let mut series = opts("#sid_rs");
        for l in [&mut relations, &mut series] {
            if !l.iter().any(|c| c.on) {
                if let Some(c) = l.first_mut() {
                    c.on = true;
                }
            }
        }
        Some(Related {
            sid: url.split("/editseries/").nth(1)?.chars().take_while(char::is_ascii_digit).collect(),
            title: one(".edit_fic_title a").map(|e| e.text().collect::<String>().trim().to_string()).unwrap_or_default(),
            rows: one(".rs_table").map(|e| related_rows(&e.inner_html())).unwrap_or_default(),
            series,
            relations,
        })
    }

    fn add_related(&mut self, cx: &mut Context<Self>) {
        let Some(Page::Related(r)) = self.stack.last() else { return };
        let pick = |l: &[Choice]| l.iter().find(|c| c.on).map(|c| c.id.clone()).unwrap_or_default();
        let (other, rel) = (pick(&r.series), pick(&r.relations));
        if other.is_empty() {
            return self.fail("You need another series to relate this one to.", cx);
        }
        self.web.ajax("m-radd", &site::form(&[("action", "wi_setrelate_sid"), ("intPostID", &r.sid), ("intSetPostID", &other), ("intRelationship", &rel)]));
        self.form_state = FormState::Saving;
        cx.notify();
    }

    fn remove_related(&mut self, other: String, cx: &mut Context<Self>) {
        let Some(Page::Related(r)) = self.stack.last_mut() else { return };
        r.rows.retain(|(id, ..)| *id != other);
        let sid = r.sid.clone();
        self.web.ajax("m-rdel", &site::form(&[("action", "wi_delrelate_sid"), ("intPostID", &sid), ("intSetPostID", &other)]));
        cx.notify();
    }

    pub(crate) fn on_manage_ajax(&mut self, tag: &str, text: &str, cx: &mut Context<Self>) -> bool {
        if !tag.starts_with("m-") {
            return false;
        }
        let body = ajax_body(text);
        let json: Option<serde_json::Value> = serde_json::from_str(body).ok();
        let err = json.as_ref().and_then(|j| j["error"].as_str()).map(site::plain).filter(|e| !e.is_empty());
        let unreachable = text.is_empty();
        match tag {
            "m-sort" => {
                let ok = json.as_ref().and_then(|j| j["success"].as_bool()) == Some(true);
                self.form_state = if ok { FormState::Done("Order saved".into()) } else { FormState::Error(json.as_ref().and_then(|j| j["data"]["message"].as_str()).unwrap_or("Scribble Hub didn't save the order.").to_string().into()) };
                if let (true, Some(Page::Chapters(l))) = (ok, self.stack.last_mut()) {
                    l.order_changed = false;
                }
            }
            "m-chdel" if unreachable => self.form_state = FormState::Error("Couldn't reach Scribble Hub; the chapter may not be deleted.".into()),
            "m-gcats" => {
                let mut cats = options(body);
                if let Some(g) = self.glossary_mut() {
                    let keep = g.cats.iter().find(|c| c.on).map(|c| c.id.clone());
                    cats.iter_mut().for_each(|c| c.on = Some(&c.id) == keep.as_ref());
                    g.cats = cats;
                }
            }
            "m-glist" => {
                if let Some(g) = self.glossary_mut() {
                    g.items = gloss_items(body);
                    g.loading = false;
                }
            }
            "m-gload" => {
                if let Some(a) = json.as_ref().and_then(|j| j.as_array()) {
                    let name = a.first().and_then(|v| v.as_str()).unwrap_or_default().to_string();
                    let desc = markup::from_html(a.get(1).and_then(|v| v.as_str()).unwrap_or_default());
                    self.manage_in.name.update(cx, |i, cx| i.set_text(name, cx));
                    self.manage_in.desc.update(cx, |i, cx| i.set_text(desc, cx));
                }
            }
            "m-gsave" => {
                self.form_state = if unreachable { FormState::Error("Couldn't reach Scribble Hub.".into()) } else { FormState::Done("Saved".into()) };
                let sid = self.glossary_mut().map(|g| {
                    g.form = None;
                    g.sid.clone()
                });
                if let Some(sid) = sid {
                    self.reload_glossary(&sid);
                }
            }
            "m-gcat" => match err {
                Some(e) => self.form_state = FormState::Error(e.into()),
                None => {
                    let value = json.as_ref().and_then(|j| j["value"].as_str().map(str::to_string).or_else(|| j["value"].as_i64().map(|v| v.to_string())));
                    self.manage_in.new_cat.update(cx, |i, cx| i.clear(cx));
                    if let Some(g) = self.glossary_mut() {
                        if let Some(v) = value {
                            g.cats.iter_mut().for_each(|c| c.on = false);
                            g.cats.push(Choice { id: v, name: String::new(), on: true });
                        }
                        let sid = g.sid.clone();
                        self.web.ajax("m-gcats", &site::form(&[("action", "wi_getsrtcategories"), ("intPostID", &sid)]));
                    }
                }
            },
            "m-radd" => {
                let row = json.as_ref().and_then(|j| j["data"].as_str()).map(related_rows).unwrap_or_default();
                self.form_state = match (unreachable, err) {
                    (true, _) => FormState::Error("Couldn't reach Scribble Hub.".into()),
                    (_, Some(e)) => FormState::Error(e.into()),
                    _ => FormState::Done("Added".into()),
                };
                if let Some(Page::Related(r)) = self.stack.last_mut() {
                    for new in row {
                        r.rows.retain(|(id, ..)| *id != new.0);
                        r.rows.push(new);
                    }
                }
            }
            _ => {}
        }
        cx.notify();
        true
    }

    pub(crate) fn glossary_page(&self, g: &Glossary, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let m = &self.manage_in;
        let busy = self.form_state == FormState::Saving;
        let named = |id: &str| g.cats.iter().find(|c| c.id == id).map(|c| c.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| "Other".into());
        let form = g.form.as_ref().map(|key| {
            let cats = div().flex().flex_wrap().gap_2().children(g.cats.iter().enumerate().filter(|(_, c)| c.id != "0" && !c.name.is_empty()).map(|(i, c)| {
                self.chip(("gcat", i), c.name.clone(), c.on).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(g) = this.glossary_mut() {
                        g.cats.iter_mut().enumerate().for_each(|(j, c)| c.on = j == i);
                    }
                    cx.notify();
                }))
            }));
            let len = m.desc.read(cx).text().chars().count();
            self.card(if key.is_some() { "Edit entry" } else { "New entry" }, None)
                .child(self.labelled("Name", None, self.field("g-name", &m.name, 0., cx)))
                .child(self.labelled("Category", None, cats))
                .child(
                    self.labelled(
                        "New category",
                        None,
                        div().flex().gap_2().child(div().flex_1().child(self.field("g-newcat", &m.new_cat, 0., cx))).child(self.big_button("g-addcat", "Add", false, false).on_click(cx.listener(|this, _, _, cx| this.add_category(cx)))),
                    ),
                )
                .child(self.labelled("Description", Some(format!("{} / 6,000", group_digits(&len.to_string()))), self.field("g-desc", &m.desc, 140., cx).font_family(SERIF)))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap_2()
                        .child(self.big_button("g-cancel", "Cancel", false, false).on_click(cx.listener(|this, _, _, cx| {
                            if let Some(g) = this.glossary_mut() {
                                g.form = None;
                            }
                            cx.notify();
                        })))
                        .child(self.big_button("g-save", "Save", true, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.save_gloss(cx) }))),
                )
        });
        let mut groups: Vec<(String, Vec<&GlossItem>)> = vec![];
        for it in &g.items {
            match groups.iter_mut().find(|(c, _)| *c == it.cat) {
                Some((_, l)) => l.push(it),
                None => groups.push((it.cat.clone(), vec![it])),
            }
        }
        let list = groups.into_iter().enumerate().map(|(gi, (cat, items))| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.muted)).child(named(&cat).to_uppercase()))
                .children(items.into_iter().enumerate().map(|(i, it)| {
                    let key = (it.cat.clone(), it.order.clone());
                    let (k1, k2) = (key.clone(), key.clone());
                    let armed = g.confirm_delete.as_ref() == Some(&key);
                    div()
                        .id(("gitem", gi * 1000 + i))
                        .group("gitem")
                        .flex()
                        .items_center()
                        .gap_3()
                        .px(px(12.))
                        .py(px(10.))
                        .rounded(px(10.))
                        .cursor_pointer()
                        .hover(move |d| d.bg(rgb(p.surface)))
                        .child(div().flex_1().min_w_0().truncate().font_family(SERIF).text_size(px(16.)).child(it.name.clone()))
                        .child(
                            div()
                                .id(("gdel", gi * 1000 + i))
                                .px(px(10.))
                                .py(px(4.))
                                .rounded(px(7.))
                                .text_size(px(12.))
                                .text_color(rgb(if armed { p.on_accent } else { p.muted }))
                                .when(armed, |d| d.bg(rgb(p.star)))
                                .when(!armed, |d| d.opacity(0.).group_hover("gitem", |s| s.opacity(1.)))
                                .hover(move |d| d.bg(rgb(if armed { p.star } else { p.sel })))
                                .child(if armed { "Delete?" } else { "Delete" })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.delete_gloss(k1.clone(), cx)
                                })),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.open_gloss_form(Some(k2.clone()), cx)))
                }))
        });
        let list: Vec<_> = list.collect();
        let empty = !g.loading && g.items.is_empty() && g.form.is_none();
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(760.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(
                    div()
                        .flex()
                        .items_end()
                        .justify_between()
                        .gap_4()
                        .child(self.page_head("Glossary", Some(g.title.clone()), cx))
                        .when(g.form.is_none(), |d| d.child(self.big_button("g-new", "New entry", true, false).on_click(cx.listener(|this, _, _, cx| this.open_gloss_form(None, cx))))),
                )
                .children(form)
                .when(g.loading, |d| d.child(div().text_color(rgb(p.muted)).child("Loading…")))
                .when(empty, |d| d.child(div().py_4().text_color(rgb(p.muted)).child("No entries yet. Characters, places and terms readers can look up.")))
                .children(list)
                .children(match &self.form_state {
                    FormState::Error(e) => Some(div().text_size(px(13.)).text_color(rgb(p.star)).child(e.clone())),
                    _ => None,
                }),
        )
    }

    pub(crate) fn related_page(&self, r: &Related, cx: &mut Context<Self>) -> Div {
        let p = self.pal();
        let busy = self.form_state == FormState::Saving;
        let picker = |key: &'static str, list: &[Choice], which: fn(&mut Related) -> &mut Vec<Choice>, cx: &mut Context<Self>| {
            div().flex().flex_wrap().gap_2().children(list.iter().enumerate().map(|(i, c)| {
                self.chip((key, i), c.name.clone(), c.on).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(Page::Related(r)) = this.stack.last_mut() {
                        which(r).iter_mut().enumerate().for_each(|(j, c)| c.on = j == i);
                    }
                    cx.notify();
                }))
            }))
        };
        let rows = r.rows.iter().enumerate().map(|(i, (id, name, rel))| {
            let id = id.clone();
            div()
                .flex()
                .items_center()
                .gap_3()
                .py(px(10.))
                .when(i > 0, |d| d.border_t_1().border_color(rgb(p.line)))
                .child(div().flex_1().min_w_0().truncate().font_family(SERIF).text_size(px(16.)).child(name.clone()))
                .child(div().text_size(px(13.)).text_color(rgb(p.muted)).child(rel.clone()))
                .child(self.big_button(("r-del", i), "Remove", false, false).on_click(cx.listener(move |this, _, _, cx| this.remove_related(id.clone(), cx))))
        });
        let rows: Vec<_> = rows.collect();
        let footer = self.form_footer(self.big_button("r-add", "Add relation", true, busy).on_click(cx.listener(move |this, _, _, cx| if !busy { this.add_related(cx) })), None);
        div().flex().flex_col().px(px(44.)).pt(px(TITLE_INSET)).pb(px(56.)).child(
            div()
                .max_w(px(760.))
                .flex()
                .flex_col()
                .gap(px(18.))
                .child(self.page_head("Related series", Some(r.title.clone()), cx))
                .child(self.card("Current", None).when(r.rows.is_empty(), |d| d.child(div().text_color(rgb(p.muted)).child("None yet."))).children(rows))
                .child(
                    self.card("Add", None)
                        .child(self.labelled("Series", None, picker("r-series", &r.series, |r| &mut r.series, cx)))
                        .child(self.labelled("Relation", None, picker("r-rel", &r.relations, |r| &mut r.relations, cx))),
                )
                .child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{gloss_items, options, related_rows, GlossItem};

    #[test]
    fn parses_lists() {
        let g = gloss_items(r#"<ul class="sortable-glossary 5"><li category="5" orderid="1" item="Rayen">Rayen <span>…</span></li><li category="5" orderid="2">The Duke</li></ul>"#);
        assert_eq!(g, [GlossItem { cat: "5".into(), order: "1".into(), name: "Rayen".into() }, GlossItem { cat: "5".into(), order: "2".into(), name: "The Duke".into() }]);
        let o = options(r#"<option value="0">Select</option><option value="1" selected>Characters</option>"#);
        assert_eq!((o[1].id.as_str(), o[1].name.as_str(), o[1].on), ("1", "Characters", true));
        let r = related_rows(r#"<tr class="rs_sid 804522"><td><a>Anectodes</a></td><td>Sequel</td><td>Delete</td></tr>"#);
        assert_eq!(r, [("804522".into(), "Anectodes".into(), "Sequel".into())]);
    }
}
