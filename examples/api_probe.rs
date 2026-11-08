//! Standalone API probe used during development:
//! `cargo run --example api_probe`.
//!
//! It exercises anonymous registration, login status, search, playback URL,
//! lyrics, charts and playlist detail without starting the TUI.

use anyhow::Result;
use seven_player::netease::NeteaseClient;

#[tokio::main]
async fn main() -> Result<()> {
    let path = std::env::temp_dir().join("7player-probe-cookies.json");
    let _ = std::fs::remove_file(&path);
    let client = NeteaseClient::new(path)?;

    println!("== anonymous register ==");
    match client.register_anonymous().await {
        Ok(()) => println!("ok"),
        Err(err) => println!("failed: {err}"),
    }

    println!("== login status ==");
    match client.login_status().await {
        Ok(Some(user)) => println!("logged in: {} ({})", user.nickname, user.uid),
        Ok(None) => println!("anonymous / logged out"),
        Err(err) => println!("failed: {err}"),
    }

    println!("== qr create ==");
    match client.qr_create().await {
        Ok((url, key)) => {
            println!("url={url}");
            match client.qr_check(&key).await {
                Ok((state, msg)) => println!("first poll: {state:?} {msg}"),
                Err(err) => println!("poll failed: {err}"),
            }
        }
        Err(err) => println!("failed: {err}"),
    }

    println!("== toplists ==");
    let lists = client.toplists().await?;
    println!("{} charts", lists.len());
    for list in lists.iter().take(5) {
        println!("  {}  {}  {}", list.id, list.name, list.update_frequency);
    }

    println!("== search ==");
    let page = client.search_songs("晴天", 0, 5).await?;
    println!("total={} got={}", page.total, page.tracks.len());
    for t in &page.tracks {
        println!(
            "  {:>10}  {} - {}  [{}]  fee={}",
            t.id,
            t.name,
            t.artist_text(),
            t.duration_text(),
            t.fee
        );
    }

    if let Some(track) = page.tracks.first() {
        println!("== song url ({}) ==", track.name);
        match client.song_url(track.id, "standard").await {
            Ok(url) => println!(
                "  {} kbps, {} bytes, level={}, kind={}, trial={}",
                url.br / 1000,
                url.size,
                url.level,
                url.kind,
                url.trial
            ),
            Err(err) => println!("  failed: {err}"),
        }

        println!("== lyric ==");
        match client.lyric(track.id).await {
            Ok(lyrics) => println!(
                "  lines={} translated={} untimed={}",
                lyrics.lines.len(),
                lyrics.has_translation,
                lyrics.untimed.len()
            ),
            Err(err) => println!("  failed: {err}"),
        }
    }

    if let Some(list) = lists.first() {
        println!("== playlist detail ({}) ==", list.name);
        match client.playlist_detail(list.id).await {
            Ok(detail) => println!(
                "  {} tracks, creator={}",
                detail.tracks.len(),
                detail.playlist.creator
            ),
            Err(err) => println!("  failed: {err}"),
        }
    }

    println!("== done ==");
    Ok(())
}
