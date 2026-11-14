use chrono::Local;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub ts: String,
    pub activity: String,
    /// 事件类型：工具上线 / 需求推进 / 答疑支持 / 开发实现 / 业务动作 / 会议 / 学习 / 其他
    #[serde(default = "d_other")]
    pub event_type: String,
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
fn d_other() -> String { "其他".into() }

/// 事件类型 → 短标签（用于记录展示与统计）
pub fn event_label(et: &str) -> &'static str {
    match et {
        "工具上线" => "工具上线",
        "需求推进" => "需求推进",
        "答疑支持" => "答疑支持",
        "开发实现" => "开发实现",
        "业务动作" => "业务动作",
        "会议" => "会议",
        "学习" => "学习",
        _ => "其他",
    }
}

/// 不需要汇报的关键词（设置页可用 exclude_keywords 追加）
const DEFAULT_EXCLUDE_KEYWORDS: &[&str] = &[
    // 影视 / 娱乐
    "影视", "电影", "电视剧", "纪录片", "综艺", "动漫", "番剧", "影片", "追剧",
    "短视频", "视频网站", "直播", "小说", "漫画", "娱乐", "游戏", "王者", "原神",
    "steam", "epic", "wegame", "斗鱼", "虎牙", "快手", "抖音", "bilibili", "b站",
    // 电商 / 生活
    "购物", "淘宝", "京东", "拼多多", "闲鱼", "天猫", "外卖", "买菜", "点餐",
    // 资讯消遣
    "微博热搜", "知乎热榜", "热搜", "八卦",
];

/// 判断记录是否值得入库：reportable 为 true，且不命中排除关键词。
/// 传入 settings 快照，避免逐条记录重复读配置文件。
pub fn is_reportable_with(rec: &Record, s: &config::Settings) -> bool {
    if !rec.reportable {
        return false;
    }
    let hay = format!("{} {} {}", rec.summary, rec.detail, rec.apps.join(" "));
    let hay_l = hay.to_lowercase();
    if DEFAULT_EXCLUDE_KEYWORDS.iter().any(|k| hay_l.contains(k)) {
        return false;
    }
    s.exclude_keywords
        .lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .any(|k| hay_l.contains(&k))
        == false
}

pub fn is_reportable(rec: &Record) -> bool {
    is_reportable_with(rec, &config::load_settings())
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

/// 记录稳定标识（勾选面板用它把「选中项」映射回记录行）
pub fn record_key(rec: &Record) -> String {
    format!("{}|{}", rec.ts, rec.summary.trim())
}

/// 指定日期区间的摘要（历史日报回溯用）：[from_ts, to_ts)
pub fn build_digest_range(from_ts: &str, to_ts: &str) -> String {
    build_digest_range_filtered(from_ts, to_ts, None)
}

/// 同上，但只保留 allowed 集合内的记录（用户勾选进日报的事件）；None = 全部
pub fn build_digest_range_filtered(
    from_ts: &str,
    to_ts: &str,
    allowed: Option<&std::collections::HashSet<String>>,
) -> String {
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
                && allowed.map_or(true, |s| s.contains(&record_key(r)))
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
    let header = "今日工作记录（WorkLog 自动采集，重复快照已合并）：".to_string();
    // 事件类型统计：日报/周报直接用它做量化（上线 X 个工具、解决 X 个问题、推进 X 个需求）
    let mut stats: Vec<(&str, usize)> = vec![
        ("工具上线", 0), ("需求推进", 0), ("答疑支持", 0), ("开发实现", 0),
        ("业务动作", 0), ("会议", 0), ("学习", 0),
    ];
    let mut stat_of = |et: &str| {
        for s in stats.iter_mut() {
            if s.0 == et { s.1 += 1; return; }
        }
    };
    let mut seen = std::collections::HashSet::new();
    let mut segs: Vec<String> = Vec::new();
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
        stat_of(&r.event_type);
        let hm: String = r.ts.chars().skip(11).take(5).collect();
        let ev = event_label(&r.event_type);
        let mut seg = if ev == "其他" {
            format!("{}【{}】{}", hm, activity_label(&r.activity), r.summary.trim())
        } else {
            format!("{}【{}｜{}】{}", hm, ev, activity_label(&r.activity), r.summary.trim())
        };
        if !r.progress.trim().is_empty() {
            seg.push_str(&format!("｜进展：{}", r.progress.trim()));
        }
        if !r.todo.trim().is_empty() {
            seg.push_str(&format!("｜待办：{}", r.todo.trim()));
        }
        segs.push(seg);
    }
    let stat_line: Vec<String> = stats
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    let stat_header = if !stat_line.is_empty() {
        Some(format!("事件统计（AI 初判，仅供参考）：{}", stat_line.join(" · ")))
    } else {
        None
    };
    // 限长时保留「最新」的记录（尾部），而不是最旧的前 2000 字——
    // 否则窗口会整段丢掉当天下午/晚上的工作，日报对不上当天记录。
    let mut kept: Vec<String> = Vec::new();
    let mut len = 0usize;
    for seg in segs.iter().rev() {
        let n = seg.chars().count() + 1;
        if !kept.is_empty() && len + n > 2000 {
            break;
        }
        kept.push(seg.clone());
        len += n;
    }
    kept.reverse();
    let mut out = vec![header];
    if let Some(sh) = stat_header {
        out.push(sh);
    }
    out.extend(kept);
    out.join("\n")
}
