//! Background jobs and the messages they send back to the UI.
//!
//! Every network operation runs in its own Tokio task so the event loop never
//! blocks on HTTP. Requests carry a sequence number where a stale reply could
//! otherwise overwrite newer UI state.

use crate::netease::lrc::Lyrics;
use crate::netease::model::{
    NcmError, Playlist, PlaylistDetail, QrState, SearchPage, SongUrl, Track, User,
};
use crate::netease::NeteaseClient;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Default)]
pub struct Bootstrap {
    pub user: Option<User>,
    pub toplists: Vec<Playlist>,
    pub recommended: Vec<Playlist>,
    pub daily: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub liked: HashSet<u64>,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub enum Msg {
    Bootstrap {
        seq: u64,
        result: Result<Bootstrap, String>,
    },
    Search {
        seq: u64,
        result: Result<SearchPage, String>,
    },
    Playlist {
        seq: u64,
        id: u64,
        result: Result<PlaylistDetail, String>,
    },
    PlayReady {
        seq: u64,
        track: Track,
        path: PathBuf,
        url: SongUrl,
    },
    PlayFailed {
        seq: u64,
        track: Track,
        error: String,
    },
    Lyric {
        track_id: u64,
        result: Result<Lyrics, String>,
    },
    QrCreated {
        seq: u64,
        result: Result<(String, String), String>,
    },
    QrStatus {
        seq: u64,
        key: String,
        result: Result<(QrState, String), String>,
    },
    LoginDone {
        seq: u64,
        result: Result<(), String>,
    },
    LogoutDone(Result<(), String>),
    LikeDone {
        id: u64,
        like: bool,
        result: Result<(), String>,
    },
}

pub fn spawn_bootstrap(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>, seq: u64) {
    tokio::spawn(async move {
        let result = bootstrap(client).await;
        let _ = tx.send(Msg::Bootstrap { seq, result });
    });
}

async fn bootstrap(client: Arc<NeteaseClient>) -> Result<Bootstrap, String> {
    let mut warnings = Vec::new();

    if !client.has_music_u() {
        // Anonymous registration is best-effort; public charts/search still work
        // if NetEase rate-limits it. Do not surface a scary toast for this.
        let _ = client.register_anonymous().await;
    }

    let user = match client.login_status().await {
        Ok(user) => user,
        Err(err) => {
            warnings.push(format!("登录状态检查失败: {err}"));
            None
        }
    };

    let toplists = match client.toplists().await {
        Ok(lists) => lists,
        Err(err) => {
            warnings.push(format!("排行榜加载失败: {err}"));
            Vec::new()
        }
    };

    let mut data = Bootstrap {
        user,
        toplists,
        warnings,
        ..Default::default()
    };

    if let Some(user) = data.user.clone() {
        let (daily, recommended, playlists, liked) = tokio::join!(
            client.recommend_songs(),
            client.recommend_resources(),
            client.user_playlists(user.uid),
            client.liked_ids(user.uid),
        );
        match daily {
            Ok(tracks) => data.daily = tracks,
            Err(err) => data.warnings.push(format!("每日推荐加载失败: {err}")),
        }
        match recommended {
            Ok(lists) => data.recommended = lists,
            Err(err) => data.warnings.push(format!("推荐歌单加载失败: {err}")),
        }
        match playlists {
            Ok(lists) => data.playlists = lists,
            Err(err) => data.warnings.push(format!("我的歌单加载失败: {err}")),
        }
        match liked {
            Ok(ids) => data.liked = ids.into_iter().collect(),
            Err(err) => data.warnings.push(format!("喜欢的音乐加载失败: {err}")),
        }
    }

    Ok(data)
}

pub fn spawn_search(
    tx: UnboundedSender<Msg>,
    client: Arc<NeteaseClient>,
    seq: u64,
    keyword: String,
) {
    tokio::spawn(async move {
        let result = client
            .search_songs(&keyword, 0, 50)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.send(Msg::Search { seq, result });
    });
}

pub fn spawn_playlist(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>, seq: u64, id: u64) {
    tokio::spawn(async move {
        let result = client.playlist_detail(id).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::Playlist { seq, id, result });
    });
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_play(
    tx: UnboundedSender<Msg>,
    client: Arc<NeteaseClient>,
    seq: u64,
    track: Track,
    quality: String,
    cache_dir: PathBuf,
) {
    tokio::spawn(async move {
        let mut url = client.song_url(track.id, &quality).await;
        if matches!(&url, Err(NcmError::NoUrl(_))) && quality != "standard" {
            // A lossless/exhigh request can fail for free accounts even though a
            // standard stream is available. Fall back instead of failing the track.
            url = client.song_url(track.id, "standard").await;
        }
        let url = match url {
            Ok(url) => url,
            Err(err) => {
                let _ = tx.send(Msg::PlayFailed {
                    seq,
                    track,
                    error: err.to_string(),
                });
                return;
            }
        };
        match client
            .cache_song(track.id, &url.url, &url.kind, &cache_dir)
            .await
        {
            Ok(path) => {
                let _ = tx.send(Msg::PlayReady {
                    seq,
                    track,
                    path,
                    url,
                });
            }
            Err(err) => {
                let _ = tx.send(Msg::PlayFailed {
                    seq,
                    track,
                    error: err.to_string(),
                });
            }
        }
    });
}

pub fn spawn_lyric(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>, track_id: u64) {
    tokio::spawn(async move {
        let result = client.lyric(track_id).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::Lyric { track_id, result });
    });
}

pub fn spawn_qr_create(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>, seq: u64) {
    tokio::spawn(async move {
        let result = client.qr_create().await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::QrCreated { seq, result });
    });
}

pub fn spawn_qr_check(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>, seq: u64, key: String) {
    tokio::spawn(async move {
        let result = client.qr_check(&key).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::QrStatus { seq, key, result });
    });
}

pub fn spawn_login_phone(
    tx: UnboundedSender<Msg>,
    client: Arc<NeteaseClient>,
    seq: u64,
    phone: String,
    password: String,
) {
    tokio::spawn(async move {
        let result = client
            .login_phone(&phone, &password)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.send(Msg::LoginDone { seq, result });
    });
}

pub fn spawn_logout(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>) {
    tokio::spawn(async move {
        let result = client.logout().await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::LogoutDone(result));
    });
}

pub fn spawn_like(tx: UnboundedSender<Msg>, client: Arc<NeteaseClient>, id: u64, like: bool) {
    tokio::spawn(async move {
        let result = client.like_song(id, like).await.map_err(|e| e.to_string());
        let _ = tx.send(Msg::LikeDone { id, like, result });
    });
}
