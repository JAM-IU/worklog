use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "d_interval")]
    pub interval_minutes: u64,
    #[serde(default = "d_true")]
    pub recording_enabled: bool,
    #[serde(default = "d_base_url")]
    pub model_base_url: String,
    #[serde(default = "d_model")]
    pub model_name: String,
    #[serde(default)]
    pub model_api_key: String,
    #[serde(default)]
    pub work_directions: Vec<String>,
    /// 日报生成模板（提示词）：大模型按此格式产出日报
    #[serde(default = "d_report_template")]
    pub report_template: String,
    /// 周报生成模板（提示词）
    #[serde(default = "d_weekly_template")]
    pub weekly_template: String,
    /// 历史日报/周报参考样本（知识沉淀）：供大模型学习写作风格与颗粒度
    #[serde(default)]
    pub reference_samples: String,
    #[serde(default = "d_true")]
    pub feishu_enabled: bool,
    #[serde(default = "d_token")]
    pub feishu_base_token: String,
    #[serde(default = "d_table")]
    pub feishu_table_id: String,
    #[serde(default = "d_dept")]
    pub feishu_dept: String,
    #[serde(default)]
    pub feishu_user_name: String,
    #[serde(default)]
    pub feishu_user_open_id: String,
    #[serde(default = "d_true")]
    pub feishu_auto_push_enabled: bool,
    #[serde(default = "d_hour")]
    pub feishu_push_hour: u32,
    #[serde(default = "d_minute")]
    pub feishu_push_minute: u32,
    /// 日报自动推送的星期几（1=周一 … 7=周日）。
    /// 默认周三~周日：周一/周二休息不推送；周日这一行只入表、不单独提醒，供当晚周报聚合。
    #[serde(default = "d_push_weekdays")]
    pub daily_push_weekdays: Vec<u32>,
    /// 飞书消息监听：把自己在飞书群/私聊里发的文本消息自动记录为「答疑支持」事件
    #[serde(default = "d_true")]
    pub im_listener_enabled: bool,
    /// 同一会话内多少分钟内合并成一条记录
    #[serde(default = "d_im_merge")]
    pub im_merge_minutes: u64,
    /// 会话名关键词（每行一个，命中才记录；留空=记录所有会话）
    #[serde(default)]
    pub im_chat_filter: String,
    /// 是否由 WorkLog 本地生成日报（带个人模板+知识沉淀），推送时直接带上
    #[serde(default = "d_true")]
    pub local_report_enabled: bool,
    /// 飞书知识库文档 token：每天日报归档追加到该文档（留空则不归档）
    #[serde(default)]
    pub knowledge_doc_token: String,
    #[serde(default = "d_true")]
    pub knowledge_archive_enabled: bool,
    /// 会议链接（每行一个：妙记 /minutes/ 链接或纪要 docx 链接），生成日报时自动拉取合并
    #[serde(default)]
    pub meeting_links: String,
    /// 手动补充的会议纪要文本（兜底）
    #[serde(default)]
    pub meeting_notes: String,
    /// 是否在生成日报时自动合并会议记录
    #[serde(default = "d_true")]
    pub meeting_auto_fetch: bool,
    /// 是否通过 VC 会议自动抓取当天纪要（vc+search → vc+detail → note+detail → docs）
    #[serde(default = "d_true")]
    pub meeting_auto_vc: bool,
    /// 不需要汇报的关键词（每行一个，命中则不落库）
    #[serde(default)]
    pub exclude_keywords: String,
}

fn d_interval() -> u64 { 3 }
fn d_true() -> bool { true }
fn d_base_url() -> String { "https://api.deepseek.com".into() }
fn d_model() -> String { "deepseek-v4-flash-vision-exp".into() }
fn d_token() -> String { "OIYSbZpouay8umsfte9c7KyBnye".into() }
fn d_table() -> String { "tbl0i1AAknL19gT8".into() }
fn d_dept() -> String { "转化".into() }
fn d_hour() -> u32 { 18 }
fn d_minute() -> u32 { 10 }
fn d_push_weekdays() -> Vec<u32> { vec![3, 4, 5, 6, 7] }
fn d_im_merge() -> u64 { 10 }
fn default_work_directions() -> Vec<String> {
    vec![
        "团队技术赋能与工具提效（工具/配置上线、需求提交与排期跟进、上线验收、答疑支持、培训）".into(),
        "首联五轮沟通与质量监测（字段口径、用户分层、评分、看板）".into(),
        "销冠智能体与用户画像（实时指导、组织架构、业务上线）".into(),
        "growAI与21天回捞（往期托管、标签分层、自动追单）".into(),
        "群转机器人与话术库（场景策略、SOP、预约转化）".into(),
        "智能问答策略（知识库、问答对、重放验证、NLG）".into(),
        "私域与低价课转化（AI介入、回复率、提课转化）".into(),
        "学情采集与标签（薄弱学科、对话采集、家长画像）".into(),
        "数据与系统工具（脚本、看板、部署、账号与链路）".into(),
        "业务复盘与团队运营（批次数据、承接动作、培训帮带）".into(),
        "会议与需求协同（产研对齐、需求文档、排期确认）".into(),
        "WorkLog日报周报（模板、知识沉淀、报告优化）".into(),
    ]
}
fn d_report_template() -> String { DEFAULT_REPORT_TEMPLATE.to_string() }
fn d_weekly_template() -> String { DEFAULT_WEEKLY_TEMPLATE.to_string() }

pub const DEFAULT_REPORT_TEMPLATE: &str = r#"📋 日报（日期）

## 今日核心成果
### 成果一：具体主题（工具名 / 需求名 / 问题主题，禁止写“N 个工具”这类泛称）
- **做了什么**：具体对象 + 动作 + 结果——工具/功能的具体名称和给谁用；需求的具体内容与排期进展；答疑的具体问题主题与结论
- **价值**：对团队提效或业务推进的实际影响

### 成果二：具体主题
- **做了什么**：具体结果

## 推进中的重点事项
- **事项**：当前状态 → 卡点 → 下一步；等待谁时直接点名

## 明日计划
1. 具体动作
2. 具体动作
3. 具体动作

## 需要协调
- 只写确实需要他人、数据、权限或排期配合的事项。

要求：技术运营视角——优先写为团队/业务提效的动作（工具/配置上线、需求提交与排期跟进、推动开发进度、上线验收、上线后答疑与问题解决、业务协同），每件事写清楚对谁有什么影响；【每项成果必须出现至少一个具体名词（工具名/功能名/需求名/系统名/人名），事件统计数字只做汇总补充、绝不能替代具体内容——例如写“上线『群内点歌与报备插件』给团队用”，而不是“上线 2 个工具”；写“推进『21天打标』需求”，而不是“推进 4 项需求”】；开发实现过程（环境调试、写脚本、终端操作、代码细节）一律不写，开发只在“交付了什么、给谁用”层面写一句；按项目合并重复记录，一件事只出现一次；严格区分已完成/进行中/待确认；没有内容的栏目直接删除；明日计划最多 3 条，只能来自记录中的待办或下一步；不写应用时长、空指标表、虚构数字、预计完成时间或额外总结。"#;

pub const DEFAULT_WEEKLY_TEMPLATE: &str = r#"📊 周报复盘（M月D日 - M月D日）

## 一、本周收获（回答“本周有什么收获”，必须写）
- 收获一：本周做成/想清楚了什么，具体到名称和事实（上线了 XX 工具、解决了 XX 问题、推进了 XX 需求），写明对团队/业务的影响
- 收获二：…

## 二、项目进展（只写本周有实质推进的项目）
### 1. 项目名：本周变化 + 当前状态
- 本周新增/变化的 2-3 条事实（完成、对齐、验证、上线、答疑）
- 仍在推进、待确认的内容直接写明状态，不要写成已完成

### 2. 项目名：本周变化 + 当前状态
- 按同样方式写事实和下一步

## 三、下周工作计划
| 优先级 | 计划动作 | 预期产出 |
| --- | --- | --- |
| P0 | 具体动作 | 具体产出 |

要求：按项目合并重复记录；只写本周有实质变化/推进的项目，一周内没有任何新动作的项目直接删除，禁止每周复制同一段描述；项目标题使用“项目名：本周变化 + 当前状态”；【每项收获/每个项目必须出现具体名称（工具名/需求名/问题主题/人名），统计数字只做汇总补充、绝不能替代具体内容】；优先写业务已经能用的能力、明确结论和真实产出；测试数据写清窗口、样本量和当前判断，小样本只能写成方向验证；需要协调的事项直接放进对应项目或计划中，只有确实存在且无法归入项目时才单独增加章节；没有内容的章节直接删除；不编造数字、日期、负责人、风险或完成状态；不写应用时长、泛泛总结、空指标表和 AI 套话；下周计划最多 4 条，只能来自本周记录中的待办或自然衔接的下一步。"#;

impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_minutes: 3,
            recording_enabled: true,
            model_base_url: "https://api.deepseek.com".into(),
            model_name: "deepseek-v4-flash-vision-exp".into(),
            model_api_key: String::new(),
            report_template: DEFAULT_REPORT_TEMPLATE.into(),
            weekly_template: DEFAULT_WEEKLY_TEMPLATE.into(),
            reference_samples: String::new(),
            work_directions: default_work_directions(),
            feishu_enabled: true,
            feishu_base_token: "OIYSbZpouay8umsfte9c7KyBnye".into(),
            feishu_table_id: "tbl0i1AAknL19gT8".into(),
            feishu_dept: "转化".into(),
            feishu_user_name: String::new(),
            feishu_user_open_id: String::new(),
            feishu_auto_push_enabled: true,
            feishu_push_hour: 18,
            feishu_push_minute: 10,
            daily_push_weekdays: vec![3, 4, 5, 6, 7],
            im_listener_enabled: true,
            im_merge_minutes: 10,
            im_chat_filter: String::new(),
            local_report_enabled: true,
            knowledge_doc_token: String::new(),
            knowledge_archive_enabled: true,
            meeting_links: String::new(),
            meeting_notes: String::new(),
            meeting_auto_fetch: true,
            meeting_auto_vc: true,
            exclude_keywords: String::new(),
        }
    }
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    let d = exe_dir().join("data");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn records_raw_dir() -> PathBuf {
    let d = data_dir().join("records").join("raw");
    let _ = std::fs::create_dir_all(&d);
    d
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

/// 自然日窗口：[(date) 00:00:00, (date+1) 00:00:00)。
/// 按自然日记录：过 0 点就算下一天，不跨天、不并入前一日。
pub fn natural_day_window(date: &str) -> (String, String) {
    let Ok(d) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return (String::new(), String::new());
    };
    let next = (d + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    (format!("{date}T00:00:00"), format!("{next}T00:00:00"))
}

/// 工作周锚点：所属周期的周三（工作周期 = 周三 ~ 周日，周一周二休息）
pub fn week_start_wednesday(anchor: chrono::NaiveDate) -> chrono::NaiveDate {
    let wd = anchor.weekday().number_from_monday(); // 1=周一 … 7=周日
    let back = if wd >= 3 { wd - 3 } else { wd + 4 };
    anchor - chrono::Duration::days(back as i64)
}

/// 记录上次成功推送时刻（供概览“已推送”状态与同日去重）
pub fn mark_pushed(ts: &str) {
    let _ = std::fs::write(
        data_dir().join("last_push.json"),
        serde_json::json!({ "ts": ts }).to_string(),
    );
}

/// 当日「勾选进日报」的事件 key 列表（人工精选；无文件 = 全部事件）
pub fn selection_path(date: &str) -> PathBuf {
    let d = data_dir().join("selection");
    let _ = std::fs::create_dir_all(&d);
    d.join(format!("{date}.json"))
}

pub fn load_selection(date: &str) -> Option<Vec<String>> {
    std::fs::read_to_string(selection_path(date))
        .ok()
        .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
}

pub fn save_selection(date: &str, keys: &[String]) {
    if let Ok(data) = serde_json::to_string(keys) {
        let _ = std::fs::write(selection_path(date), data);
    }
}

pub fn reports_dir() -> PathBuf {
    let d = data_dir().join("reports");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// 本地缓存日报（生成后落盘，回溯秒开）
pub fn save_report(date: &str, text: &str) {
    let _ = std::fs::write(reports_dir().join(format!("{date}.md")), text);
}

pub fn load_report(date: &str) -> Option<String> {
    std::fs::read_to_string(reports_dir().join(format!("{date}.md"))).ok()
}

/// 运行日志（追加写）：采集/生成/推送的关键事件与错误，便于排查
pub fn log_line(msg: &str) {
    use std::io::Write;
    let path = data_dir().join("worklog.log");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
    }
}

pub fn load_settings() -> Settings {
    std::fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> std::io::Result<()> {
    let data = serde_json::to_string_pretty(s).unwrap();
    std::fs::write(settings_path(), data)
}
