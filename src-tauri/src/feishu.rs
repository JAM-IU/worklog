use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

use crate::config;
use crate::records;

#[derive(Debug, Serialize)]
pub struct PushResult {
    pub ok: bool,
    pub reason: String,
    pub record_ids: String,
    pub digest_chars: usize,
    /// 附加说明（本地日报生成/知识库归档的结果，便于排查）
    #[serde(default)]
    pub note: String,
}

fn find_lark() -> Result<PathBuf, String> {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let cand = PathBuf::from(&appdata).join("npm").join("lark-cli.cmd");
        if cand.exists() {
            return Ok(cand);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let cand = dir.join("lark-cli.cmd");
            if cand.exists() {
                return Ok(cand);
            }
        }
    }
    Err("未找到 lark-cli，请先安装并登录（npm i -g @larksuite/cli && lark-cli auth login）".into())
}

fn run_lark(args: &[&str]) -> Result<Value, String> {
    run_lark_json(args)
}

/// 对外共享：调用 lark-cli 并返回 JSON envelope（meetings 等模块复用）
pub fn run_lark_json(args: &[&str]) -> Result<Value, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let lark = find_lark()?;
    let out = Command::new(lark)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW) // 抑制子进程的 CMD 黑窗闪现
        .output()
        .map_err(|e| format!("启动 lark-cli 失败: {e}"))?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let start = text.find('{').ok_or_else(|| format!("lark-cli 无 JSON 输出: {}", &text[..text.len().min(300)]))?;
    let end = text.rfind('}').unwrap_or(start);
    let v: Value = serde_json::from_str(&text[start..=end]).map_err(|e| format!("解析 lark-cli 输出失败: {e}"))?;
    if v.get("ok").and_then(|o| o.as_bool()) != Some(true) {
        let msg = v["error"]["message"].as_str().unwrap_or("未知错误");
        return Err(format!("lark-cli 失败: {msg}"));
    }
    Ok(v)
}

fn check_posted(date: &str) -> Result<bool, String> {
    let filter = json!({
        "logic": "and",
        "conditions": [
            ["日期", "==", format!("ExactDate({date} 00:00:00)")],
            ["数据来源", "contains", "worklog"]
        ]
    })
    .to_string();
    let s = config::load_settings();
    let v = run_lark(&[
        "base", "+record-list", "--as", "user",
        "--base-token", &s.feishu_base_token,
        "--table-id", &s.feishu_table_id,
        "--format", "json",
        "--filter-json", &filter,
    ])?;
    for key in ["records_count", "total"] {
        if let Some(n) = v["data"][key].as_u64() {
            if n > 0 {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

pub fn push(date: &str, force: bool) -> PushResult {
    let s = config::load_settings();
    if !s.feishu_enabled {
        return PushResult { ok: false, reason: "feishu_enabled=false".into(), record_ids: String::new(), digest_chars: 0, note: String::new() };
    }
    let from = config::report_window_start_ts();
    let digest = records::build_digest_window(&from);
    if digest.is_empty() {
        return PushResult { ok: false, reason: "no_work_records".into(), record_ids: String::new(), digest_chars: 0, note: String::new() };
    }
    // 把日报模板与历史样本一并推送，飞书侧 AI 生成日报时按此口径产出
    let ctx = crate::knowledge::report_context(&s, 1200);
    let mut payload_text = if ctx.trim().is_empty() { digest.clone() } else { format!("{digest}{ctx}") };
    // 会议记录一并随推送写入（供周报/复盘使用）
    if s.meeting_auto_fetch {
        let mt = crate::meetings::fetch_meeting_text(&s, date);
        if !mt.trim().is_empty() {
            payload_text.push_str("\n\n");
            payload_text.push_str(&mt);
        }
    }
    if !force {
        match check_posted(date) {
            Ok(true) => {
                return PushResult { ok: false, reason: "already_posted".into(), record_ids: String::new(), digest_chars: digest.chars().count(), note: String::new() };
            }
            Ok(false) => {}
            Err(e) => {
                return PushResult { ok: false, reason: e, record_ids: String::new(), digest_chars: digest.chars().count(), note: String::new() };
            }
        }
    }
    // 本地生成日报（带个人模板 + 知识沉淀样本），飞书侧直接归档展示
    let mut report = String::new();
    let mut note = String::new();
    if s.local_report_enabled {
        match crate::analyze::preview_report(date) {
            Ok(r) => report = r,
            Err(e) => {
                // 本地生成失败不阻断推送：带回原始输入与模板，由飞书侧 AI 兜底生成
                config::log_line(&format!("本地日报生成失败：{e}"));
                note = format!("local_report_failed: {e}");
            }
        }
    }

    let mut record = serde_json::Map::new();
    record.insert("日报标题".into(), json!(format!("{date}-{}", if s.feishu_user_name.is_empty() { "未命名" } else { &s.feishu_user_name })));
    record.insert("日期".into(), json!(date));
    record.insert("部门".into(), json!([s.feishu_dept]));
    record.insert("原始输入".into(), json!(payload_text));
    record.insert("数据来源".into(), json!(["worklog"]));
    if !report.trim().is_empty() {
        record.insert("AI生成日报".into(), json!(report));
        record.insert("发布状态".into(), json!(["已发布"]));
        record.insert("生成耗时_秒".into(), json!(60));
    }
    if s.feishu_user_open_id.starts_with("ou_") && !s.feishu_user_name.is_empty() {
        record.insert("姓名".into(), json!([{"id": s.feishu_user_open_id, "name": s.feishu_user_name}]));
    }
    let payload = json!({ "create_records": [record] });
    let tmp = std::env::temp_dir().join(format!("worklog_push_{}.json", chrono::Local::now().format("%H%M%S%3f")));
    if let Err(e) = std::fs::write(&tmp, payload.to_string()) {
        return PushResult { ok: false, reason: format!("写临时文件失败: {e}"), record_ids: String::new(), digest_chars: 0, note: String::new() };
    }
    let arg = format!("@{}", tmp.display());
    let result = run_lark(&[
        "base", "+record-batch-create", "--as", "user",
        "--base-token", &s.feishu_base_token,
        "--table-id", &s.feishu_table_id,
        "--json", &arg,
    ]);
    let _ = std::fs::remove_file(&tmp);
    match result {
        Ok(v) => {
            // 记录本次推送时刻（跨日窗口去重的锚点）
            config::mark_pushed(&chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string());
            // 归档到飞书知识库文档（每天一段，长期沉淀）
            if s.knowledge_archive_enabled && !s.knowledge_doc_token.trim().is_empty() && !report.trim().is_empty() {
                match archive_to_doc(&s, date, &report, &digest) {
                    Ok(_) => {
                        config::log_line(&format!("知识库归档成功：{date}"));
                        if !note.is_empty() { note.push_str(" | "); }
                        note.push_str("archived");
                    }
                    Err(e) => {
                        config::log_line(&format!("知识库归档失败：{e}"));
                        if !note.is_empty() { note.push_str(" | "); }
                        note.push_str(&format!("archive_failed: {e}"));
                    }
                }
            }
            PushResult {
                ok: true,
                reason: String::new(),
                record_ids: v["data"]["record_id_list"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(","))
                    .unwrap_or_default(),
                digest_chars: payload_text.chars().count(),
                note,
            }
        }
        Err(e) => PushResult { ok: false, reason: e, record_ids: String::new(), digest_chars: 0, note },
    }
}

/// 把当天日报追加到飞书知识库文档（docs +update append）
fn archive_to_doc(s: &config::Settings, date: &str, report: &str, digest: &str) -> Result<(), String> {
    let mut content = String::new();
    content.push_str(&format!("## 📋 {date} 日报\n\n{report}\n\n"));
    content.push_str("<callout emoji=\"📌\">\n**原始工作记录**（WorkLog 自动采集）\n</callout>\n\n");
    // 折叠原始记录：只保留前 30 行，避免文档过长
    let brief: String = digest.lines().take(30).collect::<Vec<_>>().join("\n");
    content.push_str(&brief);
    content.push_str("\n\n---\n\n");

    let tmp = std::env::temp_dir().join(format!("worklog_doc_{}.md", chrono::Local::now().format("%H%M%S%3f")));
    std::fs::write(&tmp, content).map_err(|e| e.to_string())?;
    let arg = format!("@{}", tmp.display());
    let res = run_lark(&[
        "docs", "+update", "--as", "user",
        "--doc", s.knowledge_doc_token.trim(),
        "--command", "append",
        "--doc-format", "markdown",
        "--content", &arg,
    ]);
    let _ = std::fs::remove_file(&tmp);
    res.map(|_| ())
}
