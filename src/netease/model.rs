//! Data models parsed from NetEase API responses.

use serde_json::Value;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum NcmError {
    #[error("网络请求失败: {0}")]
    Http(#[from] reqwest::Error),
    #[error("接口返回错误 {code}: {msg}")]
    Api { code: i64, msg: String },
    #[error("响应解析失败: {0}")]
    Json(#[from] serde_json::Error),
    #[error("加密失败: {0}")]
    Crypto(String),
    #[error("登录状态已失效，请重新登录")]
    Auth,
    #[error("当前歌曲没有可用音源（可能需要会员、已下架或地区限制）")]
    NoUrl(String),
    #[error("其他错误: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, NcmError>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Track {
    pub id: u64,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub duration: Duration,
    pub fee: i64,
    pub alias: Vec<String>,
}

impl Track {
    pub fn artist_text(&self) -> String {
        if self.artists.is_empty() {
            "未知歌手".to_string()
        } else {
            self.artists.join(" / ")
        }
    }

    pub fn duration_text(&self) -> String {
        format_duration(self.duration)
    }

    pub fn is_vip(&self) -> bool {
        self.fee == 1
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Playlist {
    pub id: u64,
    pub name: String,
    pub creator: String,
    pub track_count: u64,
    pub description: String,
    pub update_frequency: String,
    pub cover_url: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct User {
    pub uid: u64,
    pub nickname: String,
    pub vip_type: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchPage {
    pub tracks: Vec<Track>,
    pub total: u64,
}

#[derive(Debug, Clone, Default)]
pub struct SongUrl {
    pub url: String,
    pub br: u64,
    pub size: u64,
    pub level: String,
    pub kind: String,
    pub trial: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QrState {
    Expired,
    Waiting,
    Scanned,
    Authorized,
}

impl QrState {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            800 => Some(Self::Expired),
            801 => Some(Self::Waiting),
            802 => Some(Self::Scanned),
            803 => Some(Self::Authorized),
            _ => None,
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::Expired => "二维码已过期，按 r 刷新",
            Self::Waiting => "等待扫码…",
            Self::Scanned => "已扫码，请在手机上确认",
            Self::Authorized => "登录成功",
        }
    }
}

pub fn parse_track(value: &Value) -> Option<Track> {
    let id = value.get("id")?.as_u64()?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("未知歌曲")
        .to_string();

    let mut artists = Vec::new();
    for key in ["ar", "artists"] {
        if let Some(arr) = value.get(key).and_then(Value::as_array) {
            for artist in arr {
                if let Some(name) = artist.get("name").and_then(Value::as_str) {
                    artists.push(name.to_string());
                }
            }
            if !artists.is_empty() {
                break;
            }
        }
    }
    if artists.is_empty() {
        if let Some(name) = value
            .get("artist")
            .and_then(|a| a.get("name"))
            .and_then(Value::as_str)
        {
            artists.push(name.to_string());
        }
    }

    let album = value
        .get("al")
        .or_else(|| value.get("album"))
        .and_then(|a| a.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let duration_ms = value
        .get("dt")
        .or_else(|| value.get("duration"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let fee = value.get("fee").and_then(Value::as_i64).unwrap_or(0);

    let alias = value
        .get("alia")
        .or_else(|| value.get("alias"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    Some(Track {
        id,
        name,
        artists,
        album,
        duration: Duration::from_millis(duration_ms),
        fee,
        alias,
    })
}

pub fn parse_tracks(value: &Value) -> Vec<Track> {
    value
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_track).collect())
        .unwrap_or_default()
}

pub fn parse_playlist(value: &Value) -> Option<Playlist> {
    let id = value.get("id")?.as_u64()?;
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("未命名歌单")
        .to_string();
    let creator = value
        .get("creator")
        .and_then(|c| {
            c.get("nickname")
                .and_then(Value::as_str)
                .or_else(|| c.as_str())
        })
        .unwrap_or("")
        .to_string();
    let track_count = value
        .get("trackCount")
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .get("trackIds")
                .and_then(Value::as_array)
                .map(|a| a.len() as u64)
        })
        .unwrap_or(0);
    let description = value
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let update_frequency = value
        .get("updateFrequency")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let cover_url = value
        .get("coverImgUrl")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    Some(Playlist {
        id,
        name,
        creator,
        track_count,
        description,
        update_frequency,
        cover_url,
    })
}

pub fn parse_playlists(value: &Value) -> Vec<Playlist> {
    value
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_playlist).collect())
        .unwrap_or_default()
}

pub fn parse_user(value: &Value) -> Option<User> {
    let profile = value.get("profile")?;
    if profile.is_null() {
        return None;
    }
    let uid = profile.get("userId").and_then(Value::as_u64).or_else(|| {
        value
            .get("account")
            .and_then(|a| a.get("id"))
            .and_then(Value::as_u64)
    })?;
    Some(User {
        uid,
        nickname: profile
            .get("nickname")
            .and_then(Value::as_str)
            .unwrap_or("网易云用户")
            .to_string(),
        vip_type: profile.get("vipType").and_then(Value::as_i64).unwrap_or(0),
    })
}

pub fn parse_song_url(value: &Value) -> Result<SongUrl> {
    let item = value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .ok_or_else(|| NcmError::Other("播放地址响应为空".into()))?;
    let url = item
        .get("url")
        .and_then(Value::as_str)
        .filter(|u| !u.is_empty())
        .ok_or_else(|| {
            let code = item.get("code").and_then(Value::as_i64).unwrap_or(0);
            let fee = item.get("fee").and_then(Value::as_i64).unwrap_or(0);
            let free = if fee == 1 { "（VIP 歌曲）" } else { "" };
            NcmError::NoUrl(format!("code={code}{free}"))
        })?;
    let kind = item
        .get("type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| url.split('.').next_back().map(str::to_string))
        .unwrap_or_else(|| "mp3".into());
    Ok(SongUrl {
        url: url.to_string(),
        br: item.get("br").and_then(Value::as_u64).unwrap_or(0),
        size: item.get("size").and_then(Value::as_u64).unwrap_or(0),
        level: item
            .get("level")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        kind,
        trial: item
            .get("freeTrialInfo")
            .map(|v| !v.is_null())
            .unwrap_or(false),
    })
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn api_error(value: &Value) -> NcmError {
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(0);
    let message = value
        .get("message")
        .or_else(|| value.get("msg"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let msg = match code {
        -462 => "触发登录风控，请稍后重试，或改用二维码登录".to_string(),
        501 => "需要验证码，请改用二维码登录".to_string(),
        502 => "手机号或密码错误".to_string(),
        503 => "登录过于频繁，请稍后再试".to_string(),
        8810 => "需要完成安全验证，请改用二维码登录".to_string(),
        301 => "登录状态已失效，请重新登录".to_string(),
        400 if message.is_empty() => "请求参数错误".to_string(),
        _ if message.is_empty() => "未知错误".to_string(),
        _ => message,
    };
    NcmError::Api { code, msg }
}

pub fn check_code(value: &Value) -> Result<()> {
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(200);
    if code == 200 {
        Ok(())
    } else {
        Err(api_error(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_cloudsearch_track() {
        let value = json!({
            "id": 186016,
            "name": "晴天",
            "ar": [{"name": "周杰伦"}],
            "al": {"name": "叶惠美"},
            "dt": 269_000,
            "fee": 1,
            "alia": ["Sunny Day"]
        });
        let t = parse_track(&value).unwrap();
        assert_eq!(t.name, "晴天");
        assert_eq!(t.artist_text(), "周杰伦");
        assert_eq!(t.album, "叶惠美");
        assert_eq!(t.duration_text(), "04:29");
        assert!(t.is_vip());
    }

    #[test]
    fn parses_legacy_artist_field() {
        let value = json!({"id": 1, "name": "x", "artist": {"name": "y"}, "duration": 61000});
        let t = parse_track(&value).unwrap();
        assert_eq!(t.artists, vec!["y"]);
        assert_eq!(t.duration_text(), "01:01");
    }

    #[test]
    fn parses_playlist_and_user() {
        let p = json!({
            "id": 1, "name": "测试歌单", "creator": {"nickname": "Lunar"},
            "trackCount": 12, "description": "desc", "updateFrequency": "每天更新"
        });
        let p = parse_playlist(&p).unwrap();
        assert_eq!(p.creator, "Lunar");
        assert_eq!(p.track_count, 12);
        assert_eq!(p.update_frequency, "每天更新");

        let u = json!({"code": 200, "profile": {"userId": 42, "nickname": "Lunar", "vipType": 11}});
        let u = parse_user(&u).unwrap();
        assert_eq!(u.uid, 42);
        assert_eq!(u.nickname, "Lunar");
    }

    #[test]
    fn null_profile_is_logged_out() {
        assert!(parse_user(&json!({"code": 200, "profile": null})).is_none());
    }

    #[test]
    fn missing_url_is_an_error() {
        let value = json!({"data": [{"url": null, "code": 404, "fee": 1}]});
        let err = parse_song_url(&value).unwrap_err();
        assert!(matches!(err, NcmError::NoUrl(_)));
    }

    #[test]
    fn qr_state_mapping() {
        assert_eq!(QrState::from_code(803), Some(QrState::Authorized));
        assert_eq!(QrState::from_code(999), None);
    }

    #[test]
    fn api_errors_are_actionable() {
        let err = api_error(&json!({"code": -462}));
        assert!(err.to_string().contains("风控"));
        let err = api_error(&json!({"code": 502, "message": ""}));
        assert!(err.to_string().contains("密码"));
    }
}
