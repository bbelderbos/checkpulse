use rand::RngExt;
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Salt {
    inner: Mutex<(i64, [u8; 16])>,
}

impl Salt {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new((current_day(), random_salt())),
        }
    }

    pub fn visitor_hash(&self, ip: &str, user_agent: &str, site_id: &str) -> String {
        let salt = self.todays_salt();
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(ip.as_bytes());
        hasher.update(user_agent.as_bytes());
        hasher.update(site_id.as_bytes());
        to_hex(&hasher.finalize()[..16])
    }

    fn todays_salt(&self) -> [u8; 16] {
        let mut guard = self.inner.lock().unwrap();
        let today = current_day();
        if guard.0 != today {
            *guard = (today, random_salt());
        }
        guard.1
    }
}

fn current_day() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    secs / 86_400
}

fn random_salt() -> [u8; 16] {
    let mut salt = [0u8; 16];
    rand::rng().fill(&mut salt[..]);
    salt
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable_within_a_day() {
        let salt = Salt::new();
        let a = salt.visitor_hash("1.2.3.4", "Mozilla", "site");
        let b = salt.visitor_hash("1.2.3.4", "Mozilla", "site");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_differs_by_visitor() {
        let salt = Salt::new();
        let a = salt.visitor_hash("1.2.3.4", "Mozilla", "site");
        let b = salt.visitor_hash("9.9.9.9", "Mozilla", "site");
        assert_ne!(a, b);
    }

    #[test]
    fn salt_rotation_changes_hash() {
        let salt = Salt::new();
        let before = salt.visitor_hash("1.2.3.4", "Mozilla", "site");
        // Force a new day + salt.
        {
            let mut guard = salt.inner.lock().unwrap();
            *guard = (guard.0 + 1, [0u8; 16]);
        }
        let after = salt.visitor_hash("1.2.3.4", "Mozilla", "site");
        assert_ne!(before, after);
    }
}
