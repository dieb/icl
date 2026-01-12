use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(rename = "command")]
    pub _command: String,
    #[serde(default, rename = "description")]
    pub _description: String,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub presets: Vec<Preset>,
    #[serde(default)]
    pub placeholder_options: HashMap<String, String>,
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
    #[serde(default)]
    pub chain: Option<String>,
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

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_config() {
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
        assert_eq!(config._command, "ls");
        assert_eq!(config._description, "List files");
        assert_eq!(config.steps.len(), 2);
        assert_eq!(config.steps[0].step_type, StepType::Choice);
        assert_eq!(config.steps[1].step_type, StepType::Toggle);
    }

    #[test]
    fn test_parse_config_with_presets() {
        let json = r#"{
            "command": "docker",
            "steps": [],
            "presets": [
                { "label": "Running containers", "flags": "ps" },
                { "label": "All containers", "flags": "ps -a" }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.presets.len(), 2);
        assert_eq!(config.presets[0].label, "Running containers");
        assert_eq!(config.presets[0].flags, "ps");
        assert_eq!(config.presets[1].label, "All containers");
        assert_eq!(config.presets[1].flags, "ps -a");
    }

    #[test]
    fn test_parse_config_with_placeholder_options() {
        let json = r#"{
            "command": "docker logs",
            "steps": [],
            "placeholder_options": {
                "<container>": "docker ps --format '{{.Names}}\t{{.ID}}'"
            }
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.placeholder_options.len(), 1);
        assert!(config.placeholder_options.contains_key("<container>"));
    }

    #[test]
    fn test_parse_text_step() {
        let json = r#"{
            "command": "git",
            "steps": [
                {
                    "id": "message",
                    "prompt": "Commit message:",
                    "type": "text",
                    "flag": "-m",
                    "placeholder": "Enter your commit message"
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.steps[0].step_type, StepType::Text);
        assert_eq!(config.steps[0].flag, Some("-m".to_string()));
        assert_eq!(
            config.steps[0].placeholder,
            Some("Enter your commit message".to_string())
        );
    }

    #[test]
    fn test_parse_multi_step() {
        let json = r#"{
            "command": "ls",
            "steps": [
                {
                    "id": "options",
                    "prompt": "Select options:",
                    "type": "multi",
                    "options": [
                        { "label": "Long format", "flag": "-l" },
                        { "label": "All files", "flag": "-a" },
                        { "label": "Human readable", "flag": "-h" }
                    ]
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.steps[0].step_type, StepType::Multi);
        assert_eq!(config.steps[0].options.len(), 3);
    }

    #[test]
    fn test_parse_step_with_chain() {
        let json = r#"{
            "command": "docker",
            "steps": [
                {
                    "id": "action",
                    "prompt": "What to do?",
                    "type": "choice",
                    "options": [
                        { "label": "Run container", "chain": "docker-run" },
                        { "label": "List containers", "flag": "ps" }
                    ]
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.steps[0].options[0].chain,
            Some("docker-run".to_string())
        );
        assert_eq!(config.steps[0].options[1].chain, None);
    }

    #[test]
    fn test_parse_step_with_default() {
        let json = r#"{
            "command": "test",
            "steps": [
                {
                    "id": "opt",
                    "prompt": "Choose:",
                    "type": "choice",
                    "default": 2,
                    "options": [
                        { "label": "A" },
                        { "label": "B" },
                        { "label": "C" }
                    ]
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config.steps[0].default, Some(2));
    }

    #[test]
    fn test_parse_conditional_step() {
        let json = r#"{
            "command": "test",
            "steps": [
                {
                    "id": "mode",
                    "prompt": "Mode:",
                    "type": "choice",
                    "options": [
                        { "label": "Simple" },
                        { "label": "Advanced" }
                    ]
                },
                {
                    "id": "advanced_opt",
                    "prompt": "Advanced option:",
                    "type": "toggle",
                    "flag": "--verbose",
                    "when": { "mode": "Advanced" }
                }
            ]
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert!(config.steps[1].when.is_some());
        let when = config.steps[1].when.as_ref().unwrap();
        assert_eq!(when.get("mode"), Some(&"Advanced".to_string()));
    }

    #[test]
    fn test_step_type_deserialization() {
        assert_eq!(
            serde_json::from_str::<StepType>(r#""choice""#).unwrap(),
            StepType::Choice
        );
        assert_eq!(
            serde_json::from_str::<StepType>(r#""toggle""#).unwrap(),
            StepType::Toggle
        );
        assert_eq!(
            serde_json::from_str::<StepType>(r#""text""#).unwrap(),
            StepType::Text
        );
        assert_eq!(
            serde_json::from_str::<StepType>(r#""multi""#).unwrap(),
            StepType::Multi
        );
    }

    #[test]
    fn test_config_error_display_not_found() {
        let paths = vec![
            PathBuf::from(".i/foo.json"),
            PathBuf::from("/home/.config/i/foo.json"),
        ];
        let err = ConfigError::NotFound("foo".to_string(), paths);
        let display = format!("{}", err);
        assert!(display.contains("No configuration found for 'foo'"));
        assert!(display.contains(".i/foo.json"));
    }

    #[test]
    fn test_config_error_display_read_error() {
        let err = ConfigError::ReadError(
            PathBuf::from("/path/to/config.json"),
            "permission denied".to_string(),
        );
        let display = format!("{}", err);
        assert!(display.contains("Failed to read"));
        assert!(display.contains("permission denied"));
    }

    #[test]
    fn test_config_error_display_parse_error() {
        let err = ConfigError::ParseError(
            PathBuf::from("/path/to/config.json"),
            "unexpected token".to_string(),
        );
        let display = format!("{}", err);
        assert!(display.contains("Failed to parse"));
        assert!(display.contains("unexpected token"));
    }

    #[test]
    fn test_config_paths_generates_local_path() {
        let paths = config_paths("mycommand");
        assert!(paths.iter().any(|p| p.ends_with(".i/mycommand.json")));
    }

    #[test]
    fn test_config_paths_handles_compound_names() {
        let paths = config_paths("docker-run");
        assert!(paths.iter().any(|p| p.ends_with(".i/docker-run.json")));
    }

    #[test]
    fn test_answer_variants() {
        // Just verify we can create each Answer variant
        let choice = Answer::Choice(0);
        let toggle = Answer::Toggle(true);
        let text = Answer::Text("hello".to_string());
        let multi = Answer::Multi(vec![0, 2]);

        match choice {
            Answer::Choice(idx) => assert_eq!(idx, 0),
            _ => panic!("Expected Choice"),
        }
        match toggle {
            Answer::Toggle(val) => assert!(val),
            _ => panic!("Expected Toggle"),
        }
        match text {
            Answer::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected Text"),
        }
        match multi {
            Answer::Multi(indices) => assert_eq!(indices, vec![0, 2]),
            _ => panic!("Expected Multi"),
        }
    }

    #[test]
    fn test_parse_config_minimal() {
        let json = r#"{
            "command": "test",
            "steps": []
        }"#;

        let config: Config = serde_json::from_str(json).unwrap();
        assert_eq!(config._command, "test");
        assert_eq!(config._description, ""); // default
        assert!(config.steps.is_empty());
        assert!(config.presets.is_empty());
        assert!(config.placeholder_options.is_empty());
    }

    #[test]
    fn test_parse_invalid_json_fails() {
        let json = r#"{ invalid json }"#;
        let result: Result<Config, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_required_field_fails() {
        let json = r#"{ "description": "no command field" }"#;
        let result: Result<Config, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_all_bundled_configs_are_valid() {
        let config_dir = std::path::Path::new(".i");
        assert!(config_dir.exists(), ".i directory should exist");

        let mut errors = Vec::new();

        for entry in std::fs::read_dir(config_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false) {
                let content = std::fs::read_to_string(&path).unwrap();
                if let Err(e) = serde_json::from_str::<Config>(&content) {
                    errors.push(format!("{}: {}", path.display(), e));
                }
            }
        }

        assert!(errors.is_empty(), "Invalid configs:\n{}", errors.join("\n"));
    }
}
