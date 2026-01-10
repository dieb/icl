use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[allow(dead_code)]
    pub command: String,
    #[allow(dead_code)]
    #[serde(default)]
    pub description: String,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub presets: Vec<Preset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Preset {
    pub label: String,
    pub flags: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Step {
    pub id: String,
    pub prompt: String,
    #[serde(rename = "type")]
    pub step_type: StepType,
    #[serde(default)]
    pub options: Vec<StepOption>,
    #[serde(default)]
    pub flag: Option<String>,
    #[serde(default)]
    pub default: Option<usize>,
    #[serde(default)]
    pub when: Option<HashMap<String, String>>,
    #[serde(default)]
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepType {
    Choice,
    Toggle,
    Text,
    Multi,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepOption {
    pub label: String,
    #[serde(default)]
    pub flag: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Answer {
    Choice(usize),
    Toggle(bool),
    Text(String),
    Multi(Vec<usize>),
}

impl Config {
    pub fn load(command: &[String]) -> Result<Config, ConfigError> {
        let config_name = command.join("-");

        // Try loading from each location in order
        let paths = config_paths(&config_name);

        for path in &paths {
            if path.exists() {
                let content = fs::read_to_string(path)
                    .map_err(|e| ConfigError::ReadError(path.clone(), e.to_string()))?;
                let config: Config = serde_json::from_str(&content)
                    .map_err(|e| ConfigError::ParseError(path.clone(), e.to_string()))?;
                return Ok(config);
            }
        }

        Err(ConfigError::NotFound(config_name, paths))
    }
}

fn config_paths(name: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. Project-local: ./.i/<name>.json
    paths.push(PathBuf::from(format!(".i/{}.json", name)));

    // 2. User config: ~/.config/i/<name>.json
    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("i").join(format!("{}.json", name)));
    }

    // 3. Bundled: could be embedded, but for now use a data dir
    // For development, check relative to executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            paths.push(exe_dir.join("i").join(format!("{}.json", name)));
        }
    }

    paths
}

#[derive(Debug)]
pub enum ConfigError {
    NotFound(String, Vec<PathBuf>),
    ReadError(PathBuf, String),
    ParseError(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NotFound(name, paths) => {
                writeln!(f, "No configuration found for '{}'", name)?;
                writeln!(f)?;
                writeln!(f, "Create a config file at one of:")?;
                for path in paths {
                    writeln!(f, "  {}", path.display())?;
                }
                writeln!(f)?;
                write!(f, "See: https://github.com/user/i#creating-configs")
            }
            ConfigError::ReadError(path, err) => {
                write!(f, "Failed to read {}: {}", path.display(), err)
            }
            ConfigError::ParseError(path, err) => {
                write!(f, "Failed to parse {}: {}", path.display(), err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let json = r#"{
            "command": "ls",
            "description": "List files",
            "steps": [
                {
                    "id": "format",
                    "prompt": "How to display?",
                    "type": "choice",
                    "options": [
                        { "label": "List", "flag": "-l" },
                        { "label": "Grid", "flag": null }
                    ]
                },
                {
                    "id": "hidden",
                    "prompt": "Show hidden?",
                    "type": "toggle",
                    "flag": "-a"
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.command, "ls");
        assert_eq!(config.steps.len(), 2);
        assert_eq!(config.steps[0].step_type, StepType::Choice);
        assert_eq!(config.steps[1].step_type, StepType::Toggle);
    }
}
