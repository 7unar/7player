//! Audio backend smoke test: `cargo run --example audio_probe`.
//! Generates a tone with ffmpeg before running (see README) or pass a file path.

use seven_player::audio::{AudioCmd, AudioEvent, AudioHandle};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let path = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp/ncm-tone.mp3"));
    if !path.exists() {
        eprintln!("missing audio file: {}", path.display());
        std::process::exit(2);
    }
    let mut audio = AudioHandle::spawn(false, 0.4);
    tokio::time::sleep(Duration::from_millis(600)).await;
    println!("backend: {}", audio.snapshot().backend);

    audio.send(AudioCmd::Load {
        path,
        duration: Duration::from_secs(3),
    });
    for i in 0..8 {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let snap = audio.snapshot();
        println!(
            "t={:.1}s status={:?} pos={:.2}s dur={:.2}s backend={}",
            i as f32 * 0.3,
            snap.status,
            snap.position.as_secs_f32(),
            snap.duration.as_secs_f32(),
            snap.backend
        );
        while let Ok(event) = audio.events_mut().try_recv() {
            println!("event: {event:?}");
            if matches!(event, AudioEvent::Ended) {
                audio.shutdown();
                return;
            }
        }
    }
    audio.send(AudioCmd::Pause);
    tokio::time::sleep(Duration::from_millis(400)).await;
    println!("paused: {:?}", audio.snapshot());
    audio.send(AudioCmd::Resume);
    tokio::time::sleep(Duration::from_millis(400)).await;
    println!("resumed: {:?}", audio.snapshot());
    audio.shutdown();
    println!("done");
}
