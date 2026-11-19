use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsMessage {
    pub id: String,
    pub sender: String,
    pub text: String,
    pub timestamp: String,
    pub direction: String, // "in" | "out"
    pub otp_code: Option<String>,
    pub read: bool,
}

fn default_theme() -> String {
    "light".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub takeover_sms: bool,
    pub notify_sms: bool,
    pub preferred_port: Option<String>,
    #[serde(default = "default_theme")]
    pub theme: String, // "light" | "dark" | "system"
    #[serde(default)]
    pub start_minimized: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            takeover_sms: true,
            notify_sms: true,
            preferred_port: None,
            theme: default_theme(),
            start_minimized: false,
        }
    }
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&dir);
        Self { dir }
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    fn messages_path(&self) -> PathBuf {
        self.dir.join("messages.json")
    }

    pub fn load_settings(&self) -> AppSettings {
        fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) {
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = fs::write(self.settings_path(), json);
        }
    }

    pub fn load_messages(&self) -> Vec<SmsMessage> {
        fs::read_to_string(self.messages_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_messages(&self, messages: &[SmsMessage]) {
        if let Ok(json) = serde_json::to_string_pretty(messages) {
            let _ = fs::write(self.messages_path(), json);
        }
    }
}

pub fn extract_otp(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let has_keyword = text.contains("验证码")
        || text.contains("校验码")
        || text.contains("动态码")
        || text.contains("动态密码")
        || lower.contains("code")
        || lower.contains("otp")
        || lower.contains("verification");
    if !has_keyword {
        return None;
    }
    let re = regex::Regex::new(r"(?:^|[^0-9])([0-9]{4,8})(?:[^0-9]|$)").ok()?;
    re.captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())
}
