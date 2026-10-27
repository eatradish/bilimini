//! Bilibili data layer for the GPUI app: feed, search and play URLs.

use anyhow::{anyhow, Context, Result};
use bpi_rs::ids::{Bvid, Cid};
use bpi_rs::search::{SearchBiliUserParams, SearchVideoParams};
use bpi_rs::video::{VideoHomepageRecommendationsParams, VideoPlayUrlParams, VideoViewParams};
use bpi_rs::BpiClient;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct VideoItem {
    pub bvid: String,
    pub cid: u64,
    pub title: String,
    pub up: String,
    pub duration: i64,
    pub views: u64,
    pub cover: String,
    pub pubdate: u64,
    pub desc: String,
}

#[derive(Debug, Clone)]
pub struct UserItem {
    pub mid: i64,
    pub name: String,
    pub sign: String,
    pub fans: i64,
    pub videos: i64,
    pub avatar: String,
    pub level: i64,
}

#[derive(Debug, Clone)]
pub struct PlayInfo {
    pub bvid: String,
    pub cid: u64,
    pub video_url: String,
    pub audio_url: String,
    pub quality: u64,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub struct Bili {
    client: BpiClient,
}

impl Bili {
    pub fn new() -> Result<Self> {
        let builder = BpiClient::builder();
        let builder = match auth_cookie() {
            Some(cookie) => builder.cookie(cookie),
            None => builder,
        };
        let client = builder.build().context("build bpi client")?;
        Ok(Self { client })
    }

    pub async fn feed(&self) -> Result<Vec<VideoItem>> {
        let data = self
            .client
            .video()
            .homepage_recommendations(VideoHomepageRecommendationsParams::new())
            .await
            .map_err(anyhow::Error::msg)
            .context("fetch homepage recommendations")?;

        Ok(data
            .item
            .into_iter()
            .filter(|item| item.goto == "av" && !item.bvid.is_empty() && !item.title.is_empty())
            .map(|item| VideoItem {
                bvid: item.bvid,
                cid: item.cid,
                title: item.title,
                up: item.owner.name,
                duration: item.duration as i64,
                views: item.stat.as_ref().map(|stat| stat.view).unwrap_or(0),
                cover: pic_url(&item.pic),
                pubdate: item.pubdate,
                desc: String::new(),
            })
            .collect())
    }

    pub async fn search_videos(&self, keyword: &str) -> Result<Vec<VideoItem>> {
        let params = SearchVideoParams::new(keyword).map_err(anyhow::Error::msg)?;
        let data = self
            .client
            .search()
            .video(params)
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("search videos {keyword:?}"))?;

        Ok(data
            .result
            .unwrap_or_default()
            .into_iter()
            .map(|item| VideoItem {
                bvid: item.bvid,
                cid: 0,
                title: strip_tags(&item.title),
                up: item.author,
                duration: parse_duration(&item.duration),
                views: item.play,
                cover: pic_url(&item.pic),
                pubdate: item.pubdate,
                desc: String::new(),
            })
            .collect())
    }

    pub async fn search_users(&self, keyword: &str) -> Result<Vec<UserItem>> {
        let params = SearchBiliUserParams::new(keyword).map_err(anyhow::Error::msg)?;
        let data = self
            .client
            .search()
            .bili_user(params)
            .await
            .map_err(anyhow::Error::msg)
            .with_context(|| format!("search users {keyword:?}"))?;

        Ok(data
            .result
            .unwrap_or_default()
            .into_iter()
            .map(|user| UserItem {
                mid: user.mid,
                name: user.uname,
                sign: user.usign,
                fans: user.fans,
                videos: user.videos,
                avatar: pic_url(&user.upic),
                level: user.level,
            })
            .collect())
    }

    pub async fn play(&self, bvid: &str, cid: u64, qn: u64) -> Result<PlayInfo> {
        let id: Bvid = bvid.parse().map_err(anyhow::Error::msg)?;
        let cid = if cid == 0 {
            let view = self
                .client
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
            .client
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

        Ok(PlayInfo {
            bvid: bvid.to_string(),
            cid: cid.get(),
            video_url: video.base_url.clone(),
            audio_url: audio.map(|stream| stream.base_url.clone()).unwrap_or_default(),
            quality: video.id,
            width: video.width,
            height: video.height,
        })
    }

    /// Parse a raw `{"cookie": "..."}` auth file (same format as the QML app).
    pub fn auth_cookie(&self) -> Option<String> {
        auth_cookie()
    }

}

fn auth_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("bili-mini").join("auth.json")
}

fn auth_cookie() -> Option<String> {
    let text = std::fs::read_to_string(auth_path()).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["cookie"]
        .as_str()
        .filter(|cookie| !cookie.trim().is_empty())
        .map(str::to_string)
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
