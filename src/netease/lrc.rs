//! LRC lyric parsing (including timestamped translations).

use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LyricLine {
    pub time: Duration,
    pub text: String,
    pub translation: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    /// Text without timestamps (rare, e.g. instrumental notes).
    pub untimed: Vec<String>,
    pub has_translation: bool,
}

impl Lyrics {
    pub fn from_lrc(original: &str, translation: Option<&str>) -> Self {
        let (mut parsed, offset) = parse_lrc(original);
        if parsed.is_empty() {
            let untimed: Vec<String> = original
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect();
            return Self {
                lines: Vec::new(),
                untimed,
                has_translation: false,
            };
        }

        for line in &mut parsed {
            line.time = apply_offset(line.time, offset);
        }

        let translations = translation
            .map(|text| {
                let (lines, t_offset) = parse_lrc(text);
                lines
                    .into_iter()
                    .map(|l| (apply_offset(l.time, t_offset).as_millis() as i64, l.text))
                    .collect::<HashMap<i64, String>>()
            })
            .unwrap_or_default();

        let mut has_translation = false;
        if !translations.is_empty() {
            for line in &mut parsed {
                let key = line.time.as_millis() as i64;
                let found = translations.get(&key).cloned().or_else(|| {
                    // Some API responses differ by a few milliseconds.
                    translations
                        .iter()
                        .filter(|(t, _)| (**t - key).abs() <= 80)
                        .min_by_key(|(t, _)| (**t - key).abs())
                        .map(|(_, v)| v.clone())
                });
                if let Some(text) = found {
                    if !text.trim().is_empty() && text != line.text {
                        line.translation = Some(text);
                        has_translation = true;
                    }
                }
            }
        }

        parsed.sort_by_key(|l| l.time);
        parsed.dedup_by(|a, b| a.time == b.time && a.text == b.text);
        Self {
            lines: parsed,
            untimed: Vec::new(),
            has_translation,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() && self.untimed.is_empty()
    }

    /// Index of the line active at `position`.
    pub fn active_index(&self, position: Duration) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        let idx = self.lines.partition_point(|line| line.time <= position);
        idx.checked_sub(1)
    }
}

fn apply_offset(time: Duration, offset_ms: i64) -> Duration {
    if offset_ms >= 0 {
        time + Duration::from_millis(offset_ms as u64)
    } else {
        time.saturating_sub(Duration::from_millis(offset_ms.unsigned_abs()))
    }
}

/// Returns timed lines and the global offset in milliseconds.
fn parse_lrc(text: &str) -> (Vec<LyricLine>, i64) {
    let mut lines = Vec::new();
    let mut offset_ms = 0i64;

    for raw in text.lines() {
        let raw = raw.trim().trim_start_matches('\u{feff}');
        if raw.is_empty() {
            continue;
        }
        let mut rest = raw;
        let mut stamps = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some(close) = after_open.find(']') else {
                break;
            };
            let tag = &after_open[..close];
            let after = &after_open[close + 1..];
            if let Some(ms) = parse_timestamp(tag) {
                stamps.push(ms);
                rest = after;
            } else if let Some(value) = tag.strip_prefix("offset:") {
                offset_ms = value.trim().parse::<i64>().unwrap_or(0);
                rest = after;
            } else if tag.contains(':') {
                // Metadata tags such as [ti:...], [ar:...], [al:...].
                rest = after;
            } else {
                break;
            }
        }

        let content = rest.trim();
        if content.is_empty() || stamps.is_empty() {
            continue;
        }
        for ms in stamps {
            lines.push(LyricLine {
                time: Duration::from_millis(ms),
                text: content.to_string(),
                translation: None,
            });
        }
    }

    (lines, offset_ms)
}

/// Parse `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx`.
fn parse_timestamp(tag: &str) -> Option<u64> {
    let (minutes, rest) = tag.split_once(':')?;
    if minutes.len() > 3 || rest.is_empty() {
        return None;
    }
    let minutes: u64 = minutes.parse().ok()?;
    let (seconds, fraction) = match rest.split_once('.') {
        Some((s, f)) => (s, Some(f)),
        None => (rest, None),
    };
    let seconds: u64 = seconds.parse().ok()?;
    if seconds >= 60 {
        return None;
    }
    let millis = match fraction {
        None => 0,
        Some(f) if f.is_empty() || f.len() > 3 || !f.chars().all(|c| c.is_ascii_digit()) => {
            return None
        }
        Some(f) => {
            let value: u64 = f.parse().ok()?;
            match f.len() {
                1 => value * 100,
                2 => value * 10,
                _ => value,
            }
        }
    };
    Some((minutes * 60 + seconds) * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_metadata_multiple_stamps_and_offset() {
        let text = "\u{feff}[ti:晴天]\n[ar:周杰伦]\n[offset:500]\n[00:01.00]第一句\n[00:02.50][00:05.00]重复句\n";
        let lyrics = Lyrics::from_lrc(text, None);
        assert_eq!(lyrics.lines.len(), 3);
        assert_eq!(lyrics.lines[0].time, Duration::from_millis(1500));
        assert_eq!(lyrics.lines[1].time, Duration::from_millis(3000));
        assert_eq!(lyrics.lines[2].time, Duration::from_millis(5500));
        assert_eq!(lyrics.lines[1].text, "重复句");
    }

    #[test]
    fn merges_translation_by_timestamp() {
        let original = "[00:10.00]Hello\n[00:12.00]World\n";
        let translation = "[00:10.00]你好\n[00:12.00]世界\n";
        let lyrics = Lyrics::from_lrc(original, Some(translation));
        assert!(lyrics.has_translation);
        assert_eq!(lyrics.lines[0].translation.as_deref(), Some("你好"));
        assert_eq!(lyrics.lines[1].translation.as_deref(), Some("世界"));
    }

    #[test]
    fn translation_can_match_near_timestamps() {
        let original = "[00:10.00]Hello\n";
        let translation = "[00:10.05]你好\n";
        let lyrics = Lyrics::from_lrc(original, Some(translation));
        assert_eq!(lyrics.lines[0].translation.as_deref(), Some("你好"));
    }

    #[test]
    fn active_line_advances() {
        let lyrics = Lyrics::from_lrc("[00:01.00]a\n[00:02.00]b\n[00:03.00]c\n", None);
        assert_eq!(lyrics.active_index(Duration::from_millis(500)), None);
        assert_eq!(lyrics.active_index(Duration::from_millis(1000)), Some(0));
        assert_eq!(lyrics.active_index(Duration::from_millis(2500)), Some(1));
        assert_eq!(lyrics.active_index(Duration::from_secs(10)), Some(2));
    }

    #[test]
    fn plain_text_becomes_untimed() {
        let lyrics = Lyrics::from_lrc("纯音乐，请欣赏", None);
        assert!(lyrics.lines.is_empty());
        assert_eq!(lyrics.untimed, vec!["纯音乐，请欣赏"]);
        assert!(!lyrics.is_empty());
    }

    #[test]
    fn negative_offset_does_not_underflow() {
        let lyrics = Lyrics::from_lrc("[offset:-5000]\n[00:01.00]a\n", None);
        assert_eq!(lyrics.lines[0].time, Duration::ZERO);
    }
}
