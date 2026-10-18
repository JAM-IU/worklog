use crate::config;

/// 会议链接/纪要 → 会议文本（供日报生成合并使用）
/// 优先级：VC 自动抓取（当天参与过的会议纪要）→ 手动链接 → 手动纪要
pub fn fetch_meeting_text(s: &config::Settings, date: &str) -> String {
    let mut parts: Vec<String> = Vec::new();

    // 0) VC 自动抓取当天会议纪要
    if s.meeting_auto_vc {
        let vc_text = fetch_day_vc_meetings(s, date);
        if !vc_text.trim().is_empty() {
            parts.push(vc_text);
        }
    }

    // 1) 会议链接自动拉取（手动配置的妙记/纪要链接）
    for line in s.meeting_links.lines() {
        let link = line.trim();
        if link.is_empty() {
            continue;
        }
        match fetch_one(link) {
            Ok(text) => {
                if !text.trim().is_empty() {
                    parts.push(text);
                }
            }
            Err(e) => {
                config::log_line(&format!("会议拉取失败（{link}）：{e}"));
            }
        }
    }

    // 2) 手动粘贴的会议纪要
    if !s.meeting_notes.trim().is_empty() {
        parts.push(format!("【手动会议纪要】\n{}", s.meeting_notes.trim()));
    }

    if parts.is_empty() {
        return String::new();
    }
    let mut out = String::from("【今日会议记录】\n");
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n");
        }
        out.push_str(p);
    }
    let cap: String = out.chars().take(4000).collect();
    cap
}

/// 通过 VC 会议自动抓取当天纪要：vc+search → vc+detail(note_id) → note+detail(doc_token) → docs+fetch
fn fetch_day_vc_meetings(s: &config::Settings, date: &str) -> String {
    let open_id = s.feishu_user_open_id.trim();
    if !open_id.starts_with("ou_") {
        return String::new();
    }
    let start = format!("{date}T00:00+08:00");
    let end = format!("{date}T23:59+08:00");
    let Ok(search) = crate::feishu::run_lark_json(&[
        "vc", "+search", "--as", "user",
        "--participant-ids", open_id,
        "--start", &start, "--end", &end,
        "--format", "json",
    ]) else {
        return String::new();
    };
    let items = search["data"]["items"].as_array().cloned().unwrap_or_default();
    if items.is_empty() {
        return String::new();
    }

    let mut blocks: Vec<String> = Vec::new();
    for it in items.iter().take(6) {
        let Some(mid) = it["id"].as_str() else { continue };
        let title: String = it["display_info"]
            .as_str()
            .unwrap_or("")
            .lines()
            .next()
            .unwrap_or("会议")
            .to_string();
        // 会议详情 → note_id
        let Ok(detail) = crate::feishu::run_lark_json(&[
            "vc", "+detail", "--as", "user", "--meeting-ids", mid, "--format", "json",
        ]) else { continue };
        let note_id = detail["data"]["meetings"][0]["note_id"].as_str().unwrap_or("").to_string();
        if note_id.is_empty() {
            continue;
        }
        // note 详情 → 纪要文档 token
        let Ok(note) = crate::feishu::run_lark_json(&[
            "note", "+detail", "--as", "user", "--note-id", &note_id, "--format", "json",
        ]) else { continue };
        let doc = note["data"]["note"]["note_doc_token"].as_str().unwrap_or("").to_string();
        if doc.is_empty() {
            continue;
        }
        // 读纪要文档正文
        let Ok(docv) = crate::feishu::run_lark_json(&[
            "docs", "+fetch", "--as", "user", "--doc", &doc, "--doc-format", "markdown",
        ]) else { continue };
        let content = docv["data"]["document"]["content"].as_str().unwrap_or("");
        if content.trim().is_empty() {
            continue;
        }
        let summary = extract_summary(content);
        if !summary.trim().is_empty() {
            blocks.push(format!("【会议纪要】{title}\n{summary}"));
        }
    }
    if blocks.is_empty() {
        return String::new();
    }
    config::log_line(&format!("VC 自动抓取会议纪要：{date}，{} 场", blocks.len()));
    blocks.join("\n\n")
}

/// 从纪要文档中提取「总结」段（控制长度，避免提示词过长）
fn extract_summary(content: &str) -> String {
    let start = content.find("# 总结").or_else(|| content.find("## 总结"));
    let slice = match start {
        Some(st) => &content[st..],
        None => content,
    };
    let end = slice
        .find("# 待办")
        .map(|i| (i + slice[i..].find('\n').unwrap_or(0)).min(slice.len()))
        .unwrap_or_else(|| slice.len().min(2000));
    let body: String = slice[..end].chars().take(1600).collect();
    body
}

fn fetch_one(link: &str) -> Result<String, String> {
    if link.contains("/minutes/") {
        let token = link
            .split("/minutes/")
            .nth(1)
            .unwrap_or("")
            .split(['?', '&', '/'])
            .next()
            .unwrap_or("")
            .to_string();
        if token.is_empty() {
            return Err("无法从链接提取妙记 token".into());
        }
        fetch_minutes(&token)
    } else {
        fetch_doc(link)
    }
}

fn fetch_minutes(token: &str) -> Result<String, String> {
    let v = crate::feishu::run_lark_json(&[
        "minutes", "+detail", "--as", "user",
        "--minute-tokens", token,
        "--summary", "--todo", "--format", "json",
    ])?;
    // 摘要与待办字段（不同版本字段名略有差异，做宽容解析）
    let mut out = String::new();
    if let Some(s) = json_str(&v, "summary") {
        out.push_str(&s);
    }
    if let Some(s) = json_str(&v, "ai_summary") {
        out.push_str(&s);
    }
    if let Some(todos) = json_str(&v, "todos") {
        if !todos.trim().is_empty() {
            out.push_str("\n【待办】\n");
            out.push_str(&todos);
        }
    }
    if out.trim().is_empty() {
        return Err("妙记无摘要内容".into());
    }
    Ok(format!("【妙记】{}", out.trim()))
}

fn fetch_doc(link: &str) -> Result<String, String> {
    let v = crate::feishu::run_lark_json(&[
        "docs", "+fetch", "--as", "user",
        "--doc", link,
        "--doc-format", "markdown",
    ])?;
    let content = v["data"]["document"]["content"].as_str().unwrap_or("");
    if content.trim().is_empty() {
        return Err("文档无内容".into());
    }
    // 纪要文档往往很长：取标题 + 摘要 + 待办部分，控制体量
    let keep: String = content.chars().take(3500).collect();
    Ok(keep)
}

fn json_str(v: &serde_json::Value, key: &str) -> Option<String> {
    let walk = |node: &serde_json::Value| -> Option<String> {
        if let Some(obj) = node.as_object() {
            for (k, val) in obj {
                if k.eq_ignore_ascii_case(key) || k.contains(key) {
                    if let Some(s) = val.as_str() {
                        return Some(s.to_string());
                    }
                    if let Some(arr) = val.as_array() {
                        let items: Vec<String> = arr.iter().filter_map(|x| {
                            x.as_str().map(String::from).or_else(|| {
                                x.get("content").and_then(|c| c.as_str()).map(String::from)
                            })
                        }).collect();
                        if !items.is_empty() {
                            return Some(items.join("\n"));
                        }
                    }
                }
            }
        }
        None
    };
    walk(v)
        .or_else(|| v.get("data").and_then(walk))
        .or_else(|| v.get("data").and_then(|d| d.get("summary")).and_then(|n| n.as_str()).map(String::from))
}
