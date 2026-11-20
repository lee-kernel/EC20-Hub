use std::fs;
use std::path::Path;
use std::process::Command;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn hidden_pnputil() -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new("pnputil");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

struct EmbeddedFile {
    rel_path: &'static str,
    bytes: &'static [u8],
}

const EMBEDDED_DRIVER_FILES: &[EmbeddedFile] = &[
    EmbeddedFile {
        rel_path: "qcfilter.inf",
        bytes: include_bytes!("../drivers/qcfilter.inf"),
    },
    EmbeddedFile {
        rel_path: "qcfilter.cat",
        bytes: include_bytes!("../drivers/qcfilter.cat"),
    },
    EmbeddedFile {
        rel_path: "filter/amd64/qcusbfilter.sys",
        bytes: include_bytes!("../drivers/filter/amd64/qcusbfilter.sys"),
    },
    EmbeddedFile {
        rel_path: "qcser.inf",
        bytes: include_bytes!("../drivers/qcser.inf"),
    },
    EmbeddedFile {
        rel_path: "qcser.cat",
        bytes: include_bytes!("../drivers/qcser.cat"),
    },
    EmbeddedFile {
        rel_path: "qcmdm.inf",
        bytes: include_bytes!("../drivers/qcmdm.inf"),
    },
    EmbeddedFile {
        rel_path: "qcmdm.cat",
        bytes: include_bytes!("../drivers/qcmdm.cat"),
    },
    EmbeddedFile {
        rel_path: "serial/amd64/qcusbser.sys",
        bytes: include_bytes!("../drivers/serial/amd64/qcusbser.sys"),
    },
];

pub fn extract_drivers_to(base_dir: &Path) -> Result<std::path::PathBuf, String> {
    let drv_dir = base_dir.join("quectel_whql_drivers");
    for file in EMBEDDED_DRIVER_FILES {
        let target = drv_dir.join(file.rel_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建驱动目录失败: {e}"))?;
        }
        let need_write = match fs::metadata(&target) {
            Ok(meta) => meta.len() != file.bytes.len() as u64,
            Err(_) => true,
        };
        if need_write {
            fs::write(&target, file.bytes).map_err(|e| format!("释放驱动文件失败: {e}"))?;
        }
    }
    Ok(drv_dir)
}

/// 查找系统驱动库中冲突的移远 NDIS 网卡驱动 (qcwwan.inf -> oemXX.inf) 并卸载，
/// 让 Windows 10/11 自动使用原生 netwmbclass.inf (MBIM 通用移动宽带驱动)。
pub fn remove_conflicting_qcwwan_driver() -> usize {
    let Ok(out) = hidden_pnputil().args(["/enum-drivers"]).output() else {
        return 0;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut removed = 0;
    let mut current_oem: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            current_oem = None;
            continue;
        }
        let lower = trimmed.to_lowercase();
        // Match "Published Name: oem41.inf" or "发布名称: oem41.inf"
        if let Some((_, val)) = trimmed.split_once(':') {
            let v = val.trim();
            let v_lower = v.to_lowercase();
            if v_lower.starts_with("oem") && v_lower.ends_with(".inf") {
                current_oem = Some(v.to_string());
            } else if v_lower == "qcwwan.inf" {
                if let Some(ref oem) = current_oem {
                    let res = hidden_pnputil()
                        .args(["/delete-driver", oem, "/uninstall", "/force"])
                        .output();
                    if res.map(|o| o.status.success()).unwrap_or(false) {
                        removed += 1;
                    }
                }
            }
        } else if lower.contains("qcwwan.inf") {
            if let Some(ref oem) = current_oem {
                let res = hidden_pnputil()
                    .args(["/delete-driver", oem, "/uninstall", "/force"])
                    .output();
                if res.map(|o| o.status.success()).unwrap_or(false) {
                    removed += 1;
                }
            }
        }
    }
    removed
}

/// 一键安装内置移远 WHQL 串口/复合设备驱动（qcfilter.inf + qcser.inf + qcmdm.inf），
/// 并自动清理冲突的 qcwwan.inf NDIS 网卡驱动，最后触发扫描硬件改动。
pub fn install_quectel_serial_drivers(base_dir: &Path) -> Result<String, String> {
    let drv_dir = extract_drivers_to(base_dir)?;
    let infs = ["qcfilter.inf", "qcser.inf", "qcmdm.inf"];
    let mut installed_count = 0;

    for inf_name in infs {
        let inf_path = drv_dir.join(inf_name);
        let out = hidden_pnputil()
            .args([
                "/add-driver",
                &inf_path.to_string_lossy(),
                "/install",
            ])
            .output()
            .map_err(|e| format!("调用 pnputil 失败: {e}"))?;

        // pnputil exit code 0 = success, 259 (STILL_ACTIVE / already up-to-date) or 3010 (reboot required) are also acceptable
        let code = out.status.code().unwrap_or(-1);
        if out.status.success() || code == 259 || code == 3010 {
            installed_count += 1;
        } else {
            let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            if detail.contains("Access is denied")
                || detail.contains("拒绝访问")
                || detail.contains("管理员")
                || code == 5
            {
                return Err("安装驱动需要管理员权限，请以管理员身份运行 EC20 Hub".to_string());
            }
        }
    }

    let removed_ndis = remove_conflicting_qcwwan_driver();
    let _ = hidden_pnputil().args(["/scan-devices"]).output();

    if installed_count == 0 {
        return Err("未能安装驱动，请确认已使用管理员权限运行程序".to_string());
    }

    if removed_ndis > 0 {
        Ok(format!(
            "已安装移远 WHQL 串口驱动 ({installed_count}/3)，并清理了 {removed_ndis} 个冲突的移远 NDIS 网卡驱动"
        ))
    } else {
        Ok(format!(
            "已安装/修复移远 WHQL 串口驱动 ({installed_count}/3)，并已刷新系统硬件列表"
        ))
    }
}
