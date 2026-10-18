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
fn d_dept() -> String { "教学运营".into() }
fn d_hour() -> u32 { 18 }
fn d_minute() -> u32 { 10 }
fn d_report_template() -> String { DEFAULT_REPORT_TEMPLATE.to_string() }
fn d_weekly_template() -> String { DEFAULT_WEEKLY_TEMPLATE.to_string() }

pub const DEFAULT_REPORT_TEMPLATE: &str = r#"📋 日报（日期）

## 今日核心成果
### 成果一：项目/事项
- **目标**：今天要推进什么（只有确实有目标时写）
- **结果**：做了什么、产出什么、得到什么结论
- **价值**：只有能说明实际影响时写，避免拔高

### 成果二：项目/事项
- **结果**：具体进展或产出

## 推进中的重点事项
- **事项**：当前进度；下一步动作；等待谁确认时直接写清楚

## 明日计划
1. 具体动作
2. 具体动作
3. 具体动作

## 需要协调
- 只写确实需要他人、数据、权限或排期配合的事项。

要求：按项目合并重复记录；一件事只出现一次；优先写业务结果、明确结论和真实产出；严格区分已完成、进行中、待确认；没有内容的栏目直接删除；像本人给同事同步，少用套话；明日计划最多 3 条，只能来自记录中的待办或下一步；不写应用时长、空指标表、虚构数字、预计完成时间或额外总结。"#;

pub const DEFAULT_WEEKLY_TEMPLATE: &str = r#"📊 周报复盘（M月D日 - M月D日）

## 一、本周已推动、业务侧可使用的能力
### 1. 项目/能力：当前进展
- 做了什么，业务现在能用到什么
- 仍在确认或预计上线的内容要保留当前状态

## 二、本周验证
- 写清测试窗口、样本量、关键数据和当前判断；小样本只能写成方向验证。

## 三、本周其他完成事项
- **事项**：动作和产出。

## 四、当前推进中与遗留问题
| 事项 | 状态 | 下一步 |
| --- | --- | --- |
| 项目/事项 | 当前状态 | 下一步动作 |

## 五、下周工作计划
| 优先级 | 计划动作 | 预期产出 |
| --- | --- | --- |
| P0 | 具体动作 | 具体产出 |

## 六、需要协调的事项
- 写清需要谁确认什么、需要什么数据或资源、哪个环节会被卡住。

要求：按业务项目合并重复记录；优先写业务已经能用的能力，再写验证数据、遗留问题和下周动作；一件事只出现一次；没有内容的章节直接删除；不编造数字、日期、风险、负责人或完成状态；少用套话，不写时间分布、应用时长和泛泛总结。"#;

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
            work_directions: vec![
                "AI工作汇报闭环（日报/周报自动生成、数据晨报、复盘动作闭环、WorkLog采集端）".into(),
                "转化运营（线索转化、复购续报、批次复盘、ABCD分层）".into(),
                "首联质量监测（判定标准、评分模型、口径、跑测、三要素五维）".into(),
                "回捞分层触达（分层模型、历史数据、触达策略、0元课、付费记录）".into(),
                "私域激活（分层标签、甜头埋点、AI多轮、久报零约纯白）".into(),
                "9报/低价课（介入规则、标签、灰度、全年级）".into(),
                "学情采集（环境、链路、对话采集、家长画像）".into(),
                "家长沟通（答疑、回访、学习报告、续报沟通）".into(),
                "话术库平台（技术盘点、技术方案、微搭手册、资质）".into(),
                "智能问答策略（知识库、SOP、问答对、策略配置、重放）".into(),
                "群转机器人（策略话术、触发、配置、课程ID）".into(),
                "决策智能体（提示词、记忆库、JSON容错、向量接入）".into(),
                "AI质检与评分（评分模块、监控、样本验证、外呼质检）".into(),
                "跨平台聊天助手（API、OCR、窗口、稳定性）".into(),
                "系统工具优先级（销冠智能体、托管系统、21天、蒸馏）".into(),
                "增长侧需求协同（产研提需、需求文档、优先级）".into(),
                "数据处理与部署（脚本、管道、部署排查、Python）".into(),
                "日常运营支撑（数据播报、策略调整、账号归属）".into(),
                "会议沟通（复盘会、周会、对齐会、同步）".into(),
                "其他/待归类（无锚点命中的记录）".into(),
            ],
            feishu_enabled: true,
            feishu_base_token: "OIYSbZpouay8umsfte9c7KyBnye".into(),
            feishu_table_id: "tbl0i1AAknL19gT8".into(),
            feishu_dept: "教学运营".into(),
            feishu_user_name: String::new(),
            feishu_user_open_id: String::new(),
            feishu_auto_push_enabled: true,
            feishu_push_hour: 18,
            feishu_push_minute: 10,
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

/// 日报跨日窗口起点：
/// - 今天还没推送过 → 昨天推送时刻（跨日窗口，深夜工作并入今日日报）
/// - 今天已推送过 → 上次推送时刻（只补新增记录，不重复统计）
pub fn report_window_start_ts() -> String {
    let s = load_settings();
    let now = chrono::Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let file = data_dir().join("last_push.json");
    if let Ok(text) = std::fs::read_to_string(&file) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(ts) = v.get("ts").and_then(|x| x.as_str()) {
                if ts.starts_with(&today) {
                    return ts.to_string();
                }
            }
        }
    }
    let yesterday = (now.date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    format!("{yesterday}T{:02}:{:02}:00", s.feishu_push_hour, s.feishu_push_minute)
}

/// 记录上次成功推送时刻（供跨日窗口去重）
pub fn mark_pushed(ts: &str) {
    let _ = std::fs::write(
        data_dir().join("last_push.json"),
        serde_json::json!({ "ts": ts }).to_string(),
    );
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

/// 某个历史日期的跨日窗口：[(D-1) 推送时刻, D 推送时刻)
pub fn historical_window(date: &str) -> (String, String) {
    let s = load_settings();
    let Ok(d) = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
        return (String::new(), String::new());
    };
    let prev = (d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    let hh = format!("{:02}:{:02}:00", s.feishu_push_hour, s.feishu_push_minute);
    (format!("{prev}T{hh}"), format!("{date}T{hh}"))
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
