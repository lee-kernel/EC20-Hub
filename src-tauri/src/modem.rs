use crate::pdu::{self, DecodedPdu};
use serde::Serialize;
use serialport::{SerialPort, SerialPortType};
use std::io::{Read, Write};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    pub port_name: String,
    pub description: String,
    pub is_quectel_at: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModemStatus {
    pub connected: bool,
    pub port_name: String,
    pub operator: String,
    pub reg_status: String,
    pub signal_dbm: Option<i32>,
    pub signal_bars: u8,
    pub rat: String,
    pub sim_inserted: bool,
    pub imei: String,
    pub firmware: String,
    pub iccid: String,
    pub imsi: String,
    pub sim_sms_used: u32,
    pub sim_sms_total: u32,
    pub me_sms_used: u32,
    pub me_sms_total: u32,
    pub last_updated: String,
    pub error: Option<String>,
}

pub fn list_ports() -> Vec<PortInfo> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    ports
        .into_iter()
        .map(|p| {
            let mut desc = p.port_name.clone();
            let mut is_quectel_at = false;
            if let SerialPortType::UsbPort(info) = &p.port_type {
                let prod = info.product.clone().unwrap_or_default();
                let mfg = info.manufacturer.clone().unwrap_or_default();
                desc = if !prod.is_empty() {
                    format!("{prod} ({})", p.port_name)
                } else if !mfg.is_empty() {
                    format!("{mfg} ({})", p.port_name)
                } else {
                    p.port_name.clone()
                };
                if info.vid == 0x2C7C {
                    let upper = desc.to_uppercase();
                    if upper.contains("AT") || upper.contains("MI_02") {
                        is_quectel_at = true;
                    }
                }
            }
            PortInfo {
                port_name: p.port_name,
                description: desc,
                is_quectel_at,
            }
        })
        .collect()
}

pub fn detect_best_port(preferred: Option<&str>) -> Option<String> {
    let ports = list_ports();
    if let Some(pref) = preferred {
        if ports
            .iter()
            .any(|p| p.port_name.eq_ignore_ascii_case(pref) && p.is_quectel_at)
        {
            return Some(pref.to_string());
        }
    }
    if let Some(p) = ports.iter().find(|p| p.is_quectel_at) {
        return Some(p.port_name.clone());
    }
    if let Some(pref) = preferred {
        if ports.iter().any(|p| p.port_name.eq_ignore_ascii_case(pref)) {
            return Some(pref.to_string());
        }
    }
    if let Some(p) = ports
        .iter()
        .find(|p| p.description.to_uppercase().contains("QUECTEL"))
    {
        return Some(p.port_name.clone());
    }
    None
}

pub struct ModemClient {
    pub port_name: String,
    port: Box<dyn SerialPort>,
    pub logs: Vec<String>,
    pub pending_cmti: Vec<u32>,
    pub io_failed: bool,
}

impl ModemClient {
    pub fn open(port_name: &str) -> Result<Self, String> {
        let port = serialport::new(port_name, 115200)
            .timeout(Duration::from_millis(200))
            .open()
            .map_err(|e| format!("无法打开串口 {port_name}: {e}"))?;

        let mut client = Self {
            port_name: port_name.to_string(),
            port,
            logs: Vec::new(),
            pending_cmti: Vec::new(),
            io_failed: false,
        };
        client.initialize()?;
        Ok(client)
    }

    fn push_log(&mut self, line: &str) {
        if self.logs.len() >= 300 {
            self.logs.remove(0);
        }
        let now = chrono::Local::now().format("%H:%M:%S");
        self.logs.push(format!("[{now}] {line}"));
    }

    fn handle_urc(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.starts_with("+CMTI:") {
            if let Some((_, idx_str)) = trimmed.rsplit_once(',') {
                if let Ok(idx) = idx_str.trim().parse::<u32>() {
                    self.pending_cmti.push(idx);
                }
            }
        }
    }

    pub fn initialize(&mut self) -> Result<(), String> {
        self.send_at("ATE0", 1000)?;
        let _ = self.send_at("AT+CMEE=2", 1000);
        let _ = self.send_at("AT+CMGF=0", 1000);
        let _ = self.send_at("AT+CPMS=\"SM\",\"SM\",\"SM\"", 1000);
        let _ = self.send_at("AT+CNMI=2,1,0,0,0", 1000);
        Ok(())
    }

    pub fn drain_urcs(&mut self) {
        if self.io_failed {
            return;
        }
        let mut buf = [0u8; 1024];
        let mut collected = String::new();
        loop {
            match self.port.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    collected.push_str(&String::from_utf8_lossy(&buf[..n]));
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => break,
                Err(_) => {
                    self.io_failed = true;
                    break;
                }
            }
        }
        if !collected.is_empty() {
            for line in collected.lines() {
                let t = line.trim();
                if !t.is_empty() {
                    self.push_log(&format!("<< {t}"));
                    self.handle_urc(t);
                }
            }
        }
    }

    pub fn send_at(&mut self, cmd: &str, timeout_ms: u64) -> Result<Vec<String>, String> {
        if self.io_failed {
            return Err("串口连接已断开".to_string());
        }
        self.drain_urcs();
        if self.io_failed {
            return Err("串口连接已断开".to_string());
        }
        self.push_log(&format!(">> {cmd}"));

        let payload = format!("{cmd}\r\n");
        if let Err(e) = self.port.write_all(payload.as_bytes()) {
            self.io_failed = true;
            return Err(format!("串口写入失败: {e}"));
        }
        let _ = self.port.flush();

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut raw = String::new();
        let mut buf = [0u8; 1024];
        let mut lines_out = Vec::new();

        while Instant::now() < deadline {
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    raw.push_str(&String::from_utf8_lossy(&buf[..n]));
                    while let Some(pos) = raw.find('\n') {
                        let line = raw[..pos].trim().to_string();
                        raw.drain(..=pos);
                        if line.is_empty() || line == cmd.trim() {
                            continue;
                        }
                        self.push_log(&format!("<< {line}"));
                        if line == "OK" {
                            return Ok(lines_out);
                        }
                        if line == "ERROR"
                            || line.starts_with("+CME ERROR:")
                            || line.starts_with("+CMS ERROR:")
                        {
                            return Err(line);
                        }
                        self.handle_urc(&line);
                        lines_out.push(line);
                    }
                }
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    self.io_failed = true;
                    return Err(format!("串口读取失败: {e}"));
                }
            }
        }

        if !lines_out.is_empty() {
            Ok(lines_out)
        } else {
            Err(format!("AT 指令超时: {cmd}"))
        }
    }

    pub fn send_pdu_sms(&mut self, recipient: &str, text: &str) -> Result<(), String> {
        let (tpdu_len, pdu_hex) = pdu::encode_ucs2_submit_pdu(recipient, text)?;
        self.drain_urcs();
        if self.io_failed {
            return Err("串口连接已断开".to_string());
        }
        let cmd = format!("AT+CMGS={tpdu_len}\r\n");
        self.push_log(&format!(">> AT+CMGS={tpdu_len}"));
        if let Err(e) = self.port.write_all(cmd.as_bytes()) {
            self.io_failed = true;
            return Err(format!("串口写入失败: {e}"));
        }
        let _ = self.port.flush();

        // Wait for '>' prompt
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut raw = String::new();
        let mut buf = [0u8; 256];
        let mut got_prompt = false;
        while Instant::now() < deadline {
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    raw.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if raw.contains('>') {
                        got_prompt = true;
                        break;
                    }
                }
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    self.io_failed = true;
                    return Err(format!("串口读取失败: {e}"));
                }
            }
        }
        if !got_prompt {
            let _ = self.port.write_all(&[0x1B]); // ESC cancel
            return Err("未收到模块发送提示符 '>'".to_string());
        }

        let body = format!("{pdu_hex}\x1A");
        self.push_log(&format!(">> [PDU {} bytes] <Ctrl+Z>", pdu_hex.len() / 2));
        if let Err(e) = self.port.write_all(body.as_bytes()) {
            self.io_failed = true;
            return Err(format!("串口写入失败: {e}"));
        }
        let _ = self.port.flush();

        let send_deadline = Instant::now() + Duration::from_secs(15);
        raw.clear();
        while Instant::now() < send_deadline {
            match self.port.read(&mut buf) {
                Ok(n) if n > 0 => {
                    raw.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if raw.contains("\nOK") || raw.trim().ends_with("OK") {
                        self.push_log("<< OK (短信已发送)");
                        return Ok(());
                    }
                    if raw.contains("ERROR") {
                        self.push_log(&format!("<< {}", raw.trim()));
                        return Err(raw.trim().to_string());
                    }
                }
                Ok(_) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => {
                    self.io_failed = true;
                    return Err(format!("串口读取失败: {e}"));
                }
            }
        }
        Err("发送短信超时".to_string())
    }

    pub fn query_device_info(&mut self) -> (String, String, String, String) {
        let imei = self
            .send_at("AT+GSN", 1000)
            .ok()
            .and_then(|l| l.into_iter().find(|s| s.chars().all(|c| c.is_ascii_digit()) && s.len() >= 14))
            .unwrap_or_default();

        let firmware = self
            .send_at("AT+CGMR", 1000)
            .ok()
            .and_then(|l| l.into_iter().next())
            .unwrap_or_default();

        let iccid = self
            .send_at("AT+QCCID", 1000)
            .ok()
            .and_then(|l| {
                l.into_iter().find_map(|s| {
                    s.strip_prefix("+QCCID:")
                        .map(|v| v.trim().to_string())
                })
            })
            .unwrap_or_default();

        let imsi = self
            .send_at("AT+CIMI", 1000)
            .ok()
            .and_then(|l| l.into_iter().find(|s| s.chars().all(|c| c.is_ascii_digit()) && s.len() >= 14))
            .unwrap_or_default();

        (imei, firmware, iccid, imsi)
    }

    pub fn query_status(&mut self, cached: &mut ModemStatus) -> Result<(), String> {
        self.send_at("AT", 1000)?;
        if self.io_failed {
            return Err("串口连接已断开".to_string());
        }

        cached.connected = true;
        cached.port_name = self.port_name.clone();
        cached.error = None;

        if cached.imei.is_empty() || cached.iccid.is_empty() {
            let (imei, fw, iccid, imsi) = self.query_device_info();
            if self.io_failed {
                return Err("串口连接已断开".to_string());
            }
            if !imei.is_empty() {
                cached.imei = imei;
            }
            if !fw.is_empty() {
                cached.firmware = fw;
            }
            if !iccid.is_empty() {
                cached.iccid = iccid;
            }
            if !imsi.is_empty() {
                cached.imsi = imsi;
            }
        }

        // SIM status
        if let Ok(lines) = self.send_at("AT+CPIN?", 1000) {
            cached.sim_inserted = lines.iter().any(|l| l.contains("READY"));
        } else if self.io_failed {
            return Err("串口连接已断开".to_string());
        } else {
            cached.sim_inserted = false;
        }

        // Signal strength
        if let Ok(lines) = self.send_at("AT+CSQ", 1000) {
            if let Some(line) = lines.iter().find(|l| l.starts_with("+CSQ:")) {
                if let Some(val) = line.strip_prefix("+CSQ:") {
                    if let Some((rssi_s, _)) = val.trim().split_once(',') {
                        if let Ok(rssi) = rssi_s.trim().parse::<i32>() {
                            if rssi == 99 {
                                cached.signal_dbm = None;
                                cached.signal_bars = 0;
                            } else {
                                let dbm = -113 + 2 * rssi;
                                cached.signal_dbm = Some(dbm);
                                cached.signal_bars = match dbm {
                                    d if d >= -75 => 4,
                                    d if d >= -85 => 3,
                                    d if d >= -95 => 2,
                                    _ => 1,
                                };
                            }
                        }
                    }
                }
            }
        }

        // Operator
        if let Ok(lines) = self.send_at("AT+COPS?", 1000) {
            if let Some(line) = lines.iter().find(|l| l.starts_with("+COPS:")) {
                if let Some(start) = line.find('"') {
                    if let Some(end) = line[start + 1..].find('"') {
                        let op = &line[start + 1..start + 1 + end];
                        cached.operator = match op {
                            "CHINA TELECOM" | "CHN-CT" | "46011" => "中国电信".to_string(),
                            "CHINA MOBILE" | "CMCC" | "46000" | "46002" => "中国移动".to_string(),
                            "CHN-UNICOM" | "CHINA UNICOM" | "46001" => "中国联通".to_string(),
                            other => other.to_string(),
                        };
                    }
                }
            }
        }
        if cached.operator.is_empty() {
            cached.operator = "未知运营商".to_string();
        }

        // Registration
        if let Ok(lines) = self.send_at("AT+CEREG?", 1000) {
            if let Some(line) = lines.iter().find(|l| l.starts_with("+CEREG:")) {
                let parts: Vec<&str> = line.split(',').collect();
                let stat = parts.get(1).map(|s| s.trim()).unwrap_or("");
                cached.reg_status = match stat {
                    "1" => "已注册".to_string(),
                    "5" => "已注册 (漫游)".to_string(),
                    "2" => "正在搜索...".to_string(),
                    "3" => "注册被拒".to_string(),
                    _ => "未注册".to_string(),
                };
            }
        }

        // RAT
        if let Ok(lines) = self.send_at("AT+QNWINFO", 1000) {
            if let Some(line) = lines.iter().find(|l| l.starts_with("+QNWINFO:")) {
                if let Some(start) = line.find('"') {
                    if let Some(end) = line[start + 1..].find('"') {
                        let raw_rat = &line[start + 1..start + 1 + end];
                        cached.rat = if raw_rat.contains("LTE") {
                            "LTE".to_string()
                        } else if raw_rat.contains("WCDMA") || raw_rat.contains("TDSCDMA") {
                            "3G".to_string()
                        } else if raw_rat.contains("GSM") {
                            "2G".to_string()
                        } else {
                            raw_rat.to_string()
                        };
                    }
                }
            }
        }
        if cached.rat.is_empty() {
            cached.rat = "LTE".to_string();
        }

        // Storage usage: query SM and ME
        if let Ok(lines) = self.send_at("AT+CPMS=\"SM\",\"SM\",\"SM\"", 1000) {
            if let Some(line) = lines.iter().find(|l| l.starts_with("+CPMS:")) {
                let nums: Vec<u32> = line
                    .split(|c: char| !c.is_ascii_digit())
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if nums.len() >= 2 {
                    cached.sim_sms_used = nums[0];
                    cached.sim_sms_total = nums[1];
                }
            }
        }
        if let Ok(lines) = self.send_at("AT+CPMS=\"ME\",\"ME\",\"ME\"", 1000) {
            if let Some(line) = lines.iter().find(|l| l.starts_with("+CPMS:")) {
                let nums: Vec<u32> = line
                    .split(|c: char| !c.is_ascii_digit())
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if nums.len() >= 2 {
                    cached.me_sms_used = nums[0];
                    cached.me_sms_total = nums[1];
                }
            }
        }

        // Restore default SM
        let _ = self.send_at("AT+CPMS=\"SM\",\"SM\",\"SM\"", 1000);
        if self.io_failed {
            return Err("串口连接已断开".to_string());
        }

        cached.last_updated = chrono::Local::now().format("%H:%M:%S").to_string();
        Ok(())
    }

    pub fn read_all_pdu_from(&mut self, storage: &str) -> Vec<(u32, DecodedPdu)> {
        let mut results = Vec::new();
        let cmd = format!("AT+CPMS=\"{storage}\",\"{storage}\",\"{storage}\"");
        if self.send_at(&cmd, 1000).is_err() {
            return results;
        }
        let Ok(lines) = self.send_at("AT+CMGL=4", 5000) else {
            return results;
        };

        let mut i = 0;
        while i < lines.len() {
            let line = &lines[i];
            if line.starts_with("+CMGL:") {
                let idx_opt = line
                    .strip_prefix("+CMGL:")
                    .and_then(|s| s.split(',').next())
                    .and_then(|s| s.trim().parse::<u32>().ok());
                if let (Some(idx), Some(pdu_hex)) = (idx_opt, lines.get(i + 1)) {
                    if let Ok(decoded) = pdu::decode_pdu(pdu_hex) {
                        results.push((idx, decoded));
                    }
                    i += 2;
                    continue;
                }
            }
            i += 1;
        }
        results
    }

    pub fn delete_sms_indices(&mut self, storage: &str, indices: &[u32]) {
        if indices.is_empty() {
            return;
        }
        let cmd = format!("AT+CPMS=\"{storage}\",\"{storage}\",\"{storage}\"");
        if self.send_at(&cmd, 1000).is_ok() {
            for &idx in indices {
                let _ = self.send_at(&format!("AT+CMGD={idx}"), 1500);
            }
        }
    }

    pub fn clear_all_sms(&mut self) -> Result<(), String> {
        for storage in ["SM", "ME"] {
            let _ = self.send_at(&format!("AT+CPMS=\"{storage}\",\"{storage}\",\"{storage}\""), 1000);
            let _ = self.send_at("AT+CMGD=1,4", 3000);
        }
        Ok(())
    }
}
