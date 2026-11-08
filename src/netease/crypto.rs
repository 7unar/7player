//! NetEase Cloud Music request encryption (weapi + eapi).
//!
//! The algorithms are reimplemented from the public protocol description used by
//! open-source clients such as `NeteaseCloudMusicApi` and pigma. No code or
//! credentials from the official client are included.

use aes::cipher::{BlockEncrypt, KeyInit};
use aes::Aes128;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use md5::{Digest, Md5};
use num_bigint::BigUint;
use rand::RngCore;

const IV: &[u8] = b"0102030405060708";
const PRESET_KEY: &[u8] = b"0CoJUm6Qyw8W8jud";
const EAPI_KEY: &[u8] = b"e82ckenh8dichen8";
const BASE62: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// NetEase's 1024-bit RSA modulus (the leading 00 sign byte is unnecessary).
const RSA_N_HEX: &str = "e0b509f6259df8642dbc35662901477df22677ec152b5ff68ace615bb7b725152b3ab17a876aea8a5aa76d2e417629ec4ee341f56135fccf695280104e0312ecbda92557c93870114af6c9d05c4f7f0c3685b7a46bee255932575cce10b424d813cfe4875d3e82047b97ddef52741d546b8e289dc6935b3ece0462db0a22b8e7";
const RSA_E: u32 = 65_537;

/// Encrypt a weapi JSON payload; returns a form body (`params=...&encSecKey=...`).
pub fn weapi(plaintext: &str) -> String {
    let mut raw = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut raw);
    let secret: Vec<u8> = raw.iter().map(|b| BASE62[(*b % 62) as usize]).collect();

    let first = aes_cbc_encrypt(plaintext.as_bytes(), PRESET_KEY, IV);
    let first_b64 = B64.encode(first);
    let second = aes_cbc_encrypt(first_b64.as_bytes(), &secret, IV);
    let params = B64.encode(second);

    let mut reversed = secret.clone();
    reversed.reverse();
    let enc_sec_key = rsa_encrypt(&reversed);

    format!(
        "params={}&encSecKey={}",
        form_encode(&params),
        form_encode(&enc_sec_key)
    )
}

/// Encrypt an eapi payload; returns `params=<uppercase hex>`.
pub fn eapi(path: &str, text: &str) -> String {
    let message = format!("nobody{path}use{text}md5forencrypt");
    let digest = md5_hex(&message);
    let data = format!("{path}-36cd479b6b5-{text}-36cd479b6b5-{digest}");
    let encrypted = aes_ecb_encrypt(data.as_bytes(), EAPI_KEY);
    let hex: String = encrypted.iter().map(|b| format!("{b:02x}")).collect();
    format!("params={}", form_encode(&hex))
}

pub fn md5_hex(data: &str) -> String {
    let digest = Md5::digest(data.as_bytes());
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// Encode a value for `application/x-www-form-urlencoded`.
pub fn form_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'*' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The anonymous-registration device fingerprint transform.
pub fn cloudmusic_encode_id(device_id: &str) -> String {
    const KEY: &[u8] = b"3go8&$8*3*3h0k(2)2";
    let xored: Vec<u8> = device_id
        .bytes()
        .enumerate()
        .map(|(i, c)| c ^ KEY[i % KEY.len()])
        .collect();
    B64.encode(Md5::digest(&xored))
}

pub fn base64_utf8(s: &str) -> String {
    B64.encode(s.as_bytes())
}

fn pkcs7_pad(data: &[u8], block: usize) -> Vec<u8> {
    let pad = block - data.len() % block;
    let mut out = data.to_vec();
    out.extend(std::iter::repeat_n(pad as u8, pad));
    out
}

fn aes_cbc_encrypt(data: &[u8], key: &[u8], iv: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key");
    let padded = pkcs7_pad(data, 16);
    let mut out = Vec::with_capacity(padded.len());
    let mut prev = [0u8; 16];
    prev.copy_from_slice(iv);
    for chunk in padded.chunks(16) {
        let mut block = [0u8; 16];
        for i in 0..16 {
            block[i] = chunk[i] ^ prev[i];
        }
        cipher.encrypt_block((&mut block).into());
        out.extend_from_slice(&block);
        prev = block;
    }
    out
}

fn aes_ecb_encrypt(data: &[u8], key: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).expect("AES-128 key");
    let mut out = pkcs7_pad(data, 16);
    for chunk in out.chunks_mut(16) {
        cipher.encrypt_block(chunk.into());
    }
    out
}

/// Textbook RSA with the fixed NetEase public key, zero-padded to 128 bytes.
fn rsa_encrypt(data: &[u8]) -> String {
    let n = BigUint::parse_bytes(RSA_N_HEX.as_bytes(), 16).expect("valid modulus");
    let e = BigUint::from(RSA_E);
    let mut padded = vec![0u8; 128usize.saturating_sub(data.len())];
    padded.extend_from_slice(data);
    let m = BigUint::from_bytes_be(&padded);
    let c = m.modpow(&e, &n).to_bytes_be();
    let mut fixed = vec![0u8; 128usize.saturating_sub(c.len())];
    fixed.extend_from_slice(&c);
    fixed.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aes::cipher::BlockDecrypt;

    fn form_value(body: &str, key: &str) -> String {
        let mut value = String::new();
        for pair in body.split('&') {
            let (k, v) = pair.split_once('=').unwrap();
            if k == key {
                value = v.to_string();
            }
        }
        value
    }

    fn percent_decode(s: &str) -> Vec<u8> {
        let bytes = s.as_bytes();
        let mut out = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap();
                out.push(u8::from_str_radix(hex, 16).unwrap());
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        out
    }

    #[test]
    fn md5_matches_known_vector() {
        assert_eq!(md5_hex("hello"), "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn form_encode_escapes_base64_specials() {
        assert_eq!(form_encode("a+b/c="), "a%2Bb%2Fc%3D");
        assert_eq!(form_encode("a-b_c.d*"), "a-b_c.d*");
    }

    #[test]
    fn weapi_body_has_two_fields() {
        let body = weapi(r#"{"s":"晴天"}"#);
        assert!(body.starts_with("params="));
        assert!(body.contains("&encSecKey="));
        let enc = form_value(&body, "encSecKey");
        assert_eq!(enc.len(), 256, "1024-bit RSA output is 256 hex chars");
        assert!(enc.chars().all(|c| c.is_ascii_hexdigit()));
        let params = B64
            .decode(percent_decode(&form_value(&body, "params")))
            .unwrap();
        assert!(!params.is_empty());
        assert_eq!(params.len() % 16, 0);
    }

    #[test]
    fn eapi_is_deterministic_and_decryptable() {
        let body = eapi("/api/song/enhance/player/url", r#"{"ids":"[1]"}"#);
        let hex = percent_decode(&form_value(&body, "params"));
        let hex = String::from_utf8(hex).unwrap();
        let params: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        assert_eq!(params.len() % 16, 0);
        let cipher = Aes128::new_from_slice(EAPI_KEY).unwrap();
        let mut plain = params.clone();
        for chunk in plain.chunks_mut(16) {
            cipher.decrypt_block(chunk.into());
        }
        let pad = *plain.last().unwrap() as usize;
        plain.truncate(plain.len() - pad);
        let text = String::from_utf8(plain).unwrap();
        assert!(text.starts_with("/api/song/enhance/player/url-36cd479b6b5-"));
        assert!(text.contains(r#"{"ids":"[1]"}-36cd479b6b5-"#));
        assert_eq!(
            body,
            eapi("/api/song/enhance/player/url", r#"{"ids":"[1]"}"#)
        );
    }

    #[test]
    fn device_fingerprint_transform_is_stable() {
        let out = cloudmusic_encode_id("0123456789ABCDEF");
        assert_eq!(out, cloudmusic_encode_id("0123456789ABCDEF"));
        assert!(!out.is_empty());
    }
}
