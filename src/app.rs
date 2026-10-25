use crate::config::{ActionDef, Config, GestureDef, MouseButton};
use crate::gesture;
use eframe::egui;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Gestures,
    Settings,
    About,
}

pub struct UiApp {
    config: Arc<RwLock<Config>>,
    logs: Arc<Mutex<Vec<String>>>,
    paused: Arc<AtomicBool>,
    tab: Tab,
    /// 正在编辑的手势在 config.gestures 中的下标；None 表示新建
    edit_index: Option<usize>,
    draft: GestureDef,
    pad_points: Vec<egui::Vec2>,
    status: String,
}

pub fn run_ui(
    config: Arc<RwLock<Config>>,
    logs: Arc<Mutex<Vec<String>>>,
    paused: Arc<AtomicBool>,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([920.0, 640.0])
            .with_title("GestureCraft"),
        ..Default::default()
    };
    eframe::run_native(
        "GestureCraft",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            load_cjk_font(&cc.egui_ctx);
            let draft = default_draft();
            Box::new(UiApp {
                config,
                logs,
                paused,
                tab: Tab::Gestures,
                edit_index: None,
                draft,
                pad_points: Vec::new(),
                status: String::new(),
            })
        }),
    );
}

fn load_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();
    for path in candidates {
        if let Ok(data) = std::fs::read(path) {
            let name = "cjk".to_string();
            fonts.font_data.insert(name.clone(), egui::FontData::from_owned(data));
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, name);
            break;
        }
    }
    ctx.set_fonts(fonts);
}

fn default_draft() -> GestureDef {
    GestureDef {
        name: "新手势".into(),
        pattern: String::new(),
        enabled: true,
        action: ActionDef::RunApp {
            path: String::new(),
            args: String::new(),
        },
    }
}

impl eframe::App for UiApp {
    fn on_close_event(&mut self) -> bool {
        let _ = self.config.read().unwrap().save();
        true
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("nav").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("GestureCraft");
            ui.separator();
            if ui.selectable_label(self.tab == Tab::Gestures, "手势列表").clicked() {
                self.tab = Tab::Gestures;
            }
            if ui.selectable_label(self.tab == Tab::Settings, "设置").clicked() {
                self.tab = Tab::Settings;
            }
            if ui.selectable_label(self.tab == Tab::About, "关于").clicked() {
                self.tab = Tab::About;
            }
            ui.separator();
            let paused = self.paused.load(Ordering::Relaxed);
            let label = if paused { "▶ 恢复监听" } else { "⏸ 暂停监听" };
            if ui.button(label).clicked() {
                self.paused.store(!paused, Ordering::Relaxed);
            }
            if paused {
                ui.colored_label(egui::Color32::from_rgb(200, 80, 0), "已暂停");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Gestures => self.show_gestures(ui),
            Tab::Settings => self.show_settings(ui),
            Tab::About => self.show_about(ui),
        });

        // 底部日志条
        egui::TopBottomPanel::bottom("log").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.strong("日志");
                let logs = self.logs.lock().unwrap();
                if let Some(last) = logs.last() {
                    ui.label(egui::WidgetText::from(last.as_str()).small());
                }
            });
        });
    }
}

impl UiApp {
    fn show_gestures(&mut self, ui: &mut egui::Ui) {
        ui.heading("手势列表");
        ui.separator();

        let names: Vec<(String, String, bool, bool)> = {
            let cfg = self.config.read().unwrap();
            cfg.gestures
                .iter()
                .map(|g| (g.name.clone(), g.pattern.clone(), g.enabled, true))
                .collect()
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, (name, pattern, enabled, _)) in names.iter().enumerate() {
                ui.horizontal(|ui| {
                    let mut en = *enabled;
                    if ui.checkbox(&mut en, "").changed() {
                        self.config.write().unwrap().gestures[i].enabled = en;
                    }
                    ui.strong(name);
                    ui.monospace(pattern);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("删除").clicked() {
                            self.config.write().unwrap().gestures.remove(i);
                            self.status = format!("已删除: {name}");
                            return;
                        }
                        if ui.small_button("编辑").clicked() {
                            self.edit_index = Some(i);
                            self.draft = self.config.read().unwrap().gestures[i].clone();
                            self.pad_points.clear();
                        }
                    });
                });
                ui.separator();
            }
        });

        ui.add_space(4.0);
        if ui.button("＋ 新建手势").clicked() {
            self.edit_index = None;
            self.draft = default_draft();
            self.pad_points.clear();
        }

        if let Some(idx) = self.edit_index {
            ui.add_space(8.0);
            ui.separator();
            ui.heading(format!("编辑第 {} 项", idx + 1));
        } else if self.draft.name != "新手势" || !self.draft.pattern.is_empty() {
            ui.add_space(8.0);
            ui.separator();
            ui.heading("新建手势");
        } else {
            return;
        }

        self.show_editor(ui);
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("editor").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("名称");
            ui.text_edit_singleline(&mut self.draft.name);
            ui.end_row();

            ui.label("动作类型");
            let kinds = ["运行程序", "命令行", "打开网址", "发送按键", "窗口控制", "脚本"];
            let current = kinds
                .iter()
                .position(|k| *k == self.draft.action.kind_name())
                .unwrap_or(0);
            egui::ComboBox::from_id_source("kind")
                .selected_text(self.draft.action.kind_name())
                .show_ui(ui, |ui| {
                    for (i, k) in kinds.iter().enumerate() {
                        if ui.selectable_label(i == current, *k).clicked() {
                            self.draft.action = make_action(i);
                        }
                    }
                });
            ui.end_row();

            ui.label("动作参数");
            match &mut self.draft.action {
                ActionDef::RunApp { path, args } => {
                    ui.vertical(|ui| {
                        ui.add(egui::TextEdit::singleline(path).hint_text("程序路径"));
                        ui.add(egui::TextEdit::singleline(args).hint_text("参数（空格分隔）"));
                    });
                }
                ActionDef::Command { cmd }
                | ActionDef::Url { url: cmd } => {
                    ui.add(
                        egui::TextEdit::singleline(cmd)
                            .hint_text(if matches!(self.draft.action, ActionDef::Command { .. }) { "命令行" } else { "https://..." }),
                    );
                }
                ActionDef::SendKeys { keys } => {
                    ui.add(
                        egui::TextEdit::singleline(keys)
                            .hint_text("如 hello{ENTER}，支持 {TAB} {SLEEP 100}"),
                    );
                }
                ActionDef::Window { op } => {
                    egui::ComboBox::from_id_source("wop")
                        .selected_text(op.clone())
                        .show_ui(ui, |ui| {
                            ui.selectable_value(op, "minimize".into(), "最小化");
                            ui.selectable_value(op, "maximize".into(), "最大化");
                            ui.selectable_value(op, "close".into(), "关闭");
                        });
                }
                ActionDef::Script { code } => {
                    ui.add_sized([420.0, 90.0], egui::TextEdit::multiline(code));
                }
            }
            ui.end_row();
        });

        ui.add_space(4.0);
        ui.label("在手势板上拖动画出轨迹（方向序列 U/D/L/R）：");
        let (rect, resp) = ui
            .allocate_exact_size(egui::vec2(360.0, 200.0), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 6.0, egui::Color32::from_rgb(245, 245, 245));
        painter.rect_stroke(rect, 6.0, (1.0, egui::Color32::GRAY));
        if resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.pad_points.push(p - rect.min);
            }
        }
        if resp.drag_stopped() {
            let pts: Vec<(f32, f32)> = self.pad_points.iter().map(|v| (v.x, v.y)).collect();
            self.draft.pattern = gesture::sequence_from_f32(&pts, 30.0);
        }
        if !self.pad_points.is_empty() {
            let stroke = egui::Stroke::new(3.0, egui::Color32::from_rgb(30, 100, 220));
            painter.add(egui::Shape::line(
                self.pad_points.iter().map(|p| rect.min + *p).collect(),
                stroke,
            ));
        }

        ui.horizontal(|ui| {
            ui.label("方向序列:");
            ui.monospace(egui::RichText::new(self.draft.pattern.clone()).strong());
            if ui.small_button("手输/清空").clicked() {
                self.pad_points.clear();
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("保存").clicked() {
                if self.draft.pattern.is_empty() {
                    self.status = "方向序列不能为空".into();
                } else if self.draft.name.trim().is_empty() {
                    self.status = "名称不能为空".into();
                } else {
                    let mut cfg = self.config.write().unwrap();
                    match self.edit_index {
                        Some(i) if i < cfg.gestures.len() => cfg.gestures[i] = self.draft.clone(),
                        _ => cfg.gestures.push(self.draft.clone()),
                    }
                    let _ = cfg.save();
                    drop(cfg);
                    self.edit_index = None;
                    self.draft = default_draft();
                    self.pad_points.clear();
                    self.status = "已保存".into();
                }
            }
            if ui.button("取消").clicked() {
                self.edit_index = None;
                self.draft = default_draft();
                self.pad_points.clear();
            }
            if !self.status.is_empty() {
                ui.label(&self.status);
            }
        });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();
        let mut trigger = self.config.read().unwrap().trigger;
        ui.horizontal(|ui| {
            ui.label("触发键");
            if ui.radio(trigger == MouseButton::Right, "鼠标右键").clicked() {
                trigger = MouseButton::Right;
                self.config.write().unwrap().trigger = trigger;
            }
            if ui.radio(trigger == MouseButton::Middle, "鼠标中键").clicked() {
                trigger = MouseButton::Middle;
                self.config.write().unwrap().trigger = trigger;
            }
        });
        let mut th = self.config.read().unwrap().threshold_px;
        ui.horizontal(|ui| {
            ui.label(format!("起手势位移阈值: {:.0} px", th));
            if ui.add(egui::Slider::new(&mut th, 10.0..=100.0)).changed() {
                self.config.write().unwrap().threshold_px = th;
            }
        });
        if ui.button("保存设置").clicked() {
            match self.config.write().unwrap().save() {
                Ok(_) => self.status = "设置已保存".into(),
                Err(e) => self.status = format!("保存失败: {e}"),
            }
        }
        ui.add_space(10.0);
        ui.label("提示：暂停监听按钮在左侧栏。拖动开始后超过阈值即进入手势模式，松开触发动作并吞掉本次点击。");
    }

    fn show_about(&mut self, ui: &mut egui::Ui) {
        ui.heading("GestureCraft v0.1.0");
        ui.separator();
        ui.label("原创 Rust 实现的全局鼠标手势工具。");
        ui.add_space(6.0);
        ui.label("功能概念参考了 WGestures 2 的产品形态，但全部代码为原创实现，未使用其任何代码或资源。");
        ui.label("License: MIT");
        ui.add_space(6.0);
        ui.strong("路线图");
        ui.label("· 屏幕边缘/角落触发  · 轨迹悬浮窗  · 托盘图标  · 开机自启  · Lua 脚本");
    }
}

fn make_action(kind_index: usize) -> ActionDef {
    match kind_index {
        0 => ActionDef::RunApp { path: String::new(), args: String::new() },
        1 => ActionDef::Command { cmd: String::new() },
        2 => ActionDef::Url { url: String::new() },
        3 => ActionDef::SendKeys { keys: String::new() },
        4 => ActionDef::Window { op: "minimize".into() },
        _ => ActionDef::Script { code: String::new() },
    }
}
