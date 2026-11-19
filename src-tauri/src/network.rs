use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::Networks;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkStats {
    pub adapter_name: String,
    pub adapter_up: bool,
    pub session_rx_bytes: u64,
    pub session_tx_bytes: u64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
}

#[derive(Debug, Clone)]
struct ResolvedAdapter {
    friendly_name: String,
    description: String,
}

/// Windows 下创建子进程时不弹出控制台窗口
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hidden_command(program: &str) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

fn base64_encode(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

struct PsOutput {
    stdout: String,
    stderr: String,
    success: bool,
}

/// 运行 PowerShell 脚本，强制以 UTF-8 输出，避免中文系统下 GBK 代码页导致的乱码。
fn run_powershell(script: &str, envs: &[(&str, &str)]) -> Result<PsOutput, String> {
    let full = format!(
        "$ErrorActionPreference='Continue';\
         [Console]::OutputEncoding=[System.Text.Encoding]::UTF8;\
         $OutputEncoding=[System.Text.Encoding]::UTF8;\n{script}"
    );
    // 使用 -EncodedCommand (UTF-16LE + Base64)，彻底规避命令行引号转义与代码页问题
    let utf16: Vec<u8> = full.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let encoded = base64_encode(&utf16);
    let mut cmd = hidden_command("powershell");
    cmd.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-EncodedCommand",
        &encoded,
    ]);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd.output().map_err(|e| e.to_string())?;
    Ok(PsOutput {
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        success: output.status.success(),
    })
}

fn find_quectel_adapter_via_dotnet() -> Option<ResolvedAdapter> {
    let script = "[System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object { $_.Description -match 'Quectel|RMNET|MBIM' } | Select-Object -First 1 | ForEach-Object { \"$($_.Name)|$($_.Description)\" }";
    let out = run_powershell(script, &[]).ok()?;
    let text = out.stdout;
    let (name, desc) = text.split_once('|')?;
    if name.trim().is_empty() {
        return None;
    }
    Some(ResolvedAdapter {
        friendly_name: name.trim().to_string(),
        description: desc.trim().to_string(),
    })
}

pub struct TrafficTracker {
    networks: Networks,
    resolved: Option<ResolvedAdapter>,
    last_resolve: Option<Instant>,
    baseline_rx: Option<u64>,
    baseline_tx: Option<u64>,
}

/// 未找到网卡时，重新探测的最小间隔（PowerShell 启动较慢，避免每次轮询都调用）
const RESOLVE_RETRY: Duration = Duration::from_secs(30);

impl TrafficTracker {
    /// 构造函数保持轻量，不调用 PowerShell，避免阻塞启动
    pub fn new() -> Self {
        Self {
            networks: Networks::new_with_refreshed_list(),
            resolved: None,
            last_resolve: None,
            baseline_rx: None,
            baseline_tx: None,
        }
    }

    pub fn poll(&mut self) -> NetworkStats {
        if self.resolved.is_none()
            && self.last_resolve.map_or(true, |t| t.elapsed() >= RESOLVE_RETRY)
        {
            self.last_resolve = Some(Instant::now());
            self.resolved = find_quectel_adapter_via_dotnet();
        }

        self.networks.refresh(true);

        let mut matched_label = self
            .resolved
            .as_ref()
            .map(|r| format!("{} ({})", r.friendly_name, r.description))
            .unwrap_or_else(|| "未检测到 Quectel 网卡".to_string());

        let mut total_rx = 0u64;
        let mut total_tx = 0u64;
        let mut found = false;

        for (name, data) in &self.networks {
            let is_match = if let Some(ref res) = self.resolved {
                name.eq_ignore_ascii_case(&res.friendly_name)
            } else {
                let lower = name.to_lowercase();
                lower.contains("quectel")
                    || lower.contains("蜂窝")
                    || lower.contains("cellular")
                    || lower.contains("rmnet")
            };

            if is_match {
                if self.resolved.is_none() {
                    matched_label = name.clone();
                }
                total_rx = data.total_received();
                total_tx = data.total_transmitted();
                found = true;
                break;
            }
        }

        if found && self.baseline_rx.is_none() {
            self.baseline_rx = Some(total_rx);
            self.baseline_tx = Some(total_tx);
        }

        let session_rx = total_rx.saturating_sub(self.baseline_rx.unwrap_or(total_rx));
        let session_tx = total_tx.saturating_sub(self.baseline_tx.unwrap_or(total_tx));

        NetworkStats {
            adapter_name: matched_label,
            adapter_up: found && (total_rx > 0 || total_tx > 0),
            session_rx_bytes: session_rx,
            session_tx_bytes: session_tx,
            total_rx_bytes: total_rx,
            total_tx_bytes: total_tx,
        }
    }
}

pub fn check_4g_route() -> Result<String, String> {
    let script = r#"
$n = [System.Net.NetworkInformation.NetworkInterface]::GetAllNetworkInterfaces() | Where-Object { $_.Description -match 'Quectel|RMNET|MBIM' } | Select-Object -First 1
if (-not $n) { '未找到 Quectel 网卡'; exit 0 }
$props = $n.GetIPProperties()
$ip = ($props.UnicastAddresses | Where-Object { $_.Address.AddressFamily -eq 'InterNetwork' } | Select-Object -First 1).Address.IPAddressToString
$gw = ($props.GatewayAddresses | Where-Object { $_.Address.AddressFamily -eq 'InterNetwork' } | Select-Object -First 1).Address.IPAddressToString
"网卡: $($n.Name) | 状态: $($n.OperationalStatus) | IPv4: $ip | 网关: $gw"
"#;
    let out = run_powershell(script, &[])?;
    if !out.stdout.is_empty() {
        Ok(out.stdout)
    } else if !out.stderr.is_empty() {
        Err(out.stderr)
    } else {
        Ok("检查完成".to_string())
    }
}

pub fn set_adapter_enabled(enable: bool) -> Result<String, String> {
    let adapter = find_quectel_adapter_via_dotnet()
        .ok_or_else(|| "未找到 Quectel 网卡".to_string())?;
    let action_cmd = if enable {
        "Enable-NetAdapter -Name $env:EC20_IFACE -Confirm:$false -ErrorAction Stop"
    } else {
        "Disable-NetAdapter -Name $env:EC20_IFACE -Confirm:$false -ErrorAction Stop"
    };
    let out = run_powershell(action_cmd, &[("EC20_IFACE", &adapter.friendly_name)])?;

    if out.success {
        Ok(format!(
            "已{}网卡: {}",
            if enable { "开启" } else { "关闭" },
            adapter.friendly_name
        ))
    } else {
        let combined = if !out.stderr.is_empty() {
            out.stderr
        } else {
            out.stdout
        };
        if combined.contains("Access")
            || combined.contains("denied")
            || combined.contains("拒绝")
            || combined.contains("权限")
            || combined.contains("提升")
            || combined.contains("管理员")
        {
            Err("需要管理员权限才能启用/禁用网卡，请以管理员身份运行本程序。".to_string())
        } else {
            Err(if combined.is_empty() {
                "切换网卡状态失败（可能需要管理员权限）".to_string()
            } else {
                combined
            })
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterPriorityItem {
    pub if_index: u32,
    pub name: String,
    pub description: String,
    pub status: String,
    pub metric: u32,
    pub is_modem: bool,
    pub is_default_route: bool,
    pub tag: String,
}

pub fn list_adapter_priorities() -> Result<Vec<AdapterPriorityItem>, String> {
    let script = r#"
$defRoute = Get-NetRoute -DestinationPrefix '0.0.0.0/0' -ErrorAction SilentlyContinue |
  ForEach-Object {
    $ipIf = Get-NetIPInterface -InterfaceIndex $_.InterfaceIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue | Select-Object -First 1
    $eff = $_.RouteMetric + ($(if ($ipIf) { $ipIf.InterfaceMetric } else { 0 }))
    [PSCustomObject]@{ InterfaceIndex = $_.InterfaceIndex; Eff = $eff }
  } | Sort-Object Eff | Select-Object -First 1
$defIdx = if ($defRoute) { [int]$defRoute.InterfaceIndex } else { -1 }

$ipMap = @{}
Get-NetIPInterface -AddressFamily IPv4 -ErrorAction SilentlyContinue | ForEach-Object {
  $ipMap[[int]$_.InterfaceIndex] = [int]$_.InterfaceMetric
}

$items = Get-NetAdapter -ErrorAction SilentlyContinue |
  Where-Object { $_.Status -ne 'Not Present' -and $_.Name -notmatch 'Loopback' } |
  ForEach-Object {
    $idx = [int]$_.ifIndex
    $desc = [string]$_.InterfaceDescription
    $name = [string]$_.Name
    $metric = if ($ipMap.ContainsKey($idx)) { $ipMap[$idx] } else { 9999 }
    $isModem = ($desc -match 'Quectel|RMNET|MBIM')
    $tag = if ($isModem) {
      '模块网卡'
    } elseif ($name -match 'Wi-Fi|WLAN|无线' -or $desc -match 'Wi-Fi|WLAN|Wireless|802\.11') {
      'Wi-Fi'
    } elseif ($desc -match 'USB|Dock') {
      'USB'
    } else {
      ''
    }
    [PSCustomObject]@{
      ifIndex = $idx
      name = $name
      description = $desc
      status = [string]$_.Status
      metric = $metric
      isModem = $isModem
      isDefaultRoute = ($idx -eq $defIdx)
      tag = $tag
    }
  } | Sort-Object metric, name

@($items) | ConvertTo-Json -Compress
"#;
    let out = run_powershell(script, &[])?;
    if out.stdout.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str::<Vec<AdapterPriorityItem>>(&out.stdout)
        .map_err(|e| format!("解析网卡列表失败: {e}"))
}

pub fn apply_adapter_priorities(if_indices: Vec<u32>) -> Result<String, String> {
    if if_indices.is_empty() {
        return Err("网卡列表为空".to_string());
    }
    let pairs: Vec<String> = if_indices
        .iter()
        .enumerate()
        .map(|(i, idx)| format!("{idx}:{}", (i + 1) * 10))
        .collect();
    let pairs_str = pairs.join(",");

    let script = r#"
$ErrorActionPreference = 'Stop'
foreach ($pair in $env:EC20_METRICS.Split(',')) {
  $parts = $pair.Split(':')
  $idx = [int]$parts[0]
  $m = [int]$parts[1]
  Set-NetIPInterface -InterfaceIndex $idx -AutomaticMetric Disabled -InterfaceMetric $m
}
'OK'
"#;
    let out = run_powershell(script, &[("EC20_METRICS", &pairs_str)])?;
    if out.success && out.stdout.contains("OK") {
        Ok("已应用网卡优先级顺序".to_string())
    } else {
        let combined = if !out.stderr.is_empty() {
            out.stderr
        } else {
            out.stdout
        };
        if combined.contains("Access")
            || combined.contains("denied")
            || combined.contains("拒绝")
            || combined.contains("权限")
        {
            Err("需要管理员权限才能修改网卡优先级，请以管理员身份运行本程序。".to_string())
        } else {
            Err(if combined.is_empty() {
                "应用网卡优先级失败".to_string()
            } else {
                combined
            })
        }
    }
}

