use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::{config, records};

/// 启动飞书消息监听：把「自己」在飞书群里/私聊里发的文本消息
/// 自动记录为「答疑支持」事件入库，日报/周报自动统计答疑量。
///
/// 实现：常驻一个 lark-cli event consume im.message.receive_v1 --as bot 子进程，
/// 逐行读 NDJSON 事件，过滤 sender == 本人，按会话 + 时间窗口合并后落库。
pub fn start_im_listener(app: AppHandle) {
    std::thread::spawn(move || {
        let mut child: Option<(Child, ChildStdin)> = None;
        let mut reader: Option<BufReader<std::process::ChildStdout>> = None;
        // chat_id -> (会话类型 私聊/群聊, 待合并消息)
        let mut pending: HashMap<String, (String, Vec<String>)> = HashMap::new();
        let mut last_flush: HashMap<String, Instant> = HashMap::new();
        let mut chat_names: HashMap<String, String> = HashMap::new();
        let mut names_at: Instant = Instant::now() - Duration::from_secs(3600);
        let mut settings: config::Settings = config::load_settings();
        let mut settings_at: Instant = Instant::now() - Duration::from_secs(3600);
        let mut failed_until = Instant::now();

        loop {
            // 监听关闭时：退出子进程，进入低频率轮询
            if !settings.im_listener_enabled {
                child = None;
                reader = None;
                pending.clear();
                last_flush.clear();
                std::thread::sleep(Duration::from_secs(30));
                settings = config::load_settings();
                continue;
            }

            // 需要（重新）拉起子进程
            if child.is_none() {
                if Instant::now() < failed_until {
                    std::thread::sleep(Duration::from_secs(30));
                    settings = config::load_settings();
                    continue;
                }
                match spawn_listener() {
                    Ok((mut c, stdin)) => {
                        let stdout = c.stdout.take();
                        child = Some((c, stdin));
                        reader = stdout.map(BufReader::new);
                        config::log_line("飞书消息监听已启动（bot 身份，自动记录本人回复）");
                    }
                    Err(e) => {
                        config::log_line(&format!("飞书消息监听启动失败：{e}"));
                        failed_until = Instant::now() + Duration::from_secs(300);
                        std::thread::sleep(Duration::from_secs(30));
                        settings = config::load_settings();
                        continue;
                    }
                }
            }

            // 阻塞读一行事件（EOF = 子进程退出，下一轮自动重启）
            let mut line = String::new();
            match reader.as_mut().and_then(|r| r.read_line(&mut line).ok()) {
                Some(0) | None => {
                    // 子进程退出：关掉 stdin 优雅退出，稍后重启
                    child = None;
                    reader = None;
                    config::log_line("飞书消息监听连接中断，60 秒后自动重启");
                    failed_until = Instant::now() + Duration::from_secs(60);
                    continue;
                }
                Some(_) => {}
            }

            // 每 60 秒刷新一次设置
            if settings_at.elapsed() > Duration::from_secs(60) {
                settings = config::load_settings();
                settings_at = Instant::now();
                if !settings.im_listener_enabled {
                    continue;
                }
            }

            if let Err(e) = handle_event(
                &app,
                &line,
                &settings,
                &mut pending,
                &mut last_flush,
                &mut chat_names,
                &mut names_at,
            ) {
                config::log_line(&format!("IM 事件处理失败：{e}"));
            }
        }
    });
}

fn spawn_listener() -> Result<(Child, ChildStdin), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let lark = crate::feishu::find_lark()?;
    let mut cmd = Command::new(lark);
    cmd.args(["event", "consume", "im.message.receive_v1", "--as", "bot"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = cmd.spawn().map_err(|e| format!("spawn 失败: {e}"))?;
    let stdin = child.stdin.take().ok_or("无法取得子进程 stdin")?;
    Ok((child, stdin))
}

#[allow(clippy::too_many_arguments)]
fn handle_event(
    app: &AppHandle,
    line: &str,
    s: &config::Settings,
    pending: &mut HashMap<String, (String, Vec<String>)>,
    last_flush: &mut HashMap<String, Instant>,
    chat_names: &mut HashMap<String, String>,
    names_at: &mut Instant,
) -> Result<(), String> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).map_err(|e| format!("事件非 JSON: {e}"))?;
    // 只记录「自己」发的文本消息
    let sender = v["sender_id"].as_str().unwrap_or("");
    if sender.trim().is_empty() || sender != s.feishu_user_open_id.trim() {
        return Ok(());
    }
    if v["sender_type"].as_str() != Some("user") {
        return Ok(());
    }
    if v["message_type"].as_str() != Some("text") {
        return Ok(());
    }
    let chat_type = v["chat_type"].as_str().unwrap_or("");
    if chat_type != "group" && chat_type != "p2p" {
        return Ok(());
    }
    let chat_id = v["chat_id"].as_str().unwrap_or("").to_string();
    if chat_id.is_empty() {
        return Ok(());
    }
    let content = v["content"].as_str().unwrap_or("").trim().to_string();
    if content.chars().count() < 2 {
        return Ok(());
    }
    // 命中排除关键词（影视/游戏/个人项目等）则跳过
    let probe = records::Record {
        ts: String::new(),
        activity: "parent_comm".into(),
        event_type: "答疑支持".into(),
        summary: content.clone(),
        detail: String::new(),
        progress: String::new(),
        todo: String::new(),
        apps: vec!["飞书".into()],
        contains_sensitive: false,
        reportable: true,
    };
    if !records::is_reportable_with(&probe, s) {
        return Ok(());
    }

    // 会话名过滤（每行一个关键词，命中才记录）
    if !s.im_chat_filter.trim().is_empty() {
        let name = chat_name(chat_names, names_at, &chat_id);
        let hit = s
            .im_chat_filter
            .lines()
            .map(|l| l.trim().to_lowercase())
            .filter(|k| !k.is_empty())
            .any(|k| name.to_lowercase().contains(&k));
        if !hit {
            return Ok(());
        }
    }

    // 时间窗口合并：同一会话超过 merge_minutes 没消息，或积攒超过 20 条 → 先落一条
    let now = Instant::now();
    let merge = Duration::from_secs(s.im_merge_minutes.max(1) * 60);
    let kind = if chat_type == "p2p" { "私聊" } else { "群聊" };
    let due = last_flush
        .get(&chat_id)
        .map(|t| now.duration_since(*t) >= merge)
        .unwrap_or(true);
    let batch = &mut pending.entry(chat_id.clone()).or_insert_with(|| (kind.to_string(), Vec::new())).1;
    if due || batch.len() >= 20 {
        flush(pending, last_flush, &chat_id, chat_names, app);
    }
    pending
        .entry(chat_id.clone())
        .or_insert_with(|| (kind.to_string(), Vec::new()))
        .1
        .push(content);
    last_flush.insert(chat_id, now);

    // 顺带扫一遍其他会话：超时未活跃的批次也落库，避免长时间积压
    let stale: Vec<String> = last_flush
        .iter()
        .filter(|(_, t)| now.duration_since(**t) >= merge)
        .map(|(cid, _)| cid.clone())
        .collect();
    for cid in stale {
        flush(pending, last_flush, &cid, chat_names, app);
    }
    Ok(())
}

fn flush(
    pending: &mut HashMap<String, (String, Vec<String>)>,
    last_flush: &mut HashMap<String, Instant>,
    chat_id: &str,
    chat_names: &HashMap<String, String>,
    app: &AppHandle,
) {
    let Some((kind, msgs)) = pending.remove(chat_id) else {
        return;
    };
    if msgs.is_empty() {
        return;
    }
    last_flush.remove(chat_id);
    let n = msgs.len();
    let joined = msgs.join("；");
    let short: String = joined.chars().take(60).collect();
    let name = chat_names
        .get(chat_id)
        .cloned()
        .unwrap_or_else(|| "飞书会话".to_string());
    let rec = records::Record {
        ts: chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        activity: "parent_comm".into(),
        event_type: "答疑支持".into(),
        summary: format!("{kind}「{name}」回复消息：{short}"),
        detail: format!(
            "飞书消息监听自动记录：{kind} {n} 条回复——{}",
            joined.chars().take(200).collect::<String>()
        ),
        progress: String::new(),
        todo: String::new(),
        apps: vec!["飞书".into()],
        contains_sensitive: false,
        reportable: true,
    };
    let date = records::today_str();
    if records::append(&rec, &date).is_ok() {
        config::log_line(&format!("IM 答疑入库：{kind} {n} 条"));
        let _ = app.emit("record-updated", ());
    }
}

/// 会话名缓存：每 30 分钟拉一次会话列表（user 身份）
fn chat_name(cache: &mut HashMap<String, String>, at: &mut Instant, chat_id: &str) -> String {
    if cache.is_empty() || at.elapsed() > Duration::from_secs(1800) {
        if let Ok(v) = crate::feishu::run_lark_json(&[
            "im", "+chat-list", "--as", "user",
            "--types", "group,p2p",
            "--format", "json",
        ]) {
            if let Some(items) = v["data"]["items"].as_array() {
                for it in items {
                    let id = it["chat_id"]
                        .as_str()
                        .or_else(|| it["id"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let name = it["name"]
                        .as_str()
                        .or_else(|| it["title"].as_str())
                        .unwrap_or("")
                        .to_string();
                    if !id.is_empty() && !name.is_empty() {
                        cache.insert(id, name);
                    }
                }
            }
        }
        *at = Instant::now();
    }
    cache.get(chat_id).cloned().unwrap_or_default()
}
