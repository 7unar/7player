//! Minimal persistent cookie jar for the NetEase web/eapi session.
//!
//! NetEase authenticates with the `MUSIC_U` cookie, so it must survive restarts.
//! The file is written with mode 0600 because it is equivalent to a login token.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const HEX: &[u8] = b"0123456789ABCDEF";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Persisted {
    cookies: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CookieJar {
    cookies: HashMap<String, String>,
    path: PathBuf,
    csrf: String,
}

impl CookieJar {
    pub fn load(path: PathBuf) -> Self {
        let cookies = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str::<Persisted>(&text).ok())
            .map(|p| p.cookies)
            .unwrap_or_default();
        let mut jar = Self {
            cookies,
            path,
            csrf: String::new(),
        };
        jar.ensure_identity();
        jar.csrf = jar.cookies.get("__csrf").cloned().unwrap_or_default();
        jar
    }

    fn ensure_identity(&mut self) {
        if !self.cookies.get("deviceId").is_some_and(|v| v.len() >= 32) {
            self.cookies.insert("deviceId".into(), random_device_id());
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string();
        let nuid = format!(
            "{:016x}{:016x}",
            rand::random::<u64>(),
            rand::random::<u64>()
        );
        self.cookies
            .entry("_ntes_nuid".into())
            .or_insert(nuid.clone());
        self.cookies
            .entry("_ntes_nnid".into())
            .or_insert_with(|| format!("{nuid},{now_ms}"));
        self.cookies
            .entry("NMTID".into())
            .or_insert_with(|| format!("{:016x}", rand::random::<u64>()));
        self.cookies.entry("WNMCID".into()).or_insert_with(|| {
            format!(
                "{:02x}{:02x}{:02x}.{}",
                rand::random::<u8>(),
                rand::random::<u8>(),
                rand::random::<u8>(),
                now_ms
            )
        });
        self.cookies
            .entry("os".into())
            .or_insert_with(|| "pc".into());
        self.cookies
            .entry("appver".into())
            .or_insert_with(|| "2.7.1.198277".into());
        self.cookies
            .entry("osver".into())
            .or_insert_with(|| "10".into());
        self.cookies.entry("channel".into()).or_default();
        self.cookies
            .entry("ntes_kaola_ad".into())
            .or_insert_with(|| "1".into());
        self.cookies
            .entry("__remember_me".into())
            .or_insert_with(|| "true".into());
    }

    pub fn device_id(&self) -> &str {
        self.cookies
            .get("deviceId")
            .map(String::as_str)
            .unwrap_or("")
    }

    pub fn csrf(&self) -> &str {
        &self.csrf
    }

    pub fn has_music_u(&self) -> bool {
        self.cookies
            .get("MUSIC_U")
            .is_some_and(|v| !v.trim().is_empty())
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    /// Build a Cookie header using the PC or iPhone client identity.
    pub fn header(&self, eapi: bool) -> String {
        let mut cookies = self.cookies.clone();
        let (os, appver, osver) = if eapi {
            ("iphone", "9.0.90", "16.2")
        } else {
            ("pc", "2.7.1.198277", "10")
        };
        cookies.insert("os".into(), os.into());
        cookies.insert("appver".into(), appver.into());
        cookies.insert("osver".into(), osver.into());
        if !self.csrf.is_empty() {
            cookies.insert("__csrf".into(), self.csrf.clone());
        }
        cookies
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Merge `Set-Cookie` headers from a response.
    pub fn update_from_response(&mut self, headers: &reqwest::header::HeaderMap) {
        for value in headers.get_all(reqwest::header::SET_COOKIE).iter() {
            let Ok(raw) = value.to_str() else { continue };
            // Expires contains commas but not semicolons, so the first segment is
            // always the cookie pair.
            let pair = raw.split(';').next().unwrap_or("").trim();
            let Some((name, val)) = pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let val = val.trim();
            if name.is_empty() {
                continue;
            }
            let expired = raw.to_ascii_lowercase().contains("max-age=0")
                || raw.to_ascii_lowercase().contains("1970");
            if expired || val.is_empty() {
                self.cookies.remove(name);
            } else {
                self.cookies.insert(name.to_string(), val.to_string());
            }
            if name == "__csrf" {
                self.csrf = if expired {
                    String::new()
                } else {
                    val.to_string()
                };
            }
        }
    }

    pub fn clear_auth(&mut self) {
        self.cookies.remove("MUSIC_U");
        self.cookies.remove("__csrf");
        self.csrf.clear();
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = serde_json::to_string_pretty(&Persisted {
            cookies: self.cookies.clone(),
        })
        .map_err(std::io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        write_private(&tmp, text.as_bytes())?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn random_device_id() -> String {
    let mut rng = rand::thread_rng();
    (0..52).map(|_| HEX[rng.gen_range(0..16)] as char).collect()
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn temp_jar() -> CookieJar {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.keep().join("cookies.json");
        CookieJar::load(path)
    }

    #[test]
    fn generates_stable_device_id_within_instance() {
        let jar = temp_jar();
        assert_eq!(jar.device_id().len(), 52);
        assert!(jar.device_id().chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(jar.device_id(), jar.device_id());
    }

    #[test]
    fn header_uses_client_identity() {
        let jar = temp_jar();
        let pc = jar.header(false);
        assert!(pc.contains("os=pc"));
        assert!(pc.contains("deviceId="));
        let ios = jar.header(true);
        assert!(ios.contains("os=iphone"));
        assert!(ios.contains("appver=9.0.90"));
    }

    #[test]
    fn persists_and_reloads_auth_cookie() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cookies.json");
        let mut jar = CookieJar::load(path.clone());
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append(
            reqwest::header::SET_COOKIE,
            "MUSIC_U=secret; Path=/; HttpOnly".parse().unwrap(),
        );
        headers.append(
            reqwest::header::SET_COOKIE,
            "__csrf=abc123; Path=/".parse().unwrap(),
        );
        jar.update_from_response(&headers);
        assert!(jar.has_music_u());
        assert_eq!(jar.csrf(), "abc123");
        jar.save().unwrap();

        let reloaded = CookieJar::load(path);
        assert!(reloaded.has_music_u());
        assert_eq!(reloaded.get("MUSIC_U"), Some("secret"));
        assert_eq!(reloaded.csrf(), "abc123");
    }

    #[test]
    fn clearing_auth_keeps_device_id() {
        let mut jar = temp_jar();
        let device = jar.device_id().to_string();
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append(
            reqwest::header::SET_COOKIE,
            "MUSIC_U=secret; Path=/".parse().unwrap(),
        );
        jar.update_from_response(&headers);
        assert!(jar.has_music_u());
        jar.clear_auth();
        assert!(!jar.has_music_u());
        assert_eq!(jar.device_id(), device);
    }

    #[test]
    fn shared_arc_is_safe_for_concurrent_header_builds() {
        let jar = Arc::new(temp_jar());
        let mut handles = Vec::new();
        for _ in 0..4 {
            let jar = jar.clone();
            handles.push(std::thread::spawn(move || jar.header(false)));
        }
        for h in handles {
            assert!(h.join().unwrap().contains("deviceId="));
        }
    }
}
