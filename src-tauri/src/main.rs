#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod analyze;
mod capture;
mod config;
mod feishu;
mod knowledge;
mod meetings;
mod records;
mod stats;

use chrono::Datelike;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};

use capture::CaptureState;

/// 自动推送去重标记（当天已推送的日期）
static LAST_AUTO_PUSH: Mutex<Option<String>> = Mutex::new(None);

// ---------- CLI 模式：--push 无窗口推送 / --import-dailylog 迁移历史 ----------
fn cli_main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--weekly-report") {
        let week = args
            .iter()
            .position(|a| a == "--week")
            .and_then(|i| args.get(i + 1).cloned())
            .unwrap_or_else(|| {
                let d = chrono::Local::now().date_naive();
                // 工作周期 = 周三 ~ 周日，周报按所属周期的周三取整
                config::week_start_wednesday(d).format("%Y-%m-%d").to_string()
            });
        match analyze::generate_weekly_report(&week) {
            Ok(text) => println!("{text}"),
            Err(e) => println!("ERROR: {e}"),
        }
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--import-dailylog") {
        let days: u32 = args
            .iter()
            .position(|a| a == "--days")
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(14);
        let mut out = Vec::new();
        match knowledge::import_dailylog_records(days) {
            Ok(msg) => out.push(msg),
            Err(e) => out.push(format!("记录导入失败: {e}")),
        }
        match knowledge::import_dailylog_reports() {
            Ok(msg) => out.push(msg),
            Err(e) => out.push(format!("报告样本导入失败: {e}")),
        }
        println!("{}", out.join("\n"));
        std::process::exit(0);
    }
    if !args.iter().any(|a| a == "--push") {
        return Ok(());
    }
    let force = args.iter().any(|a| a == "--force");
    let date = args
        .iter()
        .position(|a| a == "--date")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(records::today_str);
    let result = feishu::push(&date, force);
    println!("{}", serde_json::to_string_pretty(&result).unwrap());
    std::process::exit(if result.ok { 0 } else { 1 });
}

// ---------- Tauri commands ----------
#[tauri::command]
fn get_settings() -> config::Settings {
    config::load_settings()
}

#[tauri::command]
fn save_settings(settings: config::Settings) -> Result<(), String> {
    config::save_settings(&settings).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_today_records() -> Vec<records::Record> {
    let mut list = records::read_day(&records::today_str());
    list.reverse();
    list
}

#[tauri::command]
async fn push_now(force: bool) -> feishu::PushResult {
    tauri::async_runtime::spawn_blocking(move || feishu::push(&records::today_str(), force))
        .await
        .unwrap_or_else(|e| feishu::PushResult {
            ok: false,
            reason: format!("推送任务异常退出: {e}"),
            record_ids: String::new(),
            digest_chars: 0,
            note: String::new(),
        })
}

#[tauri::command]
async fn preview_report() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| analyze::preview_report(&records::today_str()))
        .await
        .map_err(|e| format!("日报任务异常退出: {e}"))?
}

#[tauri::command]
fn set_recording(state: tauri::State<Arc<CaptureState>>, enabled: bool) {
    state.set_running(enabled);
}

#[tauri::command]
fn get_recording(state: tauri::State<Arc<CaptureState>>) -> bool {
    state.is_running()
}

#[tauri::command]
async fn import_dailylog(days: u32) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = Vec::new();
        out.push(knowledge::import_dailylog_records(days)?);
        match knowledge::import_dailylog_reports() {
            Ok(m) => out.push(m),
            Err(e) => out.push(format!("（报告样本：{e}）")),
        }
        Ok(out.join("\n"))
    })
    .await
    .map_err(|e| format!("导入任务异常退出: {e}"))?
}

#[tauri::command]
async fn import_dailylog_reports() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(knowledge::import_dailylog_reports)
        .await
        .map_err(|e| format!("导入任务异常退出: {e}"))?
}

#[tauri::command]
fn reference_stats() -> serde_json::Value {
    let s = config::load_settings();
    serde_json::json!({
        "reference_chars": s.reference_samples.chars().count(),
        "template_chars": s.report_template.chars().count(),
        "samples": s.reference_samples.matches("<!--").count(),
    })
}

#[tauri::command]
async fn fetch_meetings() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let s = config::load_settings();
        let text = meetings::fetch_meeting_text(&s, &records::today_str());
        if text.trim().is_empty() {
            Ok("今天没有配置会议链接，或链接暂时无法读取。请在设置页粘贴妙记/纪要链接。".into())
        } else {
            Ok(text)
        }
    })
    .await
    .map_err(|e| format!("会议读取任务异常退出: {e}"))?
}

#[tauri::command]
async fn get_report(date: String, regenerate: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !regenerate {
            if let Some(cached) = config::load_report(&date) {
                return Ok(cached);
            }
        }
        analyze::generate_report_for(&date)
    })
    .await
    .map_err(|e| format!("日报任务异常退出: {e}"))?
}

#[tauri::command]
async fn save_report_feedback(date: String, body: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || knowledge::save_report_feedback(&date, &body))
        .await
        .map_err(|e| format!("保存修订任务异常退出: {e}"))?
}

#[tauri::command]
fn stats_overview() -> serde_json::Value {
    stats::overview()
}

#[tauri::command]
fn day_list() -> Vec<serde_json::Value> {
    stats::day_list()
}

#[tauri::command]
fn day_records(date: String) -> Vec<records::Record> {
    let mut list = records::read_day(&date);
    list.reverse();
    list
}

#[tauri::command]
fn report_dates() -> Vec<String> {
    records::list_record_dates()
}

#[tauri::command]
async fn get_weekly_report(week_start: String, regenerate: bool) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        if !regenerate {
            if let Some(cached) = config::load_report(&format!("week-{week_start}")) {
                return Ok(cached);
            }
        }
        analyze::generate_weekly_report(&week_start)
    })
    .await
    .map_err(|e| format!("周报任务异常退出: {e}"))?
}

#[tauri::command]
fn week_list() -> Vec<serde_json::Value> {
    let mut weeks: Vec<String> = records::list_record_dates()
        .iter()
        .filter_map(|d| {
            chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok().map(|nd| {
                // 工作周期 = 周三 ~ 周日：按所属周期的周三做锚点分组
                config::week_start_wednesday(nd)
                    .format("%Y-%m-%d")
                    .to_string()
            })
        })
        .collect();
    weeks.sort_by(|a, b| b.cmp(a));
    weeks.dedup();
    weeks
        .into_iter()
        .map(|ws| {
            let d = chrono::NaiveDate::parse_from_str(&ws, "%Y-%m-%d").unwrap();
            let we = d + chrono::Duration::days(6);
            serde_json::json!({
                "start": ws,
                "end": we.format("%Y-%m-%d").to_string(),
                "label": format!("{} ~ {}", ws, we.format("%Y-%m-%d")),
            })
        })
        .collect()
}

// ---------- 自动推送定时器（应用内，无需依赖任务计划） ----------
fn start_push_scheduler(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(30));
        let s = config::load_settings();
        if !(s.feishu_enabled && s.feishu_auto_push_enabled) {
            continue;
        }
        let now = chrono::Local::now();
        let today = now.format("%Y-%m-%d").to_string();
        // 只在配置的「日报推送日」触发（1=周一 … 7=周日；默认周三~周日，周一周二休息）
        let weekday = now.weekday().number_from_monday();
        if !s.daily_push_weekdays.contains(&weekday) {
            continue;
        }
        let due = now.format("%H:%M").to_string()
            >= format!("{:02}:{:02}", s.feishu_push_hour, s.feishu_push_minute);
        if !due {
            continue;
        }
        {
            let last = LAST_AUTO_PUSH.lock().unwrap();
            if last.as_deref() == Some(today.as_str()) {
                continue;
            }
        }
        let result = feishu::push(&today, false);
        if result.ok || result.reason == "already_posted" {
            *LAST_AUTO_PUSH.lock().unwrap() = Some(today.clone());
        }
        let _ = app.emit("push-result", serde_json::to_value(&result).unwrap_or_default());
        let _ = app.emit("record-updated", ());
    });
}

// ---------- 主入口 ----------
fn main() {
    let _ = cli_main();

    let state = Arc::new(CaptureState::new());

    tauri::Builder::default()
        .manage(state.clone())
        .setup(move |app| {
            setup_tray(app.handle())?;
            let app_handle = app.handle().clone();
            capture::start_loop(app_handle.clone(), state.clone());
            start_push_scheduler(app_handle);
            // 启动时按设置恢复记录状态
            let s = config::load_settings();
            state.set_running(s.recording_enabled);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            get_today_records,
            push_now,
            preview_report,
            set_recording,
            get_recording,
            import_dailylog,
            import_dailylog_reports,
            reference_stats,
            fetch_meetings,
            get_report,
            save_report_feedback,
            report_dates,
            get_weekly_report,
            week_list,
            stats_overview,
            day_list,
            day_records
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("WorkLog 启动失败");
}

fn setup_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
    let toggle_i = MenuItem::with_id(app, "toggle", "开始/停止记录", true, None::<&str>)?;
    let push_i = MenuItem::with_id(app, "push", "推送到飞书", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出程序", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &toggle_i, &push_i, &quit_i])?;

    TrayIconBuilder::with_id("worklog-tray")
        .icon(tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?)
        .tooltip("WorkLog · 工作汇报采集")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "toggle" => {
                let state = app.state::<Arc<CaptureState>>();
                state.set_running(!state.is_running());
                let _ = app.emit("record-state-changed", ());
            }
            "push" => {
                let handle = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let result = feishu::push(&records::today_str(), false);
                    let _ = handle.emit("push-result", serde_json::to_value(&result).unwrap_or_default());
                });
                show_main(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.unminimize();
    }
}
