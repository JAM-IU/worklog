use chrono::Local;
use serde_json::{json, Value};

use crate::config;
use crate::records::Record;

const ANALYZE_PROMPT: &str = r#"你是工作汇报采集助手（WorkLog）。分析这张屏幕截图，记录此刻正在进行的工作活动。

【核心原则】
1. 尽可能记录有日报价值的工作内容：工作主题、任务名称、项目方向、需求内容、技术问题、交付物、进展、待办。
2. 结合下方【业务工作方向】判断当前任务属于哪个业务方向，写入 detail 字段（如"AI工作汇报闭环-日报自动化""转化运营-T8.3批次复盘"）。
3. 保护隐私：不记录私人聊天细节、联系人身份、账号、密钥、完整链接等敏感信息。
4. 隐私保护不等于空泛描述：只脱敏"身份信息"和"敏感字段"，工作任务本身尽量保留细节。

【你应该记录什么】
1. 当前正在做的具体工作任务。
2. 当前正在阅读、编辑、核对、调试、整理、沟通的内容主题。
3. 任务所属工作方向（优先从下方【业务工作方向】里匹配）。
4. 能用于日报的具体进展、待办、交付物。
5. 若在聊天工具里处理工作，提取"工作事项"和"任务含义"，不复述原文。

【你不应该记录什么】
1. 桌面壁纸、系统状态栏、时间、电量、天气、Dock、任务栏等无关信息。
2. 联系人昵称、群名、备注名、头像文字、账号名。
3. 聊天消息逐字内容、私人聊天细节。
4. 手机号、邮箱、身份证、银行卡、地址、验证码、密码、Token、API Key、Cookie。
5. 客户个人身份、员工个人身份、具体薪酬绩效、敏感财务明细。

【沟通界面处理（微信/飞书/钉钉/Slack/邮件/私信/群聊等）】
允许输出：沟通的工作主题、任务方向、可识别的工作需求/待办/结论/进展、文件或链接的大致用途。
禁止输出：联系人是谁、群名、对方原话、逐字内容、完整链接、账号、手机号、邮箱、订单号、密钥。
私人闲聊 → 只输出"当前包含私人沟通内容，已脱敏，不纳入日报"。

【输出要求】
1. 只输出一个 JSON 对象，不要输出任何其他文字、注释或 markdown 代码块标记。
2. 截图无工作内容（纯私人界面、壁纸、无活动）时 summary 输出"无工作活动"。
3. 不要编造截图中不存在的内容。
4. 隐私与工作记录冲突时，优先保留"脱敏后的工作事项"。

{
  "activity": "conversion|insight|script|review|data_ops|parent_comm|dev|writing|meeting|research|browsing|idle|other",
  "summary": "不超过40字的中文工作摘要",
  "detail": "任务名称/主题/业务方向，保留工作细节，脱敏身份",
  "progress": "进展或交付物，没有则为空字符串",
  "todo": "识别到的待办，没有则为空字符串",
  "apps": ["检测到的应用/网站"],
  "contains_sensitive": true,
  "reportable": true
}

【是否值得汇报（reportable 字段，重要）】
- 只有与工作/业务相关、能写进日报的内容才为 true；
- 以下一律 false（不写入汇报）：私人聊天与闲聊、影视/游戏/购物/小说/娱乐浏览、纯系统操作（切窗口/桌面/设置）、与工作无关的个人事务；
- 判断以画面内容为准：工作文档、业务系统、代码、数据、业务沟通 = true；无业务含义 = false。"#;

fn build_prompt(settings: &config::Settings) -> String {
    let mut p = ANALYZE_PROMPT.to_string();
    if !settings.work_directions.is_empty() {
        p.push_str("\n\n【业务工作方向】\n");
        for d in &settings.work_directions {
            p.push_str(&format!("- {d}\n"));
        }
        p.push_str("判断任务所属方向时优先从上表匹配；匹配不到再归入最接近的方向，并在 detail 里写明任务主题。");
    }
    p
}

/// 构造请求体：DeepSeek 推理模型需显式关闭 thinking，否则推理 token 会吃掉预算导致正文为空
fn build_body(s: &config::Settings, messages: serde_json::Value, max_tokens: u32, temperature: f64) -> serde_json::Value {
    let mut body = json!({
        "model": s.model_name,
        "messages": messages,
        "temperature": temperature,
        "max_tokens": max_tokens,
    });
    if s.model_base_url.contains("deepseek") {
        body["thinking"] = json!({"type": "disabled"});
    }
    body
}

fn chat(s: &config::Settings, body: serde_json::Value, label: &str) -> Result<String, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(180))
        .build()
        .map_err(|e| format!("{label}创建客户端失败: {e}"))?;
    let url = format!("{}/chat/completions", s.model_base_url.trim_end_matches('/'));
    // 网络抖动重试：偶发 "error sending request" 不该让整篇日报丢掉
    let mut last_err = String::new();
    let mut resp_opt = None;
    for attempt in 1..=3 {
        match client
            .post(&url)
            .bearer_auth(s.model_api_key.trim())
            .json(&body)
            .send()
        {
            Ok(r) => {
                resp_opt = Some(r);
                break;
            }
            Err(e) => {
                last_err = format!("{label}请求失败（第 {attempt} 次）: {e}");
                config::log_line(&last_err);
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }
        }
    }
    let resp = resp_opt.ok_or(last_err)?;
    if !resp.status().is_success() {
        return Err(format!("{label}模型返回 {}: {}", resp.status(), resp.text().unwrap_or_default()));
    }
    let v: Value = resp.json().map_err(|e| format!("{label}解析响应失败: {e}"))?;
    let content = v["choices"][0]["message"]["content"].as_str().unwrap_or("").to_string();
    if content.trim().is_empty() {
        // 推理模型常见：仅返回 reasoning_content 而正文为空
        let reason = v["choices"][0]["message"]["reasoning_content"].as_str().unwrap_or("");
        let finish = v["choices"][0]["finish_reason"].as_str().unwrap_or("");
        return Err(format!(
            "{label}返回空内容（finish={finish}，reasoning={}字）——通常是 max_tokens 被推理过程占用",
            reason.chars().count()
        ));
    }
    Ok(content)
}

/// 调用视觉模型分析截图，返回 Record；失败抛字符串错误。
pub fn analyze_image(data_url: &str) -> Result<Record, String> {
    let s = config::load_settings();
    let messages = json!([{
        "role": "user",
        "content": [
            {"type": "image_url", "image_url": {"url": data_url}},
            {"type": "text", "text": build_prompt(&s)},
        ]
    }]);
    let body = build_body(&s, messages, 4096, 1.0);
    let content = chat(&s, body, "截图分析")?;
    parse_record(&content)
}

pub fn parse_record(content: &str) -> Result<Record, String> {
    let start = content.find('{').ok_or("模型输出无 JSON")?;
    let end = content.rfind('}').ok_or("模型输出无 JSON")?;
    let v: Value = serde_json::from_str(&content[start..=end]).map_err(|e| format!("JSON 解析失败: {e}"))?;
    Ok(Record {
        ts: Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        activity: crate::records::normalize_activity(v["activity"].as_str().unwrap_or("other")),
        summary: v["summary"].as_str().unwrap_or("").to_string(),
        detail: v["detail"].as_str().unwrap_or("").to_string(),
        progress: v["progress"].as_str().unwrap_or("").to_string(),
        todo: v["todo"].as_str().unwrap_or("").to_string(),
        apps: v["apps"].as_array().cloned().unwrap_or_default().into_iter()
            .filter_map(|a| a.as_str().map(String::from)).collect(),
        contains_sensitive: v["contains_sensitive"].as_bool().unwrap_or(false),
        reportable: v["reportable"].as_bool().unwrap_or(true),
    })
}

/// 日报预览（今天）：跨日窗口 + 模板 + 历史样本 + 会议记录
pub fn preview_report(date: &str) -> Result<String, String> {
    generate_report_for(date)
}

/// 生成指定日期的日报（每一天都用统一的跨日窗口），生成后落盘缓存
pub fn generate_report_for(date: &str) -> Result<String, String> {
    let now = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    // 每天固定一个「跨日窗口」[(D-1) 推送时刻, D 推送时刻)，今天再收口到 min(现在, 今日推送时刻)。
    // 今天的起点用 day_window_start()：上次推送不是今天时，从上次推送时刻起算——
    // 周一/周二休息日没推送，周三的日报会把空档一起带上；
    // 上次推送就是今天时，回到昨天推送时刻，重推/预览都看完整一天。
    let is_today = date == crate::records::today_str();
    let (day_from, day_end) = config::historical_window(date);
    let from = if is_today { config::day_window_start() } else { day_from };
    let to = if is_today && now < day_end { now } else { day_end };
    if from.is_empty() {
        return Err(format!("无效日期: {date}"));
    }
    let digest = crate::records::build_digest_range(&from, &to);
    if digest.is_empty() {
        return Ok(format!("{date} 没有工作记录（窗口 {from} ~ {to}）。").into());
    }
    let s = config::load_settings();
    let ctx = crate::knowledge::report_context(&s, 3000);
    // 会议记录合并（业务结论的主要载体）——仅当天拉取，历史日期跳过
    let meeting_text = if date == crate::records::today_str() && s.meeting_auto_fetch {
        crate::meetings::fetch_meeting_text(&s, date)
    } else {
        String::new()
    };
    let meeting_block = if meeting_text.trim().is_empty() {
        String::new()
    } else {
        format!("\n\n【今日会议（业务核心，优先提炼结论与待办）】\n{meeting_text}")
    };
    let prompt = format!(
        "你是帮本人整理日报的同事。今天是 {date}。请根据会议记录与原始工作记录写一份简短、像本人同步的日报。\n\n\
         【基本要求】\n1) 先按项目/主题合并重复记录，不按时间逐条罗列；保留 2-5 条最值得汇报的进展；按模板使用“今日核心成果 / 推进中的重点事项 / 明日计划 / 需要协调”；\
         2) 会议记录是业务核心：有明确结论或待办时优先提炼，工作轨迹只作为补充；\
         3) 严格区分已完成、进行中和待确认，不把\"正在处理\"写成\"已完成\"；禁止编造；\
         4) 没有内容的模板章节直接删除，不输出\"暂无\"、\"无\"、\"待补充\"；\
         5) 明日计划最多 3 条，只能来自记录中的待办或自然衔接的下一步；\
         6) 少用\"赋能\"、\"闭环\"、\"抓手\"、\"夯实\"等套话，不写空泛价值判断；\
         7) 不输出应用时长、空指标表、预计完成时间、额外总结或元评论；\
         8) 【最重要】历史参考样本只用于学习写作风格与颗粒度：样本里的具体事项、项目名、数字、结论一律禁止写进今天的日报；今天日报的全部事实必须且只能来自【今日原始工作记录】与【今日会议】，两者都没有的内容视为不存在。{ctx}{meeting_block}\n\n【今日原始工作记录】\n{digest}\n\n直接输出日报正文。"
    );
    let body = build_body(&s, json!([{"role": "user", "content": prompt}]), 4096, 0.7);
    let content = chat(&s, body, "日报生成")?;
    crate::config::log_line(&format!("日报生成成功（{date}，{}字）", content.chars().count()));
    crate::config::save_report(date, &content);
    Ok(content)
}

/// 生成周报：聚合指定工作周期（周三 ~ 周日，共 5 天）的工作轨迹与已生成的每日日报，按周报模板产出
pub fn generate_weekly_report(week_start: &str) -> Result<String, String> {
    let d = chrono::NaiveDate::parse_from_str(week_start, "%Y-%m-%d")
        .map_err(|_| format!("无效周起始日: {week_start}"))?;
    // 工作周期 = 周三至周日（周一周二休息）：只聚合 5 天，不把休息日和未来日期算进来
    let week_end = d + chrono::Duration::days(4);
    // 本周已生成的每日日报（作为聚合输入，保留每天的成果提炼）
    let mut daily_block = String::new();
    let mut covered_dates = std::collections::HashSet::new();
    let mut cur = d;
    while cur <= week_end {
        let ds = cur.format("%Y-%m-%d").to_string();
        if let Some(r) = config::load_report(&ds) {
            daily_block.push_str(&format!("\n【{ds} 日报】\n{r}"));
            covered_dates.insert(ds);
        }
        cur += chrono::Duration::days(1);
    }

    // 只补充尚未生成日报的日期，避免同一件事从日报和原始快照重复进入周报。
    let mut uncovered_digest = String::new();
    let mut missing = d;
    while missing <= week_end {
        let ds = missing.format("%Y-%m-%d").to_string();
        if !covered_dates.contains(&ds) {
            let day_from = format!("{ds}T00:00:00");
            let day_to = format!("{}T00:00:00", (missing + chrono::Duration::days(1)).format("%Y-%m-%d"));
            let part = crate::records::build_digest_range(&day_from, &day_to);
            if !part.trim().is_empty() {
                uncovered_digest.push_str(&format!("\n【{ds} 未生成日报的原始记录】\n{part}"));
            }
        }
        missing += chrono::Duration::days(1);
    }

    if uncovered_digest.is_empty() && daily_block.is_empty() {
        return Ok(format!("{week_start} ~ {} 没有工作记录。", week_end.format("%Y-%m-%d")));
    }

    let s = config::load_settings();
    let mut ctx = String::new();
    if !s.weekly_template.trim().is_empty() {
        ctx.push_str("\n\n【周报模板（必须遵循此格式）】\n");
        ctx.push_str(s.weekly_template.trim());
    }
    if !s.reference_samples.trim().is_empty() {
        ctx.push_str("\n\n【历史周报/日报参考样本（学习风格与颗粒度，不要照抄内容）】\n");
        let refs: String = s.reference_samples.trim().chars().take(3000).collect();
        ctx.push_str(&refs);
    }

    let weekly_source = if daily_block.trim().is_empty() {
        format!("【本周原始工作记录】\n{uncovered_digest}")
    } else {
        format!("【本周已生成日报（优先使用）】{daily_block}\n\n【仅补充尚未生成日报日期的原始记录】{uncovered_digest}")
    };

    let prompt = format!(
        "你是帮本人整理周报的同事。本周为 {week_start} ~ {week_end}。请根据本周工作轨迹与每日日报，写一份简短、像本人同步的周报。\n\n\
         【基本要求】\n1) 按项目/主题合并重复记录，不按时间段罗列；以“项目/能力分组 + 下周工作计划表”为主，严格按模板组织，不要为了凑完整而强行增加验证、遗留问题或需要协调章节；\
         2) 只写有事实依据的内容：没有数据不写指标，没有卡点不凑风险，没有下周安排不编建议；\
         3) 严格区分已完成、进行中和待确认，不把计划写成结果；禁止编造；\
         4) 没有内容的模板章节直接删除，不输出\"暂无\"、\"无\"、\"待补充\"；\
         5) 完成事项最多 6 条，推进事项最多 4 条，下周计划最多 3 条；\
         6) 少用形容词和\"赋能\"、\"闭环\"、\"抓手\"、\"夯实\"等套话；不写时间分布、应用时长、空指标表或泛泛建议；\
         7) 每个项目标题先写项目名，再用一句话说明本周进展和当前状态；项目内优先保留 2-4 条事实，完成事项、问题和下一步合并写；下周计划最多 4 条，使用优先级/动作/预期产出表格；\
         8) 【最重要】历史参考样本只用于学习写作风格与颗粒度：样本里的具体事项、项目名、数字、结论一律禁止写进周报；本周周报的全部事实必须且只能来自本周的日报与原始工作记录，两者都没有的内容视为不存在；\n          9) 直接输出周报正文，不要写作过程或元评论。{ctx}\n\n{weekly_source}"
    );
    let body = build_body(&s, json!([{"role": "user", "content": prompt}]), 4096, 0.7);
    let content = chat(&s, body, "周报生成")?;
    crate::config::log_line(&format!("周报生成成功（{week_start}，{}字）", content.chars().count()));
    crate::config::save_report(&format!("week-{week_start}"), &content);
    Ok(content)
}
