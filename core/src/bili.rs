//! Adapter on top of `bpi-rs`: normalises responses into the JSON shape the
//! QML front-end consumes, and keeps the optional logged-in cookie state.

use std::collections::{BTreeMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use bpi_rs::BpiClient;
use bpi_rs::ids::{Bvid, Cid, Mid};
use bpi_rs::search::{SearchBiliUserParams, SearchVideoParams};
use bpi_rs::user::UserUploadedVideosParams;
use bpi_rs::video::{VideoHomepageRecommendationsParams, VideoPlayUrlParams, VideoViewParams};
use cookie_scoop::{
    BrowserName, CookieHeaderOptions, GetCookiesOptions, get_cookies, to_cookie_header,
};
use flate2::read::DeflateDecoder;
use md5::{Digest, Md5};
use serde::Deserialize;
use serde_json::{Value, json};

const REFERER: &str = "https://www.bilibili.com/";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
                  (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const COOKIE_NAMES: [&str; 7] = [
    "SESSDATA",
    "bili_jct",
    "DedeUserID",
    "DedeUserID__ckMd5",
    "buvid3",
    "buvid4",
    "b_nut",
];

pub struct Bili {
    client: RwLock<Arc<BpiClient>>,
    cookie: RwLock<Option<String>>,
    auth_path: PathBuf,
    http: reqwest::Client,
}

#[derive(Deserialize)]
#[serde(rename = "i")]
struct DanmakuDoc {
    #[serde(rename = "d", default)]
    items: Vec<DanmakuEntry>,
}

#[derive(Deserialize)]
struct DanmakuEntry {
    #[serde(rename = "@p")]
    p: String,
    #[serde(rename = "$text", default)]
    text: String,
}

impl Bili {
    pub fn new() -> Self {
        let auth_path = auth_path();
        let cookie = load_cookie(&auth_path);
        let client = build_client(cookie.as_deref());

        // The danmaku XML endpoint sends raw (header-less) deflate; reqwest's
        // automatic decoder expects zlib, so handle it ourselves.
        let http = reqwest::Client::builder()
            .user_agent(UA)
            .no_deflate()
            .build()
            .expect("bili: failed to build http client");

        Self {
            client: RwLock::new(Arc::new(client)),
            cookie: RwLock::new(cookie),
            auth_path,
            http,
        }
    }

    fn client(&self) -> Arc<BpiClient> {
        self.client
            .read()
            .expect("bili: client lock poisoned")
            .clone()
    }

    pub async fn feed(&self, fresh_idx: u32) -> Result<Vec<Value>> {
        // `fresh_idx` is the refresh counter the web UI increments on every
        // scroll; each value returns a new batch of recommendations.
        let params = VideoHomepageRecommendationsParams::new()
            .fresh_idx(fresh_idx.max(1))
            .map_err(anyhow::Error::msg)?;
        let data = self
            .client()
            .video()
            .homepage_recommendations(params)
            .await
            .map_err(anyhow::Error::msg)
            .context("fetch homepage recommendations")?;

        Ok(data
            .item
            .into_iter()
            .filter(|item| item.goto == "av" && !item.bvid.is_empty() && !item.title.is_empty())
            .map(|item| {
                json!({
                    "bvid": item.bvid,
                    "cid": item.cid,
                    "title": item.title,
                    "up": item.owner.name,
                    "duration": item.duration,
                    "views": item.stat.as_ref().map(|stat| stat.view).unwrap_or(0),
                    "cover": pic_url(&item.pic),
                    "pubdate": item.pubdate,
                    "desc": "",
                })
            })
            .collect())
    }

    pub async fn search(&self, keyword: &str, kind: &str) -> Result<Vec<Value>> {
        if kind == "user" {
            let params = SearchBiliUserParams::new(keyword).map_err(anyhow::Error::msg)?;
            let data = self
                .client()
                .search()
                .bili_user(params)
                .await
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("search users {keyword:?}"))?;

            return Ok(data
                .result
                .unwrap_or_default()
                .into_iter()
                .map(|user| {
                    json!({
                        "mid": user.mid,
                        "name": user.uname,
                        "sign": user.usign,
                        "fans": user.fans,
                        "videos": user.videos,
                        "avatar": pic_url(&user.upic),
                        "level": user.level,
                    })
                })
                .collect());
        }

        let params = SearchVideoParams::new(keyword).map_err(anyhow::Error::msg)?;
        let data = self
            .client()
            .search()
            .video(params)
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("search {keyword:?}"))?;

        Ok(data
            .result
            .unwrap_or_default()
            .into_iter()
            .map(|item| {
                json!({
                    "bvid": item.bvid,
                    "cid": 0,
                    "title": strip_tags(&item.title),
                    "up": item.author,
                    "duration": parse_duration(&item.duration),
                    "views": item.play,
                    "cover": pic_url(&item.pic),
                    "pubdate": item.pubdate,
                    "desc": "",
                })
            })
            .collect())
    }

    pub async fn user_videos(&self, mid: u64, page: u32) -> Result<Value> {
        let user = Mid::new(mid).map_err(anyhow::Error::msg)?;
        let params = UserUploadedVideosParams::new(user)
            .with_page(page.max(1))
            .map_err(anyhow::Error::msg)?;

        let data = self
            .client()
            .user()
            .uploaded_videos(params)
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("fetch uploads for mid {mid}"))?;

        if data.is_risk {
            bail!("B 站风控，暂时取不到这个 UP 的投稿列表");
        }

        let items: Vec<Value> = data
            .list
            .videos
            .into_iter()
            .map(|video| {
                json!({
                    "bvid": video.bvid.as_str(),
                    "cid": 0,
                    "title": video.title,
                    "up": video.author,
                    "duration": parse_duration(&video.length),
                    "views": video.play,
                    "cover": pic_url(&video.pic),
                    "pubdate": video.created,
                    "desc": video.description,
                })
            })
            .collect();

        Ok(json!({ "items": items, "count": data.page.count }))
    }

    pub async fn comments(&self, bvid: &str, offset: &str, sort: &str) -> Result<Value> {
        let id: Bvid = bvid.parse().map_err(anyhow::Error::msg)?;
        let view = self
            .client()
            .video()
            .view(VideoViewParams::from_bvid(id))
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("resolve aid for {bvid}"))?;

        let aid = view.aid.get();
        let mode = if sort == "time" { "2" } else { "3" };

        let (img, sub) = self.wbi_keys().await?;
        let mixin = wbi_mixin_key(&img, &sub);

        let mut params: BTreeMap<&str, String> = BTreeMap::new();
        params.insert("oid", aid.to_string());
        params.insert("type", "1".to_string());
        params.insert("mode", mode.to_string());
        params.insert("pagination_str", format!("{{\"offset\":\"{offset}\"}}"));
        params.insert("plat", "1".to_string());
        params.insert("web_location", "1315875".to_string());

        let query = wbi_signed_query(&params, &mixin);
        let url = format!("https://api.bilibili.com/x/v2/reply/wbi/main?{query}");

        let payload: Value = self
            .http
            .get(&url)
            .header("Referer", REFERER)
            .send()
            .await
            .context("fetch comments")?
            .error_for_status()
            .context("comments http status")?
            .json()
            .await
            .context("parse comments")?;

        if payload["code"].as_i64().unwrap_or(-1) != 0 {
            bail!(
                "B 站评论接口返回 {}: {}",
                payload["code"],
                payload["message"].as_str().unwrap_or("")
            );
        }

        let data = &payload["data"];
        let mut items: Vec<Value> = Vec::new();
        let mut seen: HashSet<i64> = HashSet::new();

        if offset.is_empty()
            && let Some(top) = data["top_replies"].as_array()
        {
            for comment in top {
                let value = comment_to_json(comment);
                if seen.insert(value["rpid"].as_i64().unwrap_or(0)) {
                    items.push(value);
                }
            }
        }

        if let Some(replies) = data["replies"].as_array() {
            for comment in replies {
                let value = comment_to_json(comment);
                if seen.insert(value["rpid"].as_i64().unwrap_or(0)) {
                    items.push(value);
                }
            }
        }

        let next = data["cursor"]["pagination_reply"]["next_offset"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let is_end = data["cursor"]["is_end"].as_bool().unwrap_or(true);
        let count = data["cursor"]["all_count"].as_u64().unwrap_or(0);

        Ok(json!({
            "items": items,
            "next": next,
            "is_end": is_end,
            "count": count,
        }))
    }

    async fn wbi_keys(&self) -> Result<(String, String)> {
        let payload: Value = self
            .http
            .get("https://api.bilibili.com/x/web-interface/nav")
            .header("Referer", REFERER)
            .send()
            .await
            .context("fetch nav for wbi keys")?
            .error_for_status()?
            .json()
            .await?;

        let img = payload["data"]["wbi_img"]["img_url"].as_str().unwrap_or("");
        let sub = payload["data"]["wbi_img"]["sub_url"].as_str().unwrap_or("");
        if img.is_empty() || sub.is_empty() {
            bail!("拿不到 WBI 签名 key");
        }
        Ok((wbi_file_key(img), wbi_file_key(sub)))
    }

    pub async fn danmaku(&self, cid: u64) -> Result<Vec<Value>> {
        let url = format!("https://comment.bilibili.com/{cid}.xml");
        let response = self
            .http
            .get(&url)
            .header("Referer", REFERER)
            .header("Accept-Encoding", "identity")
            .send()
            .await
            .context("fetch danmaku xml")?
            .error_for_status()
            .context("danmaku http status")?;
        let bytes = response.bytes().await.context("read danmaku body")?;

        let xml = if bytes.first() == Some(&b'<') {
            String::from_utf8_lossy(&bytes).into_owned()
        } else {
            let mut out = String::new();
            DeflateDecoder::new(&bytes[..])
                .read_to_string(&mut out)
                .context("inflate danmaku xml")?;
            out
        };

        let doc: DanmakuDoc = quick_xml::de::from_str(&xml).context("parse danmaku xml")?;
        let mut items: Vec<Value> = doc
            .items
            .into_iter()
            .filter_map(|entry| {
                let mut parts = entry.p.split(',');
                let time: f32 = parts.next()?.parse().ok()?;
                let mode: i32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(1);
                let _size = parts.next();
                let color: i32 = parts
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(16777215);
                Some(json!({
                    "time": time,
                    "mode": mode,
                    "color": color,
                    "text": entry.text,
                }))
            })
            .collect();

        // The XML order is by dmid, not by playback time.
        items.sort_by(|a, b| {
            a["time"]
                .as_f64()
                .partial_cmp(&b["time"].as_f64())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(items)
    }

    pub async fn view(&self, bvid: &str) -> Result<Value> {
        let id: Bvid = bvid.parse().map_err(anyhow::Error::msg)?;
        let view = self
            .client()
            .video()
            .view(VideoViewParams::from_bvid(id))
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("fetch video {bvid}"))?;

        serde_json::to_value(view).context("serialize video view")
    }

    pub async fn auth_status(&self) -> Result<Value> {
        let nav = self
            .client()
            .login()
            .nav()
            .await
            .map_err(anyhow::Error::msg)
            .context("fetch login nav")?;

        Ok(json!({
            "logged_in": nav.is_login,
            "uname": nav.uname,
        }))
    }

    pub async fn import_cookies(&self) -> Result<Value> {
        let names = COOKIE_NAMES.iter().map(|name| name.to_string()).collect();
        let result = get_cookies(
            GetCookiesOptions::new("https://www.bilibili.com")
                .browsers(vec![BrowserName::Firefox, BrowserName::Chrome])
                .names(names),
        )
        .await;

        if result.cookies.is_empty() {
            let detail = if result.warnings.is_empty() {
                String::new()
            } else {
                format!("（{}）", result.warnings.join("；"))
            };
            bail!("没有从浏览器读到 B 站 cookie{detail}");
        }

        let header = to_cookie_header(&result.cookies, &CookieHeaderOptions::default());
        if !header
            .split(';')
            .any(|part| part.trim().starts_with("SESSDATA="))
        {
            bail!("浏览器里的 B 站会话缺少 SESSDATA，可能没有登录过");
        }

        let client = build_client(Some(&header));
        let nav = client
            .login()
            .nav()
            .await
            .map_err(anyhow::Error::msg)
            .context("validate imported cookies")?;

        if !nav.is_login {
            bail!("浏览器里的 B 站 cookie 已失效，请重新登录后再导入");
        }

        *self.client.write().expect("bili: client lock poisoned") = Arc::new(client);
        *self.cookie.write().expect("bili: cookie lock poisoned") = Some(header.clone());
        save_cookie(&self.auth_path, &header)?;

        Ok(json!({
            "ok": true,
            "uname": nav.uname,
            "warnings": result.warnings,
        }))
    }

    pub async fn play(&self, bvid: &str, cid: u64, qn: u64) -> Result<Value> {
        let id: Bvid = bvid.parse().map_err(anyhow::Error::msg)?;
        let cid = if cid == 0 {
            // Search results don't carry a cid; resolve it from the detail API.
            let view = self
                .client()
                .video()
                .view(VideoViewParams::from_bvid(id.clone()))
                .await
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("resolve cid for {bvid}"))?;
            view.cid
        } else {
            Cid::new(cid).map_err(anyhow::Error::msg)?
        };

        let params = VideoPlayUrlParams::from_bvid(id, cid)
            .quality(qn)
            .format_flags(4048)
            .fourk(true);

        let data = self
            .client()
            .video()
            .play_url(params)
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("fetch play url for {bvid}"))?;

        let dash = data.dash.ok_or_else(|| anyhow!("这个视频没有 DASH 流"))?;

        let video = dash
            .video
            .iter()
            .filter(|stream| stream.id <= qn)
            .max_by_key(|stream| (stream.id, codec_rank(&stream.codecs)))
            .or_else(|| dash.video.iter().max_by_key(|stream| stream.id))
            .ok_or_else(|| anyhow!("没有可用的视频流"))?;

        let audio = dash.audio.iter().max_by_key(|stream| stream.bandwidth);

        Ok(json!({
            "video_url": video.base_url,
            "audio_url": audio.map(|stream| stream.base_url.clone()).unwrap_or_default(),
            "cid": cid.get(),
            "quality": video.id,
            "width": video.width,
            "height": video.height,
            "codecs": video.codecs,
            "duration": dash.duration,
            "headers": {
                "Referer": REFERER,
                "User-Agent": UA,
            },
        }))
    }
}

fn build_client(cookie: Option<&str>) -> BpiClient {
    let builder = BpiClient::builder();
    let builder = match cookie {
        Some(cookie) => builder.cookie(cookie),
        None => builder,
    };
    builder.build().expect("bpi-rs: failed to build client")
}

fn auth_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("bili-mini").join("auth.json")
}

fn load_cookie(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["cookie"]
        .as_str()
        .filter(|cookie| !cookie.trim().is_empty())
        .map(|cookie| cookie.to_string())
}

fn save_cookie(path: &Path, cookie: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    }
    let body = serde_json::to_vec_pretty(&json!({ "cookie": cookie }))?;
    std::fs::write(path, body).with_context(|| format!("write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn codec_rank(codecs: &str) -> u8 {
    if codecs.starts_with("avc") {
        3
    } else if codecs.starts_with("hev") || codecs.starts_with("hvc") {
        2
    } else if codecs.starts_with("av01") {
        1
    } else {
        0
    }
}

const MIXIN_TABLE: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

fn wbi_file_key(url: &str) -> String {
    url.rsplit('/')
        .next()
        .unwrap_or("")
        .split('.')
        .next()
        .unwrap_or("")
        .to_string()
}

fn wbi_mixin_key(img: &str, sub: &str) -> String {
    let raw: Vec<char> = format!("{img}{sub}").chars().collect();
    MIXIN_TABLE
        .iter()
        .map(|&index| raw.get(index).copied().unwrap_or('0'))
        .take(32)
        .collect()
}

fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn wbi_signed_query(params: &BTreeMap<&str, String>, mixin: &str) -> String {
    let wts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);

    let mut pairs: Vec<(&str, String)> = params
        .iter()
        .map(|(key, value)| (*key, value.replace(['!', '\'', '(', ')', '*'], "")))
        .collect();
    pairs.push(("wts", wts.to_string()));
    pairs.sort_by(|a, b| a.0.cmp(b.0));

    let query = pairs
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                encode_uri_component(key),
                encode_uri_component(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&");

    let mut hasher = Md5::new();
    hasher.update(query.as_bytes());
    hasher.update(mixin.as_bytes());
    let digest = hasher.finalize();
    let w_rid = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    format!("{query}&w_rid={w_rid}")
}

fn comment_to_json(comment: &Value) -> Value {
    json!({
        "rpid": comment["rpid"].as_i64().unwrap_or(0),
        "name": comment["member"]["uname"].as_str().unwrap_or(""),
        "avatar": pic_url(comment["member"]["avatar"].as_str().unwrap_or("")),
        "message": comment["content"]["message"].as_str().unwrap_or(""),
        "like": comment["like"].as_i64().unwrap_or(0),
        "ctime": comment["ctime"].as_i64().unwrap_or(0),
        "floor": comment["floor"].as_i64().unwrap_or(0),
        "replies": comment["count"].as_i64().unwrap_or(0),
    })
}

fn pic_url(value: &str) -> String {
    if value.starts_with("//") {
        format!("https:{value}")
    } else if let Some(rest) = value.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        value.to_string()
    }
}

fn strip_tags(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn parse_duration(value: &str) -> i64 {
    value
        .split(':')
        .filter_map(|part| part.trim().parse::<i64>().ok())
        .fold(0i64, |acc, part| acc * 60 + part)
}
