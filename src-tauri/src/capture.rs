use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use tauri::{AppHandle, Emitter};

use crate::analyze;
use crate::config;
use crate::records;

/// 截图 → 缩放 JPEG → base64 data URL（1600px 上限 + JPEG 压缩，控制请求体大小）
pub fn screenshot_data_url() -> Result<String, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("枚举显示器失败: {e}"))?;
    if monitors.is_empty() {
        return Err("未找到显示器".into());
    }
    // 只用固有方法（width/height/as_raw），不依赖 image crate 版本一致性
    let img = monitors[0].capture_image().map_err(|e| format!("截屏失败: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let raw = img.as_raw().clone();
    let rgba = image::RgbaImage::from_raw(w, h, raw).ok_or("图像数据不完整")?;
    let max_side = 1600u32;
    let scale = if w.max(h) > max_side { max_side as f32 / w.max(h) as f32 } else { 1.0 };
    let nw = (w as f32 * scale) as u32;
    let nh = (h as f32 * scale) as u32;
    let resized = image::DynamicImage::ImageRgba8(rgba).resize(nw, nh, image::imageops::FilterType::Lanczos3);
    let mut buf: Vec<u8> = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    resized
        .write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("编码 JPEG 失败: {e}"))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&buf)
    ))
}

/// 32x32 灰度签名（画面去重：像素差阈值判断画面是否变化）
fn gray_signature(raw: &[u8], w: u32, h: u32) -> Vec<u8> {
    let Some(rgba) = image::RgbaImage::from_raw(w, h, raw.to_vec()) else {
        return Vec::new();
    };
    image::DynamicImage::ImageRgba8(rgba)
        .resize_exact(32, 32, image::imageops::FilterType::Triangle)
        .to_luma8()
        .into_raw()
}

fn sig_diff(a: &[u8], b: &[u8]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return f32::MAX;
    }
    let sum: i64 = a.iter().zip(b.iter()).map(|(x, y)| (*x as i64 - *y as i64).abs()).sum();
    sum as f32 / a.len() as f32
}

pub struct CaptureState {
    pub running: AtomicBool,
    pub last_sig: Mutex<Option<Vec<u8>>>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self { running: AtomicBool::new(false), last_sig: Mutex::new(None) }
    }
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
    pub fn set_running(&self, v: bool) {
        self.running.store(v, Ordering::SeqCst);
    }
}

pub fn start_loop(app: AppHandle, state: Arc<CaptureState>) {
    std::thread::spawn(move || loop {
        if !state.is_running() {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let s = config::load_settings();
        match run_once(&state) {
            Ok(Some(rec)) => {
                let date = records::today_str();
                if !rec.summary.trim().is_empty() && rec.summary != "无工作活动" {
                    // 只落库值得汇报的内容（reportable + 排除关键词）
                    let kept = records::append_if_reportable(&rec, &date);
                    if !kept {
                        let _ = app.emit("record-skipped", rec.summary.clone());
                    }
                }
                let _ = app.emit("record-updated", ());
            }
            Ok(None) => {}
            Err(e) => {
                config::log_line(&format!("采集/分析失败：{e}"));
                let _ = app.emit("capture-error", e.clone());
            }
        }
        std::thread::sleep(Duration::from_secs(s.interval_minutes.max(1) * 60));
    });
}

/// 单次采集：截屏 → 去重 → 分析。Ok(None) 表示画面未变化被跳过。
fn run_once(state: &CaptureState) -> Result<Option<records::Record>, String> {
    let monitors = xcap::Monitor::all().map_err(|e| format!("枚举显示器失败: {e}"))?;
    let img = monitors
        .first()
        .ok_or("未找到显示器")?
        .capture_image()
        .map_err(|e| format!("截屏失败: {e}"))?;
    let (w, h) = (img.width(), img.height());
    let raw = img.as_raw();
    let sig = gray_signature(raw, w, h);
    {
        let mut last = state.last_sig.lock().unwrap();
        if let Some(prev) = last.as_ref() {
            if sig_diff(prev, &sig) < 1.5 {
                return Ok(None);
            }
        }
        *last = Some(sig);
    }
    let data_url = screenshot_data_url()?;
    match analyze::analyze_image(&data_url) {
        Ok(rec) => Ok(Some(rec)),
        Err(e) => Err(e),
    }
}
