//! 认证相关密码学工具。
//!
//! - 密码与密保答案只存 salted SHA-256，绝不存明文；
//! - Token 明文只在登录响应中出现一次，数据库只存 hash；
//! - 比较使用恒定时间实现，避免时序侧信道；
//! - 不把密码、Token 写入任何日志（报告 08 章安全默认值）。

use sha2::{Digest, Sha256};
use uuid::Uuid;

pub fn sha256_hex(input: &str) -> String {
    let mut h = Sha256::new();
    h.update(input.as_bytes());
    hex::encode(h.finalize())
}

pub fn new_salt() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn new_token() -> String {
    format!(
        "{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

pub fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn hash_password(password: &str, salt: &str) -> String {
    sha256_hex(&format!("{salt}:{password}"))
}

pub fn hash_answer(answer: &str, salt: &str) -> String {
    let normalized = answer.trim().to_lowercase();
    sha256_hex(&format!("{salt}:answer:{normalized}"))
}

/// 恒定时间字符串比较（长度不同也走完整循环）。
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let max = ab.len().max(bb.len());
    let mut diff = (ab.len() ^ bb.len()) as u8;
    for i in 0..max {
        let x = ab.get(i).copied().unwrap_or(0);
        let y = bb.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_is_salted_and_stable() {
        let salt = new_salt();
        let h1 = hash_password("admin", &salt);
        let h2 = hash_password("admin", &salt);
        assert_eq!(h1, h2);
        assert_ne!(h1, hash_password("admin", &new_salt()));
        assert_ne!(h1, "admin");
    }

    #[test]
    fn constant_time_compare_works() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(!constant_time_eq("", "a"));
    }

    #[test]
    fn tokens_are_unique() {
        assert_ne!(new_token(), new_token());
        assert_eq!(new_token().len(), 64);
    }
}
