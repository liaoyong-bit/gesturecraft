//! GestureCraft —— 原创实现的 Windows 全局鼠标手势工具。
//! 功能概念参考了 WGestures 2 的产品形态，全部代码为本项目原创，未使用其任何代码或资源。

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod action;
mod app;
mod config;
mod engine;
mod gesture;
mod hook;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

fn main() {
    let config = Arc::new(RwLock::new(config::Config::load_or_default()));
    let logs: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let paused = Arc::new(AtomicBool::new(false));
    let ui_ctx: Arc<std::sync::OnceLock<eframe::egui::Context>> = Arc::new(Default::default());
    let toast: Arc<Mutex<Option<(String, std::time::Instant)>>> = Arc::new(Mutex::new(None));

    #[cfg(windows)]
    hook::start_hook_thread(engine::Ctx {
        config: config.clone(),
        logs: logs.clone(),
        paused: paused.clone(),
        ui_ctx: ui_ctx.clone(),
        toast: toast.clone(),
    });

    app::run_ui(config, logs, paused, ui_ctx, toast);
}
