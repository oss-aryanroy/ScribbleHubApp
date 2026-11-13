use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::BTreeMap;
use std::ops::Range;
use std::path::PathBuf;

#[cfg(unix)]
#[repr(C)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: *const i8,
}

#[cfg(unix)]
unsafe extern "C" {
    fn localtime_r(t: *const i64, out: *mut Tm) -> *mut Tm;
}

#[cfg(windows)]
#[repr(C)]
struct Tm([i32; 9]);

#[cfg(windows)]
unsafe extern "C" {
    fn _localtime64_s(out: *mut Tm, t: *const i64) -> i32;
    fn _mkgmtime64(tm: *mut Tm) -> i64;
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[cfg(unix)]
fn offset(t: i64) -> i64 {
    let mut tm = std::mem::MaybeUninit::<Tm>::zeroed();
    let ok = unsafe { !localtime_r(&t, tm.as_mut_ptr()).is_null() };
    if ok { unsafe { tm.assume_init().gmtoff } } else { 0 }
}

#[cfg(windows)]
fn offset(t: i64) -> i64 {
    let mut tm = Tm([0; 9]);
    if unsafe { _localtime64_s(&mut tm, &t) } != 0 {
        return 0;
    }
    unsafe { _mkgmtime64(&mut tm) - t }
}

pub fn day_of(t: i64) -> i64 {
    (t + offset(t)).div_euclid(86_400)
}

pub fn today() -> i64 {
    day_of(now())
}

pub fn clock_of(t: i64) -> i64 {
    (t + offset(t)).rem_euclid(86_400)
}

pub fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn key(day: i64) -> String {
    let (y, m, d) = civil(day);
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn parse_day(s: &str) -> Option<i64> {
    let mut it = s.trim().split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, d) = (it.next()??, it.next()??, it.next()??);
    ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| days_from_civil(y, m as u32, d as u32))
}

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

pub fn short_date(day: i64) -> String {
    let (_, m, d) = civil(day);
    format!("{d} {}", MONTHS[m as usize - 1])
}

pub fn long_date(day: i64) -> String {
    format!("{}, {}", WEEKDAYS[weekday(day)], short_date(day))
}

pub fn weekday(day: i64) -> usize {
    (day + 3).rem_euclid(7) as usize
}

pub fn when(t: i64) -> String {
    let c = clock_of(t);
    let hm = format!("{:02}:{:02}", c / 3600, c % 3600 / 60);
    match today() - day_of(t) {
        0 => hm,
        1 => format!("Yesterday {hm}"),
        _ => format!("{} {hm}", short_date(day_of(t))),
    }
}

pub fn dir() -> PathBuf {
    let var = |k: &str| std::env::var_os(k).map(PathBuf::from);
    let home = || var("HOME").unwrap_or_else(|| ".".into());
    let base = if cfg!(target_os = "macos") {
        home().join("Library/Application Support")
    } else if cfg!(windows) {
        var("APPDATA").unwrap_or_else(home)
    } else {
        var("XDG_DATA_HOME").unwrap_or_else(|| home().join(".local/share"))
    };
    base.join("ScribbleHub")
}

fn read<T: DeserializeOwned + Default>(rel: &str) -> T {
    std::fs::read(dir().join(rel)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn write<T: Serialize>(rel: &str, v: &T) {
    let path = dir().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = path.with_extension("tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(v) {
        if std::fs::write(&tmp, bytes).is_ok() {
            std::fs::rename(&tmp, &path).ok();
        }
    }
}

fn safe(key: &str) -> String {
    key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Look {
    pub font: u8,
    pub size: f32,
    pub width: f32,
    pub spacing: u8,
    pub highlight: bool,
    pub typewriter: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look { font: 0, size: 17., width: 680., spacing: 1, highlight: true, typewriter: true }
    }
}

pub const FONTS: [(&str, &str); 3] = [("Charis", "Charis SIL"), ("Literata", "Literata"), ("Sans", crate::UI_FONT)];
pub const SPACINGS: [(&str, f32); 3] = [("Tight", 1.45), ("Relaxed", 1.65), ("Airy", 1.9)];

impl Look {
    pub fn family(&self) -> &'static str {
        FONTS[(self.font as usize).min(2)].1
    }

    pub fn line(&self) -> f32 {
        SPACINGS[(self.spacing as usize).min(2)].1
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Deadline {
    pub label: String,
    pub words: u32,
    pub start: i64,
    pub by: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Sprint {
    pub at: i64,
    pub mins: u32,
    pub words: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Card {
    pub title: String,
    pub status: u8,
    pub summary: String,
    pub notes: String,
    pub beat: Option<u8>,
}

pub const STATUSES: [&str; 5] = ["Idea", "Drafting", "Editing", "Ready", "Published"];

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Outline {
    pub template: u8,
    pub cards: BTreeMap<String, Card>,
    pub ideas: Vec<String>,
    pub next_idea: u32,
}

pub const TEMPLATES: [(&str, &[&str]); 3] = [
    ("None", &[]),
    ("Three-act", &["Setup", "Inciting incident", "First turn", "Midpoint", "Crisis", "Climax", "Resolution"]),
    (
        "Save the Cat",
        &[
            "Opening image", "Theme stated", "Set-up", "Catalyst", "Debate", "Break into two", "B story", "Fun and games", "Midpoint", "Bad guys close in", "All is lost", "Dark night", "Break into three", "Finale",
            "Final image",
        ],
    ),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Data {
    pub days: BTreeMap<String, i64>,
    pub daily_goal: u32,
    pub session_goal: u32,
    pub deadline: Option<Deadline>,
    pub sprints: Vec<Sprint>,
    pub look: Look,
    pub outlines: BTreeMap<String, Outline>,
}

impl Default for Data {
    fn default() -> Self {
        Data { days: BTreeMap::new(), daily_goal: 1000, session_goal: 500, deadline: None, sprints: vec![], look: Look::default(), outlines: BTreeMap::new() }
    }
}

pub const STREAK_MIN: i64 = 100;

impl Data {
    pub fn load() -> Data {
        read("writer.json")
    }

    pub fn save(&self) {
        write("writer.json", self)
    }

    pub fn words_on(&self, day: i64) -> i64 {
        self.days.get(&key(day)).copied().unwrap_or(0).max(0)
    }

    pub fn add_words(&mut self, day: i64, n: i64) {
        *self.days.entry(key(day)).or_default() += n;
    }

    pub fn streak(&self, today: i64) -> (u32, u32) {
        let hit = |d: i64| self.words_on(d) >= STREAK_MIN;
        let mut d = if hit(today) { today } else { today - 1 };
        let mut cur = 0;
        while hit(d) {
            cur += 1;
            d -= 1;
        }
        let (mut best, mut run, mut prev) = (0u32, 0u32, None::<i64>);
        for (k, &w) in &self.days {
            let Some(day) = parse_day(k) else { continue };
            if w < STREAK_MIN {
                continue;
            }
            run = if prev == Some(day - 1) { run + 1 } else { 1 };
            best = best.max(run);
            prev = Some(day);
        }
        (cur, best.max(cur))
    }

    pub fn words_between(&self, from: i64, to: i64) -> i64 {
        (from..=to).map(|d| self.words_on(d)).sum()
    }

    pub fn outline(&mut self, sid: &str) -> &mut Outline {
        self.outlines.entry(sid.to_string()).or_default()
    }
}

pub fn pace(d: &Deadline, data: &Data, today: i64) -> (i64, i64, i64) {
    let written = data.words_between(d.start, today);
    let left = (d.by - today + 1).max(0);
    let remaining = (d.words as i64 - written).max(0);
    (written, left, if left == 0 { remaining } else { (remaining + left - 1) / left })
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Draft {
    pub key: String,
    pub title: String,
    pub html: String,
    pub url: String,
    pub sid: String,
    pub series: String,
    pub updated: i64,
    pub uploaded: bool,
    pub words: usize,
}

impl Draft {
    pub fn load(key: &str) -> Option<Draft> {
        let d: Draft = read(&format!("drafts/{}.json", safe(key)));
        (!d.key.is_empty()).then_some(d)
    }

    pub fn save(&self) {
        write(&format!("drafts/{}.json", safe(&self.key)), self)
    }

    pub fn waiting() -> Vec<Draft> {
        let mut out: Vec<Draft> = std::fs::read_dir(dir().join("drafts"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| std::fs::read(e.path()).ok())
            .filter_map(|b| serde_json::from_slice::<Draft>(&b).ok())
            .filter(|d| !d.uploaded && !d.key.is_empty())
            .collect();
        out.sort_by_key(|d| -d.updated);
        out
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub enum SnapKind {
    #[default]
    Auto,
    Named,
    Site,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Snap {
    pub at: i64,
    pub kind: SnapKind,
    pub name: String,
    pub title: String,
    pub html: String,
    pub words: usize,
}

const AUTO_KEEP: usize = 40;

pub fn snaps(key: &str) -> Vec<Snap> {
    read(&format!("snaps/{}.json", safe(key)))
}

pub fn add_snap(key: &str, snap: Snap) -> bool {
    let mut all = snaps(key);
    if snap.kind != SnapKind::Named && all.last().is_some_and(|s| s.html == snap.html && s.title == snap.title) {
        return false;
    }
    all.push(snap);
    let autos = all.iter().filter(|s| s.kind == SnapKind::Auto).count();
    if autos > AUTO_KEEP {
        let mut drop = autos - AUTO_KEEP;
        all.retain(|s| {
            let gone = drop > 0 && s.kind == SnapKind::Auto;
            drop -= usize::from(gone);
            !gone
        });
    }
    write(&format!("snaps/{}.json", safe(key)), &all);
    true
}

#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Same(String),
    Gone(String),
    New(String),
}

fn lcs<T: PartialEq + Clone>(a: &[T], b: &[T]) -> Vec<(i8, T)> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i * (m + 1) + j] = if a[i] == b[j] { t[(i + 1) * (m + 1) + j + 1] + 1 } else { t[(i + 1) * (m + 1) + j].max(t[i * (m + 1) + j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, vec![]);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((0, a[i].clone()));
            i += 1;
            j += 1;
        } else if t[(i + 1) * (m + 1) + j] >= t[i * (m + 1) + j + 1] {
            out.push((-1, a[i].clone()));
            i += 1;
        } else {
            out.push((1, b[j].clone()));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|x| (-1, x.clone())));
    out.extend(b[j..].iter().map(|x| (1, x.clone())));
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub old: Vec<Part>,
    pub new: Vec<Part>,
}

pub fn compare(old: &str, new: &str) -> Vec<Row> {
    let paras = |s: &str| s.split('\n').map(str::trim).filter(|p| !p.is_empty() && *p != "\u{FFFC}").map(String::from).collect::<Vec<_>>();
    let (a, b) = (paras(old), paras(new));
    let ops = lcs(&a, &b);
    let mut rows = vec![];
    let mut i = 0;
    while i < ops.len() {
        match ops[i].0 {
            0 => {
                rows.push(Row { old: vec![Part::Same(ops[i].1.clone())], new: vec![Part::Same(ops[i].1.clone())] });
                i += 1;
            }
            _ => {
                let mut gone = vec![];
                let mut added = vec![];
                while i < ops.len() && ops[i].0 != 0 {
                    if ops[i].0 < 0 { gone.push(ops[i].1.clone()) } else { added.push(ops[i].1.clone()) }
                    i += 1;
                }
                for k in 0..gone.len().max(added.len()) {
                    match (gone.get(k), added.get(k)) {
                        (Some(g), Some(a)) => {
                            let (wg, wa): (Vec<&str>, Vec<&str>) = (g.split_inclusive(' ').collect(), a.split_inclusive(' ').collect());
                            let mut old = vec![];
                            let mut new = vec![];
                            for (op, w) in lcs(&wg, &wa) {
                                let push = |v: &mut Vec<Part>, p: Part| match (v.last_mut(), &p) {
                                    (Some(Part::Same(s)), Part::Same(t)) | (Some(Part::Gone(s)), Part::Gone(t)) | (Some(Part::New(s)), Part::New(t)) => s.push_str(t),
                                    _ => v.push(p),
                                };
                                match op {
                                    0 => {
                                        push(&mut old, Part::Same(w.to_string()));
                                        push(&mut new, Part::Same(w.to_string()));
                                    }
                                    -1 => push(&mut old, Part::Gone(w.to_string())),
                                    _ => push(&mut new, Part::New(w.to_string())),
                                }
                            }
                            rows.push(Row { old, new });
                        }
                        (Some(g), None) => rows.push(Row { old: vec![Part::Gone(g.clone())], new: vec![] }),
                        (None, Some(a)) => rows.push(Row { old: vec![], new: vec![Part::New(a.clone())] }),
                        _ => {}
                    }
                }
            }
        }
    }
    rows
}

pub fn trim_rows(rows: Vec<Row>, context: usize) -> Vec<Result<Row, usize>> {
    let same = |r: &Row| r.old.len() == 1 && r.new.len() == 1 && matches!((&r.old[0], &r.new[0]), (Part::Same(_), Part::Same(_)));
    let changed: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| !same(r)).map(|(i, _)| i).collect();
    let near = |i: usize| changed.iter().any(|&c| c.abs_diff(i) <= context);
    let mut out = vec![];
    let mut hidden = 0;
    for (i, r) in rows.into_iter().enumerate() {
        if near(i) {
            if hidden > 0 {
                out.push(Err(hidden));
                hidden = 0;
            }
            out.push(Ok(r));
        } else {
            hidden += 1;
        }
    }
    if hidden > 0 {
        out.push(Err(hidden));
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Analysis {
    pub words: usize,
    pub sentences: usize,
    pub dialogue: f32,
    pub grade: f32,
    pub long: Vec<Range<usize>>,
    pub adverbs: Vec<Range<usize>>,
    pub passive: Vec<Range<usize>>,
    pub repeats: Vec<(String, Vec<Range<usize>>)>,
}

const STOP: &[&str] = &[
    "the", "and", "a", "to", "of", "in", "it", "is", "was", "that", "he", "she", "her", "his", "i", "you", "for", "on", "with", "as", "at", "be", "had", "but", "they", "not", "this", "have", "from", "by", "or", "an", "were", "are", "him",
    "my", "me", "so", "we", "what", "all", "their", "there", "would", "could", "do", "did", "if", "no", "out", "up", "one", "been", "said", "its", "them", "into", "who", "which", "will", "your", "can", "when", "than", "our", "us", "about",
    "just", "like", "now", "then", "back", "down",
];

const FILLERS: &[&str] = &["then", "just", "like", "back", "down", "really", "very", "suddenly", "started", "began"];

const NOT_ADVERBS: &[&str] = &[
    "only", "family", "reply", "early", "daily", "holy", "ugly", "belly", "apply", "supply", "july", "italy", "rely", "ally", "bully", "jelly", "lovely", "friendly", "lonely", "silly", "likely", "fly", "butterfly", "assembly", "anomaly",
    "elderly", "costly", "deadly", "lively", "orderly", "monthly", "weekly", "hourly", "nightly", "yearly", "curly", "chilly", "hilly", "kindly",
];

const PARTICIPLES: &[&str] = &[
    "taken", "given", "done", "seen", "known", "made", "held", "thrown", "written", "broken", "eaten", "found", "left", "told", "caught", "built", "sent", "brought", "bought", "kept", "beaten", "chosen", "driven", "forgotten", "hidden",
    "spoken", "stolen", "torn", "worn", "born", "shown", "drawn", "grown", "frozen", "hit", "cut", "put", "set", "shot", "struck", "bound", "led", "fed", "paid", "said",
];

fn syllables(w: &str) -> usize {
    let w = w.to_lowercase();
    let mut n = 0;
    let mut prev = false;
    for c in w.chars() {
        let v = "aeiouy".contains(c);
        if v && !prev {
            n += 1;
        }
        prev = v;
    }
    if w.ends_with('e') && !w.ends_with("le") && n > 1 {
        n -= 1;
    }
    n.max(1)
}

pub fn analyze(text: &str) -> Analysis {
    let mut a = Analysis::default();
    let mut words: Vec<(Range<usize>, String)> = vec![];
    let mut start = None;
    for (i, c) in text.char_indices().chain([(text.len(), ' ')]) {
        let wordy = c.is_alphanumeric() || c == '\'' || c == '’';
        match (wordy, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                let w = text[s..i].trim_matches(|c| c == '\'' || c == '’');
                if w.chars().any(char::is_alphabetic) {
                    words.push((s..s + w.len(), w.to_lowercase()));
                }
                start = None;
            }
            _ => {}
        }
    }
    a.words = words.len();
    let mut sents: Vec<Range<usize>> = vec![];
    let mut s0 = 0;
    for (i, c) in text.char_indices() {
        if matches!(c, '.' | '!' | '?' | '\n') {
            let end = i + c.len_utf8();
            if text[s0..end].chars().any(char::is_alphabetic) {
                sents.push(s0..end);
            }
            s0 = end;
        }
    }
    if text[s0..].chars().any(char::is_alphabetic) {
        sents.push(s0..text.len());
    }
    a.sentences = sents.len();
    for s in &sents {
        let n = words.iter().filter(|(r, _)| r.start >= s.start && r.end <= s.end).count();
        if n > 35 {
            let t = &text[s.clone()];
            let lead = t.len() - t.trim_start().len();
            a.long.push(s.start + lead..s.end);
        }
    }
    let (mut inside, mut quoted, mut total) = (false, 0usize, 0usize);
    for c in text.chars() {
        match c {
            '“' => inside = true,
            '”' => inside = false,
            '"' => inside = !inside,
            '\n' => inside = false,
            _ if !c.is_whitespace() => {
                total += 1;
                quoted += usize::from(inside);
            }
            _ => {}
        }
    }
    a.dialogue = if total == 0 { 0. } else { quoted as f32 / total as f32 };
    let syl: usize = words.iter().map(|(_, w)| syllables(w)).sum();
    if a.words > 0 && a.sentences > 0 {
        a.grade = (0.39 * a.words as f32 / a.sentences as f32 + 11.8 * syl as f32 / a.words as f32 - 15.59).max(0.);
    }
    for (k, (r, w)) in words.iter().enumerate() {
        if w.len() > 4 && w.ends_with("ly") && !NOT_ADVERBS.contains(&w.as_str()) {
            a.adverbs.push(r.clone());
        }
        if matches!(w.as_str(), "am" | "is" | "are" | "was" | "were" | "be" | "been" | "being") {
            let mut j = k + 1;
            if words.get(j).is_some_and(|(_, w)| w.ends_with("ly")) {
                j += 1;
            }
            if let Some((r2, p)) = words.get(j) {
                let same_sentence = !text[r.end..r2.start].contains(['.', '!', '?', '\n']);
                if same_sentence && ((p.ends_with("ed") && p.len() > 3) || PARTICIPLES.contains(&p.as_str())) {
                    a.passive.push(r.start..r2.end);
                }
            }
        }
    }
    let mut counts: BTreeMap<&str, Vec<Range<usize>>> = BTreeMap::new();
    for (r, w) in &words {
        if w.len() >= 3 && (FILLERS.contains(&w.as_str()) || !STOP.contains(&w.as_str())) {
            counts.entry(w.as_str()).or_default().push(r.clone());
        }
    }
    let floor = (a.words / 300).max(6);
    let mut over: Vec<(String, Vec<Range<usize>>)> = counts
        .into_iter()
        .filter(|(w, rs)| rs.len() >= if FILLERS.contains(w) { floor } else { floor * 2 })
        .map(|(w, rs)| (w.to_string(), rs))
        .collect();
    over.retain(|(_, rs)| rs.iter().filter(|r| text[r.start..].chars().next().is_some_and(char::is_uppercase)).count() * 2 <= rs.len());
    over.sort_by_key(|(_, rs)| std::cmp::Reverse(rs.len()));
    over.truncate(3);
    a.repeats = over;
    a
}

pub fn find_all(text: &str, needle: &str, case: bool, whole: bool) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return vec![];
    }
    let (hay, pat) = if case { (text.to_string(), needle.to_string()) } else { (text.to_lowercase(), needle.to_lowercase()) };
    if hay.len() != text.len() {
        return vec![];
    }
    let wordy = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    hay.match_indices(&pat)
        .map(|(i, m)| i..i + m.len())
        .filter(|r| !whole || (!wordy(text[..r.start].chars().next_back()) && !wordy(text[r.end..].chars().next())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip() {
        assert_eq!(key(0), "1970-01-01");
        assert_eq!(parse_day("2026-11-13").map(key).as_deref(), Some("2026-11-13"));
        assert_eq!(parse_day("2028-02-29").map(civil), Some((2028, 2, 29)));
        assert_eq!(weekday(parse_day("2026-11-13").unwrap()), 4);
        assert!(parse_day("2026-13-01").is_none() && parse_day("soon").is_none());
    }

    #[test]
    fn streaks_and_pace() {
        let mut d = Data::default();
        let t = parse_day("2026-11-13").unwrap();
        for k in 1..=5 {
            d.add_words(t - k, 300);
        }
        d.add_words(t - 9, 300);
        d.add_words(t - 10, 300);
        assert_eq!(d.streak(t), (5, 5));
        d.add_words(t, 120);
        assert_eq!(d.streak(t).0, 6);
        let dl = Deadline { label: "x".into(), words: 10_000, start: t - 5, by: t + 9 };
        let (written, left, per_day) = pace(&dl, &d, t);
        assert_eq!((written, left), (1620, 10));
        assert_eq!(per_day, 838);
    }

    #[test]
    fn compare_finds_word_changes() {
        let rows = compare("One.\nShe searched online.\nEnd.", "One.\nShe searched the map.\nNew line.\nEnd.");
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[1].old, vec![Part::Same("She searched ".into()), Part::Gone("online.".into())]);
        assert_eq!(rows[1].new, vec![Part::Same("She searched ".into()), Part::New("the map.".into())]);
        assert_eq!(rows[2], Row { old: vec![], new: vec![Part::New("New line.".into())] });
        let trimmed = trim_rows(compare("a\nb\nc\nd\ne\nf", "a\nb\nc\nd\ne\nF"), 1);
        assert_eq!(trimmed.len(), 3);
        assert_eq!(trimmed[0], Err(4));
    }

    #[test]
    fn hints() {
        let text = "She quickly ran. The door was opened by him. Then she ran then ran then ran then ran then ran then ran.\n“Hello,” she said.";
        let a = analyze(text);
        assert_eq!(&text[a.adverbs[0].clone()], "quickly");
        assert_eq!(&text[a.passive[0].clone()], "was opened");
        assert_eq!(a.repeats.first().map(|r| r.0.as_str()), Some("then"));
        assert!(a.dialogue > 0. && a.sentences == 4);
        let names = analyze(&"Sera ran. ".repeat(20));
        assert!(names.repeats.iter().all(|r| r.0 != "sera"));
        assert_eq!(find_all("Shoku and shoku, Shokuhou", "shoku", false, true).len(), 2);
        assert_eq!(find_all("Shoku and shoku", "Shoku", true, false), vec![0..5]);
    }
}
