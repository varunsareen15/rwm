use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub bindings: HashMap<String, String>,
    #[serde(default)]
    pub bar: BarConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BarConfig {
    pub font: String,
    // pub font_size: u16,
    pub workspace_style: String,
    pub workspace_icons: Vec<String>,
    #[serde(default)]
    pub modules: Vec<BarModule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BarModule {
    pub command: String,
    pub interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        // Default Keybinds
        bindings.insert("Mod+Return".to_string(), "Spawn kitty".to_string());
        bindings.insert("Mod+p".to_string(), "Spawn dmenu_run".to_string());
        bindings.insert("Mod+Shift+q".to_string(), "KillFocused".to_string());
        bindings.insert("Mod+Control+q".to_string(), "Quit".to_string());
        bindings.insert("Mod+j".to_string(), "FocusNext".to_string());
        bindings.insert("Mod+k".to_string(), "FocusPrev".to_string());
        bindings.insert("Mod+Shift+j".to_string(), "MoveWindowNext".to_string());
        bindings.insert("Mod+Shift+k".to_string(), "MoveWindowPrev".to_string());
        bindings.insert("Mod+Space".to_string(), "CycleLayout".to_string());
        bindings.insert("Mod+b".to_string(), "ToggleBar".to_string());
        bindings.insert("Mod+minus".to_string(), "SplitHorizontal".to_string());
        bindings.insert(
            "Mod+Shift+backslash".to_string(),
            "SplitVertical".to_string(),
        );
        bindings.insert("Mod+Shift+Return".to_string(), "PromoteMaster".to_string());

        // Workspaces 1-9
        for i in 1..=9 {
            bindings.insert(format!("Mod+{}", i), format!("Workspace {}", i));
            bindings.insert(format!("Mod+Shift+{}", i), format!("MoveToWorkspace {}", i));
        }

        Self {
            bindings,
            bar: BarConfig::default(),
        }
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            font: "6x13".to_string(), // Fallback
            //font_size: 13,
            workspace_style: "Numbers".to_string(),
            workspace_icons: vec![
                "1".to_string(),
                "2".to_string(),
                "3".to_string(),
                "4".to_string(),
                "5".to_string(),
                "6".to_string(),
                "7".to_string(),
                "8".to_string(),
                "9".to_string(),
            ],
            modules: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut config = Self::default();

        let config_path = dirs::config_dir()
            .map(|p| p.join("rwm").join("rwm.toml"))
            .unwrap_or_else(|| PathBuf::from("rwm.toml"));

        if config_path.exists() {
            let content = fs::read_to_string(&config_path).unwrap_or_default();
            match toml::from_str::<Config>(&content) {
                Ok(cfg) => {
                    for (key, value) in cfg.bindings {
                        config.bindings.insert(key, value);
                    }
                    config.bar = cfg.bar;
                    log::info!("Loaded config grom {:?}", config_path);
                }

                Err(e) => log::error!("Failed to parse config: {}", e),
            }
        } else {
            log::info!("Config not found at {:?}, using defaults", config_path);
        }
        config
    }
}
