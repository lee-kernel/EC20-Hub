mod modem;
mod network;
mod pdu;
mod storage;

use modem::{ModemClient, ModemStatus};
use network::{AdapterPriorityItem, NetworkStats, TrafficTracker};
use serde::Serialize;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use storage::{extract_otp, AppSettings, SmsMessage, Store};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent,
};
#[cfg(not(windows))]
use tauri_plugin_notification::NotificationExt;

#[cfg(windows)]
const APP_AUMID: &str = "com.ec20hub.app";

#[cfg(windows)]
fn register_windows_toast_aumid(data_dir: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let _ = std::fs::create_dir_all(data_dir);
    let icon_path = data_dir.join("icon.png");
    let _ = std::fs::write(&icon_path, include_bytes!("../icons/icon.png"));
    let icon_str = icon_path.to_string_lossy().to_string();

    let key = format!(r"HKCU\Software\Classes\AppUserModelId\{APP_AUMID}");
    let entries = [
        ("DisplayName", "EC20 Hub"),
        ("IconUri", icon_str.as_str()),
        ("IconBackgroundColor", "0"),
    ];
    for (name, val) in entries {
        let _ = std::process::Command::new("reg.exe")
            .args(["add", &key, "/v", name, "/t", "REG_SZ", "/d", val, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

fn send_sms_notification(app: &AppHandle, title: &str, body: &str) {
    #[cfg(windows)]
    {
        let app_handle = app.clone();
        let _ = tauri_winrt_notification::Toast::new(APP_AUMID)
            .title(title)
            .text1(body)
            .sound(Some(tauri_winrt_notification::Sound::Default))
            .on_activated(move |_action| {
                open_sms_page(&app_handle);
                Ok(())
            })
            .show();
    }

    #[cfg(not(windows))]
    {
        let _ = app
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show();
    }
}

#[derive(Clone)]
struct SharedSnapshot {
    status: ModemStatus,
    network: NetworkStats,
    settings: AppSettings,
    messages: Vec<SmsMessage>,
    logs: Vec<String>,
}

#[derive(Clone)]
struct AppState {
    snapshot: Arc<RwLock<SharedSnapshot>>,
    modem: Arc<Mutex<Option<ModemClient>>>,
    store: Arc<Mutex<Store>>,
    traffic: Arc<Mutex<TrafficTracker>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DashboardPayload {
    status: ModemStatus,
    network: NetworkStats,
    settings: AppSettings,
    unread_count: usize,
}

fn build_dashboard(snap: &SharedSnapshot) -> DashboardPayload {
    DashboardPayload {
        status: snap.status.clone(),
        network: snap.network.clone(),
        settings: snap.settings.clone(),
        unread_count: snap.messages.iter().filter(|m| !m.read).count(),
    }
}

fn sync_sms_with_modem(modem: &mut ModemClient, state: &AppState, app: &AppHandle) -> usize {
    let (takeover, notify, existing_messages) = {
        let snap = state.snapshot.read().unwrap();
        (
            snap.settings.takeover_sms,
            snap.settings.notify_sms,
            snap.messages.clone(),
        )
    };

    modem.pending_cmti.clear();
    let mut collected_all = Vec::new();
    let mut pending_deletes: Vec<(&'static str, Vec<u32>)> = Vec::new();
    for storage_name in ["SM", "ME"] {
        let raw_parts = modem.read_all_pdu_from(storage_name);
        if raw_parts.is_empty() {
            continue;
        }
        let merged = pdu::merge_concat_parts(raw_parts);
        let mut indices_to_delete = Vec::new();
        for (indices, decoded) in merged {
            indices_to_delete.extend(indices);
            collected_all.push(decoded);
        }
        if !indices_to_delete.is_empty() {
            pending_deletes.push((storage_name, indices_to_delete));
        }
    }

    if collected_all.is_empty() {
        return 0;
    }

    let mut new_msgs = Vec::new();
    let mut notifications = Vec::new();
    for decoded in collected_all {
        let duplicate = existing_messages.iter().any(|m| {
            m.direction == "in"
                && m.sender == decoded.sender
                && m.timestamp == decoded.timestamp
                && m.text == decoded.text
        });
        if duplicate {
            continue;
        }
        let otp = extract_otp(&decoded.text);
        let msg = SmsMessage {
            id: format!(
                "{}-{}",
                chrono::Local::now().timestamp_millis(),
                existing_messages.len() + new_msgs.len()
            ),
            sender: decoded.sender.clone(),
            text: decoded.text.clone(),
            timestamp: if decoded.timestamp.is_empty() {
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
            } else {
                decoded.timestamp
            },
            direction: "in".to_string(),
            otp_code: otp.clone(),
            read: false,
        };
        new_msgs.push(msg);

        if notify {
            let body = if let Some(code) = otp {
                format!("验证码 {code} - {}", decoded.text)
            } else {
                decoded.text.clone()
            };
            let title = format!("新短信 · {}", decoded.sender);
            notifications.push((title, body));
        }
    }

    let added = new_msgs.len();
    let saved_ok = if added > 0 {
        let updated_messages = {
            let mut snap = state.snapshot.write().unwrap();
            for m in new_msgs.into_iter().rev() {
                snap.messages.insert(0, m);
            }
            snap.messages.clone()
        };
        let res = state.store.lock().unwrap().save_messages(&updated_messages);
        let _ = app.emit("sms-updated", ());
        res.is_ok()
    } else {
        // 读出的短信均已存在于本地归档中
        !existing_messages.is_empty()
    };

    // 仅在本地归档落盘成功后再清理 SIM 卡与模块存储，避免落盘失败或中断导致短信彻底丢失
    if takeover && saved_ok {
        for (storage_name, indices) in &pending_deletes {
            modem.delete_sms_indices(storage_name, indices);
        }
    }

    for (title, body) in notifications {
        send_sms_notification(app, &title, &body);
    }

    added
}

#[tauri::command]
fn get_dashboard_state(state: State<AppState>) -> DashboardPayload {
    let snap = state.snapshot.read().unwrap();
    build_dashboard(&snap)
}

#[tauri::command]
fn get_messages(state: State<AppState>) -> Vec<SmsMessage> {
    state.snapshot.read().unwrap().messages.clone()
}

#[tauri::command]
fn mark_all_read(state: State<AppState>) {
    let maybe_updated = {
        let mut snap = state.snapshot.write().unwrap();
        let mut changed = false;
        for m in &mut snap.messages {
            if !m.read {
                m.read = true;
                changed = true;
            }
        }
        if changed {
            Some(snap.messages.clone())
        } else {
            None
        }
    };
    if let Some(updated) = maybe_updated {
        let _ = state.store.lock().unwrap().save_messages(&updated);
    }
}

#[tauri::command]
fn delete_message(id: String, state: State<AppState>) -> Vec<SmsMessage> {
    let updated = {
        let mut snap = state.snapshot.write().unwrap();
        snap.messages.retain(|m| m.id != id);
        snap.messages.clone()
    };
    let _ = state.store.lock().unwrap().save_messages_allow_empty(&updated);
    updated
}

#[tauri::command]
fn clear_messages(state: State<AppState>) -> Vec<SmsMessage> {
    {
        let mut snap = state.snapshot.write().unwrap();
        snap.messages.clear();
    }
    let _ = state.store.lock().unwrap().save_messages_allow_empty(&[]);
    Vec::new()
}

#[tauri::command]
fn update_settings(settings: AppSettings, state: State<AppState>, app: AppHandle) -> AppSettings {
    let payload = {
        let mut snap = state.snapshot.write().unwrap();
        snap.settings = settings.clone();
        build_dashboard(&snap)
    };
    state.store.lock().unwrap().save_settings(&settings);
    let _ = app.emit("status-updated", payload);
    settings
}

const AUTOSTART_TASK_NAME: &str = "EC20HubAutoStart";

fn hidden_sys_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

#[tauri::command]
async fn get_autostart_enabled() -> bool {
    tokio::task::spawn_blocking(|| {
        hidden_sys_command("schtasks")
            .args(["/Query", "/TN", AUTOSTART_TASK_NAME])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
async fn set_autostart_enabled(enable: bool) -> Result<bool, String> {
    tokio::task::spawn_blocking(move || {
        if enable {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let task_cmd = format!("\"{}\" --autostart", exe.display());
            let out = hidden_sys_command("schtasks")
                .args([
                    "/Create",
                    "/TN",
                    AUTOSTART_TASK_NAME,
                    "/TR",
                    &task_cmd,
                    "/SC",
                    "ONLOGON",
                    "/RL",
                    "HIGHEST",
                    "/F",
                ])
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(true)
            } else {
                Err("创建开机自启任务失败（需要管理员权限）".to_string())
            }
        } else {
            let out = hidden_sys_command("schtasks")
                .args(["/Delete", "/TN", AUTOSTART_TASK_NAME, "/F"])
                .output()
                .map_err(|e| e.to_string())?;
            if out.status.success() {
                Ok(false)
            } else {
                // 若任务本就不存在也视为已关闭
                let exists = hidden_sys_command("schtasks")
                    .args(["/Query", "/TN", AUTOSTART_TASK_NAME])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                if !exists {
                    Ok(false)
                } else {
                    Err("取消开机自启任务失败（需要管理员权限）".to_string())
                }
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn poll_modem_once(state: &AppState, app: &AppHandle) {
    let mut guard = state.modem.lock().unwrap();

    // Step 1: If already connected, try querying existing modem
    let mut disconnected_err: Option<String> = None;
    if let Some(modem) = guard.as_mut() {
        let mut st = state.snapshot.read().unwrap().status.clone();
        modem.drain_urcs();
        match modem.query_status(&mut st) {
            Ok(()) => {
                sync_sms_with_modem(modem, state, app);
                if modem.io_failed {
                    disconnected_err = Some("串口连接已断开，正在重连...".to_string());
                } else {
                    let logs = modem.logs.clone();
                    let mut snap = state.snapshot.write().unwrap();
                    snap.status = st;
                    snap.logs = logs;
                    return;
                }
            }
            Err(e) => {
                disconnected_err = Some(e);
            }
        }
    }

    // If existing modem failed, drop it first so its OS serial handle is closed before scanning
    if disconnected_err.is_some() {
        *guard = None;
    }

    // Step 2: Try to detect and open the best Quectel AT port
    let preferred = state.snapshot.read().unwrap().settings.preferred_port.clone();
    let port_opt = modem::detect_best_port(preferred.as_deref());
    if let Some(port) = port_opt {
        match ModemClient::open(&port) {
            Ok(mut client) => {
                let mut st = ModemStatus::default();
                if let Err(e) = client.query_status(&mut st) {
                    let mut snap = state.snapshot.write().unwrap();
                    snap.status.connected = false;
                    snap.status.port_name = port;
                    snap.status.error = Some(e);
                } else {
                    sync_sms_with_modem(&mut client, state, app);
                    let logs = client.logs.clone();
                    *guard = Some(client);
                    let mut snap = state.snapshot.write().unwrap();
                    snap.status = st;
                    snap.logs = logs;
                }
            }
            Err(e) => {
                let mut snap = state.snapshot.write().unwrap();
                snap.status.connected = false;
                snap.status.port_name = port;
                snap.status.error = Some(e);
            }
        }
    } else {
        let mut snap = state.snapshot.write().unwrap();
        snap.status.connected = false;
        snap.status.port_name.clear();
        snap.status.error = Some(
            disconnected_err.unwrap_or_else(|| "未检测到移远 AT 串口".to_string()),
        );
    }
}

#[tauri::command]
async fn refresh_now(state: State<'_, AppState>, app: AppHandle) -> Result<DashboardPayload, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        poll_modem_once(&st, &app);

        let net = st.traffic.lock().unwrap().poll();
        let payload = {
            let mut snap = st.snapshot.write().unwrap();
            snap.network = net;
            build_dashboard(&snap)
        };
        let _ = app.emit("status-updated", payload.clone());
        payload
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn reboot_modem(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut guard = st.modem.lock().unwrap();
        if guard.is_none() {
            let preferred = st.snapshot.read().unwrap().settings.preferred_port.clone();
            if let Some(port) = modem::detect_best_port(preferred.as_deref()) {
                if let Ok(client) = ModemClient::open(&port) {
                    *guard = Some(client);
                }
            }
        }
        if let Some(modem) = guard.as_mut() {
            let res = modem.send_at("AT+CFUN=1,1", 2000);
            *guard = None;
            let payload = {
                let mut snap = st.snapshot.write().unwrap();
                snap.status.connected = false;
                snap.status.reg_status = "正在重启...".to_string();
                snap.status.error = Some("模块正在重启，请稍候...".to_string());
                build_dashboard(&snap)
            };
            let _ = app.emit("status-updated", payload);
            if let Err(e) = res {
                // AT+CFUN=1,1 可能导致 USB 立即断开从而读取不到 OK，若不是写入前就断开则仍视为已下发
                if e.contains("串口写入失败") || e.contains("串口连接已断开") {
                    return Err(format!("重启指令发送失败（模块已断开）: {e}"));
                }
            }
            Ok("已发送重启指令，模块将在约 15 秒后自动重连".to_string())
        } else {
            Err("模块未连接".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn send_sms(
    recipient: String,
    text: String,
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<SmsMessage, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        {
            let mut guard = st.modem.lock().unwrap();
            let modem = guard.as_mut().ok_or_else(|| "模块未连接".to_string())?;
            let res = modem.send_pdu_sms(&recipient, &text);
            if modem.io_failed {
                *guard = None;
            }
            res?;
        }

        let msg = SmsMessage {
            id: format!(
                "{}-out-{}",
                chrono::Local::now().timestamp_millis(),
                st.snapshot.read().unwrap().messages.len()
            ),
            sender: recipient,
            text,
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            direction: "out".to_string(),
            otp_code: None,
            read: true,
        };

        let updated = {
            let mut snap = st.snapshot.write().unwrap();
            snap.messages.insert(0, msg.clone());
            snap.messages.clone()
        };
        let _ = st.store.lock().unwrap().save_messages(&updated);
        let _ = app.emit("sms-updated", ());
        Ok(msg)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn clear_modem_storage(state: State<'_, AppState>, app: AppHandle) -> Result<String, String> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || {
        let mut status = st.snapshot.read().unwrap().status.clone();
        let logs = {
            let mut guard = st.modem.lock().unwrap();
            let modem = guard.as_mut().ok_or_else(|| "模块未连接".to_string())?;
            let clear_res = modem.clear_all_sms();
            if modem.io_failed {
                *guard = None;
                return Err("串口连接已断开，请等待模块自动重连后再试".to_string());
            }
            clear_res?;
            if let Err(e) = modem.query_status(&mut status) {
                *guard = None;
                return Err(e);
            }
            modem.logs.clone()
        };
        let payload = {
            let mut snap = st.snapshot.write().unwrap();
            snap.status = status;
            snap.logs = logs;
            build_dashboard(&snap)
        };
        let _ = app.emit("status-updated", payload);
        Ok("已清空 SIM 卡与模块中的原始短信".to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn check_4g_route() -> Result<String, String> {
    tokio::task::spawn_blocking(network::check_4g_route)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_adapter_enabled(enable: bool) -> Result<String, String> {
    tokio::task::spawn_blocking(move || network::set_adapter_enabled(enable))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn list_adapter_priorities() -> Result<Vec<AdapterPriorityItem>, String> {
    tokio::task::spawn_blocking(network::list_adapter_priorities)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn apply_adapter_priorities(if_indices: Vec<u32>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || network::apply_adapter_priorities(if_indices))
        .await
        .map_err(|e| e.to_string())?
}

fn spawn_background_worker(state: AppState, app: AppHandle) {
    std::thread::spawn(move || loop {
        poll_modem_once(&state, &app);

        let net = state.traffic.lock().unwrap().poll();
        let payload = {
            let mut snap = state.snapshot.write().unwrap();
            snap.network = net;
            build_dashboard(&snap)
        };
        let _ = app.emit("status-updated", payload);

        std::thread::sleep(Duration::from_secs(3));
    });
}

#[tauri::command]
fn open_main_window(app: AppHandle) {
    show_main_window(&app);
}

#[tauri::command]
fn open_sms_window(app: AppHandle) {
    open_sms_page(&app);
}

fn show_main_window(app: &AppHandle) {
    if let Some(popup) = app.get_webview_window("tray-popup") {
        let _ = popup.hide();
    }
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_always_on_top(true);
        let _ = win.set_focus();
        let _ = win.set_always_on_top(false);
    }
}

fn open_sms_page(app: &AppHandle) {
    show_main_window(app);
    let _ = app.emit("navigate-sms", ());
}

const TRAY_POPUP_W: f64 = 380.0;
const TRAY_POPUP_H: f64 = 112.0;

fn ensure_tray_popup(app: &AppHandle) -> Option<WebviewWindow> {
    if let Some(win) = app.get_webview_window("tray-popup") {
        return Some(win);
    }
    WebviewWindowBuilder::new(app, "tray-popup", WebviewUrl::App("index.html?view=tray".into()))
        .title("EC20 Hub 状态")
        .inner_size(TRAY_POPUP_W, TRAY_POPUP_H)
        .decorations(false)
        .resizable(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .visible(false)
        .build()
        .ok()
}

fn toggle_tray_popup(app: &AppHandle) {
    let Some(win) = ensure_tray_popup(app) else {
        return;
    };
    if win.is_visible().unwrap_or(false) {
        let _ = win.hide();
        return;
    }
    if let Ok(Some(monitor)) = app.primary_monitor() {
        let work = monitor.work_area();
        let scale = monitor.scale_factor();
        let phys_w = (TRAY_POPUP_W * scale).round() as i32;
        let phys_h = (TRAY_POPUP_H * scale).round() as i32;
        let margin = (12.0 * scale).round() as i32;
        let x = work.position.x + work.size.width as i32 - phys_w - margin;
        let y = work.position.y + work.size.height as i32 - phys_h - margin;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
    if let Some(state) = app.try_state::<AppState>() {
        let payload = {
            let snap = state.snapshot.read().unwrap();
            build_dashboard(&snap)
        };
        let _ = win.emit("status-updated", payload);
    }
    let _ = win.show();
    let _ = win.set_focus();
}

fn setup_tray(app: &AppHandle, state: AppState) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let sync_item = MenuItem::with_id(app, "sync", "立即同步短信", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &sync_item, &quit_item])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("EC20 Hub · 后台运行中")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app_handle, event| match event.id.as_ref() {
            "show" => show_main_window(app_handle),
            "sync" => {
                let st = state.clone();
                let ah = app_handle.clone();
                std::thread::spawn(move || {
                    poll_modem_once(&st, &ah);
                    let payload = {
                        let snap = st.snapshot.read().unwrap();
                        build_dashboard(&snap)
                    };
                    let _ = ah.emit("status-updated", payload);
                });
            }
            "quit" => {
                app_handle.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_tray_popup(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("ec20-hub"));
            #[cfg(windows)]
            register_windows_toast_aumid(&data_dir);
            let store = Store::new(data_dir);
            let settings = store.load_settings();
            let messages = store.load_messages();
            let traffic = TrafficTracker::new();
            let last_net = NetworkStats {
                adapter_name: "检测中...".to_string(),
                ..Default::default()
            };

            let start_hidden = settings.start_minimized
                || std::env::args().any(|a| a == "--autostart");

            let shared = AppState {
                snapshot: Arc::new(RwLock::new(SharedSnapshot {
                    status: ModemStatus::default(),
                    network: last_net,
                    settings,
                    messages,
                    logs: Vec::new(),
                })),
                modem: Arc::new(Mutex::new(None)),
                store: Arc::new(Mutex::new(store)),
                traffic: Arc::new(Mutex::new(traffic)),
            };

            app.manage(shared.clone());
            if let Some(win) = app.get_webview_window("main") {
                if let Some(icon) = app.default_window_icon().cloned() {
                    let _ = win.set_icon(icon);
                }
                if start_hidden {
                    let _ = win.hide();
                }
            }
            let _ = ensure_tray_popup(app.handle());
            setup_tray(app.handle(), shared.clone())?;
            spawn_background_worker(shared, app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "tray-popup" => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_dashboard_state,
            open_main_window,
            open_sms_window,
            refresh_now,
            reboot_modem,
            update_settings,
            get_messages,
            mark_all_read,
            delete_message,
            clear_messages,
            send_sms,
            clear_modem_storage,
            check_4g_route,
            set_adapter_enabled,
            list_adapter_priorities,
            apply_adapter_priorities,
            get_autostart_enabled,
            set_autostart_enabled,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
