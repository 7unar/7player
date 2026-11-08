//! NetEase Cloud Music API client.
//!
//! Uses the public web API (`weapi`, AES+RSA) and the mobile API (`eapi`,
//! AES-ECB) with a persisted cookie jar. All endpoints are unofficial and may
//! change; see README for the supported feature set.

pub mod cookie;
pub mod crypto;
pub mod lrc;
pub mod model;

use crate::netease::cookie::CookieJar;
use crate::netease::lrc::Lyrics;
use crate::netease::model::{
    check_code, parse_playlist, parse_playlists, parse_song_url, parse_tracks, parse_user,
    Playlist, PlaylistDetail, QrState, Result, SearchPage, SongUrl, Track, User,
};
use reqwest::header::{ACCEPT, ACCEPT_LANGUAGE, CONNECTION, COOKIE, HOST, REFERER, USER_AGENT};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BASE: &str = "https://music.163.com";
const EAPI_HOST: &str = "interface.music.163.com";
const UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const MOBILE_UA: &str = "NeteaseMusic 9.0.90/5038 (iPhone; iOS 16.2; zh_CN)";

#[derive(Clone)]
pub struct NeteaseClient {
    http: reqwest::Client,
    jar: Arc<Mutex<CookieJar>>,
    ua: String,
}

impl NeteaseClient {
    pub fn new(cookie_path: PathBuf) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(8))
            .gzip(true)
            .build()?;
        Ok(Self {
            http,
            jar: Arc::new(Mutex::new(CookieJar::load(cookie_path))),
            ua: UA.to_string(),
        })
    }

    fn jar(&self) -> MutexGuard<'_, CookieJar> {
        self.jar
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn has_music_u(&self) -> bool {
        self.jar().has_music_u()
    }

    pub fn flush_cookies(&self) {
        if let Err(err) = self.jar().save() {
            log_line(&format!("保存 cookie 失败: {err}"));
        }
    }

    pub fn clear_auth(&self) {
        self.jar().clear_auth();
        self.flush_cookies();
    }

    pub fn cookie_path(&self) -> PathBuf {
        self.jar().path().to_path_buf()
    }

    // ------------------------------------------------------------------
    // Transport
    // ------------------------------------------------------------------

    async fn send_weapi(&self, path: &str, params: Vec<(&str, String)>) -> Result<Value> {
        let (body, csrf, cookie_header) = {
            let jar = self.jar();
            let csrf = jar.csrf().to_string();
            let mut map = serde_json::Map::new();
            for (key, value) in params {
                map.insert(key.to_string(), Value::String(value));
            }
            map.insert("csrf_token".into(), Value::String(csrf.clone()));
            let json = Value::Object(map).to_string();
            (crypto::weapi(&json), csrf, jar.header(false))
        };

        let mapped = if let Some(rest) = path.strip_prefix("/api/") {
            format!("/weapi/{rest}")
        } else {
            path.to_string()
        };
        let url = format!("{BASE}{mapped}?csrf_token={}", crypto::form_encode(&csrf));
        let resp = self
            .http
            .post(&url)
            .header(USER_AGENT, &self.ua)
            .header(ACCEPT, "*/*")
            .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
            .header(CONNECTION, "keep-alive")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header(HOST, "music.163.com")
            .header(REFERER, BASE)
            .header(COOKIE, cookie_header)
            .body(body)
            .send()
            .await?;
        self.read_json(resp, false).await
    }

    async fn send_eapi(&self, path: &str, data: Value, mobile_ua: bool) -> Result<Value> {
        let (body, cookie_header) = {
            let jar = self.jar();
            let csrf = jar.csrf().to_string();
            let device_id = jar.device_id().to_string();
            let cookie_header = jar.header(true);
            let now_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let buildver: String = now_ms.to_string().chars().take(10).collect();
            let request_id = format!("{now_ms}_{:04}", rand::random::<u16>() % 1000);

            let mut data = match data {
                Value::Object(map) => map,
                _ => {
                    return Err(crate::netease::model::NcmError::Crypto(
                        "eapi 参数必须是对象".into(),
                    ))
                }
            };
            data.insert("csrf_token".into(), Value::String(csrf.clone()));
            data.insert(
                "header".into(),
                json!({
                    "osver": "16.2",
                    "deviceId": device_id,
                    "os": "iPhone OS",
                    "appver": "9.0.90",
                    "versioncode": "140",
                    "mobilename": "",
                    "buildver": buildver,
                    "resolution": "1920x1080",
                    "__csrf": csrf,
                    "channel": "",
                    "requestId": request_id,
                }),
            );
            let json = Value::Object(data).to_string();
            let body = crypto::eapi(path, &json);
            (body, cookie_header)
        };

        let eapi_path = path.replacen("/api", "/eapi", 1);
        let url = format!("{BASE}{eapi_path}");
        let mut request = self
            .http
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header(COOKIE, cookie_header);
        if mobile_ua {
            request = request.header(USER_AGENT, MOBILE_UA);
        } else {
            request = request
                .header(USER_AGENT, &self.ua)
                .header(ACCEPT, "*/*")
                .header(ACCEPT_LANGUAGE, "zh-CN,zh;q=0.9,en;q=0.8")
                .header(HOST, EAPI_HOST)
                .header(REFERER, BASE);
        }
        let resp = request.body(body).send().await?;
        self.read_json(resp, true).await
    }

    async fn read_json(&self, resp: reqwest::Response, eapi: bool) -> Result<Value> {
        let status = resp.status();
        let headers = resp.headers().clone();
        if let Ok(mut jar) = self.jar.lock() {
            jar.update_from_response(&headers);
        }
        let text = resp.text().await?;
        if !status.is_success() {
            let preview: String = text.chars().take(240).collect();
            return Err(crate::netease::model::NcmError::Other(format!(
                "HTTP {}: {}",
                status.as_u16(),
                preview.trim()
            )));
        }
        let value: Value = serde_json::from_str(&text).map_err(|err| {
            let preview: String = text.chars().take(240).collect();
            crate::netease::model::NcmError::Other(format!(
                "接口{}返回了非 JSON 内容 ({err}): {}",
                if eapi { " eapi " } else { "" },
                preview.trim()
            ))
        })?;
        Ok(value)
    }

    // ------------------------------------------------------------------
    // Authentication
    // ------------------------------------------------------------------

    /// Register a device-level anonymous session. Search and public charts work
    /// better with this cookie; it is not a real account login.
    pub async fn register_anonymous(&self) -> Result<()> {
        let device_id = self.jar().device_id().to_string();
        let encoded = crypto::cloudmusic_encode_id(&device_id);
        let username = crypto::base64_utf8(&format!("{device_id} {encoded}"));
        let value = self
            .send_weapi("/api/register/anonimous", vec![("username", username)])
            .await?;
        check_code(&value)?;
        self.flush_cookies();
        Ok(())
    }

    /// Create a login QR code; returns `(url, unikey)`.
    pub async fn qr_create(&self) -> Result<(String, String)> {
        let value = self
            .send_weapi("/weapi/login/qrcode/unikey", vec![("type", "1".into())])
            .await?;
        check_code(&value)?;
        let unikey = value
            .get("unikey")
            .and_then(Value::as_str)
            .ok_or_else(|| crate::netease::model::NcmError::Other("二维码响应缺少 unikey".into()))?
            .to_string();
        let url = format!("https://music.163.com/login?codekey={unikey}");
        Ok((url, unikey))
    }

    pub async fn qr_check(&self, key: &str) -> Result<(QrState, String)> {
        let value = self
            .send_weapi(
                "/weapi/login/qrcode/client/login",
                vec![("type", "1".into()), ("key", key.to_string())],
            )
            .await?;
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
        let message = value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let state =
            QrState::from_code(code).ok_or_else(|| crate::netease::model::NcmError::Api {
                code,
                msg: if message.is_empty() {
                    "未知扫码状态".into()
                } else {
                    message.clone()
                },
            })?;
        if state == QrState::Authorized {
            self.flush_cookies();
        }
        Ok((state, message))
    }

    /// Phone + password login. Returns `Ok(())` when the server accepted it.
    /// Some accounts need a captcha; the QR flow is the most reliable option.
    pub async fn login_phone(&self, phone: &str, password: &str) -> Result<()> {
        let value = self
            .send_weapi(
                "/api/w/login/cellphone",
                vec![
                    ("type", "1".into()),
                    ("https", "true".into()),
                    ("phone", phone.to_string()),
                    ("countrycode", "86".into()),
                    ("password", crypto::md5_hex(password)),
                    ("remember", "true".into()),
                ],
            )
            .await?;
        check_code(&value)?;
        self.flush_cookies();
        Ok(())
    }

    pub async fn login_status(&self) -> Result<Option<User>> {
        let value = self.send_weapi("/api/nuser/account/get", vec![]).await?;
        if value.get("profile").is_some_and(|p| p.is_null()) {
            return Ok(None);
        }
        if let Some(user) = parse_user(&value) {
            return Ok(Some(user));
        }
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
        if matches!(code, 301 | 302 | 400) {
            return Ok(None);
        }
        Ok(None)
    }

    pub async fn logout(&self) -> Result<()> {
        let value = self.send_weapi("/api/logout", vec![]).await?;
        let code = value.get("code").and_then(Value::as_i64).unwrap_or(200);
        if code != 200 {
            // Still clear local credentials: the user asked to log out.
            self.clear_auth();
            return Err(crate::netease::model::api_error(&value));
        }
        self.clear_auth();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Discovery / library
    // ------------------------------------------------------------------

    pub async fn search_songs(&self, keyword: &str, offset: u64, limit: u64) -> Result<SearchPage> {
        let value = self
            .send_weapi(
                "/weapi/search/get",
                vec![
                    ("s", keyword.to_string()),
                    ("type", "1".into()),
                    ("offset", offset.to_string()),
                    ("limit", limit.min(100).to_string()),
                ],
            )
            .await?;
        check_code(&value)?;
        let result = value.get("result").cloned().unwrap_or(Value::Null);
        let tracks = parse_tracks(result.get("songs").unwrap_or(&Value::Null));
        let total = result
            .get("songCount")
            .and_then(Value::as_u64)
            .unwrap_or(tracks.len() as u64);
        Ok(SearchPage { tracks, total })
    }

    pub async fn toplists(&self) -> Result<Vec<Playlist>> {
        let value = self.send_weapi("/api/toplist", vec![]).await?;
        check_code(&value)?;
        Ok(parse_playlists(value.get("list").unwrap_or(&Value::Null)))
    }

    pub async fn recommend_songs(&self) -> Result<Vec<Track>> {
        let value = self
            .send_weapi("/api/v3/discovery/recommend/songs", vec![])
            .await?;
        if value.get("code").and_then(Value::as_i64) == Some(301) {
            return Err(crate::netease::model::NcmError::Auth);
        }
        check_code(&value)?;
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        Ok(parse_tracks(data.get("dailySongs").unwrap_or(&Value::Null)))
    }

    pub async fn recommend_resources(&self) -> Result<Vec<Playlist>> {
        let value = self
            .send_weapi("/api/v1/discovery/recommend/resource", vec![])
            .await?;
        if value.get("code").and_then(Value::as_i64) == Some(301) {
            return Err(crate::netease::model::NcmError::Auth);
        }
        check_code(&value)?;
        Ok(parse_playlists(
            value.get("recommend").unwrap_or(&Value::Null),
        ))
    }

    pub async fn user_playlists(&self, uid: u64) -> Result<Vec<Playlist>> {
        let value = self
            .send_weapi(
                "/api/user/playlist",
                vec![
                    ("uid", uid.to_string()),
                    ("limit", "100".into()),
                    ("offset", "0".into()),
                    ("includeVideo", "true".into()),
                ],
            )
            .await?;
        if value.get("code").and_then(Value::as_i64) == Some(301) {
            return Err(crate::netease::model::NcmError::Auth);
        }
        check_code(&value)?;
        Ok(parse_playlists(
            value.get("playlist").unwrap_or(&Value::Null),
        ))
    }

    pub async fn playlist_detail(&self, id: u64) -> Result<PlaylistDetail> {
        let value = self
            .send_weapi(
                "/api/v6/playlist/detail",
                vec![
                    ("id", id.to_string()),
                    ("n", "1000".into()),
                    ("s", "8".into()),
                ],
            )
            .await?;
        check_code(&value)?;
        let raw = value.get("playlist").cloned().unwrap_or(Value::Null);
        let playlist = parse_playlist(&raw).unwrap_or_else(|| Playlist {
            id,
            name: format!("歌单 {id}"),
            ..Default::default()
        });
        let mut tracks = parse_tracks(raw.get("tracks").unwrap_or(&Value::Null));
        if tracks.is_empty() {
            if let Some(ids) = raw.get("trackIds").and_then(Value::as_array) {
                let ids: Vec<u64> = ids
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_u64))
                    .collect();
                if !ids.is_empty() {
                    tracks = self.song_detail(&ids).await?;
                }
            }
        }
        Ok(PlaylistDetail { playlist, tracks })
    }

    pub async fn song_detail(&self, ids: &[u64]) -> Result<Vec<Track>> {
        let c: String = ids
            .iter()
            .map(|id| format!(r#"{{"id":"{id}"}}"#))
            .collect::<Vec<_>>()
            .join(",");
        let value = self
            .send_weapi("/api/v3/song/detail", vec![("c", format!("[{c}]"))])
            .await?;
        check_code(&value)?;
        Ok(parse_tracks(value.get("songs").unwrap_or(&Value::Null)))
    }

    // ------------------------------------------------------------------
    // Playback / lyrics
    // ------------------------------------------------------------------

    pub async fn song_url(&self, id: u64, level: &str) -> Result<SongUrl> {
        let level = crate::config::normalize_quality(level).unwrap_or("exhigh");
        let encode_type = if level == "lossless" { "flac" } else { "aac" };
        let value = self
            .send_eapi(
                "/api/song/enhance/player/url/v1",
                json!({
                    "ids": format!("[{id}]"),
                    "level": level,
                    "encodeType": encode_type,
                }),
                false,
            )
            .await?;
        if value.get("code").and_then(Value::as_i64) == Some(301) {
            return Err(crate::netease::model::NcmError::Auth);
        }
        if let Some(code) = value.get("code").and_then(Value::as_i64) {
            if code != 200 {
                return Err(crate::netease::model::api_error(&value));
            }
        }
        parse_song_url(&value)
    }

    pub async fn lyric(&self, id: u64) -> Result<Lyrics> {
        let value = self
            .send_weapi(
                "/api/song/lyric",
                vec![
                    ("id", id.to_string()),
                    ("lv", "-1".into()),
                    ("kv", "-1".into()),
                    ("tv", "-1".into()),
                ],
            )
            .await?;
        check_code(&value)?;
        let original = value
            .get("lrc")
            .and_then(|l| l.get("lyric"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let translation = value
            .get("tlyric")
            .and_then(|l| l.get("lyric"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty());
        Ok(Lyrics::from_lrc(original, translation))
    }

    pub async fn like_song(&self, id: u64, like: bool) -> Result<()> {
        let value = self
            .send_weapi(
                "/api/radio/like",
                vec![
                    ("alg", "itembased".into()),
                    ("trackId", id.to_string()),
                    ("like", like.to_string()),
                    ("time", "3".into()),
                ],
            )
            .await?;
        check_code(&value)?;
        Ok(())
    }

    pub async fn liked_ids(&self, uid: u64) -> Result<Vec<u64>> {
        let value = self
            .send_weapi("/api/song/like/get", vec![("uid", uid.to_string())])
            .await?;
        check_code(&value)?;
        Ok(value
            .get("ids")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default())
    }

    /// Download an audio URL into `path` (streaming, atomic).
    pub async fn download(&self, url: &str, path: &std::path::Path) -> Result<()> {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                crate::netease::model::NcmError::Other(format!("创建缓存目录失败: {e}"))
            })?;
        }
        let resp = self
            .http
            .get(url)
            .header(USER_AGENT, &self.ua)
            .header(REFERER, BASE)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(crate::netease::model::NcmError::Other(format!(
                "音频下载失败: HTTP {}",
                resp.status().as_u16()
            )));
        }
        let tmp = path.with_extension("part");
        let mut file = tokio::fs::File::create(&tmp)
            .await
            .map_err(|e| crate::netease::model::NcmError::Other(format!("写入缓存失败: {e}")))?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await.map_err(|e| {
                crate::netease::model::NcmError::Other(format!("写入缓存失败: {e}"))
            })?;
        }
        file.flush().await.ok();
        drop(file);
        tokio::fs::rename(&tmp, path)
            .await
            .map_err(|e| crate::netease::model::NcmError::Other(format!("保存缓存失败: {e}")))?;
        Ok(())
    }

    /// Fetch an audio URL into the local cache, reusing existing files.
    pub async fn cache_song(
        &self,
        id: u64,
        url: &str,
        kind: &str,
        cache_dir: &std::path::Path,
    ) -> Result<PathBuf> {
        let ext = match kind {
            "flac" => "flac",
            "mp3" => "mp3",
            "aac" | "m4a" => "m4a",
            _ => "mp3",
        };
        let path = cache_dir.join(format!("{id}.{ext}"));
        if let Ok(meta) = tokio::fs::metadata(&path).await {
            if meta.len() > 1024 {
                return Ok(path);
            }
        }
        self.download(url, &path).await?;
        Ok(path)
    }
}

fn log_line(message: &str) {
    use std::io::Write;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crate::config::log_path())
    {
        let _ = writeln!(file, "[{}] {message}", timestamp());
    }
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}
