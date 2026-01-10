use regex::Regex;
use crate::option::CliOption;

fn extract_choices(description: &str) -> (String, Option<Vec<String>>) {
    // Match patterns like [possible values: a, b, c] or [values: a, b, c]
    let choices_pattern = Regex::new(
        r"\[(?:possible\s+)?values?:\s*([^\]]+)\]"
    ).unwrap();

    if let Some(cap) = choices_pattern.captures(description) {
        let values_str = cap.get(1).unwrap().as_str();
        let choices: Vec<String> = values_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        // Remove the [possible values: ...] from description
        let clean_desc = choices_pattern.replace(description, "").trim().to_string();

        if !choices.is_empty() {
            return (clean_desc, Some(choices));
        }
    }

    (description.to_string(), None)
}

pub fn parse_help(input: &str) -> Vec<CliOption> {
    let mut options = Vec::new();

    // Pattern matches various help formats:
    // -f, --flag           Description
    // -f, --flag=VALUE     Description
    // -f, --flag <VALUE>   Description
    // -f, --flag...        Description (repeatable)
    // --flag               Description
    // -f                   Description
    let pattern = Regex::new(
        r"(?m)^\s*(-([a-zA-Z]),?\s*)?(?:--([\w-]+))?(?:\.\.\.)?(?:[=\s]<([^>]+)>|=(\S+))?\s{2,}(.+)$"
    ).unwrap();

    // Also try a simpler pattern for edge cases
    let simple_pattern = Regex::new(
        r"(?m)^\s*(-([a-zA-Z]))\s{2,}(.+)$"
    ).unwrap();

    let long_only_pattern = Regex::new(
        r"(?m)^\s*--([\w-]+)(?:[=\s]<([^>]+)>|=(\S+))?\s{2,}(.+)$"
    ).unwrap();

    for cap in pattern.captures_iter(input) {
        let short = cap.get(2).map(|m| m.as_str().chars().next().unwrap());
        let long = cap.get(3).map(|m| m.as_str().to_string());
        let value_hint = cap.get(4).or(cap.get(5)).map(|m| m.as_str().to_string());
        let raw_description = cap.get(6).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        let (description, choices) = extract_choices(&raw_description);

        if short.is_some() || long.is_some() {
            options.push(CliOption::new(
                short,
                long,
                description,
                value_hint.is_some() || choices.is_some(),
                value_hint,
                choices,
            ));
        }
    }

    // If main pattern didn't find much, try simpler patterns
    if options.is_empty() {
        for cap in simple_pattern.captures_iter(input) {
            let short = cap.get(2).map(|m| m.as_str().chars().next().unwrap());
            let raw_description = cap.get(3).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let (description, choices) = extract_choices(&raw_description);

            if let Some(s) = short {
                options.push(CliOption::new(
                    Some(s),
                    None,
                    description,
                    choices.is_some(),
                    None,
                    choices,
                ));
            }
        }

        for cap in long_only_pattern.captures_iter(input) {
            let long = cap.get(1).map(|m| m.as_str().to_string());
            let value_hint = cap.get(2).or(cap.get(3)).map(|m| m.as_str().to_string());
            let raw_description = cap.get(4).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
            let (description, choices) = extract_choices(&raw_description);

            if let Some(l) = long {
                options.push(CliOption::new(
                    None,
                    Some(l),
                    description,
                    value_hint.is_some() || choices.is_some(),
                    value_hint,
                    choices,
                ));
            }
        }
    }

    // Deduplicate by flag name
    let mut seen = std::collections::HashSet::new();
    options.retain(|opt| {
        let key = format!("{:?}{:?}", opt.short, opt.long);
        seen.insert(key)
    });

    options
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gnu_style() {
        let help = "  -v, --verbose    Enable verbose output";
        let opts = parse_help(help);
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].short, Some('v'));
        assert_eq!(opts[0].long, Some("verbose".to_string()));
    }

    #[test]
    fn test_with_value() {
        let help = "  -o, --output <FILE>    Output file";
        let opts = parse_help(help);
        assert_eq!(opts.len(), 1);
        assert!(opts[0].takes_value);
        assert_eq!(opts[0].value_hint, Some("FILE".to_string()));
    }

    #[test]
    fn test_with_choices() {
        let help = "  --color <WHEN>    Coloring [possible values: auto, always, never]";
        let opts = parse_help(help);
        assert_eq!(opts.len(), 1);
        assert!(opts[0].takes_value);
        assert!(opts[0].has_choices());
        assert_eq!(opts[0].choices, Some(vec!["auto".to_string(), "always".to_string(), "never".to_string()]));
        assert_eq!(opts[0].description, "Coloring");
    }

    #[test]
    fn test_cargo_help_format() {
        let help = r#"Options:
  -V, --version                  Print version info and exit
      --list                     List installed commands
      --explain <CODE>           Provide a detailed explanation
  -v, --verbose...               Use verbose output
  -q, --quiet                    Do not print cargo log messages
      --color <WHEN>             Coloring [possible values: auto, always, never]
  -h, --help                     Print help"#;
        let opts = parse_help(help);
        println!("Parsed {} options:", opts.len());
        for opt in &opts {
            println!("  {:?} {:?} - {} | choices: {:?}", opt.short, opt.long, opt.description, opt.choices);
        }
        assert!(opts.len() >= 5, "Expected at least 5 options, got {}", opts.len());

        // Check color option has choices
        let color_opt = opts.iter().find(|o| o.long.as_deref() == Some("color")).unwrap();
        assert!(color_opt.has_choices(), "color option should have choices");
    }
}
