use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

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

    fn settings_tmp_path(&self) -> PathBuf {
        self.dir.join("settings.json.tmp")
    }

    fn messages_path(&self) -> PathBuf {
        self.dir.join("messages.json")
    }

    fn messages_tmp_path(&self) -> PathBuf {
        self.dir.join("messages.json.tmp")
    }

    fn messages_bak_path(&self) -> PathBuf {
        self.dir.join("messages.json.bak")
    }

    fn atomic_write(tmp_path: &Path, target_path: &Path, content: &[u8]) -> Result<(), String> {
        if let Some(parent) = target_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        {
            let mut file = fs::File::create(tmp_path).map_err(|e| e.to_string())?;
            file.write_all(content).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
        }
        fs::rename(tmp_path, target_path).map_err(|e| e.to_string())
    }

    pub fn load_settings(&self) -> AppSettings {
        fs::read_to_string(self.settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) {
        if let Ok(json) = serde_json::to_string_pretty(settings) {
            let _ = Self::atomic_write(
                &self.settings_tmp_path(),
                &self.settings_path(),
                json.as_bytes(),
            );
        }
    }

    fn read_messages_file(path: &Path) -> Option<Vec<SmsMessage>> {
        let raw = fs::read_to_string(path).ok()?;
        serde_json::from_str::<Vec<SmsMessage>>(&raw).ok()
    }

    pub fn load_messages(&self) -> Vec<SmsMessage> {
        let main_path = self.messages_path();
        let bak_path = self.messages_bak_path();

        match Self::read_messages_file(&main_path) {
            Some(msgs) if !msgs.is_empty() => {
                // 主文件正常且有数据，顺便同步一份到 .bak
                if !bak_path.exists() {
                    let _ = fs::copy(&main_path, &bak_path);
                }
                msgs
            }
            main_res => {
                // 主文件不存在、损坏，或被意外写成空数组 `[]` 时，尝试从 .bak 备份恢复
                if let Some(bak_msgs) = Self::read_messages_file(&bak_path) {
                    if !bak_msgs.is_empty() {
                        let _ = fs::copy(&bak_path, &main_path);
                        return bak_msgs;
                    }
                }
                main_res.unwrap_or_default()
            }
        }
    }

    /// 原子保存短信列表；若列表非空则同步更新 `.bak` 备份文件。
    pub fn save_messages(&self, messages: &[SmsMessage]) -> Result<(), String> {
        let json = serde_json::to_string_pretty(messages).map_err(|e| e.to_string())?;
        let main_path = self.messages_path();
        let tmp_path = self.messages_tmp_path();
        let bak_path = self.messages_bak_path();

        Self::atomic_write(&tmp_path, &main_path, json.as_bytes())?;
        if !messages.is_empty() {
            let _ = fs::copy(&main_path, &bak_path);
        }
        Ok(())
    }

    /// 用户主动删除/清空短信时调用，同步更新主文件与 `.bak` 备份，允许变为空列表。
    pub fn save_messages_allow_empty(&self, messages: &[SmsMessage]) -> Result<(), String> {
        let json = serde_json::to_string_pretty(messages).map_err(|e| e.to_string())?;
        let main_path = self.messages_path();
        let tmp_path = self.messages_tmp_path();
        let bak_path = self.messages_bak_path();

        Self::atomic_write(&tmp_path, &main_path, json.as_bytes())?;
        if messages.is_empty() {
            let _ = fs::remove_file(&bak_path);
        } else {
            let _ = fs::copy(&main_path, &bak_path);
        }
        Ok(())
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
