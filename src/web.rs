use futures::{FutureExt, channel::mpsc, channel::oneshot, future::BoxFuture};
use gpui::{Bounds, Pixels, Window};
use http_client::{AsyncBody, HttpClient, Request, Response, Url, http::HeaderValue};
use wry::{NewWindowResponse, PageLoadEvent, Rect, WebContext, WebView, WebViewBuilder, dpi};

use crate::site;

pub enum Event {
    Finished,
    Page { url: String, title: String, html: String },
    Toc { html: String },
    Suggest { query: String, html: String },
    Alerts { html: String },
    RlDeleted { ok: bool },
    Ajax { tag: String, text: String },
    Saved { ok: bool, msg: String },
    SeriesDeleted { id: String, ok: bool },
    RlMore { html: String, next: String, more: bool },
}

pub struct Web {
    view: WebView,
    _ctx: WebContext,
}

impl Web {
    pub fn new(window: &Window) -> anyhow::Result<(Self, mpsc::UnboundedReceiver<Event>)> {
        let (tx, rx) = mpsc::unbounded();
        let on_load = tx.clone();
        let mut ctx = WebContext::new(Some(crate::writer::dir().join("WebView")));
        let view = WebViewBuilder::new_with_web_context(&mut ctx)
            .with_visible(false)
            .with_on_page_load_handler(move |e, _| {
                if matches!(e, PageLoadEvent::Finished) {
                    on_load.unbounded_send(Event::Finished).ok();
                }
            })
            .with_ipc_handler(move |req| {
                let Ok(v) = serde_json::from_str::<serde_json::Value>(req.body()) else { return };
                let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
                let event = match v["kind"].as_str() {
                    Some("page") => Event::Page { url: s("url"), title: s("title"), html: s("html") },
                    Some("toc") => Event::Toc { html: s("html") },
                    Some("suggest") => Event::Suggest { query: s("q"), html: s("html") },
                    Some("alerts") => Event::Alerts { html: s("html") },
                    Some("rldel") => Event::RlDeleted { ok: v["ok"].as_bool().unwrap_or(false) },
                    Some("ajax") => Event::Ajax { tag: s("tag"), text: s("text") },
                    Some("saved") => Event::Saved { ok: v["ok"].as_bool().unwrap_or(false), msg: s("msg") },
                    Some("seriesdel") => Event::SeriesDeleted { id: s("id"), ok: v["ok"].as_bool().unwrap_or(false) },
                    Some("rlmore") => Event::RlMore { html: s("html"), next: s("next"), more: v["more"].as_bool().unwrap_or(false) },
                    _ => return,
                };
                tx.unbounded_send(event).ok();
            })
            .with_new_window_req_handler(|_, _| NewWindowResponse::Deny)
            .build_as_child(window)?;
        Ok((Self { view, _ctx: ctx }, rx))
    }

    pub fn load(&self, url: &str) {
        self.view.load_url(url).ok();
    }

    pub fn ajax(&self, tag: &str, body: &str) {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
        self.view
            .evaluate_script(&format!(
                "fetch('/wp-admin/admin-ajax.php',{{method:'POST',headers:{{'Content-Type':'application/x-www-form-urlencoded'}},body:{b}}})\
                 .then(r=>r.text(),()=>'').then(t=>window.ipc.postMessage(JSON.stringify({{kind:'ajax',tag:{t},text:t}})))",
                b = q(body),
                t = q(tag),
            ))
            .ok();
    }

    pub fn get(&self, tag: &str, url: &str) {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
        self.view
            .evaluate_script(&format!(
                "fetch({u},{{credentials:'same-origin'}}).then(r=>r.text(),()=>'').then(t=>window.ipc.postMessage(JSON.stringify({{kind:'ajax',tag:{t},text:t}})))",
                u = q(url),
                t = q(tag),
            ))
            .ok();
    }

    pub fn eval(&self, js: &str) {
        self.view.evaluate_script(js).ok();
    }

    pub fn snapshot(&self) {
        self.view
            .evaluate_script(
                "window.ipc.postMessage(JSON.stringify({kind:'page',url:location.href,title:document.title,\
                 html:document.documentElement.outerHTML}))",
            )
            .ok();
    }

    pub fn fetch_toc(&self, series_id: &str) {
        let id: String = series_id.chars().filter(char::is_ascii_digit).collect();
        self.view
            .evaluate_script(&format!(
                "fetch('/wp-admin/admin-ajax.php',{{method:'POST',\
                 headers:{{'Content-Type':'application/x-www-form-urlencoded'}},\
                 body:'action=wi_gettocchp&strSID={id}&strmypostid=0&strFic=yes'}})\
                 .then(r=>r.text()).then(html=>window.ipc.postMessage(JSON.stringify({{kind:'toc',html}})))"
            ))
            .ok();
    }

    pub fn suggest(&self, query: &str) {
        let q = serde_json::to_string(query).unwrap_or_default();
        self.view
            .evaluate_script(&format!(
                "(q=>fetch('/wp-admin/admin-ajax.php',{{method:'POST',\
                 headers:{{'Content-Type':'application/x-www-form-urlencoded'}},\
                 body:'action=wi_ajaxsearchmain&strType=series&strOne='+encodeURIComponent(q)}})\
                 .then(r=>r.text()).then(html=>window.ipc.postMessage(JSON.stringify({{kind:'suggest',q,html}}))))({q})"
            ))
            .ok();
    }

    pub fn fetch_alerts(&self) {
        self.view
            .evaluate_script(
                "fetch('/wp-admin/admin-ajax.php',{method:'POST',\
                 headers:{'Content-Type':'application/x-www-form-urlencoded'},body:'action=wi_getalerts'})\
                 .then(r=>r.text()).then(t=>{if(t.endsWith('0'))t=t.slice(0,-1);\
                 window.ipc.postMessage(JSON.stringify({kind:'alerts',html:JSON.parse(t).data||''}))})",
            )
            .ok();
    }

    pub fn fetch_rl_more(&self, m: &site::RlMore) {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
        self.view
            .evaluate_script(&format!(
                "(n,l,o,s=>fetch('/wp-admin/admin-ajax.php',{{method:'POST',\
                 headers:{{'Content-Type':'application/x-www-form-urlencoded'}},\
                 body:'action=wi_rl_load_page&nonce='+encodeURIComponent(n)+'&list_id='+encodeURIComponent(l)+\
                 '&offset='+encodeURIComponent(o)+'&filters=%7B%7D&sort='+encodeURIComponent(s)+'&is_app=0'}})\
                 .then(r=>r.text()).then(t=>{{if(t.endsWith('0'))t=t.slice(0,-1);const j=JSON.parse(t);\
                 window.ipc.postMessage(JSON.stringify({{kind:'rlmore',html:(j.data&&j.data.html)||'',\
                 next:String((j.data&&j.data.next_offset)||''),more:!!(j.data&&j.data.has_more)}}))}}))\
                 ({n},{l},{o},{s})",
                n = q(&m.nonce),
                l = q(&m.list_id),
                o = q(&m.offset),
                s = q(&m.sort),
            ))
            .ok();
    }

    pub fn mark_rl_read(&self, sid: &str, cid: &str) {
        self.view
            .evaluate_script(&format!(
                "fetch('/wp-admin/admin-ajax.php',{{method:'POST',\
                 headers:{{'Content-Type':'application/x-www-form-urlencoded'}},\
                 body:'action=wi_addchangerl&strCID={cid}&strSID={sid}'}})"
            ))
            .ok();
    }

    pub fn delete_rl(&self, to: &str, nonce: &str, sids: &[String]) {
        let q = |s: &str| serde_json::to_string(s).unwrap_or_default();
        self.view
            .evaluate_script(&format!(
                "((u,n,ids)=>fetch(u,{{method:'POST',\
                 headers:{{'Content-Type':'application/x-www-form-urlencoded'}},\
                 body:'hid_delete_list=0&hid_delete_rl='+encodeURIComponent(ids)+'&wi_rl_nonce='+encodeURIComponent(n)}})\
                 .then(r=>r.ok,()=>false).then(ok=>window.ipc.postMessage(JSON.stringify({{kind:'rldel',ok}}))))\
                 ({u},{n},{ids})",
                u = q(to),
                n = q(nonce),
                ids = q(&sids.join(",")),
            ))
            .ok();
    }

    pub fn delete_series(&self, id: &str) {
        let id: String = id.chars().filter(char::is_ascii_digit).collect();
        self.view
            .evaluate_script(&format!(
                "fetch('/wp-admin/admin-ajax.php',{{method:'POST',\
                 headers:{{'Content-Type':'application/x-www-form-urlencoded'}},\
                 body:'action=wi_deleteseries&strPostID={id}'}})\
                 .then(r=>r.ok,()=>false).then(ok=>window.ipc.postMessage(JSON.stringify({{kind:'seriesdel',id:'{id}',ok}})))"
            ))
            .ok();
    }

    pub fn show(&self, bounds: Option<Bounds<Pixels>>) {
        if let Some(b) = bounds {
            self.view
                .set_bounds(Rect {
                    position: dpi::LogicalPosition::new(f32::from(b.origin.x), f32::from(b.origin.y)).into(),
                    size: dpi::LogicalSize::new(f32::from(b.size.width), f32::from(b.size.height)).into(),
                })
                .ok();
        }
        self.view.set_visible(bounds.is_some()).ok();
    }
}

pub struct Images;

impl HttpClient for Images {
    fn type_name(&self) -> &'static str {
        "ureq"
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(&self, req: Request<AsyncBody>) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let uri = req.uri().to_string();
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let fetch = || -> anyhow::Result<Response<AsyncBody>> {
                let mut resp = ureq::get(&uri).call()?;
                let bytes = resp.body_mut().read_to_vec()?;
                Ok(Response::builder().status(resp.status().as_u16()).body(bytes.into())?)
            };
            tx.send(fetch()).ok();
        });
        async move { rx.await? }.boxed()
    }
}
