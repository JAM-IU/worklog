use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config;
use crate::records;

/// dailylog 数据目录（可用环境变量 DAILYLOG_DATA 覆盖）
pub fn dailylog_data_dir() -> PathBuf {
    std::env::var("DAILYLOG_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("D:\\dailylog\\data"))
}

/// 导入 dailylog 最近 N 天的原始记录（分类规范化 + 按时间戳去重）
pub fn import_dailylog_records(days: u32) -> Result<String, String> {
    let src = dailylog_data_dir().join("records").join("raw");
    if !src.exists() {
        return Err(format!("未找到 dailylog 数据目录: {}", src.display()));
    }
    let today = chrono::Local::now().date_naive();
    let mut imported_total = 0usize;
    let mut skipped_total = 0usize;
    let mut files_done = Vec::new();

    for entry in std::fs::read_dir(&src).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
        let Ok(date) = chrono::NaiveDate::parse_from_str(stem, "%Y-%m-%d") else { continue };
        let age = (today - date).num_days();
        if age < 0 || age > days as i64 {
            continue;
        }
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let existing = records::read_day(stem);
        let mut seen: std::collections::HashSet<String> = existing.iter().map(|r| r.ts.clone()).collect();
        let mut list = existing;
        let mut added = 0usize;
        let mut skipped = 0usize;

        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
            let ts = v["ts"].as_str().unwrap_or("").to_string();
            if ts.is_empty() || seen.contains(&ts) {
                skipped += 1;
                continue;
            }
            let rec = records::Record {
                ts: ts.clone(),
                activity: records::normalize_activity(v["activity"].as_str().unwrap_or("other")),
                summary: v["summary"].as_str().unwrap_or("").to_string(),
                detail: v["detail"].as_str().unwrap_or("").to_string(),
                progress: v["progress"].as_str().unwrap_or("").to_string(),
                todo: v["todo"].as_str().unwrap_or("").to_string(),
                apps: v["apps"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                    .unwrap_or_default(),
                contains_sensitive: v["contains_sensitive"].as_bool().unwrap_or(false),
                reportable: v["reportable"].as_bool().unwrap_or(true),
            };
            seen.insert(ts);
            list.push(rec);
            added += 1;
        }
        if added > 0 {
            list.sort_by(|a, b| a.ts.cmp(&b.ts));
            let body: String = list
                .iter()
                .map(|r| format!("{}\n", serde_json::to_string(r).unwrap()))
                .collect();
            std::fs::write(records::raw_file(stem), body).map_err(|e| e.to_string())?;
            files_done.push(format!("{stem}(+{added})"));
        }
        imported_total += added;
        skipped_total += skipped;
    }

    if imported_total == 0 && files_done.is_empty() {
        return Ok(format!("最近 {days} 天内没有可导入的 dailylog 记录（或已全部导入）"));
    }
    Ok(format!(
        "已导入 {imported_total} 条记录（跳过重复 {skipped_total} 条）；涉及日期：{}",
        files_done.join("、")
    ))
}

/// 把 dailylog 的历史日报/周报导入为参考样本（知识沉淀）
pub fn import_dailylog_reports() -> Result<String, String> {
    let dir = dailylog_data_dir().join("reports");
    if !dir.exists() {
        return Err(format!("未找到 dailylog 报告目录: {}", dir.display()));
    }
    let mut s = config::load_settings();
    let mut added = Vec::new();
    let mut text = s.reference_samples.clone();

    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok().map(|x| x.path()))
        .filter(|p| p.extension().map(|e| e == "md").unwrap_or(false))
        .collect();
    entries.sort();

    for p in entries {
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("report.md").to_string();
        if text.contains(&format!("<!-- {name} -->")) {
            continue; // 已导入过
        }
        let Some(body) = read_report(&p) else { continue };
        text.push_str(&format!("\n\n<!-- {name} -->\n{body}"));
        added.push(name);
    }

    if added.is_empty() {
        return Ok("历史报告已全部导入过，无新增".to_string());
    }
    s.reference_samples = text;
    config::save_settings(&s).map_err(|e| e.to_string())?;
    Ok(format!("已导入 {} 份报告样本：{}", added.len(), added.join("、")))
}

/// 保存用户手工修订后的报告：覆盖当前报告，并把修订版放到参考样本最前面。
/// 最新反馈优先进入 report_context 的截断窗口，后续生成会优先模仿用户真正确认过的写法。
pub fn save_report_feedback(date: &str, body: &str) -> Result<String, String> {
    let text = body.trim();
    if text.is_empty() {
        return Err("修订内容不能为空".to_string());
    }
    config::save_report(date, text);
    let mut s = config::load_settings();
    let marker = format!("<!-- 用户修订报告-{date} -->");
    let mut old = s.reference_samples.clone();
    if let Some(start) = old.find(&marker) {
        let end = old[start + marker.len()..]
            .find("<!-- ")
            .map(|i| start + marker.len() + i)
            .unwrap_or(old.len());
        old.replace_range(start..end, "");
    }
    let sample = format!(
        "{marker}\n# 日报 {date}\n\n{text}\n\n"
    );
    // 反馈样本放在最前面；限制总长度，避免历史样本无限增长。
    s.reference_samples = format!("{sample}{}", old.trim_start())
        .chars()
        .take(30000)
        .collect();
    config::save_settings(&s).map_err(|e| e.to_string())?;
    Ok("已保存修订版，并加入个人写作样本".to_string())
}

fn read_report(p: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(p).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 控制单份样本长度，避免提示词过长
    Some(trimmed.chars().take(2500).collect())
}

/// 报告参考上下文（模板 + 历史样本），供 WorkLog 预览与飞书推送共用
pub fn report_context(s: &config::Settings, max_ref_chars: usize) -> String {
    let mut out = String::new();
    if !s.report_template.trim().is_empty() {
        out.push_str("\n\n【日报模板（必须遵循此格式）】\n");
        out.push_str(s.report_template.trim());
    }
    if !s.reference_samples.trim().is_empty() {
        out.push_str("\n\n【个人已确认的日报/周报样本（优先学习语气、取舍和颗粒度，不要照抄内容）】\n");
        let refs: String = s.reference_samples.trim().chars().take(max_ref_chars).collect();
        out.push_str(&refs);
    }
    out
}
