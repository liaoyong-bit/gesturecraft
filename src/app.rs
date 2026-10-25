use crate::config::{
    builtin_presets, system_op_label, window_op_label, ActionDef, Config, GestureDef, MouseButton,
};
use crate::engine;
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
    /// 新建态显式标志（修复"新建按钮点了没反应"）
    creating: bool,
    draft: GestureDef,
    pad_points: Vec<egui::Vec2>,
    /// 测试框状态：最近一次拖动的轨迹与匹配结果
    test_points: Vec<egui::Vec2>,
    test_result: Option<Result<String, String>>,
    status: String,
}

pub fn run_ui(
    config: Arc<RwLock<Config>>,
    logs: Arc<Mutex<Vec<String>>>,
    paused: Arc<AtomicBool>,
    ui_ctx: Arc<std::sync::OnceLock<egui::Context>>,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([980.0, 720.0])
            .with_title("GestureCraft"),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "GestureCraft",
        options,
        Box::new(move |cc| {
            engine::Ctx::set_ui_ctx_once(&ui_ctx, cc.egui_ctx.clone());
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            load_cjk_font(&cc.egui_ctx);
            let draft = default_draft();
            Box::new(UiApp {
                config,
                logs,
                paused,
                tab: Tab::Gestures,
                edit_index: None,
                creating: false,
                draft,
                pad_points: Vec::new(),
                test_points: Vec::new(),
                test_result: None,
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
        action: ActionDef::RunApp { path: String::new(), args: String::new() },
    }
}

impl eframe::App for UiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("nav").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.heading("GestureCraft");
            ui.separator();
            if ui.selectable_label(self.tab == Tab::Gestures, "手势列表").clicked() {
                self.tab = Tab::Gestures;
            }
            if ui.selectable_label(self.tab == Tab::Settings, "设置 / 测试").clicked() {
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

        // 底部日志面板必须先于 CentralPanel 添加，否则被挤成零高度不可见
        egui::TopBottomPanel::bottom("log").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.strong("日志");
            let logs = self.logs.lock().unwrap();
            egui::ScrollArea::vertical()
                .max_height(90.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let start = logs.len().saturating_sub(6);
                    for line in &logs[start..] {
                        ui.label(egui::WidgetText::from(line.as_str()).small());
                    }
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Gestures => self.show_gestures(ui),
            Tab::Settings => self.show_settings(ui),
            Tab::About => self.show_about(ui),
        });
    }
}

impl UiApp {
    // ---------- 手势列表 ----------

    fn show_gestures(&mut self, ui: &mut egui::Ui) {
        ui.heading("已设置手势");
        ui.separator();

        let snapshot: Vec<(String, String, bool, String)> = {
            let cfg = self.config.read().unwrap();
            cfg.gestures
                .iter()
                .map(|gd| {
                    (
                        gd.name.clone(),
                        gesture::arrows(&gd.pattern),
                        gd.enabled,
                        gd.action.summary(),
                    )
                })
                .collect()
        };

        if snapshot.is_empty() {
            ui.colored_label(
                egui::Color32::GRAY,
                "还没有手势。从下方内置手势库一键添加，或点「＋ 新建手势」。",
            );
            ui.add_space(4.0);
        } else {
            egui::Grid::new("gesture_table")
                .num_columns(6)
                .spacing([10.0, 6.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("名称");
                    ui.strong("方向");
                    ui.strong("动作");
                    ui.strong("启用");
                    ui.strong("");
                    ui.end_row();
                    for (i, (name, arrow, enabled, summary)) in snapshot.iter().enumerate() {
                        ui.label(name);
                        ui.monospace(egui::RichText::new(arrow.as_str()).strong());
                        ui.label(summary);
                        let mut en = *enabled;
                        if ui.checkbox(&mut en, "").changed() {
                            self.config.write().unwrap().gestures[i].enabled = en;
                            let _ = self.config.read().unwrap().save();
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("编辑").clicked() {
                                self.edit_index = Some(i);
                                self.creating = false;
                                self.draft = self.config.read().unwrap().gestures[i].clone();
                                self.pad_points.clear();
                            }
                            if ui.small_button("删除").clicked() {
                                self.config.write().unwrap().gestures.remove(i);
                                let _ = self.config.read().unwrap().save();
                                self.status = format!("已删除: {name}");
                            }
                        });
                        ui.end_row();
                    }
                });
        }

        ui.add_space(6.0);
        if ui.button("＋ 新建手势").clicked() {
            self.edit_index = None;
            self.creating = true;
            self.draft = default_draft();
            self.pad_points.clear();
        }

        ui.add_space(10.0);
        ui.heading("内置手势库（点击即添加，可在编辑器里微调）");
        ui.separator();

        let presets = builtin_presets();
        let existing: Vec<(String, String)> = {
            let cfg = self.config.read().unwrap();
            cfg.gestures.iter().map(|gd| (gd.name.clone(), gd.pattern.clone())).collect()
        };
        let mut to_add: Option<GestureDef> = None;
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            egui::Grid::new("preset_table")
                .num_columns(4)
                .spacing([10.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    for p in &presets {
                        let already =
                            existing.iter().any(|(n, pt)| *n == p.name || *pt == p.pattern);
                        ui.label(&p.name);
                        ui.monospace(egui::RichText::new(gesture::arrows(&p.pattern)).strong());
                        ui.label(p.action.summary());
                        if already {
                            ui.colored_label(egui::Color32::GRAY, "已添加");
                        } else if ui.small_button("＋ 添加").clicked() {
                            to_add = Some(p.clone());
                        }
                        ui.end_row();
                    }
                });
        });
        if let Some(p) = to_add {
            self.config.write().unwrap().gestures.push(p.clone());
            let _ = self.config.read().unwrap().save();
            self.status = format!("已添加: {}", p.name);
        }

        // 编辑器（新建或编辑）
        if let Some(idx) = self.edit_index {
            ui.add_space(10.0);
            ui.separator();
            ui.heading(format!("编辑第 {} 项", idx + 1));
            self.show_editor(ui);
        } else if self.creating {
            ui.add_space(10.0);
            ui.separator();
            ui.heading("新建手势");
            self.show_editor(ui);
        }

        if !self.status.is_empty() {
            ui.add_space(4.0);
            ui.colored_label(egui::Color32::from_rgb(30, 120, 30), &self.status);
        }
    }

    fn show_editor(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("editor")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("名称");
                ui.text_edit_singleline(&mut self.draft.name);
                ui.end_row();

                ui.label("动作类型");
                let kinds = [
                    "运行程序", "命令行", "打开网址", "发送按键", "按键组合", "窗口控制",
                    "音量", "系统", "浏览器", "脚本",
                ];
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
                    ActionDef::Command { cmd } => {
                        ui.add(egui::TextEdit::singleline(cmd).hint_text("命令行"));
                    }
                    ActionDef::Url { url } => {
                        ui.add(egui::TextEdit::singleline(url).hint_text("https://..."));
                    }
                    ActionDef::SendKeys { keys } => {
                        ui.add(
                            egui::TextEdit::singleline(keys)
                                .hint_text("如 hello{ENTER}，支持 {TAB} {SLEEP 100}"),
                        );
                    }
                    ActionDef::Hotkey { combo } => {
                        ui.add(
                            egui::TextEdit::singleline(combo)
                                .hint_text("如 ctrl+shift+t、alt+tab、win+d"),
                        );
                    }
                    ActionDef::Window { op } => {
                        egui::ComboBox::from_id_source("wop")
                            .selected_text(window_op_label(op))
                            .show_ui(ui, |ui| {
                                for (v, label) in [
                                    ("minimize", "最小化"),
                                    ("maximize", "最大化"),
                                    ("restore", "还原"),
                                    ("close", "关闭"),
                                    ("topmost", "置顶切换"),
                                    ("snap_left", "左半屏"),
                                    ("snap_right", "右半屏"),
                                ] {
                                    ui.selectable_value(op, v.into(), label);
                                }
                            });
                    }
                    ActionDef::Volume { op } => {
                        egui::ComboBox::from_id_source("vop")
                            .selected_text(op.clone())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(op, "up".into(), "增大");
                                ui.selectable_value(op, "down".into(), "减小");
                                ui.selectable_value(op, "mute".into(), "静音");
                            });
                    }
                    ActionDef::System { op } => {
                        egui::ComboBox::from_id_source("sop")
                            .selected_text(system_op_label(op))
                            .show_ui(ui, |ui| {
                                ui.selectable_value(op, "show_desktop".into(), "显示桌面");
                                ui.selectable_value(op, "task_view".into(), "任务视图");
                                ui.selectable_value(op, "clipboard_history".into(), "剪贴板历史");
                                ui.selectable_value(op, "lock".into(), "锁屏");
                            });
                    }
                    ActionDef::Browser { op } => {
                        egui::ComboBox::from_id_source("bop")
                            .selected_text(op.clone())
                            .show_ui(ui, |ui| {
                                ui.selectable_value(op, "back".into(), "后退");
                                ui.selectable_value(op, "forward".into(), "前进");
                                ui.selectable_value(op, "refresh".into(), "刷新");
                            });
                    }
                    ActionDef::Script { code } => {
                        ui.add_sized([420.0, 90.0], egui::TextEdit::multiline(code));
                    }
                }
                ui.end_row();
            });

        ui.add_space(6.0);
        ui.label("方向序列（点九宫格快录，或在下方手绘板拖画）：");
        ui.horizontal(|ui| {
            // 九宫格方向快选（小键盘方位）
            egui::Grid::new("dirpad").num_columns(3).spacing([6.0, 6.0]).show(ui, |ui| {
                for row in [["7", "8", "9"], ["4", "", "6"], ["1", "2", "3"]] {
                    for cell in row {
                        if cell.is_empty() {
                            ui.label("");
                        } else if ui
                            .button(egui::RichText::new(gesture::arrow_for(cell.chars().next().unwrap())).size(18.0))
                            .clicked()
                        {
                            self.draft.pattern.push_str(cell);
                        }
                    }
                }
                ui.end_row();
            });
            ui.vertical(|ui| {
                ui.monospace(
                    egui::RichText::new(gesture::arrows(&self.draft.pattern))
                        .strong()
                        .size(20.0),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("⌫ 退格").clicked() {
                        self.draft.pattern.pop();
                    }
                    if ui.small_button("清空").clicked() {
                        self.draft.pattern.clear();
                        self.pad_points.clear();
                    }
                });
            });
        });

        // 手绘板
        let (rect, resp) = ui
            .allocate_exact_size(egui::vec2(360.0, 150.0), egui::Sense::click_and_drag());
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
            self.draft.pattern = gesture::sequence_from_f32(&pts, 25.0);
        }
        if !self.pad_points.is_empty() {
            let stroke = egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(30, 100, 220));
            painter.add(egui::Shape::line(
                self.pad_points.iter().map(|p| rect.min + *p).collect(),
                stroke,
            ));
        }

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
                    self.creating = false;
                    self.draft = default_draft();
                    self.pad_points.clear();
                    self.status = "已保存".into();
                }
            }
            if ui.button("取消").clicked() {
                self.edit_index = None;
                self.creating = false;
                self.draft = default_draft();
                self.pad_points.clear();
            }
        });
    }

    // ---------- 设置 / 测试 ----------

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        ui.separator();

        let (mut trigger, mut th, mut fuzzy) = {
            let cfg = self.config.read().unwrap();
            (cfg.trigger, cfg.threshold_px, cfg.fuzzy_tolerance)
        };
        ui.horizontal(|ui| {
            ui.label("触发键");
            if ui.radio(trigger == MouseButton::Right, "鼠标右键").clicked() {
                trigger = MouseButton::Right;
            }
            if ui.radio(trigger == MouseButton::Middle, "鼠标中键").clicked() {
                trigger = MouseButton::Middle;
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!("起手势位移阈值: {:.0} px", th));
            ui.add(egui::Slider::new(&mut th, 10.0..=100.0));
        });
        ui.horizontal(|ui| {
            ui.label("模糊匹配容差");
            ui.add(egui::Slider::new(&mut fuzzy, 0..=2u32));
            ui.label(match fuzzy {
                0 => "精确匹配".to_string(),
                1 => "宽松（推荐）：允许歪 45°、多/少拐一段".to_string(),
                _ => "最宽松：单方向手势也会近似命中，易误触".to_string(),
            });
        });
        if ui.button("保存设置").clicked() {
            let mut cfg = self.config.write().unwrap();
            cfg.trigger = trigger;
            cfg.threshold_px = th;
            cfg.fuzzy_tolerance = fuzzy;
            match cfg.save() {
                Ok(_) => self.status = "设置已保存".into(),
                Err(e) => self.status = format!("保存失败: {e}"),
            }
        }

        ui.add_space(10.0);
        ui.heading("手势测试（框内按住左键拖动，松开即检测已启用手势）");
        ui.separator();

        let (rect, resp) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().min(560.0), 220.0),
            egui::Sense::click_and_drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, egui::Color32::from_rgb(248, 249, 252));
        painter.rect_stroke(rect, 8.0, (2.0, egui::Color32::from_rgb(160, 170, 190)));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            "在此区域内按住左键拖出手势",
            egui::TextStyle::Heading.resolve(ui.style()),
            egui::Color32::from_rgb(200, 204, 214),
        );
        if resp.dragged() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.test_points.push(p - rect.min);
            }
        }
        if resp.drag_stopped() {
            let pts: Vec<(f32, f32)> = self.test_points.iter().map(|v| (v.x, v.y)).collect();
            let seq = gesture::sequence_from_f32(&pts, 25.0);
            if seq.is_empty() {
                self.test_result = Some(Err("位移太小，没识别出方向".into()));
            } else {
                let hit: Option<(String, String, usize)> = {
                    let cfg = self.config.read().unwrap();
                    let (best, _) =
                        engine::match_gestures(&seq, &cfg.gestures, cfg.fuzzy_tolerance);
                    best.map(|(gd, d)| {
                        (
                            gd.name.clone(),
                            gesture::arrows(&gd.pattern),
                            d,
                        )
                    })
                };
                self.test_result = Some(match hit {
                    Some((name, arrow, d)) => Ok(format!("✓ 测试成功：「{name}」{arrow}（差距 {d}）")),
                    None => Err(format!("✗ 未匹配，画出：{}", gesture::arrows(&seq))),
                });
            }
        }
        if !self.test_points.is_empty() {
            let stroke = egui::Stroke::new(3.0_f32, egui::Color32::from_rgb(30, 100, 220));
            painter.add(egui::Shape::line(
                self.test_points.iter().map(|p| rect.min + *p).collect(),
                stroke,
            ));
        }
        if let Some(res) = &self.test_result {
            ui.add_space(4.0);
            match res {
                Ok(msg) => ui.colored_label(
                    egui::Color32::from_rgb(20, 130, 40),
                    egui::RichText::new(msg).strong(),
                ),
                Err(msg) => ui.colored_label(egui::Color32::from_rgb(180, 40, 40), msg),
            };
        }
    }

    fn show_about(&mut self, ui: &mut egui::Ui) {
        ui.heading("GestureCraft v0.3.0");
        ui.separator();
        ui.label("原创 Rust 实现的 Windows 全局鼠标手势工具。");
        ui.add_space(4.0);
        ui.label("· 8 方向手势（↑↓←→↖↗↙↘），连贯一笔自动拆段");
        ui.label("· 模糊匹配：歪一点、多拐少拐一档也能命中，容差可调");
        ui.label("· 内置手势库一键添加，编辑器九宫格快录");
        ui.label("· 普通右键点击不受影响（延迟触发，无菜单异常）");
        ui.add_space(6.0);
        ui.label("功能概念参考了 WGestures 2 的产品形态，全部代码为原创实现。");
        ui.label("License: MIT");
    }
}

fn make_action(kind_index: usize) -> ActionDef {
    match kind_index {
        0 => ActionDef::RunApp { path: String::new(), args: String::new() },
        1 => ActionDef::Command { cmd: String::new() },
        2 => ActionDef::Url { url: String::new() },
        3 => ActionDef::SendKeys { keys: String::new() },
        4 => ActionDef::Hotkey { combo: String::new() },
        5 => ActionDef::Window { op: "minimize".into() },
        6 => ActionDef::Volume { op: "up".into() },
        7 => ActionDef::System { op: "show_desktop".into() },
        8 => ActionDef::Browser { op: "back".into() },
        _ => ActionDef::Script { code: String::new() },
    }
}
