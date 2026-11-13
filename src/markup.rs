#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Marks {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Para,
    Heading,
    Quote,
    Bullet,
    Number(usize),
    Center,
    Rule,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: Kind,
    pub runs: Vec<(String, Marks)>,
}

fn runs(line: &str) -> Vec<(String, Marks)> {
    let mut out: Vec<(String, Marks)> = vec![];
    let mut m = Marks::default();
    let mut cur = String::new();
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        let (flip, len): (Option<fn(&mut Marks)>, usize) = if rest.starts_with("**") {
            (Some(|m| m.bold = !m.bold), 2)
        } else if rest.starts_with("__") {
            (Some(|m| m.underline = !m.underline), 2)
        } else if rest.starts_with("~~") {
            (Some(|m| m.strike = !m.strike), 2)
        } else if c == '*' {
            (Some(|m| m.italic = !m.italic), 1)
        } else {
            (None, c.len_utf8())
        };
        match flip {
            Some(f) => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), m));
                }
                f(&mut m);
            }
            None => cur.push(c),
        }
        rest = &rest[len..];
    }
    if !cur.is_empty() {
        out.push((cur, m));
    }
    out
}

pub fn parse(text: &str) -> Vec<Block> {
    let mut n = 0;
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            if matches!(l, "***" | "---" | "* * *") {
                n = 0;
                return Block { kind: Kind::Rule, runs: vec![] };
            }
            let numbered = l.split_once(". ").filter(|(d, _)| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()));
            let (kind, body) = if let Some(b) = l.strip_prefix("## ") {
                (Kind::Heading, b)
            } else if let Some(b) = l.strip_prefix("> ") {
                (Kind::Quote, b)
            } else if let Some(b) = l.strip_prefix("- ") {
                (Kind::Bullet, b)
            } else if let Some(b) = l.strip_prefix("-> ") {
                (Kind::Center, b)
            } else if let Some((_, b)) = numbered {
                n += 1;
                (Kind::Number(n), b)
            } else {
                (Kind::Para, l)
            };
            if !matches!(kind, Kind::Number(_)) {
                n = 0;
            }
            Block { kind, runs: runs(body) }
        })
        .collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn inline(b: &Block) -> String {
    b.runs
        .iter()
        .map(|(t, m)| {
            let mut s = esc(t);
            if m.strike {
                s = format!("<del>{s}</del>");
            }
            if m.underline {
                s = format!("<span style=\"text-decoration: underline;\">{s}</span>");
            }
            if m.italic {
                s = format!("<em>{s}</em>");
            }
            if m.bold {
                s = format!("<strong>{s}</strong>");
            }
            s
        })
        .collect()
}

pub fn to_html(text: &str) -> String {
    let blocks = parse(text);
    let mut out: Vec<String> = vec![];
    let mut i = 0;
    while i < blocks.len() {
        let b = &blocks[i];
        let list = match b.kind {
            Kind::Bullet => Some("ul"),
            Kind::Number(_) => Some("ol"),
            _ => None,
        };
        if let Some(tag) = list {
            let same = |k: Kind| if tag == "ul" { k == Kind::Bullet } else { matches!(k, Kind::Number(_)) };
            let mut items = vec![];
            while i < blocks.len() && same(blocks[i].kind) {
                items.push(format!("<li>{}</li>", inline(&blocks[i])));
                i += 1;
            }
            out.push(format!("<{tag}>{}</{tag}>", items.join("")));
            continue;
        }
        out.push(match b.kind {
            Kind::Rule => "<hr>".to_string(),
            Kind::Heading => format!("<h3>{}</h3>", inline(b)),
            Kind::Quote => format!("<blockquote><p>{}</p></blockquote>", inline(b)),
            Kind::Center => format!("<p style=\"text-align: center;\">{}</p>", inline(b)),
            _ => format!("<p>{}</p>", inline(b)),
        });
        i += 1;
    }
    out.join("\n")
}

#[derive(Default, Debug, PartialEq)]
struct Look {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    center: bool,
    lost: Option<&'static str>,
}

pub(crate) fn decls(style: &str) -> Vec<(String, String)> {
    let mut decls: Vec<(String, String)> = vec![];
    for tok in style.replace(';', " ; ").split_whitespace() {
        if tok == ";" {
            decls.push((String::new(), String::new()));
        } else if let Some((k, v)) = tok.split_once(':').filter(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphabetic() || c == '-')) {
            decls.push((k.to_lowercase(), v.to_lowercase()));
        } else if let Some(last) = decls.last_mut() {
            if !last.1.is_empty() {
                last.1.push(' ');
            }
            last.1.push_str(&tok.to_lowercase());
        }
    }
    decls
        .into_iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k, v.trim().trim_end_matches("!important").trim().to_string()))
        .collect()
}

fn look(style: &str) -> Look {
    let mut l = Look::default();
    for (k, v) in decls(style) {
        let black = matches!(v.replace(' ', "").as_str(), "#000" | "#000000" | "black" | "rgb(0,0,0)" | "windowtext" | "inherit" | "initial" | "currentcolor");
        match k.as_str() {
            "font-weight" => l.bold |= v == "bold" || v == "bolder" || v.parse::<u32>().is_ok_and(|w| w >= 600),
            "font-style" => l.italic |= v == "italic" || v == "oblique",
            "text-decoration" | "text-decoration-line" => {
                l.underline |= v.contains("underline");
                l.strike |= v.contains("line-through");
            }
            "text-align" => match v.as_str() {
                "center" => l.center = true,
                "left" | "start" | "justify" | "inherit" | "initial" => {}
                _ => l.lost = l.lost.or(Some("text alignment")),
            },
            "color" if black => {}
            "background-color" | "background" if matches!(v.as_str(), "transparent" | "#fff" | "#ffffff" | "white" | "inherit" | "initial" | "none") => {}
            "color" | "background-color" | "background" => l.lost = Some("text colours"),
            "font-size" => l.lost = l.lost.or(Some("text sizes")),
            "vertical-align" if v == "baseline" => {}
            "vertical-align" => l.lost = l.lost.or(Some("footnotes")),
            "white-space" | "font-family" | "font-variant" | "line-height" | "direction" | "text-indent" | "letter-spacing" | "word-spacing" => {}
            m if m.starts_with("margin") || m.starts_with("padding") || m.starts_with("font-variant") => {}
            _ => l.lost = l.lost.or(Some("other styling")),
        }
    }
    l
}

pub fn from_html(html: &str) -> String {
    use scraper::{ElementRef, Html, Node};
    fn inline(el: ElementRef) -> String {
        let mut out = String::new();
        for child in el.children() {
            match child.value() {
                Node::Text(t) => out.push_str(&t.replace('\u{a0}', " ").replace('\n', " ")),
                Node::Element(e) => {
                    let Some(c) = ElementRef::wrap(child) else { continue };
                    if e.name() == "br" {
                        out.push('\n');
                        continue;
                    }
                    let l = look(e.attr("style").unwrap_or_default());
                    let n = e.name();
                    let marks = [
                        ("**", l.bold || matches!(n, "strong" | "b")),
                        ("*", l.italic || matches!(n, "em" | "i")),
                        ("__", l.underline || n == "u"),
                        ("~~", l.strike || matches!(n, "del" | "s" | "strike")),
                    ];
                    let inner = inline(c);
                    let wrapped: Vec<String> = inner
                        .split('\n')
                        .map(|part| {
                            let t = part.trim();
                            if t.is_empty() {
                                return part.to_string();
                            }
                            let (open, close): (String, String) = (marks.iter().filter(|m| m.1).map(|m| m.0).collect(), marks.iter().rev().filter(|m| m.1).map(|m| m.0).collect());
                            let lead = if part.starts_with(' ') { " " } else { "" };
                            let trail = if part.ends_with(' ') { " " } else { "" };
                            format!("{lead}{open}{t}{close}{trail}")
                        })
                        .collect();
                    out.push_str(&wrapped.join("\n"));
                }
                _ => {}
            }
        }
        out
    }
    fn blocks(el: ElementRef, out: &mut Vec<String>) {
        let push = |out: &mut Vec<String>, pre: &str, text: String| {
            for line in text.split('\n') {
                let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
                if !line.is_empty() {
                    out.push(format!("{pre}{line}"));
                }
            }
        };
        for child in el.children() {
            match child.value() {
                Node::Text(t) => push(out, "", t.to_string()),
                Node::Element(e) => {
                    let Some(c) = ElementRef::wrap(child) else { continue };
                    let centred = look(e.attr("style").unwrap_or_default()).center;
                    match e.name() {
                        "p" => push(out, if centred { "-> " } else { "" }, inline(c)),
                        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => push(out, "## ", inline(c)),
                        "hr" => out.push("***".into()),
                        "blockquote" => {
                            let mut inner = vec![];
                            blocks(c, &mut inner);
                            out.extend(inner.into_iter().map(|l| format!("> {l}")));
                        }
                        "ul" | "ol" => {
                            let pre = if e.name() == "ul" { "- " } else { "1. " };
                            for li in c.children().filter_map(ElementRef::wrap).filter(|l| l.value().name() == "li") {
                                push(out, pre, inline(li));
                            }
                        }
                        "div" | "section" | "article" | "body" | "html" => blocks(c, out),
                        _ => push(out, "", inline(c)),
                    }
                }
                _ => {}
            }
        }
    }
    let doc = Html::parse_fragment(html);
    let mut out = vec![];
    blocks(doc.root_element(), &mut out);
    out.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html() {
        assert_eq!(to_html("Hello **there**\n\n*She* said <no>.\n***\nEnd"), "<p>Hello <strong>there</strong></p>\n<p><em>She</em> said &lt;no&gt;.</p>\n<hr>\n<p>End</p>");
        assert_eq!(to_html("unclosed *it"), "<p>unclosed <em>it</em></p>");
        assert!(to_html("  \n \n").is_empty());
        assert_eq!(to_html("## Part one\n> quoted\n-> centred __u__ ~~s~~"), "<h3>Part one</h3>\n<blockquote><p>quoted</p></blockquote>\n<p style=\"text-align: center;\">centred <span style=\"text-decoration: underline;\">u</span> <del>s</del></p>");
        assert_eq!(to_html("- a\n- b\n1. x\n1. y\nafter"), "<ul><li>a</li><li>b</li></ul>\n<ol><li>x</li><li>y</li></ol>\n<p>after</p>");
    }

    #[test]
    fn back_from_html() {
        let text = "## Part one\n\nHello **there** and *you*\n\n-> centred __u__ ~~s~~\n\n> quoted\n\n- a\n\n- b\n\n1. x\n\n***\n\nEnd";
        assert_eq!(from_html(&to_html(text)), text);
        assert_eq!(from_html("<p>One&nbsp;<b>two </b>three</p>\n<p></p><p>Four<br>Five</p>"), "One **two** three\n\nFour\n\nFive");
    }

    #[test]
    fn docs_paste_is_plain() {
        let html = include_str!("../tests/fixtures/chapter-docs-paste.html");
        let text = from_html(html);
        assert!(text.starts_with("[author] *Cha Ching.* Double Update today"));
        assert!(text.contains("**") && !text.contains("<"));
        assert_eq!(look("color: #000000 background-color: transparent;font-weight: 700;vertical-align: baseline"), Look { bold: true, ..Default::default() });
        assert_eq!(look("color: #ff0000").lost, Some("text colours"));
        assert_eq!(from_html("<p><span style=\"font-weight: 700; font-style: italic\">both</span> x</p>"), "***both*** x");
    }

    #[test]
    fn numbering_restarts() {
        let k: Vec<_> = parse("1. a\n1. b\ntext\n1. c").iter().map(|b| b.kind).collect();
        assert_eq!(k, [Kind::Number(1), Kind::Number(2), Kind::Para, Kind::Number(1)]);
        assert_eq!(parse("---")[0].kind, Kind::Rule);
    }
}
