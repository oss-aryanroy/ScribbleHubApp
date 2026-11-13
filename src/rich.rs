use crate::input;
use crate::{Pal, SERIF, THEMES};
use gpui::{prelude::*, *};
use std::ops::Range;

pub const OBJ: char = '\u{FFFC}';
const OBJ_S: &str = "\u{FFFC}";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mark {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub sup: bool,
    pub sub: bool,
    pub color: Option<u32>,
    pub bg: Option<u32>,
    pub size: Option<SharedString>,
    pub link: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Flag {
    Bold,
    Italic,
    Underline,
    Strike,
    Sup,
}

impl Mark {
    pub fn get(&self, f: Flag) -> bool {
        match f {
            Flag::Bold => self.bold,
            Flag::Italic => self.italic,
            Flag::Underline => self.underline,
            Flag::Strike => self.strike,
            Flag::Sup => self.sup,
        }
    }

    fn set(&mut self, f: Flag, on: bool) {
        match f {
            Flag::Bold => self.bold = on,
            Flag::Italic => self.italic = on,
            Flag::Underline => self.underline = on,
            Flag::Strike => self.strike = on,
            Flag::Sup => {
                self.sup = on;
                self.sub &= !on;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    P,
    H(u8),
    Quote,
    Bullet,
    Number,
    Rule,
    Image { src: SharedString, html: Option<SharedString> },
    Raw(SharedString),
}

impl Kind {
    pub fn atom(&self) -> bool {
        matches!(self, Kind::Rule | Kind::Image { .. } | Kind::Raw(_))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Para {
    pub kind: Kind,
    pub align: Align,
}

impl Default for Kind {
    fn default() -> Self {
        Kind::P
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Doc {
    pub text: String,
    runs: Vec<(usize, Mark)>,
    pub paras: Vec<Para>,
}

impl Default for Doc {
    fn default() -> Self {
        Doc { text: String::new(), runs: vec![], paras: vec![Para::default()] }
    }
}

impl Doc {
    pub fn lines(&self) -> Vec<Range<usize>> {
        let mut out = vec![];
        let mut s = 0;
        for (i, _) in self.text.match_indices('\n') {
            out.push(s..i);
            s = i + 1;
        }
        out.push(s..self.text.len());
        out
    }

    pub fn line_of(&self, at: usize) -> usize {
        self.text.as_bytes()[..at].iter().filter(|b| **b == b'\n').count()
    }

    pub fn line_start(&self, at: usize) -> usize {
        self.text[..at].rfind('\n').map_or(0, |i| i + 1)
    }

    pub fn line_end(&self, at: usize) -> usize {
        self.text[at..].find('\n').map_or(self.text.len(), |i| at + i)
    }

    pub fn is_blank(&self) -> bool {
        self.text.chars().all(char::is_whitespace)
    }

    pub fn mark_of(&self, at: usize) -> Mark {
        let mut o = 0;
        for (len, m) in &self.runs {
            if at < o + len {
                return m.clone();
            }
            o += len;
        }
        Mark::default()
    }

    pub fn mark_at(&self, at: usize) -> Mark {
        let (ls, le) = (self.line_start(at), self.line_end(at));
        let mut m = if at > ls {
            let prev = self.text[..at].char_indices().next_back().map_or(0, |(i, _)| i);
            let m = self.mark_of(prev);
            if at < le && self.mark_of(at).link == m.link {
                return m;
            }
            m
        } else if at < le {
            self.mark_of(at)
        } else {
            Mark::default()
        };
        m.link = None;
        m
    }

    pub fn spans(&self, r: Range<usize>) -> Vec<(&str, &Mark)> {
        let mut out = vec![];
        let mut o = 0;
        for (len, m) in &self.runs {
            let (s, e) = (o.max(r.start), (o + len).min(r.end));
            o += len;
            if s < e {
                out.push((&self.text[s..e], m));
            }
        }
        out
    }

    fn split(&mut self, at: usize) -> usize {
        let mut o = 0;
        for i in 0..self.runs.len() {
            let len = self.runs[i].0;
            if at == o {
                return i;
            }
            if at < o + len {
                let m = self.runs[i].1.clone();
                self.runs[i].0 = at - o;
                self.runs.insert(i + 1, (o + len - at, m));
                return i + 1;
            }
            o += len;
        }
        self.runs.len()
    }

    pub fn style(&mut self, r: Range<usize>, f: impl Fn(&mut Mark)) {
        if r.is_empty() {
            return;
        }
        let (a, b) = (self.split(r.start), self.split(r.end));
        for (_, m) in &mut self.runs[a..b] {
            f(m);
        }
        self.tidy();
    }

    pub fn all(&self, r: Range<usize>, f: impl Fn(&Mark) -> bool) -> bool {
        let spans = self.spans(r);
        let text: Vec<_> = spans.into_iter().filter(|(t, _)| t.chars().any(|c| c != '\n')).collect();
        !text.is_empty() && text.iter().all(|(_, m)| f(m))
    }

    pub fn replace(&mut self, r: Range<usize>, ins: &str, mark: &Mark) {
        let l0 = self.line_of(r.start);
        let k = self.text[r.clone()].matches('\n').count();
        if k > 0 {
            if r.start == self.line_start(r.start) {
                self.paras.drain(l0..l0 + k);
            } else {
                self.paras.drain(l0 + 1..=l0 + k);
            }
        }
        let (a, b) = (self.split(r.start), self.split(r.end));
        self.runs.drain(a..b);
        self.text.replace_range(r.clone(), "");
        let at = r.start;
        let m = ins.matches('\n').count();
        if m > 0 {
            let base = self.paras[l0].clone();
            let (ls, le) = (self.line_start(at), self.line_end(at));
            let plain = Para { kind: Kind::P, align: if base.kind.atom() { Align::Left } else { base.align } };
            if at == ls && le > ls {
                let p = if base.kind.atom() { plain } else { base };
                self.paras.splice(l0..l0, std::iter::repeat_n(p, m));
            } else {
                let p = if base.kind.atom() || (matches!(base.kind, Kind::H(_)) && at == le) { plain } else { base };
                self.paras.splice(l0 + 1..l0 + 1, std::iter::repeat_n(p, m));
            }
        }
        if !ins.is_empty() {
            let i = self.split(at);
            self.runs.insert(i, (ins.len(), mark.clone()));
            self.text.insert_str(at, ins);
        }
        self.tidy();
    }

    fn tidy(&mut self) {
        let mut out: Vec<(usize, Mark)> = Vec::with_capacity(self.runs.len());
        for (len, m) in self.runs.drain(..).filter(|r| r.0 > 0) {
            match out.last_mut() {
                Some(l) if l.1 == m => l.0 += len,
                _ => out.push((len, m)),
            }
        }
        self.runs = out;
        for (p, l) in self.paras.iter_mut().zip(self.text.split('\n')) {
            if p.kind.atom() && l != OBJ_S {
                p.kind = Kind::P;
            }
        }
        debug_assert_eq!(self.paras.len(), self.text.matches('\n').count() + 1);
    }

    pub fn slice(&self, r: Range<usize>) -> Doc {
        let mut d = self.clone();
        d.replace(r.end..self.text.len(), "", &Mark::default());
        d.replace(0..r.start, "", &Mark::default());
        d
    }

    pub fn insert(&mut self, at: usize, d: &Doc) {
        let l0 = self.line_of(at);
        let (ls, le) = (self.line_start(at), self.line_end(at));
        let before = at == ls && le > ls;
        self.replace(at..at, &d.text, &Mark::default());
        let mut o = at;
        for (len, m) in &d.runs {
            self.style(o..o + len, |x| *x = m.clone());
            o += len;
        }
        let m = d.paras.len() - 1;
        if before {
            for i in 0..m {
                self.paras[l0 + i] = d.paras[i].clone();
            }
        } else {
            for i in (if ls == le { 0 } else { 1 })..=m {
                self.paras[l0 + i] = d.paras[i].clone();
            }
        }
        self.tidy();
    }

    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count()
    }
}

fn esc(s: &str) -> String {
    s.chars()
        .filter(|c| *c != OBJ)
        .collect::<String>()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace("  ", " \u{a0}")
        .replace('\u{a0}', "&nbsp;")
}

fn run_html(t: &str, m: &Mark) -> String {
    let mut s = esc(t);
    let mut css = vec![];
    if let Some(c) = m.color {
        css.push(format!("color: #{c:06x};"));
    }
    if let Some(c) = m.bg {
        css.push(format!("background-color: #{c:06x};"));
    }
    if let Some(z) = &m.size {
        css.push(format!("font-size: {z};"));
    }
    match (m.underline, m.strike) {
        (true, true) => css.push("text-decoration: underline line-through;".into()),
        (true, false) => css.push("text-decoration: underline;".into()),
        (false, true) => css.push("text-decoration: line-through;".into()),
        _ => {}
    }
    if !css.is_empty() {
        s = format!("<span style=\"{}\">{s}</span>", css.join(" "));
    }
    if m.sup {
        s = format!("<sup>{s}</sup>");
    }
    if m.sub {
        s = format!("<sub>{s}</sub>");
    }
    if m.italic {
        s = format!("<em>{s}</em>");
    }
    if m.bold {
        s = format!("<strong>{s}</strong>");
    }
    s
}

impl Doc {
    fn inline_html(&self, r: Range<usize>) -> String {
        let spans = self.spans(r);
        let mut out = String::new();
        let mut i = 0;
        while i < spans.len() {
            let link = spans[i].1.link.clone();
            let mut inner = String::new();
            while i < spans.len() && spans[i].1.link == link {
                inner.push_str(&run_html(spans[i].0, spans[i].1));
                i += 1;
            }
            match link {
                Some(h) => out.push_str(&format!("<a href=\"{}\">{inner}</a>", esc(&h))),
                None => out.push_str(&inner),
            }
        }
        if out.is_empty() { "&nbsp;".into() } else { out }
    }

    pub fn to_html(&self) -> String {
        if self.is_blank() {
            return String::new();
        }
        let lines = self.lines();
        let style = |p: &Para| match p.align {
            Align::Left => "",
            Align::Center => " style=\"text-align: center;\"",
            Align::Right => " style=\"text-align: right;\"",
            Align::Justify => " style=\"text-align: justify;\"",
        };
        let mut out: Vec<String> = vec![];
        let mut i = 0;
        while i < self.paras.len() {
            let p = &self.paras[i];
            let group = match p.kind {
                Kind::Bullet => Some(("<ul>", "</ul>", "li")),
                Kind::Number => Some(("<ol>", "</ol>", "li")),
                Kind::Quote => Some(("<blockquote>", "</blockquote>", "p")),
                _ => None,
            };
            if let Some((open, close, tag)) = group {
                let mut s = open.to_string();
                while i < self.paras.len() && self.paras[i].kind == p.kind {
                    s.push_str(&format!("<{tag}{}>{}</{tag}>", style(&self.paras[i]), self.inline_html(lines[i].clone())));
                    i += 1;
                }
                out.push(s + close);
                continue;
            }
            out.push(match &p.kind {
                Kind::H(n) => format!("<h{n}{}>{}</h{n}>", style(p), self.inline_html(lines[i].clone())),
                Kind::Rule => "<hr>".into(),
                Kind::Image { src, html } => format!("<p{}>{}</p>", style(p), html.clone().unwrap_or_else(|| format!("<img src=\"{}\" alt=\"\">", esc(src)).into())),
                Kind::Raw(h) => h.to_string(),
                _ => format!("<p{}>{}</p>", style(p), self.inline_html(lines[i].clone())),
            });
            i += 1;
        }
        out.join("\n")
    }
}

pub fn parse_color(v: &str) -> Option<u32> {
    let v = v.trim().to_lowercase().replace(' ', "");
    if let Some(h) = v.strip_prefix('#') {
        return match h.len() {
            3 => u32::from_str_radix(&h.chars().flat_map(|c| [c, c]).collect::<String>(), 16).ok(),
            6 => u32::from_str_radix(h, 16).ok(),
            _ => None,
        };
    }
    if let Some(inner) = v.strip_prefix("rgba(").or(v.strip_prefix("rgb(")).and_then(|s| s.strip_suffix(')')) {
        let n: Vec<u32> = inner.split(',').take(3).filter_map(|x| x.parse::<f32>().ok()).map(|x| x.clamp(0., 255.) as u32).collect();
        return (n.len() == 3).then(|| n[0] << 16 | n[1] << 8 | n[2]);
    }
    Some(match v.as_str() {
        "black" => 0x000000,
        "white" => 0xffffff,
        "red" => 0xff0000,
        "green" => 0x008000,
        "blue" => 0x0000ff,
        "yellow" => 0xffff00,
        "orange" => 0xffa500,
        "purple" => 0x800080,
        "gray" | "grey" => 0x808080,
        _ => return None,
    })
}

fn apply_style(m: &mut Mark, style: &str, lost: &mut Vec<&'static str>) {
    let mut lose = |w| {
        if !lost.contains(&w) {
            lost.push(w)
        }
    };
    for (k, v) in crate::markup::decls(style) {
        let inherit = matches!(v.as_str(), "inherit" | "initial" | "unset" | "currentcolor" | "windowtext");
        match k.as_str() {
            "font-weight" => m.bold = v == "bold" || v == "bolder" || v.parse::<u32>().is_ok_and(|w| w >= 600),
            "font-style" => m.italic = v == "italic" || v == "oblique",
            "text-decoration" | "text-decoration-line" => {
                m.underline = v.contains("underline");
                m.strike = v.contains("line-through");
            }
            "color" if inherit => {}
            "color" => match parse_color(&v) {
                Some(0) => m.color = None,
                Some(c) => m.color = Some(c),
                None => lose("text colours"),
            },
            "background-color" | "background" if inherit || matches!(v.as_str(), "transparent" | "none") => m.bg = None,
            "background-color" | "background" => match parse_color(&v) {
                Some(0xffffff) => m.bg = None,
                Some(c) => m.bg = Some(c),
                None => lose("text colours"),
            },
            "font-size" if inherit || v == "medium" => m.size = None,
            "font-size" => m.size = Some(v.into()),
            "vertical-align" => match v.as_str() {
                "super" => m.sup = true,
                "sub" => m.sub = true,
                "baseline" => {}
                _ => lose("other styling"),
            },
            "text-align" | "white-space" | "font-family" | "line-height" | "direction" | "text-indent" | "letter-spacing" | "word-spacing" | "font-variant" | "display" | "unicode-bidi" => {}
            p if p.starts_with("margin") || p.starts_with("padding") || p.starts_with("font-variant") || p.starts_with("-webkit") || p.starts_with("mso-") => {}
            _ => lose("other styling"),
        }
    }
}

fn align_of(e: &scraper::node::Element, inherited: Align) -> Align {
    let v = crate::markup::decls(e.attr("style").unwrap_or_default()).into_iter().rev().find(|(k, _)| k == "text-align").map(|(_, v)| v).or(e.attr("align").map(str::to_lowercase));
    match v.as_deref() {
        Some("center") => Align::Center,
        Some("right" | "end") => Align::Right,
        Some("justify") => Align::Justify,
        Some("left" | "start") => Align::Left,
        _ => inherited,
    }
}

enum Item {
    Text(String, Mark),
    Break,
    Edge,
    Block(Kind),
}

#[derive(Default)]
struct Reader {
    lines: Vec<(Para, Vec<(String, Mark)>)>,
    lost: Vec<&'static str>,
}

const INLINE: [&str; 33] = [
    "a", "abbr", "b", "bdi", "bdo", "big", "br", "cite", "code", "data", "del", "dfn", "em", "font", "i", "img", "ins", "kbd", "label", "mark", "q", "s", "samp", "small", "span", "strike", "strong", "sub", "sup", "time", "tt", "u", "var",
];

impl Reader {
    fn lose(&mut self, w: &'static str) {
        if !self.lost.contains(&w) {
            self.lost.push(w);
        }
    }

    fn push(&mut self, p: Para, runs: Vec<(String, Mark)>) {
        let mut out: Vec<(String, Mark)> = vec![];
        let mut space = true;
        for (t, m) in runs {
            let mut s = String::new();
            for c in t.chars() {
                if c.is_ascii_whitespace() {
                    if !space {
                        s.push(' ');
                    }
                    space = true;
                } else {
                    s.push(c);
                    space = false;
                }
            }
            out.push((s, m));
        }
        if out.iter().all(|(t, _)| t.chars().all(char::is_whitespace)) {
            out.clear();
        }
        while let Some(last) = out.last_mut() {
            let trimmed = last.0.trim_end_matches(' ').len();
            last.0.truncate(trimmed);
            if last.0.is_empty() {
                out.pop();
            } else {
                break;
            }
        }
        self.lines.push((p, out));
    }

    fn flush(&mut self, mut items: Vec<Item>, p: Para) {
        let blank = |i: &Item| matches!(i, Item::Text(t, _) if t.chars().all(|c| c.is_ascii_whitespace()));
        while items.last().is_some_and(|i| blank(i) || matches!(i, Item::Edge)) {
            items.pop();
        }
        if matches!(items.last(), Some(Item::Break)) {
            items.pop();
        }
        let mut cur: Vec<(String, Mark)> = vec![];
        let mut after_block = false;
        for it in items {
            match it {
                Item::Text(t, m) => {
                    after_block &= t.chars().all(|c| c.is_ascii_whitespace());
                    cur.push((t, m));
                }
                Item::Break => {
                    after_block = false;
                    self.push(p.clone(), std::mem::take(&mut cur));
                }
                Item::Edge => {
                    if cur.iter().any(|(t, _)| !t.trim().is_empty()) {
                        after_block = false;
                        self.push(p.clone(), std::mem::take(&mut cur));
                    }
                    cur.clear();
                }
                Item::Block(kind) => {
                    if cur.iter().any(|(t, _)| !t.trim().is_empty()) {
                        self.push(p.clone(), std::mem::take(&mut cur));
                    }
                    cur.clear();
                    let align = if matches!(kind, Kind::Image { .. }) { p.align } else { Align::Left };
                    self.lines.push((Para { kind, align }, vec![(OBJ_S.into(), Mark::default())]));
                    after_block = true;
                }
            }
        }
        if !after_block {
            self.push(p, cur);
        }
    }

    fn inline(&mut self, el: scraper::ElementRef, m: &Mark, items: &mut Vec<Item>) {
        use scraper::{ElementRef, Node};
        for child in el.children() {
            match child.value() {
                Node::Text(t) => items.push(Item::Text(t.to_string(), m.clone())),
                Node::Element(_) => self.inline_one(ElementRef::wrap(child).unwrap(), m, items),
                _ => {}
            }
        }
    }

    fn inline_one(&mut self, el: scraper::ElementRef, m: &Mark, items: &mut Vec<Item>) {
        let e = el.value();
        let mut m = m.clone();
        match e.name() {
            "br" => return items.push(Item::Break),
            "img" => {
                return items.push(Item::Block(Kind::Image { src: e.attr("src").unwrap_or_default().to_string().into(), html: Some(el.html().into()) }));
            }
            "script" | "style" => return,
            "strong" | "b" => m.bold = true,
            "em" | "i" | "cite" | "dfn" | "var" => m.italic = true,
            "u" | "ins" => m.underline = true,
            "del" | "s" | "strike" => m.strike = true,
            "sup" => m.sup = true,
            "sub" => m.sub = true,
            "mark" => m.bg = Some(0xffff00),
            "a" => match e.attr("href") {
                Some(h) => m.link = Some(h.to_string().into()),
                None if e.attr("name").is_some() || e.attr("id").is_some() => self.lose("links"),
                None => {}
            },
            "font" => {
                if let Some(c) = e.attr("color").and_then(parse_color) {
                    m.color = (c != 0).then_some(c);
                }
                if e.attr("size").is_some() || e.attr("face").is_some() {
                    self.lose("other styling");
                }
            }
            "span" | "abbr" | "bdi" | "bdo" | "big" | "code" | "data" | "kbd" | "label" | "q" | "samp" | "small" | "time" | "tt" => {}
            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "blockquote" => {
                items.push(Item::Edge);
                apply_style(&mut m, e.attr("style").unwrap_or_default(), &mut self.lost);
                self.inline(el, &m, items);
                return items.push(Item::Edge);
            }
            "ul" | "ol" => {
                self.lose("nested lists");
                for li in el.children().filter_map(scraper::ElementRef::wrap) {
                    items.push(Item::Edge);
                    self.inline(li, &m, items);
                }
                return items.push(Item::Edge);
            }
            "table" | "figure" | "iframe" | "video" | "audio" | "embed" | "object" | "pre" | "dl" | "svg" | "canvas" => return items.push(Item::Block(Kind::Raw(el.html().into()))),
            _ => self.lose("other formatting"),
        }
        apply_style(&mut m, e.attr("style").unwrap_or_default(), &mut self.lost);
        self.inline(el, &m, items);
    }

    fn blocks(&mut self, el: scraper::ElementRef, base: Para) {
        use scraper::{ElementRef, Node};
        let mut loose: Vec<Item> = vec![];
        for child in el.children() {
            let Node::Element(e) = child.value() else {
                if let Node::Text(t) = child.value() {
                    loose.push(Item::Text(t.to_string(), Mark::default()));
                }
                continue;
            };
            let c = ElementRef::wrap(child).unwrap();
            let name = e.name();
            if INLINE.contains(&name) {
                self.inline_one(c, &Mark::default(), &mut loose);
                continue;
            }
            if loose.iter().any(|i| !matches!(i, Item::Text(t, _) if t.trim().is_empty())) {
                self.flush(std::mem::take(&mut loose), base.clone());
            }
            loose.clear();
            let align = align_of(e, base.align);
            let start = |r: &mut Self| {
                let mut m = Mark::default();
                apply_style(&mut m, e.attr("style").unwrap_or_default(), &mut r.lost);
                m
            };
            match name {
                "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "address" => {
                    let kind = match name.strip_prefix('h').and_then(|n| n.parse::<u8>().ok()) {
                        Some(n) => Kind::H(n),
                        None => base.kind.clone(),
                    };
                    let m = start(self);
                    let mut items = vec![];
                    self.inline(c, &m, &mut items);
                    self.flush(items, Para { kind, align });
                }
                "div" if e.attr("class").is_some_and(|c| !c.trim().is_empty()) => self.lines.push((Para { kind: Kind::Raw(c.html().into()), align: Align::Left }, vec![(OBJ_S.into(), Mark::default())])),
                "div" | "section" | "article" | "main" | "header" | "footer" | "center" | "body" | "html" => {
                    let align = if name == "center" { Align::Center } else { align };
                    self.blocks(c, Para { align, ..base.clone() })
                }
                "blockquote" => self.blocks(c, Para { kind: Kind::Quote, align }),
                "ul" | "ol" => {
                    let kind = if name == "ul" { Kind::Bullet } else { Kind::Number };
                    for li in c.children().filter_map(ElementRef::wrap) {
                        let m = Mark::default();
                        let mut items = vec![];
                        if li.value().name() == "li" {
                            self.inline(li, &m, &mut items);
                        } else {
                            self.inline_one(li, &m, &mut items);
                        }
                        self.flush(items, Para { kind: kind.clone(), align: align_of(li.value(), align) });
                    }
                }
                "hr" => self.lines.push((Para { kind: Kind::Rule, align: Align::Left }, vec![(OBJ_S.into(), Mark::default())])),
                "script" | "style" | "meta" | "link" | "title" | "head" => {}
                _ => self.lines.push((Para { kind: Kind::Raw(c.html().into()), align: Align::Left }, vec![(OBJ_S.into(), Mark::default())])),
            }
        }
        if loose.iter().any(|i| !matches!(i, Item::Text(t, _) if t.trim().is_empty())) {
            self.flush(loose, base);
        }
    }
}

impl Doc {
    pub fn from_html(html: &str) -> (Doc, Vec<&'static str>) {
        let frag = scraper::Html::parse_fragment(html);
        let mut r = Reader::default();
        r.blocks(frag.root_element(), Para::default());
        if r.lines.is_empty() {
            return (Doc::default(), r.lost);
        }
        let mut d = Doc { text: String::new(), runs: vec![], paras: vec![] };
        for (i, (p, runs)) in r.lines.into_iter().enumerate() {
            if i > 0 {
                d.text.push('\n');
                d.runs.push((1, Mark::default()));
            }
            d.paras.push(p);
            for (t, m) in runs {
                d.text.push_str(&t);
                d.runs.push((t.len(), m));
            }
        }
        d.tidy();
        (d, r.lost)
    }
}

actions!(
    rich_editor,
    [ToggleBold, ToggleItalic, ToggleUnderline, FindInChapter, FocusMode, Undo, Redo, SelectUp, SelectDown, WordLeft, WordRight, SelectWordLeft, SelectWordRight, DocStart, DocEnd, SelectHome, SelectEnd, DeleteWordLeft]
);

pub fn bind_keys(cx: &mut App) {
    let c = Some("RichEditor");
    cx.bind_keys([
        KeyBinding::new("secondary-b", ToggleBold, c),
        KeyBinding::new("secondary-f", FindInChapter, c),
        KeyBinding::new("secondary-shift-f", FocusMode, c),
        KeyBinding::new("secondary-i", ToggleItalic, c),
        KeyBinding::new("secondary-u", ToggleUnderline, c),
        KeyBinding::new("secondary-z", Undo, c),
        KeyBinding::new("secondary-shift-z", Redo, c),
        KeyBinding::new("shift-up", SelectUp, c),
        KeyBinding::new("shift-down", SelectDown, c),
        KeyBinding::new("alt-left", WordLeft, c),
        KeyBinding::new("alt-right", WordRight, c),
        KeyBinding::new("alt-shift-left", SelectWordLeft, c),
        KeyBinding::new("alt-shift-right", SelectWordRight, c),
        KeyBinding::new("cmd-up", DocStart, c),
        KeyBinding::new("cmd-down", DocEnd, c),
        KeyBinding::new("shift-home", SelectHome, c),
        KeyBinding::new("cmd-shift-left", SelectHome, c),
        KeyBinding::new("shift-end", SelectEnd, c),
        KeyBinding::new("cmd-shift-right", SelectEnd, c),
        KeyBinding::new("alt-backspace", DeleteWordLeft, c),
    ]);
    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        KeyBinding::new("ctrl-left", WordLeft, c),
        KeyBinding::new("ctrl-right", WordRight, c),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, c),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, c),
        KeyBinding::new("ctrl-home", DocStart, c),
        KeyBinding::new("ctrl-end", DocEnd, c),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, c),
        KeyBinding::new("ctrl-y", Redo, c),
    ]);
}

pub enum RichEvent {
    Changed,
    Reveal(Pixels, Pixels),
    Escape,
    Find,
    ToggleFocus,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hl {
    Under(u32),
    Back(u32),
}

impl EventEmitter<RichEvent> for RichEditor {}

#[derive(Clone, Default)]
struct Lay {
    start: usize,
    len: usize,
    bounds: Bounds<Pixels>,
    line: Option<WrappedLine>,
    lh: Pixels,
    align: Align,
}

impl Lay {
    fn rows(&self) -> Vec<(usize, usize, Pixels, Pixels)> {
        let Some(line) = &self.line else { return vec![(0, 0, px(0.), px(0.))] };
        let ul = &line.unwrapped_layout;
        let mut out = vec![];
        let (mut si, mut sx) = (0, px(0.));
        for b in line.wrap_boundaries() {
            let g = &ul.runs[b.run_ix].glyphs[b.glyph_ix];
            out.push((si, g.index, sx, g.position.x));
            (si, sx) = (g.index, g.position.x);
        }
        out.push((si, ul.len, sx, ul.width));
        out
    }

    fn shift(&self, row_w: Pixels) -> Pixels {
        match self.align {
            Align::Center => (self.bounds.size.width - row_w) / 2.,
            Align::Right => self.bounds.size.width - row_w,
            _ => px(0.),
        }
    }

    fn atom(&self) -> bool {
        self.line.is_none() && self.len > 0
    }

    fn point(&self, at: usize) -> Point<Pixels> {
        let o = self.bounds.origin;
        if self.atom() {
            return if at <= self.start { o } else { point(self.bounds.right() - px(2.), o.y) };
        }
        let i = at.saturating_sub(self.start).min(self.len);
        let rows = self.rows();
        let (k, &(_, _, sx, ex)) = rows.iter().enumerate().find(|(_, r)| i <= r.1).unwrap_or((rows.len() - 1, rows.last().unwrap()));
        let x = self.line.as_ref().map_or(px(0.), |l| l.unwrapped_layout.x_for_index(i)) - sx + self.shift(ex - sx);
        point(o.x + x, o.y + self.lh * k as f32)
    }

    fn index(&self, pos: Point<Pixels>) -> usize {
        if self.atom() {
            return if pos.x < self.bounds.center().x { self.start } else { self.start + self.len };
        }
        let Some(line) = &self.line else { return self.start };
        let rows = self.rows();
        let k = (((pos.y - self.bounds.top()) / self.lh).floor().max(0.) as usize).min(rows.len() - 1);
        let (si, ei, sx, ex) = rows[k];
        let x = pos.x - self.bounds.left() - self.shift(ex - sx) + sx;
        self.start + line.unwrapped_layout.closest_index_for_x(x).clamp(si, ei)
    }

    fn row_of(&self, at: usize) -> (usize, usize) {
        if self.line.is_none() {
            return (self.start, self.start + self.len);
        }
        let i = at.saturating_sub(self.start).min(self.len);
        let rows = self.rows();
        let r = rows.iter().find(|r| i <= r.1).unwrap_or(rows.last().unwrap());
        (self.start + r.0, self.start + r.1)
    }
}

pub struct RichEditor {
    pub focus: FocusHandle,
    doc: Doc,
    placeholder: SharedString,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    selecting: bool,
    pending: Option<Mark>,
    undo: Vec<(Doc, Range<usize>)>,
    redo: Vec<(Doc, Range<usize>)>,
    typing_at: Option<usize>,
    goal_x: Option<Pixels>,
    clip: Option<Doc>,
    lays: Vec<Lay>,
    reveal: bool,
    pub pal: Pal,
    pub width: f32,
    pub family: SharedString,
    pub size: f32,
    pub spacing: f32,
    pub spotlight: bool,
    pub hl: Vec<(Range<usize>, Hl)>,
}

impl RichEditor {
    pub fn new(placeholder: &'static str, cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle().tab_stop(true),
            doc: Doc::default(),
            placeholder: placeholder.into(),
            selected: 0..0,
            reversed: false,
            marked: None,
            selecting: false,
            pending: None,
            undo: vec![],
            redo: vec![],
            typing_at: None,
            goal_x: None,
            clip: None,
            lays: vec![],
            reveal: false,
            pal: THEMES[0],
            width: 600.,
            family: SERIF.into(),
            size: 17.,
            spacing: 1.65,
            spotlight: false,
            hl: vec![],
        }
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn html(&self) -> String {
        self.doc.to_html()
    }

    pub fn selection(&self) -> Range<usize> {
        self.selected.clone()
    }

    pub fn select(&mut self, r: Range<usize>, cx: &mut Context<Self>) {
        let r = r.start.min(self.doc.text.len())..r.end.min(self.doc.text.len());
        self.selected = r;
        self.reversed = false;
        self.after_move(cx);
    }

    pub fn replace_ranges(&mut self, ranges: &[Range<usize>], with: &str, cx: &mut Context<Self>) {
        if ranges.is_empty() {
            return;
        }
        let ranges = ranges.to_vec();
        let with = with.to_string();
        self.edit(false, cx, |s| {
            for r in ranges.iter().rev() {
                let m = s.doc.mark_of(r.start);
                s.doc.replace(r.clone(), &with, &m);
            }
            let end = ranges[0].start + with.len();
            s.selected = end..end;
        });
    }

    pub fn load_undoable(&mut self, html: &str, cx: &mut Context<Self>) {
        let (doc, _) = Doc::from_html(html);
        self.edit(false, cx, |s| {
            s.doc = doc;
            s.selected = 0..0;
        });
    }

    pub fn set_html(&mut self, html: &str, cx: &mut Context<Self>) -> Vec<&'static str> {
        let (doc, lost) = Doc::from_html(html);
        self.doc = doc;
        self.selected = 0..0;
        self.reset_history();
        cx.notify();
        lost
    }

    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
        self.marked = None;
        self.typing_at = None;
    }

    pub fn cursor(&self) -> usize {
        if self.reversed { self.selected.start } else { self.selected.end }
    }

    fn move_to(&mut self, at: usize, cx: &mut Context<Self>) {
        self.selected = at..at;
        self.reversed = false;
        self.after_move(cx);
    }

    fn select_to(&mut self, at: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = at
        } else {
            self.selected.end = at
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        self.after_move(cx);
    }

    fn after_move(&mut self, cx: &mut Context<Self>) {
        self.pending = None;
        self.goal_x = None;
        self.typing_at = None;
        self.reveal = true;
        cx.notify();
    }

    fn prev_boundary(&self, at: usize) -> usize {
        self.doc.text[..at].char_indices().next_back().map_or(0, |(i, _)| i)
    }

    fn next_boundary(&self, at: usize) -> usize {
        self.doc.text[at..].chars().next().map_or(at, |c| at + c.len_utf8())
    }

    fn word_left(&self, at: usize) -> usize {
        let t = &self.doc.text[..at];
        let t = t.trim_end_matches(|c: char| !c.is_alphanumeric() && c != '\n');
        let t = if t.len() == at && t.ends_with('\n') { &t[..t.len() - 1] } else { t };
        t.trim_end_matches(char::is_alphanumeric).len()
    }

    fn word_right(&self, at: usize) -> usize {
        let t = &self.doc.text[at..];
        let skip = if t.starts_with('\n') { 1 } else { 0 };
        let rest = t[skip..].trim_start_matches(|c: char| !c.is_alphanumeric() && c != '\n');
        let rest = rest.trim_start_matches(char::is_alphanumeric);
        self.doc.text.len() - rest.len()
    }

    fn around(&self, at: usize, para: bool) -> Range<usize> {
        if para {
            return self.doc.line_start(at)..self.doc.line_end(at);
        }
        let t = &self.doc.text;
        let s = t[..at].trim_end_matches(char::is_alphanumeric).len();
        let e = t.len() - t[at..].trim_start_matches(char::is_alphanumeric).len();
        s..e
    }

    fn lay_ix(&self, at: usize) -> Option<usize> {
        self.lays.iter().rposition(|l| l.start <= at)
    }

    fn index_at(&self, pos: Point<Pixels>) -> usize {
        let dist = |l: &Lay| {
            if pos.y < l.bounds.top() {
                l.bounds.top() - pos.y
            } else if pos.y >= l.bounds.bottom() {
                pos.y - l.bounds.bottom() + px(0.5)
            } else {
                px(0.)
            }
        };
        let Some(l) = self.lays.iter().min_by(|a, b| dist(a).partial_cmp(&dist(b)).unwrap_or(std::cmp::Ordering::Equal)) else { return 0 };
        let y = pos.y.clamp(l.bounds.top(), (l.bounds.bottom() - px(1.)).max(l.bounds.top()));
        l.index(point(pos.x, y)).min(self.doc.text.len())
    }

    fn caret(&self) -> Option<(Point<Pixels>, Pixels)> {
        let l = &self.lays[self.lay_ix(self.cursor())?];
        Some((l.point(self.cursor()), l.lh))
    }

    fn vertical(&mut self, dir: i32) -> usize {
        let c = self.cursor();
        let Some(k) = self.lay_ix(c) else { return c };
        let l = &self.lays[k];
        let p = l.point(c);
        let x = *self.goal_x.get_or_insert(p.x);
        let y = p.y + l.lh * (0.5 + dir as f32);
        if y >= l.bounds.top() && y < l.bounds.bottom() && !l.atom() {
            return l.index(point(x, y));
        }
        match (dir, self.lays.get((k as i32 + dir) as usize)) {
            (_, Some(n)) if k as i32 + dir >= 0 => n.index(point(x, if dir > 0 { n.bounds.top() + px(1.) } else { n.bounds.bottom() - px(1.) })),
            (d, _) if d < 0 => 0,
            _ => self.doc.text.len(),
        }
    }

    fn vmove(&mut self, dir: i32, select: bool, cx: &mut Context<Self>) {
        let to = self.vertical(dir);
        let goal = self.goal_x;
        if select { self.select_to(to, cx) } else { self.move_to(to, cx) }
        self.goal_x = goal;
    }

    fn home_end(&self, end: bool) -> usize {
        let c = self.cursor();
        match self.lay_ix(c).map(|k| self.lays[k].row_of(c)) {
            Some((s, e)) => if end { e } else { s },
            None => if end { self.doc.line_end(c) } else { self.doc.line_start(c) },
        }
    }

    fn edit(&mut self, typing: bool, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        let coalesce = typing && self.selected.is_empty() && self.typing_at == Some(self.selected.start);
        if !coalesce {
            self.undo.push((self.doc.clone(), self.selected.clone()));
            if self.undo.len() > 300 {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        f(self);
        self.reversed = false;
        self.typing_at = typing.then(|| self.cursor());
        self.goal_x = None;
        self.reveal = true;
        cx.emit(RichEvent::Changed);
        cx.notify();
    }

    fn put(&mut self, text: &str, cx: &mut Context<Self>) {
        let text: String = text.replace("\r\n", "\n").replace('\r', "\n").chars().filter(|c| *c != OBJ).collect();
        let r = self.marked.take().unwrap_or(self.selected.clone());
        let typing = r.is_empty() && !text.contains('\n') && text.chars().count() <= 2;
        self.edit(typing, cx, |s| {
            let mark = match s.pending.take() {
                Some(m) => m,
                None if !r.is_empty() => s.doc.mark_of(r.start),
                None => s.doc.mark_at(r.start),
            };
            let l = s.doc.line_of(r.start);
            if r.is_empty() && !text.is_empty() && s.doc.paras[l].kind.atom() {
                let ls = s.doc.line_start(r.start);
                if r.start == ls {
                    s.doc.replace(ls..ls, &format!("{text}\n"), &Mark::default());
                    s.selected = ls + text.len()..ls + text.len();
                } else {
                    let le = s.doc.line_end(ls);
                    s.doc.replace(le..le, &format!("\n{text}"), &Mark::default());
                    let at = le + 1 + text.len();
                    s.selected = at..at;
                }
                return;
            }
            s.doc.replace(r.clone(), &text, &mark);
            let at = r.start + text.len();
            s.selected = at..at;
        });
    }

    fn autoformat(&mut self, cx: &mut Context<Self>) -> bool {
        let c = self.cursor();
        let l = self.doc.line_of(c);
        if !self.selected.is_empty() || self.doc.paras[l].kind != Kind::P {
            return false;
        }
        let ls = self.doc.line_start(c);
        let kind = match &self.doc.text[ls..c] {
            "-" | "*" => Kind::Bullet,
            "1." => Kind::Number,
            ">" => Kind::Quote,
            "#" => Kind::H(1),
            "##" => Kind::H(2),
            "###" => Kind::H(3),
            _ => return false,
        };
        self.edit(false, cx, |s| {
            s.doc.replace(ls..c, "", &Mark::default());
            s.doc.paras[l].kind = kind;
            s.selected = ls..ls;
        });
        true
    }

    fn enter(&mut self, cx: &mut Context<Self>) {
        let c = self.cursor();
        let l = self.doc.line_of(c);
        let (ls, le) = (self.doc.line_start(c), self.doc.line_end(c));
        let line = self.doc.text[ls..le].to_string();
        let kind = self.doc.paras[l].kind.clone();
        if self.selected.is_empty() && ls == le && matches!(kind, Kind::Bullet | Kind::Number | Kind::Quote) {
            return self.edit(false, cx, |s| s.doc.paras[l].kind = Kind::P);
        }
        if self.selected.is_empty() && c == le && kind == Kind::P && matches!(line.trim(), "***" | "---" | "* * *") {
            return self.edit(false, cx, |s| {
                s.doc.replace(ls..le, &format!("{OBJ_S}\n"), &Mark::default());
                s.doc.paras[l].kind = Kind::Rule;
                s.selected = ls + 4..ls + 4;
            });
        }
        let r = self.selected.clone();
        self.edit(false, cx, |s| {
            let m = s.doc.mark_at(r.start);
            s.doc.replace(r.clone(), "\n", &m);
            s.selected = r.start + 1..r.start + 1;
        });
    }

    fn delete_selection(&mut self, cx: &mut Context<Self>) {
        let r = self.selected.clone();
        if r.is_empty() {
            return;
        }
        self.edit(false, cx, |s| {
            s.doc.replace(r.clone(), "", &Mark::default());
            s.selected = r.start..r.start;
        });
    }

    fn backspace(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            let c = self.cursor();
            let l = self.doc.line_of(c);
            let ls = self.doc.line_start(c);
            let kind = self.doc.paras[l].kind.clone();
            if c == ls && !kind.atom() && kind != Kind::P {
                return self.edit(false, cx, |s| s.doc.paras[l].kind = Kind::P);
            }
            self.selected = if c == ls && l > 0 && self.doc.paras[l - 1].kind.atom() {
                self.doc.line_start(c - 1)..c
            } else if kind.atom() && c > ls {
                ls..c
            } else {
                self.prev_boundary(c)..c
            };
        }
        self.delete_selection(cx)
    }

    fn delete(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            let c = self.cursor();
            let l = self.doc.line_of(c);
            let (ls, le) = (self.doc.line_start(c), self.doc.line_end(c));
            self.selected = if c == le && l + 1 < self.doc.paras.len() && self.doc.paras[l + 1].kind.atom() {
                c..self.doc.line_end(c + 1)
            } else if self.doc.paras[l].kind.atom() && c == ls {
                ls..le
            } else {
                c..self.next_boundary(c)
            };
        }
        self.delete_selection(cx)
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            return;
        }
        let d = self.doc.slice(self.selected.clone());
        let plain: String = d.text.chars().filter(|c| *c != OBJ).collect();
        self.clip = Some(d);
        cx.write_to_clipboard(ClipboardItem::new_string(plain));
    }

    fn paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) else { return };
        let rich = self.clip.clone().filter(|d| d.text.chars().filter(|c| *c != OBJ).collect::<String>() == text);
        let Some(d) = rich else { return self.put(&text, cx) };
        let r = self.selected.clone();
        self.edit(false, cx, |s| {
            s.doc.replace(r.clone(), "", &Mark::default());
            let mut at = r.start;
            if d.paras.first().is_some_and(|p| p.kind.atom()) || d.paras.last().is_some_and(|p| p.kind.atom()) {
                if at != s.doc.line_end(at) {
                    s.doc.replace(at..at, "\n", &Mark::default());
                }
                if at != s.doc.line_start(at) {
                    s.doc.replace(at..at, "\n", &Mark::default());
                    at += 1;
                }
            }
            s.doc.insert(at, &d);
            let end = at + d.text.len();
            s.selected = end..end;
        });
    }

    fn restore(&mut self, redo: bool, cx: &mut Context<Self>) {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let Some((doc, sel)) = from.pop() else { return };
        to.push((std::mem::replace(&mut self.doc, doc), self.selected.clone()));
        self.selected = sel.start.min(self.doc.text.len())..sel.end.min(self.doc.text.len());
        self.typing_at = None;
        self.pending = None;
        self.reveal = true;
        cx.emit(RichEvent::Changed);
        cx.notify();
    }

    pub fn current(&self) -> (Mark, Para) {
        let c = self.cursor();
        let m = match &self.pending {
            Some(m) => m.clone(),
            None if self.selected.is_empty() => self.doc.mark_at(c),
            None => self.doc.mark_of(self.selected.start),
        };
        (m, self.doc.paras[self.doc.line_of(c)].clone())
    }

    pub fn can_undo(&self) -> (bool, bool) {
        (!self.undo.is_empty(), !self.redo.is_empty())
    }

    fn restyle(&mut self, cx: &mut Context<Self>, f: impl Fn(&mut Mark)) {
        if self.selected.is_empty() {
            let mut m = self.current().0;
            f(&mut m);
            self.pending = Some(m);
            return cx.notify();
        }
        let r = self.selected.clone();
        self.edit(false, cx, |s| s.doc.style(r, f));
    }

    pub fn toggle(&mut self, flag: Flag, cx: &mut Context<Self>) {
        let on = if self.selected.is_empty() { !self.current().0.get(flag) } else { !self.doc.all(self.selected.clone(), |m| m.get(flag)) };
        self.restyle(cx, move |m| m.set(flag, on));
    }

    pub fn set_color(&mut self, c: Option<u32>, cx: &mut Context<Self>) {
        self.restyle(cx, move |m| m.color = c);
    }

    pub fn set_bg(&mut self, c: Option<u32>, cx: &mut Context<Self>) {
        self.restyle(cx, move |m| m.bg = c);
    }

    pub fn set_size(&mut self, z: Option<&'static str>, cx: &mut Context<Self>) {
        self.restyle(cx, move |m| m.size = z.map(Into::into));
    }

    pub fn link_at(&self) -> Option<(SharedString, Range<usize>)> {
        let c = self.selected.start;
        let probe = if c < self.doc.line_end(c) { c } else { self.prev_boundary(c) };
        let link = self.doc.mark_of(probe).link?;
        let same = |at: usize| self.doc.mark_of(at).link.as_ref() == Some(&link);
        let (mut s, mut e) = (probe, self.next_boundary(probe));
        while s > 0 && same(self.prev_boundary(s)) {
            s = self.prev_boundary(s);
        }
        while e < self.doc.text.len() && same(e) {
            e = self.next_boundary(e);
        }
        Some((link, s..e))
    }

    pub fn set_link(&mut self, href: Option<SharedString>, cx: &mut Context<Self>) {
        let r = if self.selected.is_empty() { self.link_at().map(|(_, r)| r) } else { Some(self.selected.clone()) };
        match (r, href) {
            (Some(r), href) => self.edit(false, cx, |s| s.doc.style(r, |m| m.link = href.clone())),
            (None, Some(h)) => {
                let at = self.cursor();
                self.edit(false, cx, |s| {
                    let m = Mark { link: Some(h.clone()), ..s.doc.mark_at(at) };
                    s.doc.replace(at..at, &h, &m);
                    s.selected = at + h.len()..at + h.len();
                });
            }
            (None, None) => {}
        }
    }

    fn sel_lines(&self) -> Range<usize> {
        self.doc.line_of(self.selected.start)..self.doc.line_of(self.selected.end) + 1
    }

    pub fn toggle_kind(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let lines = self.sel_lines();
        let all = self.doc.paras[lines.clone()].iter().filter(|p| !p.kind.atom()).all(|p| p.kind == kind);
        self.edit(false, cx, |s| {
            for p in s.doc.paras[lines].iter_mut().filter(|p| !p.kind.atom()) {
                p.kind = if all { Kind::P } else { kind.clone() };
            }
        });
    }

    pub fn set_align(&mut self, a: Align, cx: &mut Context<Self>) {
        let lines = self.sel_lines();
        self.edit(false, cx, |s| s.doc.paras[lines].iter_mut().for_each(|p| p.align = a));
    }

    pub fn insert_block(&mut self, kind: Kind, cx: &mut Context<Self>) {
        let r = self.selected.clone();
        self.edit(false, cx, |s| {
            s.doc.replace(r.clone(), "", &Mark::default());
            let mut at = r.start;
            let l = s.doc.line_of(at);
            if s.doc.paras[l].kind.atom() {
                at = s.doc.line_end(at);
            }
            let (ls, le) = (s.doc.line_start(at), s.doc.line_end(at));
            let reuse = ls == le && !s.doc.paras[l].kind.atom();
            if !reuse {
                if at != le {
                    s.doc.replace(at..at, "\n", &Mark::default());
                }
                if at != ls || s.doc.paras[s.doc.line_of(at)].kind.atom() {
                    s.doc.replace(at..at, "\n", &Mark::default());
                    at += 1;
                }
            }
            s.doc.replace(at..at, OBJ_S, &Mark::default());
            let l = s.doc.line_of(at);
            s.doc.paras[l] = Para { kind: kind.clone(), align: if matches!(kind, Kind::Image { .. }) { Align::Center } else { Align::Left } };
            let end = at + OBJ_S.len();
            if end == s.doc.text.len() {
                s.doc.replace(end..end, "\n", &Mark::default());
            }
            s.selected = end + 1..end + 1;
        });
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        self.restore(false, cx)
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        self.restore(true, cx)
    }

    fn from_utf16(&self, offset: usize) -> usize {
        let (mut utf8, mut utf16) = (0, 0);
        for ch in self.doc.text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn to_utf16(&self, offset: usize) -> usize {
        self.doc.text[..offset].chars().map(char::len_utf16).sum()
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.to_utf16(r.start)..self.to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.from_utf16(r.start)..self.from_utf16(r.end)
    }
}

impl EntityInputHandler for RichEditor {
    fn text_for_range(&mut self, r: Range<usize>, actual: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        let range = self.range_from_utf16(&r);
        actual.replace(self.range_to_utf16(&range));
        Some(self.doc.text[range].to_string())
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
        if let Some(r) = r {
            self.selected = self.range_from_utf16(&r);
            self.marked = None;
        }
        if text == " " && self.marked.is_none() && self.autoformat(cx) {
            return;
        }
        self.put(text, cx);
    }

    fn replace_and_mark_text_in_range(&mut self, r: Option<Range<usize>>, text: &str, new_selected: Option<Range<usize>>, _: &mut Window, cx: &mut Context<Self>) {
        let range = r.map(|r| self.range_from_utf16(&r)).or(self.marked.clone()).unwrap_or(self.selected.clone());
        let mark = self.doc.mark_at(range.start);
        self.doc.replace(range.clone(), text, &mark);
        self.marked = (!text.is_empty()).then(|| range.start..range.start + text.len());
        self.selected = new_selected
            .map(|s| self.range_from_utf16(&s))
            .map(|s| range.start + s.start..range.start + s.end)
            .unwrap_or_else(|| range.start + text.len()..range.start + text.len());
        cx.emit(RichEvent::Changed);
        cx.notify();
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        let (p, lh) = self.caret()?;
        Some(Bounds::new(p, size(px(2.), lh)))
    }

    fn character_index_for_point(&mut self, p: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.to_utf16(self.index_at(p)))
    }
}

impl Focusable for RichEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn metrics(kind: &Kind, size: f32, spacing: f32) -> (f32, f32, FontWeight) {
    let (k, weight) = match kind {
        Kind::H(1) => (28. / 17., FontWeight::BOLD),
        Kind::H(2) => (24. / 17., FontWeight::BOLD),
        Kind::H(_) => (20. / 17., FontWeight::BOLD),
        _ => (1., FontWeight::NORMAL),
    };
    let z = (size * k).round();
    let lh = if k > 1. { (z * 1.3).round() } else { (z * spacing).round() };
    (z, lh, weight)
}

fn size_px(css: &str) -> Option<f32> {
    let num = |s: &str| s.trim().parse::<f32>().ok();
    if let Some(v) = css.strip_suffix("pt").and_then(num) {
        Some(v * 17. / 12.)
    } else if let Some(v) = css.strip_suffix("px").and_then(num) {
        Some(v * 17. / 16.)
    } else if let Some(v) = css.strip_suffix("em").and_then(num) {
        Some(v * 17.)
    } else {
        css.strip_suffix('%').and_then(num).map(|v| v * 0.17)
    }
}

impl Render for RichEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.pal;
        let w = self.width;
        let lines = self.doc.lines();
        self.lays.resize(lines.len(), Lay::default());
        let entity = cx.entity();
        let empty = self.doc.text.is_empty();
        let mut number = 0;
        let cur_line = self.doc.line_of(self.cursor());
        let mut blocks: Vec<AnyElement> = vec![];
        for (ix, (para, r)) in self.doc.paras.iter().zip(lines).enumerate() {
            number = if para.kind == Kind::Number { number + 1 } else { 0 };
            let next_same = self.doc.paras.get(ix + 1).is_some_and(|n| n.kind == para.kind);
            let gap = match para.kind {
                Kind::Bullet | Kind::Number | Kind::Quote if next_same => 6.,
                Kind::H(_) => 10.,
                _ => 14.,
            };
            if para.kind.atom() {
                blocks.push(self.atom_block(ix, r.start, para, gap, entity.clone()));
                continue;
            }
            let (mut size, mut lh, weight) = metrics(&para.kind, self.size, self.spacing);
            let family = self.family.clone();
            let lit = self.spotlight && ix == cur_line;
            let fade = if self.spotlight && !lit { 0.32 } else { 1. };
            let spans = self.doc.spans(r.clone());
            if let Some(z) = spans.first().and_then(|s| s.1.size.clone()).filter(|z| spans.iter().all(|s| s.1.size.as_ref() == Some(z))).and_then(|z| size_px(&z)) {
                (size, lh) = (z, (z * self.spacing).round());
            }
            let ink = if para.kind == Kind::Quote { p.muted } else { p.ink };
            let runs: Vec<TextRun> = if empty && ix == 0 {
                vec![TextRun { len: self.placeholder.len(), font: font(family.clone()), color: rgb(p.muted).into(), background_color: None, underline: None, strikethrough: None }]
            } else {
                let mut cuts: Vec<usize> = vec![r.start, r.end];
                let mut o = r.start;
                for (t, _) in &spans {
                    o += t.len();
                    cuts.push(o);
                }
                let hls: Vec<&(Range<usize>, Hl)> = self.hl.iter().filter(|(h, _)| h.start < r.end && h.end > r.start).collect();
                for (h, _) in &hls {
                    cuts.push(h.start.clamp(r.start, r.end));
                    cuts.push(h.end.clamp(r.start, r.end));
                }
                cuts.sort_unstable();
                cuts.dedup();
                cuts.windows(2)
                    .filter(|c| c[1] > c[0] && self.doc.text.is_char_boundary(c[0]) && self.doc.text.is_char_boundary(c[1]))
                    .map(|c| {
                        let m = self.doc.mark_of(c[0]);
                        let under = hls.iter().find_map(|(h, k)| match k {
                            Hl::Under(col) if h.start <= c[0] && h.end >= c[1] => Some(*col),
                            _ => None,
                        });
                        let back = hls.iter().find_map(|(h, k)| match k {
                            Hl::Back(col) if h.start <= c[0] && h.end >= c[1] => Some(*col),
                            _ => None,
                        });
                        let color: Hsla = rgb(if m.link.is_some() { p.accent } else { m.color.unwrap_or(ink) }).into();
                        TextRun {
                            len: c[1] - c[0],
                            font: Font { weight: if m.bold { FontWeight::BOLD } else { weight }, style: if m.italic { FontStyle::Italic } else { FontStyle::Normal }, ..font(family.clone()) },
                            color: color.opacity(fade),
                            background_color: back.or(m.bg).map(|c| rgb(c).into()),
                            underline: match under {
                                Some(col) => Some(UnderlineStyle { thickness: px(1.5), color: Some(rgb(col).into()), wavy: true }),
                                None => (m.underline || m.link.is_some()).then(|| UnderlineStyle { thickness: px(1.), color: None, wavy: false }),
                            },
                            strikethrough: m.strike.then(|| StrikethroughStyle { thickness: px(1.), color: None }),
                        }
                    })
                    .collect()
            };
            let text: SharedString = if empty && ix == 0 { self.placeholder.clone() } else { self.doc.text[r.clone()].to_string().into() };
            let inset = match para.kind {
                Kind::Quote => 18.,
                Kind::Bullet | Kind::Number => 30.,
                _ => 0.,
            };
            let el = ParaEl { editor: entity.clone(), ix, start: r.start, real: !(empty && ix == 0), text, runs, size: px(size), lh: px(lh), width: px(w - inset), align: para.align };
            let block = match para.kind {
                Kind::Quote => div().pl(px(15.)).border_l_2().border_color(rgb(p.accent)).child(el),
                Kind::Bullet | Kind::Number => div()
                    .flex()
                    .child(
                        div()
                            .flex_none()
                            .w(px(inset))
                            .font_family(family.clone())
                            .text_size(px(size))
                            .line_height(px(lh))
                            .text_color(rgb(p.muted))
                            .child(if para.kind == Kind::Number { format!("{number}.") } else { "\u{2022}".into() }),
                    )
                    .child(el),
                _ => div().child(el),
            };
            let block = if lit { block.mx(px(-22.)).px(px(22.)).pt(px(10.)).mb(px(-4.)).rounded(px(12.)).bg(rgb(p.surface)) } else { block };
            blocks.push(block.pb(px(gap)).into_any_element());
        }
        let focus = self.focus.clone();
        div()
            .key_context("TextInput RichEditor")
            .track_focus(&self.focus)
            .cursor(CursorStyle::IBeam)
            .relative()
            .w(px(w))
            .flex()
            .flex_col()
            .on_action(cx.listener(|this, _: &input::Backspace, _, cx| this.backspace(cx)))
            .on_action(cx.listener(|this, _: &input::Delete, _, cx| this.delete(cx)))
            .on_action(cx.listener(|this, _: &DeleteWordLeft, _, cx| {
                if this.selected.is_empty() {
                    let c = this.cursor();
                    this.selected = this.word_left(c)..c;
                }
                this.delete_selection(cx)
            }))
            .on_action(cx.listener(|this, _: &input::Left, _, cx| {
                let to = if this.selected.is_empty() { this.prev_boundary(this.cursor()) } else { this.selected.start };
                this.move_to(to, cx)
            }))
            .on_action(cx.listener(|this, _: &input::Right, _, cx| {
                let to = if this.selected.is_empty() { this.next_boundary(this.cursor()) } else { this.selected.end };
                this.move_to(to, cx)
            }))
            .on_action(cx.listener(|this, _: &input::SelectLeft, _, cx| this.select_to(this.prev_boundary(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &input::SelectRight, _, cx| this.select_to(this.next_boundary(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &WordLeft, _, cx| this.move_to(this.word_left(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &WordRight, _, cx| this.move_to(this.word_right(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &SelectWordLeft, _, cx| this.select_to(this.word_left(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &SelectWordRight, _, cx| this.select_to(this.word_right(this.cursor()), cx)))
            .on_action(cx.listener(|this, _: &input::Home, _, cx| this.move_to(this.home_end(false), cx)))
            .on_action(cx.listener(|this, _: &input::End, _, cx| this.move_to(this.home_end(true), cx)))
            .on_action(cx.listener(|this, _: &SelectHome, _, cx| this.select_to(this.home_end(false), cx)))
            .on_action(cx.listener(|this, _: &SelectEnd, _, cx| this.select_to(this.home_end(true), cx)))
            .on_action(cx.listener(|this, _: &DocStart, _, cx| this.move_to(0, cx)))
            .on_action(cx.listener(|this, _: &DocEnd, _, cx| this.move_to(this.doc.text.len(), cx)))
            .on_action(cx.listener(|this, _: &input::Up, _, cx| this.vmove(-1, false, cx)))
            .on_action(cx.listener(|this, _: &input::Down, _, cx| this.vmove(1, false, cx)))
            .on_action(cx.listener(|this, _: &SelectUp, _, cx| this.vmove(-1, true, cx)))
            .on_action(cx.listener(|this, _: &SelectDown, _, cx| this.vmove(1, true, cx)))
            .on_action(cx.listener(|this, _: &input::SelectAll, _, cx| {
                this.move_to(0, cx);
                this.select_to(this.doc.text.len(), cx)
            }))
            .on_action(cx.listener(|this, _: &input::Copy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &input::Cut, _, cx| {
                this.copy(cx);
                this.delete_selection(cx)
            }))
            .on_action(cx.listener(|this, _: &input::Paste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &input::Submit, _, cx| this.enter(cx)))
            .on_action(cx.listener(|this, _: &Undo, _, cx| this.undo(cx)))
            .on_action(cx.listener(|this, _: &Redo, _, cx| this.redo(cx)))
            .on_action(cx.listener(|this, _: &ToggleBold, _, cx| this.toggle(Flag::Bold, cx)))
            .on_action(cx.listener(|_, _: &input::Cancel, _, cx| cx.emit(RichEvent::Escape)))
            .on_action(cx.listener(|_, _: &FindInChapter, _, cx| cx.emit(RichEvent::Find)))
            .on_action(cx.listener(|_, _: &FocusMode, _, cx| cx.emit(RichEvent::ToggleFocus)))
            .on_action(cx.listener(|this, _: &ToggleItalic, _, cx| this.toggle(Flag::Italic, cx)))
            .on_action(cx.listener(|this, _: &ToggleUnderline, _, cx| this.toggle(Flag::Underline, cx)))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, window, cx| {
                window.focus(&this.focus);
                window.prevent_default();
                let i = this.index_at(e.position);
                if e.modifiers.secondary() {
                    if let Some(h) = this.doc.mark_of(i.min(this.doc.text.len().saturating_sub(1))).link {
                        return cx.open_url(&h);
                    }
                }
                this.selecting = true;
                this.goal_x = None;
                match e.click_count {
                    2 | 3 => {
                        let r = this.around(i, e.click_count == 3);
                        this.move_to(r.start, cx);
                        this.select_to(r.end, cx);
                    }
                    _ if e.modifiers.shift => this.select_to(i, cx),
                    _ => this.move_to(i, cx),
                }
                this.reveal = false;
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.selecting = false))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, _| this.selecting = false))
            .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                if this.selecting {
                    this.select_to(this.index_at(e.position), cx);
                    this.reveal = false;
                }
            }))
            .children(blocks)
            .child(
                canvas(|_, _, _| (), move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, entity.clone()), cx))
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
            )
    }
}

impl RichEditor {
    fn atom_block(&self, ix: usize, start: usize, para: &Para, gap: f32, entity: Entity<Self>) -> AnyElement {
        let p = self.pal;
        let w = self.width;
        let body = match &para.kind {
            Kind::Rule => div().h(px(36.)).flex().items_center().justify_center().font_family(SERIF).text_size(px(17.)).text_color(rgb(p.muted)).child("*      *      *"),
            Kind::Image { src, .. } => div()
                .flex()
                .map(|d| match para.align {
                    Align::Center => d.justify_center(),
                    Align::Right => d.justify_end(),
                    _ => d,
                })
                .child(img(src.clone()).max_w(px(w)).max_h(px(520.)).rounded(px(6.)).object_fit(ObjectFit::Contain).with_fallback(move || {
                    div().w(px(240.)).h(px(140.)).rounded(px(8.)).bg(rgb(p.surface)).flex().items_center().justify_center().text_size(px(13.)).text_color(rgb(p.muted)).child("Image").into_any_element()
                })),
            Kind::Raw(h) => {
                let frag = scraper::Html::parse_fragment(h);
                let tag = frag.root_element().children().filter_map(scraper::ElementRef::wrap).next().map(|e| e.value().name().to_string()).unwrap_or_default();
                let label = match tag.as_str() {
                    "table" => "Table",
                    "iframe" | "video" | "audio" | "embed" | "object" => "Embedded media",
                    "figure" => "Figure",
                    "pre" => "Preformatted text",
                    _ => "Block from Scribble Hub's editor",
                };
                let words: String = frag.root_element().text().collect::<Vec<_>>().join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
                let excerpt: String = words.chars().take(140).collect();
                div()
                    .p(px(14.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgb(p.line))
                    .bg(rgb(p.surface))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgb(p.muted)).child(format!("{label} · kept as is")))
                    .when(!excerpt.is_empty(), |d| d.child(div().text_size(px(14.)).font_family(SERIF).text_color(rgb(p.ink)).child(excerpt)))
            }
            _ => div(),
        };
        let align = para.align;
        div()
            .pb(px(gap))
            .child(
                div().relative().w(px(w)).child(body).child(
                    canvas(|b, _, _| b, move |bounds, _, window, cx| {
                        entity.update(cx, |e, _| {
                            if let Some(l) = e.lays.get_mut(ix) {
                                *l = Lay { start, len: OBJ_S.len(), bounds, line: None, lh: bounds.size.height, align };
                            }
                        });
                        let e = entity.read(cx);
                        let (sel, c, focused, accent, reveal) = (e.selected.clone(), e.cursor(), e.focus.is_focused(window), e.pal.accent, e.reveal);
                        if sel.start <= start && sel.end >= start + OBJ_S.len() {
                            window.paint_quad(fill(bounds, rgba(accent << 8 | 0x40)).corner_radii(px(6.)));
                        } else if focused && sel.is_empty() && (c == start || c == start + OBJ_S.len()) {
                            let x = if c == start { bounds.left() - px(4.) } else { bounds.right() + px(2.) };
                            window.paint_quad(fill(Bounds::new(point(x, bounds.top()), size(px(2.), bounds.size.height)), rgb(accent)));
                        }
                        if reveal && focused && sel.is_empty() && (c == start || c == start + OBJ_S.len()) {
                            entity.update(cx, |e, cx| {
                                e.reveal = false;
                                cx.emit(RichEvent::Reveal(bounds.top(), bounds.bottom()));
                            });
                        }
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                ),
            )
            .into_any_element()
    }
}

struct ParaEl {
    editor: Entity<RichEditor>,
    ix: usize,
    start: usize,
    real: bool,
    text: SharedString,
    runs: Vec<TextRun>,
    size: Pixels,
    lh: Pixels,
    width: Pixels,
    align: Align,
}

impl IntoElement for ParaEl {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl ParaEl {
    fn shape(text: &SharedString, runs: &[TextRun], size: Pixels, width: Pixels, window: &Window) -> Option<WrappedLine> {
        if text.is_empty() {
            return None;
        }
        window.text_system().shape_text(text.clone(), size, runs, Some(width), None).ok()?.into_iter().next()
    }
}

impl Element for ParaEl {
    type RequestLayoutState = ();
    type PrepaintState = Option<WrappedLine>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, _: &mut App) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = self.width.into();
        let (text, runs, size, width, lh) = (self.text.clone(), self.runs.clone(), self.size, self.width, self.lh);
        let id = window.request_measured_layout(style, move |_, _, window, _| {
            let rows = ParaEl::shape(&text, &runs, size, width, window).map_or(1, |l| l.wrap_boundaries().len() + 1);
            gpui::size(width, lh * rows as f32)
        });
        (id, ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) -> Option<WrappedLine> {
        ParaEl::shape(&self.text, &self.runs, self.size, bounds.size.width, window)
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, bounds: Bounds<Pixels>, _: &mut (), line: &mut Option<WrappedLine>, window: &mut Window, cx: &mut App) {
        let lay = Lay { start: self.start, len: if self.real { self.text.len() } else { 0 }, bounds, line: if self.real { line.clone() } else { None }, lh: self.lh, align: self.align };
        let ix = self.ix;
        let stored = lay.clone();
        self.editor.update(cx, |e, _| {
            if let Some(l) = e.lays.get_mut(ix) {
                *l = stored;
            }
        });
        let e = self.editor.read(cx);
        let (sel, c, focused, accent, reveal) = (e.selected.clone(), e.cursor(), e.focus.is_focused(window), e.pal.accent, e.reveal);
        let end = lay.start + lay.len;
        if !sel.is_empty() && sel.start <= end && sel.end > lay.start {
            let (a, b) = (sel.start.max(lay.start), sel.end.min(end));
            let rows = lay.rows();
            for (k, &(si, ei, sx, ex)) in rows.iter().enumerate() {
                let (rs, re) = (lay.start + si, lay.start + ei);
                let last = k == rows.len() - 1;
                let (s, t) = (a.max(rs), b.min(re));
                let on_to_next = last && sel.end > end;
                if s > t || (s == t && !on_to_next) {
                    continue;
                }
                let x0 = lay.point(s).x;
                let mut x1 = if t == re && !last { bounds.left() + ex - sx + lay.shift(ex - sx) } else { lay.point(t).x };
                if on_to_next {
                    x1 += px(6.);
                }
                let y = bounds.top() + self.lh * k as f32;
                window.paint_quad(fill(Bounds::from_corners(point(x0, y), point(x1, y + self.lh)), rgba(accent << 8 | 0x55)));
            }
        }
        if let Some(l) = line.as_ref() {
            let align = match self.align {
                Align::Center => TextAlign::Center,
                Align::Right => TextAlign::Right,
                _ => TextAlign::Left,
            };
            l.paint_background(bounds.origin, self.lh, align, Some(bounds), window, cx).ok();
            l.paint(bounds.origin, self.lh, align, Some(bounds), window, cx).ok();
        }
        if focused && sel.is_empty() && c >= lay.start && c <= end {
            let at = lay.point(c);
            window.paint_quad(fill(Bounds::new(at, size(px(1.5), self.lh)), rgb(accent)));
            if reveal {
                self.editor.update(cx, |e, cx| {
                    e.reveal = false;
                    cx.emit(RichEvent::Reveal(at.y, at.y + self.lh));
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Align, Doc, Kind, Mark, OBJ_S};

    fn kinds(d: &Doc) -> Vec<Kind> {
        d.paras.iter().map(|p| p.kind.clone()).collect()
    }

    #[test]
    fn reads_the_site_editor() {
        let (d, lost) = Doc::from_html("<p>Hello <strong>bold</strong> and <span style=\"color: #e03e2d;\">red</span></p><p style=\"text-align: right;\">r</p><hr><ul><li>a</li><li>b</li></ul><p><a href=\"https://x.y\">link</a></p>");
        assert!(lost.is_empty(), "{lost:?}");
        assert_eq!(d.text, format!("Hello bold and red\nr\n{OBJ_S}\na\nb\nlink"));
        assert!(d.mark_of(6).bold && !d.mark_of(0).bold);
        assert_eq!(d.mark_of(15).color, Some(0xe03e2d));
        assert_eq!(d.paras[1].align, Align::Right);
        assert_eq!(kinds(&d), [Kind::P, Kind::P, Kind::Rule, Kind::Bullet, Kind::Bullet, Kind::P]);
        assert_eq!(d.mark_of(d.text.len() - 1).link.map(|l| l.to_string()), Some("https://x.y".to_string()));
        assert_eq!(Doc::from_html(&d.to_html()).0, d);
    }

    #[test]
    fn keeps_what_it_cant_edit() {
        let html = "<p>before</p><table><tbody><tr><td>cell</td></tr></tbody></table><p><img src=\"https://i.example/a.png\" width=\"300\"></p><div class=\"wi_authornotes\">note</div>";
        let (d, lost) = Doc::from_html(html);
        assert!(lost.is_empty(), "{lost:?}");
        assert!(matches!(d.paras[1].kind, Kind::Raw(_)));
        assert!(matches!(&d.paras[2].kind, Kind::Image { src, .. } if src == "https://i.example/a.png"));
        assert!(matches!(d.paras[3].kind, Kind::Raw(_)));
        let out = d.to_html();
        assert!(out.contains("<table><tbody><tr><td>cell</td></tr></tbody></table>") && out.contains("width=\"300\"") && out.contains("wi_authornotes"), "{out}");
    }

    #[test]
    fn docs_paste() {
        let html = include_str!("../tests/fixtures/chapter-docs-paste.html");
        let (d, lost) = Doc::from_html(html);
        assert!(lost.is_empty(), "{lost:?}");
        assert!(d.text.starts_with("[author] Cha Ching. Double Update today"), "{}", &d.text[..80]);
        assert!(d.mark_of(10).italic && !d.mark_of(0).italic);
        assert!(d.text.contains("\n\n"));
        assert_eq!(Doc::from_html(&d.to_html()).0, d);
        let (d, _) = Doc::from_html("<b style=\"font-weight:normal;\"><p><span style=\"font-weight:700\">B</span>n</p></b>");
        assert!(d.mark_of(0).bold && !d.mark_of(1).bold);
    }

    #[test]
    fn breaks() {
        let (d, _) = Doc::from_html("<p>a<br>b<br></p><p><br></p><p><br><br></p>");
        assert_eq!(d.text, "a\nb\n\n\n");
    }

    #[test]
    fn editing_paragraphs() {
        let (mut d, _) = Doc::from_html("<h2>Title</h2><p>body</p>");
        d.replace(5..5, "\n", &Mark::default());
        assert_eq!(kinds(&d), [Kind::H(2), Kind::P, Kind::P]);
        d.replace(5..6, "", &Mark::default());
        assert_eq!(kinds(&d), [Kind::H(2), Kind::P]);
        d.replace(2..2, "\n", &Mark::default());
        assert_eq!(kinds(&d), [Kind::H(2), Kind::H(2), Kind::P]);
        let end = d.line_start(d.text.len());
        d.replace(0..end, "", &Mark::default());
        assert_eq!((d.text.as_str(), kinds(&d)), ("body", vec![Kind::P]));
        let bold = Mark { bold: true, ..Default::default() };
        d.replace(4..4, " more", &bold);
        assert!(d.all(4..9, |m| m.bold) && !d.all(0..9, |m| m.bold));
        d.style(0..9, |m| m.bold = false);
        assert_eq!(d.runs.len(), 1);
        let (mut d, _) = Doc::from_html("<p>x</p><hr><p>y</p>");
        d.replace(2..2 + OBJ_S.len(), "", &Mark::default());
        assert_eq!(kinds(&d), [Kind::P, Kind::P, Kind::P]);
    }

    #[test]
    fn copy_paste_keeps_styles() {
        let (d, _) = Doc::from_html("<p>one <em>two</em></p><h3>three</h3><p>four</p>");
        let s = d.slice(4..13);
        assert_eq!(s.text, "two\nthree");
        assert_eq!(kinds(&s), [Kind::P, Kind::H(3)]);
        let (mut e, _) = Doc::from_html("<p>start</p>");
        let end = e.text.len();
        e.replace(end..end, "\n", &Mark::default());
        let end = e.text.len();
        e.insert(end, &s);
        assert_eq!(e.text, "start\ntwo\nthree");
        assert_eq!(kinds(&e), [Kind::P, Kind::P, Kind::H(3)]);
        assert!(e.mark_of(6).italic);
    }
}
