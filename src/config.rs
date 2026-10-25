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
    Window { op: String }, // minimize | maximize | close
    Script { code: String },
}

impl ActionDef {
    pub fn kind_name(&self) -> &'static str {
        match self {
            ActionDef::RunApp { .. } => "运行程序",
            ActionDef::Command { .. } => "命令行",
            ActionDef::Url { .. } => "打开网址",
            ActionDef::SendKeys { .. } => "发送按键",
            ActionDef::Window { .. } => "窗口控制",
            ActionDef::Script { .. } => "脚本",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            ActionDef::RunApp { path, args } => format!("{} {}", path, args),
            ActionDef::Command { cmd } => cmd.clone(),
            ActionDef::Url { url } => url.clone(),
            ActionDef::SendKeys { keys } => keys.clone(),
            ActionDef::Window { op } => op.clone(),
            ActionDef::Script { code } => {
                let head: String = code.chars().take(24).collect();
                head
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GestureDef {
    pub name: String,
    /// 方向序列，字符取自 U/D/L/R
    pub pattern: String,
    pub enabled: bool,
    pub action: ActionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub trigger: MouseButton,
    pub threshold_px: f32,
    pub gestures: Vec<GestureDef>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            trigger: MouseButton::Right,
            threshold_px: 30.0,
            gestures: vec![
                GestureDef {
                    name: "最小化前台窗口".into(),
                    pattern: "DD".into(),
                    enabled: true,
                    action: ActionDef::Window {
                        op: "minimize".into(),
                    },
                },
                GestureDef {
                    name: "关闭前台窗口".into(),
                    pattern: "DL".into(),
                    enabled: true,
                    action: ActionDef::Window { op: "close".into() },
                },
                GestureDef {
                    name: "打开资源管理器".into(),
                    pattern: "LR".into(),
                    enabled: true,
                    action: ActionDef::RunApp {
                        path: "explorer.exe".into(),
                        args: String::new(),
                    },
                },
                GestureDef {
                    name: "打开百度".into(),
                    pattern: "UD".into(),
                    enabled: true,
                    action: ActionDef::Url {
                        url: "https://www.baidu.com".into(),
                    },
                },
            ],
        }
    }
}

impl Config {
    fn path() -> PathBuf {
        let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
        PathBuf::from(base).join("GestureCraft").join("config.json")
    }

    pub fn load_or_default() -> Config {
        let p = Self::path();
        match std::fs::read_to_string(&p) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(c) => c,
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
