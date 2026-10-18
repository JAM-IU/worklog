use chrono::Local;
use serde_json::json;

use crate::config;
use crate::records;

/// 概览页统计：今日/昨日记录数、业务方向数、专注时长、总记录、工作天数、记录率、近7日趋势
pub fn overview() -> serde_json::Value {
    let s = config::load_settings();
    let interval = s.interval_minutes.max(1) as f64;
    let today = records::today_str();
    let yesterday = (Local::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    let dates = records::list_record_dates();
    let mut daily_counts: Vec<serde_json::Value> = Vec::new();
    let mut total_records = 0usize;
    let mut work_days = 0usize;
    let mut today_count = 0usize;
    let mut yesterday_count = 0usize;
    let mut today_themes = std::collections::HashSet::new();

    // 近 7 天（含无记录日）趋势
    let mut last7: Vec<String> = Vec::new();
    for i in (0..7).rev() {
        let d = (Local::now().date_naive() - chrono::Duration::days(i))
            .format("%Y-%m-%d")
            .to_string();
        last7.push(d);
    }

    for d in &dates {
        let recs = records::read_day(d);
        let work: Vec<_> = recs.iter().filter(|r| records::is_work(r)).collect();
        if !work.is_empty() {
            work_days += 1;
        }
        total_records += work.len();
        if d == &today {
            today_count = work.len();
            for r in &work {
                today_themes.insert(r.activity.clone());
            }
        }
        if d == &yesterday {
            yesterday_count = work.len();
        }
    }
    for d in &last7 {
        let n = records::read_day(d).iter().filter(|r| records::is_work(r)).count();
        daily_counts.push(json!({"date": d, "count": n}));
    }

    // 今日业务方向分布
    let mut dist: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in records::read_day(&today).iter().filter(|r| records::is_work(r)) {
        *dist.entry(records::activity_label(&r.activity).to_string()).or_insert(0) += 1;
    }
    let mut dist_vec: Vec<_> = dist.into_iter().collect();
    dist_vec.sort_by(|a, b| b.1.cmp(&a.1));
    let distribution: Vec<serde_json::Value> = dist_vec
        .iter()
        .map(|(k, v)| json!({"label": k, "count": v}))
        .collect();

    let focus_hours_today = today_count as f64 * interval / 60.0;
    let focus_hours_yesterday = yesterday_count as f64 * interval / 60.0;
    let avg_hours = if work_days > 0 {
        total_records as f64 * interval / 60.0 / work_days as f64
    } else {
        0.0
    };
    let record_rate = if total_records > 0 {
        // 记录率：有内容记录（summary 非空）占比
        let all: usize = dates
            .iter()
            .map(|d| records::read_day(d).iter().filter(|r| !r.summary.trim().is_empty()).count())
            .sum();
        all as f64 / total_records as f64 * 100.0
    } else {
        0.0
    };

    json!({
        "today_count": today_count,
        "yesterday_count": yesterday_count,
        "themes": today_themes.len(),
        "focus_hours_today": (focus_hours_today * 10.0).round() / 10.0,
        "focus_hours_yesterday": (focus_hours_yesterday * 10.0).round() / 10.0,
        "total_records": total_records,
        "work_days": work_days,
        "avg_hours": (avg_hours * 10.0).round() / 10.0,
        "record_rate": record_rate.round() as i64,
        "daily_counts": daily_counts,
        "distribution": distribution,
        "interval_minutes": interval as u32,
    })
}

/// 今日记录页的日报列表：每天一行（记录数、时长估算、是否已生成/推送）
pub fn day_list() -> Vec<serde_json::Value> {
    let s = config::load_settings();
    let interval = s.interval_minutes.max(1) as f64;
    let last_push = std::fs::read_to_string(config::data_dir().join("last_push.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("ts").and_then(|x| x.as_str()).map(String::from))
        .unwrap_or_default();
    let pushed_date = if last_push.len() >= 10 { last_push[..10].to_string() } else { String::new() };

    records::list_record_dates()
        .iter()
        .map(|d| {
            let recs = records::read_day(d);
            let n = recs.iter().filter(|r| records::is_work(r)).count();
            let hours = (n as f64 * interval / 60.0 * 10.0).round() / 10.0;
            let has_report = config::load_report(d).is_some();
            json!({
                "date": d,
                "count": n,
                "hours": hours,
                "has_report": has_report,
                "pushed": *d == pushed_date,
            })
        })
        .collect()
}
