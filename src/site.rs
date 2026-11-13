use scraper::{ElementRef, Html, Node, Selector};
use std::ops::Range;

pub const BASE: &str = "https://www.scribblehub.com/";

#[derive(Clone, Debug)]
pub struct Card {
    pub title: String,
    pub url: String,
    pub cover: String,
    pub rating: f32,
    pub rank: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub name: String,
    pub view_more: Option<String>,
    pub cards: Vec<Card>,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub title: String,
    pub url: String,
    pub cover: String,
    pub genres: Vec<String>,
    pub chapter: String,
    pub chapter_url: String,
    pub byline: String,
}

#[derive(Clone, Debug)]
pub struct PageRef {
    pub label: String,
    pub url: Option<String>,
    pub current: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Home {
    pub shelves: Vec<Shelf>,
    pub updates: Vec<Update>,
    pub pages: Vec<PageRef>,
}

#[derive(Clone, Debug, Default)]
pub struct Series {
    pub id: String,
    pub title: String,
    pub cover: String,
    pub author: String,
    pub rating: f32,
    pub rating_label: String,
    pub stats: Vec<String>,
    pub genres: Vec<String>,
    pub synopsis: Vec<String>,
    pub first_chapter: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub cover: String,
    pub rating: f32,
    pub rating_label: String,
    pub stats: Vec<String>,
    pub genres: Vec<String>,
    pub blurb: String,
}

#[derive(Clone, Debug)]
pub struct ChapterLink {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Emphasis {
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(String, Vec<(Range<usize>, Emphasis)>),
    Rule,
}

#[derive(Clone, Debug, Default)]
pub struct Chapter {
    pub series_title: String,
    pub series_url: String,
    pub title: String,
    pub blocks: Vec<Block>,
    pub prev: Option<String>,
    pub next: Option<String>,
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("static selector")
}

fn text(e: ElementRef) -> String {
    e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn first_text(root: ElementRef, s: &str) -> String {
    root.select(&sel(s)).next().map(text).unwrap_or_default()
}

fn first_attr(root: ElementRef, s: &str, attr: &str) -> String {
    root.select(&sel(s))
        .next()
        .and_then(|e| e.value().attr(attr))
        .unwrap_or_default()
        .to_string()
}

fn link(root: ElementRef, s: &str) -> Option<String> {
    Some(first_attr(root, s, "href")).filter(|h| h.starts_with("http"))
}

fn stars(root: ElementRef) -> f32 {
    root.select(&sel("i.ficrate"))
        .map(|i| {
            let c = i.value().attr("class").unwrap_or_default();
            if c.contains("fa-star-o") {
                0.0
            } else if c.contains("half") {
                0.5
            } else {
                1.0
            }
        })
        .sum()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub name: String,
    pub avatar: Option<String>,
    pub logout: Option<String>,
}

pub fn account(html: &str) -> Option<Option<Account>> {
    let doc = Html::parse_document(html);
    let menu = doc.select(&sel("#menu_username_right")).next()?;
    if menu.select(&sel(".guest")).next().is_some() {
        return Some(None);
    }
    let img = menu.select(&sel("img")).next();
    let from_menu = menu.select(&sel(".username")).map(text).find(|t| !t.is_empty());
    let from_link = menu
        .select(&sel("a[href*='/profile/']"))
        .filter_map(|a| a.value().attr("href"))
        .find_map(|h| h.split("/profile/").nth(1)?.split('/').nth(1).filter(|n| !n.is_empty()).map(str::to_string));
    let from_img = img
        .and_then(|i| i.value().attr("alt").or(i.value().attr("title")))
        .map(str::trim)
        .filter(|a| !a.is_empty() && !a.to_lowercase().contains("avatar"))
        .map(str::to_string);
    let name = from_menu.or(from_link).or(from_img).unwrap_or_else(|| "Signed in".into());
    Some(Some(Account {
        name,
        avatar: img.and_then(|i| i.value().attr("src")).filter(|s| s.starts_with("http")).map(str::to_string),
        logout: doc.select(&sel("a[href*='logout']")).next().and_then(|a| a.value().attr("href")).map(str::to_string),
    }))
}

pub fn home(html: &str) -> Home {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let shelves = root
        .select(&sel(".wi_fic_wrap.slider"))
        .map(|shelf| {
            let name = shelf
                .select(&sel(".main_title"))
                .next()
                .and_then(|t| t.text().next())
                .unwrap_or_default()
                .trim()
                .to_string();
            let view_more = shelf
                .select(&sel(".main_title a.trend_main"))
                .next()
                .and_then(|a| a.value().attr("href"))
                .filter(|href| !href.is_empty())
                .map(|h| if h.starts_with('/') { format!("{BASE}{}", h.trim_start_matches('/')) } else { h.to_string() });
            let cards = shelf
                .select(&sel(".slick-slide:not(.slick-cloned) .novel_carousel_img, .new-novels-carousel > div > .novel_carousel_img"))
                .map(|c| Card {
                    title: first_attr(c, ".centered_novel", "title"),
                    url: first_attr(c, "a", "href"),
                    cover: first_attr(c, "img", "src"),
                    rating: stars(c),
                    rank: Some(first_text(c, ".trend_rank")).filter(|r| !r.is_empty()),
                })
                .collect();
            Shelf { name, view_more, cards }
        })
        .collect();
    let updates = root
        .select(&sel("#main_releases tr.toc_w"))
        .map(|r| Update {
            title: first_text(r, ".fp_title"),
            url: first_attr(r, ".fp_title", "href"),
            cover: first_attr(r, ".m_img_fic img", "src"),
            genres: r.select(&sel(".fic_genre")).map(text).collect(),
            chapter: first_text(r, "a[href*='/chapter/']"),
            chapter_url: first_attr(r, "a[href*='/chapter/']", "href"),
            byline: first_text(r, ".ah_fmn"),
        })
        .collect();
    let pages = root
        .select(&sel("#pagination-profile-recent li"))
        .map(|li| {
            let classes = li.value().attr("class").unwrap_or_default();
            if let Some(a) = li.select(&sel("a.page-link")).next() {
                let url = a.value().attr("href").filter(|h| !h.is_empty()).map(|h| {
                    if h.starts_with("http") {
                        h.to_string()
                    } else {
                        format!("{}{}", BASE, h.trim_start_matches('/'))
                    }
                });
                PageRef { label: text(a), url, current: false }
            } else if li.select(&sel("span.ellipse")).next().is_some() {
                PageRef { label: "…".into(), url: None, current: false }
            } else {
                PageRef {
                    label: li.select(&sel("span.current")).next().map(text).unwrap_or_default(),
                    url: None,
                    current: classes.contains("active"),
                }
            }
        })
        .collect();
    Home { shelves, updates, pages }
}

pub fn series(html: &str) -> Series {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let rating_label = root
        .select(&sel("#ratefic_user > span"))
        .next()
        .map(text)
        .unwrap_or_default();
    Series {
        id: first_attr(root, "#mypostid", "value"),
        title: first_text(root, ".fic_title"),
        cover: first_attr(root, ".fic_image img", "src"),
        author: first_text(root, ".auth_name_fic"),
        rating: root.select(&sel("#ratefic_user")).next().map(stars).unwrap_or(0.0),
        rating_label,
        stats: root.select(&sel(".fic_stats .st_item")).map(text).collect(),
        genres: root.select(&sel(".wi_fic_genre .fic_genre")).map(text).collect(),
        synopsis: root
            .select(&sel(".wi_fic_desc p"))
            .map(text)
            .filter(|p| !p.is_empty())
            .collect(),
        first_chapter: link(root, ".read_buttons a"),
    }
}

#[derive(Clone, Debug)]
pub struct Suggestion {
    pub title: String,
    pub marks: Vec<Range<usize>>,
    pub url: String,
    pub cover: String,
}

pub fn suggestions(html: &str) -> Vec<Suggestion> {
    let frag = Html::parse_fragment(html);
    frag.root_element()
        .select(&sel("li.sh-ri"))
        .map(|li| {
            let (mut title, mut marks) = (String::new(), Vec::new());
            if let Some(t) = li.select(&sel(".sh-t")).next() {
                for n in t.children() {
                    if let Some(s) = n.value().as_text() {
                        title.push_str(s);
                    } else if let Some(m) = ElementRef::wrap(n) {
                        let start = title.len();
                        title.extend(m.text());
                        marks.push(start..title.len());
                    }
                }
            }
            Suggestion { title, marks, url: first_attr(li, "a", "href"), cover: first_attr(li, "img", "src") }
        })
        .collect()
}

pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect::<Vec<_>>().join("&")
}

pub fn plain(html: &str) -> String {
    text(Html::parse_fragment(html).root_element())
}

pub fn search_url(query: &str) -> String {
    format!("{BASE}series-finder/?sf=1&sh={}", enc(query))
}

pub fn search(html: &str) -> Vec<Hit> {
    let doc = Html::parse_document(html);
    doc.root_element()
        .select(&sel(".search_main_box"))
        .map(|b| {
            let body = b.select(&sel(".search_body")).next();
            let loose: String = body
                .into_iter()
                .flat_map(|e| e.children())
                .filter_map(|n| n.value().as_text().map(|t| t.to_string()))
                .collect();
            let rest = first_text(b, ".testhide");
            Hit {
                title: first_text(b, ".search_title > a"),
                url: first_attr(b, ".search_title > a", "href"),
                cover: first_attr(b, ".search_img img", "src"),
                rating: stars(b),
                rating_label: first_text(b, ".search_ratings"),
                stats: b.select(&sel(".search_stats .nl_stat")).map(text).filter(|s| !s.is_empty()).collect(),
                genres: b.select(&sel(".search_genre .fic_genre")).map(text).collect(),
                blurb: format!("{} {}", loose.trim(), rest.trim_end_matches("<<less")).split_whitespace().collect::<Vec<_>>().join(" "),
            }
        })
        .collect()
}

pub fn toc(html: &str) -> Vec<ChapterLink> {
    let frag = Html::parse_fragment(html);
    let mut list: Vec<_> = frag
        .root_element()
        .select(&sel("li.li_toc a"))
        .map(|a| ChapterLink {
            title: text(a),
            url: a.value().attr("href").unwrap_or_default().to_string(),
        })
        .collect();
    list.reverse();
    list
}

#[derive(Clone, Debug)]
pub struct RlChapter {
    pub title: String,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct RlItem {
    pub sid: String,
    pub title: String,
    pub url: String,
    pub cover: String,
    pub rating: f32,
    pub unread: u32,
    pub current: Option<RlChapter>,
    pub latest: Option<RlChapter>,
    pub progress: (String, String),
    pub deleted: bool,
    pub author: String,
}

#[derive(Clone, Debug)]
pub struct RlMore {
    pub nonce: String,
    pub list_id: String,
    pub offset: String,
    pub sort: String,
}

#[derive(Clone, Debug, Default)]
pub struct ReadingList {
    pub lists: Vec<(String, String)>,
    pub active: usize,
    pub items: Vec<RlItem>,
    pub more: Option<RlMore>,
    pub delete: Option<(String, String)>,
}

pub fn reading_list(html: &str) -> ReadingList {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let lists: Vec<(String, String)> = root
        .select(&sel("#cssmenu li a"))
        .filter_map(|a| {
            let href = a.value().attr("href")?;
            let name = text(a);
            (!name.is_empty() && href.contains("/reading-list/")).then(|| (name, href.to_string()))
        })
        .collect();
    let active = root
        .select(&sel("#cssmenu li.active a"))
        .next()
        .and_then(|a| a.value().attr("href"))
        .and_then(|href| lists.iter().position(|(_, u)| u == href))
        .unwrap_or(0);
    let delete = root.select(&sel("form[name='seriesExtras']")).next().and_then(|f| {
        let nonce = f.select(&sel("input[name='wi_rl_nonce']")).next()?.value().attr("value")?.to_string();
        Some((f.value().attr("action").unwrap_or("/reading-list/").to_string(), nonce))
    });
    let mut rl = ReadingList { lists, active, items: reading_list_rows(html), more: None, delete };
    if let Some(e) = root.select(&sel("#sh-reading-list")).next() {
        if e.value().attr("data-has-more") == Some("1") {
            let attr = |n: &str| e.value().attr(n).unwrap_or_default().to_string();
            rl.more = Some(RlMore { nonce: attr("data-nonce"), list_id: attr("data-list-id"), offset: attr("data-offset"), sort: attr("data-sort") });
        }
    }
    rl
}

pub fn reading_list_rows(html: &str) -> Vec<RlItem> {
    let frag = Html::parse_fragment(html);
    frag.root_element()
        .select(&sel("tr.rl_i_box"))
        .filter_map(|tr| {
            let v = tr.value();
            let sid: String = v.attr("data-sid").unwrap_or_default().chars().filter(char::is_ascii_digit).collect();
            let link = |selector: &str| {
                tr.select(&sel(selector)).next().and_then(|a| a.value().attr("href").and_then(valid_url)).map(|url| RlChapter { title: first_text(tr, selector), url })
            };
            let progress: Vec<String> = tr.select(&sel(".pro_rl a")).map(text).collect();
            let current = link(&format!("span.ui_{sid} a"));
            let latest = link(&format!("#lat_{sid} a"));
            let title_el = tr.select(&sel(".rl_i_title")).next();
            let deleted = title_el.is_some_and(|t| t.select(&sel("i")).any(|i| text(i).trim() == "Deleted"));
            let series_link = title_el.and_then(|t| t.select(&sel("a[href*='/series/']")).next());
            let title = match series_link {
                Some(a) => text(a),
                None => title_el.and_then(|t| t.select(&sel("b")).next()).map(text).unwrap_or_else(|| first_text(tr, ".rl_i_title")),
            };
            let author = title_el.and_then(|t| t.select(&sel(".fp_authorname")).next()).map(text).unwrap_or_default();
            Some(RlItem {
                sid,
                url: series_link.map(|a| a.value().attr("href").unwrap_or_default().to_string()).unwrap_or_default(),
                title,
                cover: first_attr(tr, ".rl_i_image img", "src"),
                rating: v.attr("data-rate").and_then(|r| r.parse().ok()).unwrap_or(-1.),
                unread: v.attr("data-unread").and_then(|u| u.parse().ok()).unwrap_or(0),
                current,
                latest,
                progress: match progress.as_slice() {
                    [a, b] => (a.clone(), b.clone()),
                    _ => (String::new(), String::new()),
                },
                deleted,
                author,
            })
            .filter(|i| !i.title.is_empty() || !i.url.is_empty())
        })
        .collect()
}

fn valid_url(u: &str) -> Option<String> {
    u.starts_with("http").then(|| u.to_string())
}

#[derive(Clone, Debug)]
pub struct Alert {
    pub icon: Option<String>,
    pub text: String,
    pub url: Option<String>,
    pub time: String,
}

pub fn alerts(html: &str) -> Vec<Alert> {
    let frag = Html::parse_fragment(html);
    let root = frag.root_element();
    let items: Vec<Alert> = root
        .select(&sel(".ab_items li"))
        .map(|li| Alert {
            icon: li.select(&sel("img")).next().and_then(|i| i.value().attr("src")).and_then(valid_url),
            text: first_text(li, ".ab_type"),
            url: li.select(&sel(".ab_type a")).next().and_then(|a| a.value().attr("href")).and_then(valid_url),
            time: first_text(li, ".ab_time"),
        })
        .filter(|a| !a.text.is_empty())
        .collect();
    if !items.is_empty() || root.select(&sel(".ab_items")).next().is_some() {
        return items;
    }
    root.select(&sel("a[href^='http']"))
        .map(|a| Alert { icon: None, text: text(a), url: valid_url(a.value().attr("href").unwrap_or_default()), time: String::new() })
        .filter(|a| !a.text.is_empty())
        .collect()
}

pub fn chapter(html: &str) -> Chapter {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let crumbs = sel(".wi_breadcrumb a[href*='/series/']");
    let crumb = root.select(&crumbs).next();
    let mut blocks = Vec::new();
    if let Some(body) = root.select(&sel("#chp_raw")).next() {
        for child in body.children() {
            match child.value() {
                Node::Element(e) if e.name() == "hr" => blocks.push(Block::Rule),
                Node::Element(_) => {
                    let mut out = (String::new(), Vec::new());
                    collect(ElementRef::wrap(child).unwrap(), Emphasis::default(), &mut out);
                    let (s, runs) = out;
                    if !s.trim().is_empty() {
                        blocks.push(Block::Text(s.trim_end().to_string(), runs));
                    }
                }
                Node::Text(t) if !t.trim().is_empty() => {
                    blocks.push(Block::Text(t.trim().to_string(), vec![]))
                }
                _ => {}
            }
        }
    }
    Chapter {
        series_title: crumb.map(text).unwrap_or_default(),
        series_url: crumb
            .and_then(|a| a.value().attr("href"))
            .unwrap_or_default()
            .to_string(),
        title: first_text(root, ".chapter-title"),
        blocks,
        prev: link(root, "a.btn-prev"),
        next: link(root, "a.btn-next"),
    }
}

fn collect(e: ElementRef, em: Emphasis, out: &mut (String, Vec<(Range<usize>, Emphasis)>)) {
    let name = e.value().name();
    let em = Emphasis {
        bold: em.bold || matches!(name, "b" | "strong"),
        italic: em.italic || matches!(name, "i" | "em"),
    };
    for child in e.children() {
        match child.value() {
            Node::Text(t) => {
                let mut start = None;
                for (i, word) in t.split_whitespace().enumerate() {
                    let joins = i == 0 && !t.starts_with(char::is_whitespace);
                    if !out.0.is_empty() && !out.0.ends_with(['\n', ' ']) && !joins {
                        out.0.push(' ');
                    }
                    start.get_or_insert(out.0.len());
                    out.0.push_str(word);
                }
                if t.ends_with(char::is_whitespace) && !out.0.is_empty() && !out.0.ends_with(' ') {
                    out.0.push(' ');
                }
                if let Some(start) = start.filter(|_| em != Emphasis::default()) {
                    out.1.push((start..out.0.trim_end().len(), em));
                }
            }
            Node::Element(c) if c.name() == "br" => out.0.push('\n'),
            Node::Element(_) => collect(ElementRef::wrap(child).unwrap(), em, out),
            _ => {}
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Work {
    pub id: String,
    pub title: String,
    pub url: String,
    pub cover: String,
    pub rating: String,
    pub stats: Vec<(String, String)>,
    pub updated: String,
    pub genres: Vec<String>,
    pub blurb: String,
}

pub fn dashboard(html: &str) -> Option<Vec<Work>> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    root.select(&sel(".db_title")).next()?;
    Some(
        root.select(&sel(".search_main_box.dash"))
            .filter_map(|b| {
                let a = b.select(&sel(".search_title a")).next()?;
                let mut stats = vec![];
                let mut updated = String::new();
                for st in b.select(&sel(".search_stats .nl_stat")) {
                    let t = text(st);
                    if st.value().attr("title") == Some("Last Updated") {
                        updated = t;
                    } else if let Some((v, l)) = t.split_once(' ') {
                        stats.push((v.to_string(), l.to_string()));
                    }
                }
                let body = b.select(&sel(".search_body")).next()?;
                let blurb = body.children().filter_map(|n| n.value().as_text().map(|t| t.to_string())).collect::<String>();
                Some(Work {
                    id: b.value().classes().find(|c| c.chars().all(|ch| ch.is_ascii_digit()))?.to_string(),
                    title: text(a),
                    url: a.value().attr("href")?.to_string(),
                    cover: first_attr(b, ".search_img img", "src"),
                    rating: first_text(b, ".search_ratings").trim_matches(|c| c == '(' || c == ')').to_string(),
                    stats,
                    updated,
                    genres: b.select(&sel(".search_genre a")).map(text).collect(),
                    blurb: blurb.split_whitespace().collect::<Vec<_>>().join(" "),
                })
            })
            .collect(),
    )
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub id: String,
    pub name: String,
    pub on: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SeriesForm {
    pub sid: Option<String>,
    pub title: String,
    pub description: String,
    pub genres: Vec<Choice>,
    pub main_genre: String,
    pub fandom: String,
    pub tags: Vec<(String, String)>,
    pub picked_tags: Vec<String>,
    pub copyright: Vec<Choice>,
    pub mature: bool,
    pub warnings: Vec<Choice>,
    pub status: Vec<Choice>,
}

#[derive(Clone, Debug)]
pub struct Toggle {
    pub key: String,
    pub label: String,
    pub hint: String,
    pub on: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub username: String,
    pub avatar: Option<String>,
    pub dob_month: Vec<Choice>,
    pub dob_day: String,
    pub dob_year: String,
    pub gender: Vec<Choice>,
    pub location: String,
    pub homepage: String,
    pub time_format: Vec<Choice>,
    pub timezone: Vec<Choice>,
    pub about: String,
    pub privacy: Vec<Toggle>,
    pub alerts: Vec<(String, Vec<Toggle>)>,
    pub paypal: String,
    pub patreon: String,
    pub kofi: String,
    pub subscribestar: String,
    pub email: String,
    pub email_confirmed: bool,
}

fn toggles(section: ElementRef) -> Vec<Toggle> {
    section
        .select(&sel(".item_alert"))
        .filter(|it| !it.value().attr("style").unwrap_or_default().contains("display: none"))
        .filter_map(|it| {
            let b = it.select(&sel("span[onclick^='account_alert_chk']")).next()?;
            Some(Toggle {
                key: b.value().attr("class")?.to_string(),
                label: first_text(it, ".lbl_cl"),
                hint: first_text(it, ".alert_info"),
                on: b.value().attr("value") == Some("1"),
            })
        })
        .collect()
}

pub fn settings(html: &str) -> Option<Settings> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    root.select(&sel("#account_content1")).next()?;
    let val = |q: &str| root.select(&sel(q)).next().and_then(|e| e.value().attr("value")).unwrap_or_default().to_string();
    let section = |n: u8| root.select(&sel(&format!("#account_content{n}"))).next();
    let mut alerts: Vec<(String, Vec<Toggle>)> = vec![];
    if let Some(sec) = section(3) {
        for el in sec.children().filter_map(ElementRef::wrap) {
            let class = el.value().attr("class").unwrap_or_default();
            if class.contains("overview_title") {
                alerts.push((text(el), vec![]));
            } else if class.contains("item_alert") {
                let wrap = Html::parse_fragment(&format!("<div>{}</div>", el.html()));
                let t = toggles(wrap.root_element());
                if let Some((_, list)) = alerts.last_mut() {
                    list.extend(t);
                }
            }
        }
    }
    Some(Settings {
        username: first_text(root, "#account_content1 .acc_item a"),
        avatar: root.select(&sel(".avatarchange")).next().and_then(|i| i.value().attr("src")).and_then(valid_url),
        dob_month: choices(root, "#dob_month"),
        dob_day: val("#dob_day"),
        dob_year: val("#dob_year"),
        gender: choices(root, "#gender_select"),
        location: val("#acc_location"),
        homepage: val("input#acc_homepage"),
        time_format: choices(root, ".select_timeformat"),
        timezone: choices(root, "#s_timezone"),
        about: first_text(root, "#profileabout"),
        privacy: section(2).map(toggles).unwrap_or_default(),
        alerts,
        paypal: val("#fic_paypal"),
        patreon: val("#fic_patreon"),
        kofi: val("#fic_kofi"),
        subscribestar: val("#fic_subscribestar"),
        email: val("#email_new"),
        email_confirmed: root.select(&sel(".confirm_email_status.valid")).next().is_some(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChapterRow {
    pub id: String,
    pub title: String,
    pub status: String,
    pub date: String,
    pub stats: (String, String, String),
}

pub fn chapter_rows(html: &str) -> Vec<ChapterRow> {
    let frag = Html::parse_fragment(html);
    frag.root_element()
        .select(&sel("li.ui-chapter-sort"))
        .filter_map(|li| {
            let id = li.value().attr("pid")?.to_string();
            let title = li.select(&sel(&format!("[id='chp_es_title_{id}']"))).next().map(text).unwrap_or_else(|| first_text(li, "a"));
            let stat = |t: &str| li.select(&sel(&format!(".edit_col-stats[title='{t}']"))).next().map(text).unwrap_or_default();
            let st = li.select(&sel(".ce_stats_status")).next();
            Some(ChapterRow {
                id,
                title,
                status: st.map(|s| first_text(s, "span")).unwrap_or_default(),
                date: st.and_then(|s| s.value().attr("title")).unwrap_or_default().to_string(),
                stats: (stat("Pageviews"), stat("Hearts"), stat("Comments")),
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Blocked {
    pub id: String,
    pub name: String,
}

pub fn blocked(html: &str) -> Vec<Blocked> {
    let frag = Html::parse_fragment(html);
    frag.root_element()
        .select(&sel(".bk_items"))
        .filter_map(|e| {
            let id = e.value().classes().find(|c| c.chars().all(|ch| ch.is_ascii_digit()))?.to_string();
            let name = e.select(&sel("a")).map(text).find(|t| !t.is_empty() && !t.to_lowercase().contains("unblock")).unwrap_or_else(|| text(e).replace("Unblock User", "").replace("Unblock", "").trim().to_string());
            Some(Blocked { id, name })
        })
        .collect()
}

fn choices(root: ElementRef, select: &str) -> Vec<Choice> {
    root.select(&sel(&format!("{select} option")))
        .map(|o| Choice { id: o.value().attr("value").unwrap_or_default().to_string(), name: text(o), on: o.value().attr("selected").is_some() })
        .collect()
}

pub fn series_form(html: &str) -> Option<SeriesForm> {
    let doc = Html::parse_document(html);
    let root = doc.root_element();
    let title = root.select(&sel("#edittitle")).next()?;
    let label = |id: &str| root.select(&sel(&format!("label[for='{id}']"))).next().map(text).unwrap_or_default();
    let genres: Vec<Choice> = root
        .select(&sel("input.altchk-g"))
        .filter_map(|g| {
            let v = g.value();
            let id = v.attr("gmeid").map(str::to_string).or_else(|| v.attr("id").map(|i| i.trim_start_matches("gen").to_string()))?;
            let name = v.attr("isname").map(str::to_string).unwrap_or_else(|| label(v.attr("id").unwrap_or_default()));
            Some(Choice { id, name, on: v.attr("chkme") == Some("include") })
        })
        .collect();
    let picked_tags = html
        .split("tagsalledit').val([")
        .nth(1)
        .and_then(|r| r.split(']').next())
        .map(|ids| ids.split(',').map(|i| i.trim().trim_matches(|c| c == '"' || c == '\'').to_string()).filter(|i| !i.is_empty()).collect())
        .unwrap_or_default();
    let main_genre = root
        .select(&sel("#cur_m_gen"))
        .next()
        .and_then(|i| i.value().attr("value"))
        .map(str::to_string)
        .or_else(|| choices(root, "#main_genre").into_iter().find(|c| c.on).map(|c| c.id))
        .unwrap_or_default();
    let sid = root.select(&sel("#mypostid")).next().and_then(|i| i.value().attr("value")).map(str::to_string);
    Some(SeriesForm {
        sid,
        title: title.value().attr("value").unwrap_or_default().to_string(),
        description: root.select(&sel("#editdescription")).next().map(|t| t.text().collect::<String>()).unwrap_or_default().trim().to_string(),
        genres,
        main_genre,
        fandom: root.select(&sel("#editfandom")).next().map(|t| t.text().collect::<String>()).unwrap_or_default().trim().to_string(),
        tags: root.select(&sel(".tagsalledit option")).map(|o| (o.value().attr("value").unwrap_or_default().to_string(), text(o))).collect(),
        picked_tags,
        copyright: choices(root, "#copyrightselect"),
        mature: root.select(&sel("#toggle-rating")).next().is_some_and(|i| i.value().attr("checked").is_some())
            || root.select(&sel(".toggle.btn")).next().is_some_and(|t| t.value().classes().all(|c| c != "off")),
        warnings: root
            .select(&sel(".es_mature_content"))
            .map(|w| Choice {
                id: w.value().attr("ctid").unwrap_or_default().to_string(),
                name: w.next_siblings().filter_map(ElementRef::wrap).next().map(text).unwrap_or_default(),
                on: w.value().attr("value") == Some("1"),
            })
            .collect(),
        status: choices(root, "#fic_storystatus"),
    })
}

pub fn stats_chart(json: &str) -> Vec<(String, f64)> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json.trim().trim_end_matches('0').trim()).or_else(|_| serde_json::from_str(json.trim())) else {
        return vec![];
    };
    let s = |k: &str| match &v[k] {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let dates = s("date");
    let counts = s("count");
    dates
        .split(',')
        .map(|d| d.trim().trim_matches('"').to_string())
        .zip(counts.split(',').map(|c| c.trim().trim_matches('"').parse().unwrap_or(0.)))
        .collect()
}

pub fn referrers(json: &str) -> (Vec<(String, String)>, Vec<(String, String)>) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json.trim().trim_end_matches('0').trim()).or_else(|_| serde_json::from_str(json.trim())) else {
        return (vec![], vec![]);
    };
    let rows = |k: &str| {
        let frag = Html::parse_fragment(&format!("<table>{}</table>", v[k].as_str().unwrap_or_default()));
        frag.root_element()
            .select(&sel("tr"))
            .filter_map(|tr| {
                let cells: Vec<String> = tr.select(&sel("td")).map(text).collect();
                match cells.as_slice() {
                    [a, b, ..] => Some((a.clone(), b.clone())),
                    _ => None,
                }
            })
            .collect()
    };
    (rows("sh"), rows("web"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_in_name_from_profile_link() {
        let html = r#"<span id="menu_username_right"><img src="https://cdn.scribblehub.com/a.jpg">
            <a href="https://www.scribblehub.com/profile/123/some_reader/">Your Profile Page</a>
            <a href="https://www.scribblehub.com/wp-login.php?action=logout&_wpnonce=x">Log Out</a></span>"#;
        let a = account(html).flatten().expect("signed in");
        assert_eq!(a.name, "some_reader");
        assert!(a.logout.unwrap().contains("_wpnonce"));
    }

    #[test]
    fn signed_out() {
        assert_eq!(account(include_str!("../tests/fixtures/home.html")), Some(None));
        assert_eq!(account(include_str!("../tests/fixtures/suggest.html")), None);
    }

    #[test]
    fn signed_in_from_real_menu() {
        let a = account(include_str!("../tests/fixtures/home-signed-in.html")).flatten().expect("signed in");
        assert_eq!(a.name, "Graspier");
        assert!(a.avatar.as_deref().unwrap().contains("avatar"));
        assert!(a.logout.as_deref().unwrap().ends_with("/logout/"));
    }

    #[test]
    fn reading_list_page() {
        let rl = reading_list(include_str!("../tests/fixtures/reading-list.html"));
        let names: Vec<_> = rl.lists.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["Reading", "Finished Reading"]);
        assert_eq!(rl.active, 0);
        assert_eq!(rl.items.len(), 10);
        assert!(rl.more.is_none());
        let it = &rl.items[0];
        assert_eq!(it.sid, "2361389");
        assert_eq!(it.title, "I Larped as a Girl Online Until I Became One");
        assert!(it.url.contains("/series/2361389/") && it.cover.starts_with("https://"));
        assert_eq!(it.unread, 6);
        assert_eq!(it.rating, 5.);
        assert_eq!(it.current.as_ref().unwrap().title, "Chapter 3.1 – Biological Anomaly (1)");
        assert!(it.current.as_ref().unwrap().url.contains("/chapter/2367220/"));
        assert_eq!(it.latest.as_ref().unwrap().title, "Chapter 7 – A Familiar Stranger");
        assert_eq!(it.progress, ("4".to_string(), "10".to_string()));
        assert_eq!(rl.delete, Some(("/reading-list/".into(), "61a496e5ff".into())));
        let fin = reading_list(include_str!("../tests/fixtures/reading-list-finished.html"));
        assert_eq!(fin.delete, Some(("/reading-list/?list=1".into(), "8edc510373".into())));
    }

    #[test]
    fn reading_list_deleted_series() {
        let rl = reading_list(include_str!("../tests/fixtures/reading-list-finished.html"));
        let deleted: Vec<_> = rl.items.iter().filter(|i| i.deleted).collect();
        assert_eq!(deleted.len(), 4);
        let d = &deleted[0];
        assert_eq!(d.title, "Diary of the last scientist");
        assert_eq!(d.author, "Alastor_Darkwood");
        assert_eq!(d.sid, "121338");
        assert!(d.url.is_empty(), "deleted series have no series page to open");
        assert!(d.current.is_none() && d.latest.is_none());
        assert_eq!(&deleted[1].author, "Asveses");
        let live = &rl.items[0];
        assert!(!live.deleted && live.author.is_empty());
        assert!(live.url.contains("/series/804522/"));
        assert!(live.current.is_some());
    }

    #[test]
    fn settings_page() {
        let s = settings(include_str!("../tests/fixtures/account.html")).unwrap();
        assert_eq!(s.username, "Graspier");
        assert_eq!(s.dob_month.iter().find(|c| c.on).unwrap().name, "June");
        assert_eq!((s.dob_day.as_str(), s.dob_year.as_str()), ("9", "1999"));
        assert_eq!(s.gender.iter().find(|c| c.on).unwrap().name, "Female");
        assert_eq!(s.timezone.iter().find(|c| c.on).unwrap().id, "US/Eastern");
        assert_eq!(s.homepage, "http://Graspier");
        assert_eq!(s.privacy.len(), 4);
        assert_eq!(s.privacy[0].key, "general_showonlinestatus");
        let heads: Vec<_> = s.alerts.iter().map(|(h, l)| (h.as_str(), l.len())).collect();
        assert_eq!(heads, [("General", 6), ("Profile", 3), ("Authors", 2)]);
        let bg = s.alerts[0].1.iter().find(|t| t.key == "alertshow").unwrap();
        assert!(!bg.on);
        assert_eq!(s.email, "reader@example.com");
        assert!(s.email_confirmed);
        let b = blocked(r#"<div class="bk_items 77"><a href="/profile/77/x/">Xero</a> <span onclick="unblockuser_account(77,this)">Unblock User</span></div>"#);
        assert_eq!(b, [Blocked { id: "77".into(), name: "Xero".into() }]);
    }

    #[test]
    fn chapter_list() {
        let j: serde_json::Value = serde_json::from_str(include_str!("../tests/fixtures/chapters.json")).unwrap();
        let rows = chapter_rows(j["data"]["html"].as_str().unwrap());
        assert_eq!(rows.len(), 6);
        let r = &rows[0];
        assert_eq!((r.id.as_str(), r.title.as_str()), ("1285556", "[Chap 6] Fragments of incomplete memories (I)"));
        assert_eq!((r.status.as_str(), r.date.as_str()), ("Published", "Nov 2, 2024 09:43 AM"));
        assert_eq!(r.stats, ("86".into(), "5".into(), "1".into()));
    }

    #[test]
    fn view_more_lists() {
        let hits = search(include_str!("../tests/fixtures/latest-series.html"));
        assert_eq!(hits.len(), 25);
        assert!(hits.iter().all(|h| !h.title.is_empty() && h.url.contains("/series/")));
    }

    #[test]
    fn alerts_parse() {
        assert!(alerts(include_str!("../tests/fixtures/alerts-empty.html")).is_empty());
        assert!(alerts(include_str!("../tests/fixtures/latest-alerts.html")).is_empty());
        let html = r#"<ul class="ab_items">
            <li><img src="https://cdn.scribblehub.com/a.png"><div class="ab_type"><a href="https://www.scribblehub.com/profile/1/x/">Xero</a> followed you</div><div class="ab_time">2 hours ago</div></li>
            <li><div class="ab_type">Chapter 5 of <a href="https://www.scribblehub.com/read/fic/chapter/5/">Fic</a> is out</div><div class="ab_time">Jan 3, 2026</div></li>
            </ul>"#;
        let a = alerts(html);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].text, "Xero followed you");
        assert_eq!(a[0].url.as_deref(), Some("https://www.scribblehub.com/profile/1/x/"));
        assert_eq!(a[0].time, "2 hours ago");
        assert!(a[0].icon.is_some() && a[1].icon.is_none());
        assert_eq!(a[1].text, "Chapter 5 of Fic is out");
        let b = alerts(r#"<div><a href="https://www.scribblehub.com/series/1/x/">Some series</a></div>"#);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].text, "Some series");
    }

    #[test]
    fn parses_real_pages() {
        let h = home(include_str!("../tests/fixtures/home.html"));
        let names: Vec<_> = h.shelves.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Trending", "Latest Series"]);
        assert_eq!(h.shelves[0].view_more.as_deref(), Some("https://www.scribblehub.com/series-ranking/?sort=5&order=1"));
        assert_eq!(h.shelves[1].view_more.as_deref(), Some("https://www.scribblehub.com/latest-series/"));
        let first = &h.shelves[0].cards[0];
        assert!(first.url.contains("/series/") && first.cover.starts_with("https://"));
        assert_eq!(first.rank.as_deref(), Some("1"));
        assert!(h.updates.len() > 5 && h.updates[0].chapter_url.contains("/chapter/"));
        assert_eq!(h.pages.iter().filter(|p| p.current).count(), 1);
        assert!(h.pages.iter().any(|p| p.label == "…" && p.url.is_none()));
        let next = h.pages.iter().find(|p| p.label == "»").unwrap();
        assert_eq!(next.url.as_deref(), Some("https://www.scribblehub.com/?pg=2"));
        assert!(h.pages.iter().any(|p| p.label == "25" && !p.current));

        let s = series(include_str!("../tests/fixtures/series.html"));
        assert_eq!((s.id.as_str(), s.title.as_str(), s.author.as_str()), ("664073", "Rebirth of the Nephilim", "Agdistis"));
        assert_eq!(s.rating, 4.5);
        assert!(s.rating_label.contains("4.8") && s.stats.len() >= 4 && !s.genres.is_empty());
        assert!(s.synopsis[0].starts_with("Jadis has no idea"));
        assert!(s.first_chapter.unwrap().ends_with("/chapter/664075/"));

        let c = chapter(include_str!("../tests/fixtures/chapter.html"));
        assert_eq!(c.title, "Chapter 1: Meeting D");
        assert_eq!(c.series_title, "Rebirth of the Nephilim");
        assert!(c.prev.is_none() && c.next.unwrap().ends_with("/chapter/665605/"));
        assert!(c.blocks.len() > 20);
        assert!(matches!(&c.blocks[0], Block::Text(t, _) if t.starts_with("“Woah, there")));
    }

    #[test]
    fn search_results() {
        let hits = search(include_str!("../tests/fixtures/search.html"));
        assert_eq!(hits.len(), 8);
        assert_eq!(hits[0].title, "Rebirth of the Nephilim");
        assert!(hits[0].url.contains("/series/664073/") && hits[0].cover.starts_with("https://"));
        assert!(hits[0].stats.iter().any(|s| s.contains("Views")) && !hits[0].genres.is_empty());
        assert!(hits[0].blurb.starts_with("Jadis has no idea") && !hits[0].blurb.contains("more>>") && !hits[1].blurb.contains("<<less"));
        assert_eq!(search_url("rebirth & sons"), format!("{BASE}series-finder/?sf=1&sh=rebirth%20%26%20sons"));
    }

    #[test]
    fn live_suggestions() {
        let s = suggestions(include_str!("../tests/fixtures/suggest.html"));
        assert_eq!(s.len(), 10);
        assert_eq!(s[0].title, "Ante");
        assert_eq!(&s[0].title[s[0].marks[0].clone()], "te");
        assert_eq!(&s[3].title[s[3].marks[0].clone()], "TE");
        assert!(s[0].url.ends_with("/series/1851355/ante/") && s[0].cover.starts_with("https://"));
    }

    #[test]
    fn emphasis_and_toc() {
        let c = chapter(r#"<div id="chp_raw"><p>She <em>really</em> meant <b>it</b>.<br>Next</p><hr><p>&nbsp;</p></div>"#);
        let Block::Text(t, runs) = &c.blocks[0] else { panic!() };
        assert_eq!(t, "She really meant it.\nNext");
        assert_eq!(&t[runs[0].0.clone()], "really");
        assert!(runs[0].1.italic && runs[1].1.bold);
        assert_eq!(c.blocks.len(), 2);

        let t = toc(r#"<ol><li class="li_toc"><a href="https://x/2">Two</a></li><li class="li_toc"><a href="https://x/1">One</a></li></ol>"#);
        assert_eq!(t[0].title, "One");
    }

    #[test]
    fn dashboard_page() {
        let w = dashboard(include_str!("../tests/fixtures/dashboard.html")).expect("dashboard");
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].id, "804522");
        assert_eq!(w[0].title, "Anectodes of the meticulous gamer in another world");
        assert!(w[0].url.contains("/series/804522/") && w[0].cover.starts_with("https://"));
        assert_eq!(w[1].rating, "4.2");
        assert_eq!(w[1].stats[0], ("40771".to_string(), "Views".to_string()));
        assert_eq!(w[1].stats.len(), 7);
        assert_eq!(w[1].updated, "Dec 15, 2020");
        assert!(w[1].genres.contains(&"Martial Arts".to_string()));
        assert_eq!(w[1].blurb, "Countless people chase power, fame or to be rich.");
        assert!(dashboard(include_str!("../tests/fixtures/home.html")).is_none());
    }

    #[test]
    fn series_forms() {
        let e = series_form(include_str!("../tests/fixtures/editseries.html")).expect("edit form");
        assert_eq!(e.sid.as_deref(), Some("141971"));
        assert_eq!(e.title, "Rebirth of the Tyrannical Sword Saint");
        assert!(e.description.starts_with("Countless people chase power"));
        let on: Vec<_> = e.genres.iter().filter(|g| g.on).map(|g| g.name.as_str()).collect();
        assert_eq!(on, ["Action", "Adventure", "Fantasy", "Gender Bender", "Martial Arts", "School Life"]);
        assert_eq!(e.main_genre, "8");
        assert_eq!(e.picked_tags.len(), 16);
        assert_eq!(e.picked_tags[0], "124");
        assert!(e.tags.len() > 300 && e.tags.iter().any(|(id, n)| id == "124" && n == "Accelerated Growth"));
        assert_eq!(e.copyright.iter().find(|c| c.on).map(|c| c.name.as_str()), Some("All Rights Reserved"));
        assert!(!e.mature);
        assert!(e.warnings.iter().any(|w| w.id == "48" && w.name == "Gore" && !w.on));
        assert_eq!(e.status.iter().find(|c| c.on).map(|c| c.id.as_str()), Some("hiatus"));

        let a = series_form(include_str!("../tests/fixtures/add-series.html")).expect("add form");
        assert!(a.title.is_empty() && a.picked_tags.is_empty() && a.status.is_empty());
        assert!(a.genres.len() > 25 && a.genres.iter().all(|g| !g.on && !g.name.is_empty()));
        assert!(a.genres.iter().any(|g| g.id == "38" && g.name == "Fanfiction"));
        assert!(series_form(include_str!("../tests/fixtures/home.html")).is_none());
    }

    #[test]
    fn stats_json() {
        let c = stats_chart(r#"{"date":"\"Nov 10\",\"Nov 11\",\"Nov 12\"","count":"3,0,12","color":"","text":"Views"}0"#);
        assert_eq!(c, [("Nov 10".to_string(), 3.), ("Nov 11".to_string(), 0.), ("Nov 12".to_string(), 12.)]);
        let (sh, web) = referrers(r#"{"sh":"<tr><th>Referrer</th><th>Views</th></tr><tr><td>(empty)</td><td>5</td></tr>","web":"<tr><td>https://www.google.com/</td><td>1</td></tr>","show":"no"}0"#);
        assert_eq!(sh, [("(empty)".to_string(), "5".to_string())]);
        assert_eq!(web.len(), 1);
    }
}
