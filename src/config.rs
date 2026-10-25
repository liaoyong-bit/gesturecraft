use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MouseButton {
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActionDef {
    RunApp { path: String, args: String },
    Command { cmd: String },
    Url { url: String },
    SendKeys { keys: String },
    /// 按键组合，写法如 "ctrl+shift+t"、"alt+tab"、"win+d"
    Hotkey { combo: String },
    /// 窗口：minimize | maximize | restore | close | topmost | snap_left | snap_right
    Window { op: String },
    /// 音量：up | down | mute
    Volume { op: String },
    /// 系统：show_desktop | task_view | clipboard_history | lock
    System { op: String },
    /// 浏览器全局键：back | forward | refresh
    Browser { op: String },
    Script { code: String },
}

impl ActionDef {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ActionDef::RunApp { .. } => "运行程序",
            ActionDef::Command { .. } => "命令行",
            ActionDef::Url { .. } => "打开网址",
            ActionDef::SendKeys { .. } => "发送按键",
            ActionDef::Hotkey { .. } => "按键组合",
            ActionDef::Window { .. } => "窗口控制",
            ActionDef::Volume { .. } => "音量",
            ActionDef::System { .. } => "系统",
            ActionDef::Browser { .. } => "浏览器",
            ActionDef::Script { .. } => "脚本",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            ActionDef::RunApp { path, args } => {
                if args.is_empty() { path.clone() } else { format!("{path} {args}") }
            }
            ActionDef::Command { cmd } => cmd.clone(),
            ActionDef::Url { url } => url.clone(),
            ActionDef::SendKeys { keys } => format!("发送 {keys}"),
            ActionDef::Hotkey { combo } => combo.clone(),
            ActionDef::Window { op } => format!("窗口·{}", window_op_label(op)),
            ActionDef::Volume { op } => format!("音量·{op}"),
            ActionDef::System { op } => format!("系统·{}", system_op_label(op)),
            ActionDef::Browser { op } => format!("浏览器·{op}"),
            ActionDef::Script { code } => code.chars().take(20).collect(),
        }
    }
}

pub fn window_op_label(op: &str) -> &str {
    match op {
        "minimize" => "最小化",
        "maximize" => "最大化",
        "restore" => "还原",
        "close" => "关闭",
        "topmost" => "置顶切换",
        "snap_left" => "左半屏",
        "snap_right" => "右半屏",
        other => other,
    }
}

pub fn system_op_label(op: &str) -> &str {
    match op {
        "show_desktop" => "显示桌面",
        "task_view" => "任务视图",
        "clipboard_history" => "剪贴板历史",
        "lock" => "锁屏",
        other => other,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureDef {
    pub name: String,
    /// 方向序列，字符取自 8 方向小键盘编码：8↑ 2↓ 4← 6→ 7↖ 9↗ 1↙ 3↘
    pub pattern: String,
    pub enabled: bool,
    pub action: ActionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub trigger: MouseButton,
    pub threshold_px: f32,
    /// 模糊匹配容差：允许与录入序列的最大编辑距离（0=精确）
    #[serde(default = "default_fuzzy")]
    pub fuzzy_tolerance: u32,
    pub gestures: Vec<GestureDef>,
}

fn default_fuzzy() -> u32 {
    1
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: MouseButton::Right,
            threshold_px: 30.0,
            fuzzy_tolerance: default_fuzzy(),
            gestures: Vec::new(),
        }
    }
}

/// 内置手势库（参照 WGestures/MouseInc/StrokesPlus 的常见动作预设），
/// 一键添加，用户可在编辑器里对方向和动作做微调。
pub fn builtin_presets() -> Vec<GestureDef> {
    use ActionDef::*;
    vec![
        g("最小化前台窗口", "2", Window { op: "minimize".into() }),
        g("最大化前台窗口", "8", Window { op: "maximize".into() }),
        g("关闭前台窗口", "42", Window { op: "close".into() }),
        g("窗口置顶切换", "48", Window { op: "topmost".into() }),
        g("左半屏贴靠", "4", Window { op: "snap_left".into() }),
        g("右半屏贴靠", "6", Window { op: "snap_right".into() }),
        g("浏览器后退", "78", Browser { op: "back".into() }),
        g("浏览器前进", "96", Browser { op: "forward".into() }),
        g("浏览器刷新", "636", Browser { op: "refresh".into() }),
        g("关闭标签页", "262", Hotkey { combo: "ctrl+w".into() }),
        g("恢复关闭的标签", "232", Hotkey { combo: "ctrl+shift+t".into() }),
        g("切换窗口", "242", Hotkey { combo: "alt+tab".into() }),
        g("显示桌面", "828", System { op: "show_desktop".into() }),
        g("任务视图", "898", System { op: "task_view".into() }),
        g("剪贴板历史", "878", System { op: "clipboard_history".into() }),
        g("锁屏", "212", System { op: "lock".into() }),
        g("音量静音", "121", Volume { op: "mute".into() }),
        g("打开资源管理器", "46", RunApp { path: "explorer.exe".into(), args: String::new() }),
        g("打开记事本", "63", RunApp { path: "notepad.exe".into(), args: String::new() }),
        g("打开百度", "82", Url { url: "https://www.baidu.com".into() }),
    ]
}

fn g(name: &str, pattern: &str, action: ActionDef) -> GestureDef {
    GestureDef {
        name: name.into(),
        pattern: pattern.into(),
        enabled: true,
        action,
    }
}

/// 旧版 UDLR 序列 → 8 方向数字编码，并折叠相邻重复（旧版 "DD" 不可达，归一为 "2"）
fn migrate_pattern(p: &str) -> String {
    let mut out = String::new();
    for c in p.chars() {
        let ch = match c {
            'U' | 'u' => '8',
            'D' | 'd' => '2',
            'L' | 'l' => '4',
            'R' | 'r' => '6',
            other => other,
        };
        if out.chars().last() != Some(ch) {
            out.push(ch);
        }
    }
    out
}

impl Config {
    fn path() -> PathBuf {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("GestureCraft").join("config.json")
    }

    pub fn load_or_default() -> Config {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(s) => match serde_json::from_str::<Config>(&s) {
                Ok(mut c) => {
                    for gd in &mut c.gestures {
                        gd.pattern = migrate_pattern(&gd.pattern);
                    }
                    c
                }
                Err(e) => {
                    eprintln!("配置解析失败，使用默认配置: {e}");
                    Config::default()
                }
            },
            Err(_) => {
                let c = Config::default();
                let _ = c.save();
                c
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(p, serde_json::to_string_pretty(self).unwrap())
    }
}
