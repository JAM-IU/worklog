use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub ts: String,
    pub activity: String,
    pub summary: String,
    pub detail: String,
    pub progress: String,
    pub todo: String,
    pub apps: Vec<String>,
    pub contains_sensitive: bool,
    /// 是否值得写进汇报（false = 私人/娱乐/无关内容，不落库）
    #[serde(default = "default_true")]
    pub reportable: bool,
}

fn default_true() -> bool { true }

/// 不需要汇报的关键词（设置页可用 exclude_keywords 追加）
const DEFAULT_EXCLUDE_KEYWORDS: &[&str] = &[
    "影视", "电影", "电视剧", "综艺", "动漫", "番剧", "影片", "游戏", "王者", "原神",
    "steam", "epic", "wegame", "购物", "淘宝", "京东", "拼多多", "闲鱼", "天猫",
    "直播", "小说", "漫画", "娱乐", "微博热搜", "知乎热榜", "抖音", "bilibili", "b站",
    "斗鱼", "虎牙", "快手",
];

/// 判断记录是否值得入库：reportable 为 true，且不命中排除关键词
pub fn is_reportable(rec: &Record) -> bool {
    if !rec.reportable {
        return false;
    }
    let hay = format!("{} {} {}", rec.summary, rec.detail, rec.apps.join(" "));
    let s = config::load_settings();
    let mut keywords: Vec<String> = DEFAULT_EXCLUDE_KEYWORDS.iter().map(|k| k.to_string()).collect();
    for line in s.exclude_keywords.lines() {
        let k = line.trim();
        if !k.is_empty() {
            keywords.push(k.to_lowercase());
        }
    }
    let hay_l = hay.to_lowercase();
    !keywords.iter().any(|k| hay_l.contains(k.as_str()))
}

pub fn append_if_reportable(rec: &Record, date: &str) -> bool {
    if !is_reportable(rec) {
        return false;
    }
    append(rec, date).is_ok()
}

pub fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

pub fn raw_file(date: &str) -> PathBuf {
    config::records_raw_dir().join(format!("{date}.jsonl"))
}

pub fn append(rec: &Record, date: &str) -> std::io::Result<()> {
    use std::io::Write;
    let line = format!("{}\n", serde_json::to_string(rec).unwrap());
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(raw_file(date))?;
    f.write_all(line.as_bytes())
}

pub fn read_day(date: &str) -> Vec<Record> {
    let Ok(text) = std::fs::read_to_string(raw_file(date)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| serde_json::from_str::<Record>(l).ok())
        .collect()
}

pub fn activity_label(activity: &str) -> &'static str {
    match activity {
        "conversion" => "转化运营",
        "insight" => "学情采集",
        "script" => "话术库",
        "review" => "复盘分析",
        "data_ops" => "数据运营",
        "parent_comm" => "家长沟通",
        "dev" => "开发",
        "writing" => "文档",
        "meeting" => "会议",
        "research" => "学习",
        "browsing" => "生活",
        _ => "其他",
    }
}

/// 分类规范化：模型可能返回旧分类或自造词，统一收敛到业务分类体系
pub fn normalize_activity(raw: &str) -> String {
    let a = raw.trim().to_lowercase();
    let mapped = match a.as_str() {
        "conversion" | "insight" | "script" | "review" | "data_ops" | "parent_comm"
        | "dev" | "writing" | "meeting" | "research" | "browsing" | "idle" | "other" => a.as_str(),
        // 旧体系（dailylog）兼容
        "coding" | "code" | "development" | "programming" => "dev",
        "data" | "analytics" | "data_analysis" | "report" => "data_ops",
        "communication" | "comm" | "chat" | "support" | "运维" => "parent_comm",
        "doc" | "docs" | "document" => "writing",
        "study" | "learning" | "reading" => "research",
        "life" | "entertainment" => "browsing",
        _ => "other",
    };
    mapped.to_string()
}

const ENTERTAINMENT_KEYWORDS: &[&str] = &[
    "影视", "电影", "电视剧", "综艺", "动漫", "番剧", "视频网站", "影片", "游戏", "王者",
    "原神", "steam", "购物", "淘宝", "京东", "拼多多", "直播", "小说", "漫画", "娱乐",
    "音乐", "微博热搜", "知乎热榜",
];

pub fn is_work(rec: &Record) -> bool {
    let hay = format!("{} {} {}", rec.activity, rec.summary, rec.detail);
    !ENTERTAINMENT_KEYWORDS.iter().any(|k| hay.contains(k))
}

/// 当天工作记录 → 成果导向摘要（推送到飞书「日报记录」表的原始输入）
pub fn build_digest(date: &str) -> String {
    let day = read_day(date);
    let items: Vec<&Record> = day
        .iter()
        .filter(|r| is_work(r) && !r.summary.trim().is_empty() && r.summary != "无工作活动")
        .collect();
    digest_from_items(&items)
}

/// 跨日窗口摘要：从 from_ts（如昨天推送时刻）到现在的记录合并，
/// 解决"深夜工作跨到第二天"被丢进前一日日报的问题。
pub fn build_digest_window(from_ts: &str) -> String {
    let now = Local::now();
    let today = now.format("%Y-%m-%d").to_string();
    let yesterday = (now.date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let mut all = read_day(&yesterday);
    all.extend(read_day(&today));
    all.sort_by(|a, b| a.ts.cmp(&b.ts));
    let items: Vec<&Record> = all
        .iter()
        .filter(|r| {
            r.ts.as_str() >= from_ts
                && is_work(r)
                && !r.summary.trim().is_empty()
                && r.summary != "无工作活动"
        })
        .collect();
    digest_from_items(&items)
}

/// 指定日期区间的摘要（历史日报回溯用）：[from_ts, to_ts)
pub fn build_digest_range(from_ts: &str, to_ts: &str) -> String {
    let now = Local::now();
    let mut all: Vec<Record> = Vec::new();
    let from_date = &from_ts[..10.min(from_ts.len())];
    let to_date = &to_ts[..10.min(to_ts.len())];
    let mut d = chrono::NaiveDate::parse_from_str(from_date, "%Y-%m-%d").unwrap_or_else(|_| now.date_naive());
    let end = chrono::NaiveDate::parse_from_str(to_date, "%Y-%m-%d").unwrap_or_else(|_| now.date_naive());
    while d <= end {
        all.extend(read_day(&d.format("%Y-%m-%d").to_string()));
        d += chrono::Duration::days(1);
    }
    all.sort_by(|a, b| a.ts.cmp(&b.ts));
    let items: Vec<&Record> = all
        .iter()
        .filter(|r| {
            r.ts.as_str() >= from_ts
                && r.ts.as_str() < to_ts
                && is_work(r)
                && !r.summary.trim().is_empty()
                && r.summary != "无工作活动"
        })
        .collect();
    digest_from_items(&items)
}

/// 有记录的日期列表（降序，供日报回溯选择）
pub fn list_record_dates() -> Vec<String> {
    let mut dates: Vec<String> = std::fs::read_dir(config::records_raw_dir())
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    e.path()
                        .file_stem()
                        .and_then(|s| s.to_str().map(String::from))
                })
                .filter(|s| s.len() == 10 && s.chars().nth(4) == Some('-'))
                .collect()
        })
        .unwrap_or_default();
    dates.sort_by(|a, b| b.cmp(a));
    dates
}

fn digest_from_items(items: &[&Record]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let mut lines = vec!["今日工作记录（WorkLog 自动采集，重复快照已合并）：".to_string()];
    let mut seen = std::collections::HashSet::new();
    for r in items {
        // 截图采集会连续得到相同任务但进展字段略有变化；按“分类 + 摘要”去重，
        // 避免同一件事被连续快照展开成多条。真正不同的任务仍会保留。
        let key = format!("{}|{}", r.activity, r.summary.trim())
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        let hm: String = r.ts.chars().skip(11).take(5).collect();
        let mut seg = format!("{}【{}】{}", hm, activity_label(&r.activity), r.summary.trim());
        if !r.progress.trim().is_empty() {
            seg.push_str(&format!("｜进展：{}", r.progress.trim()));
        }
        if !r.todo.trim().is_empty() {
            seg.push_str(&format!("｜待办：{}", r.todo.trim()));
        }
        lines.push(seg);
    }
    let digest = lines.join("\n");
    digest.chars().take(2000).collect()
}
