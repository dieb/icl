#[derive(Debug, Clone)]
pub struct CliOption {
    pub short: Option<char>,
    pub long: Option<String>,
    pub description: String,
    pub takes_value: bool,
    pub value_hint: Option<String>,
    pub selected: bool,
    pub value: String,
    pub choices: Option<Vec<String>>,
    pub choice_index: usize,
}

impl CliOption {
    pub fn new(
        short: Option<char>,
        long: Option<String>,
        description: String,
        takes_value: bool,
        value_hint: Option<String>,
        choices: Option<Vec<String>>,
    ) -> Self {
        Self {
            short,
            long,
            description,
            takes_value,
            value_hint,
            selected: false,
            value: String::new(),
            choices,
            choice_index: 0,
        }
    }

    pub fn has_choices(&self) -> bool {
        self.choices.as_ref().map(|c| !c.is_empty()).unwrap_or(false)
    }

    pub fn current_choice(&self) -> Option<&str> {
        self.choices.as_ref().and_then(|c| c.get(self.choice_index).map(|s| s.as_str()))
    }

    pub fn next_choice(&mut self) {
        if let Some(choices) = &self.choices {
            if !choices.is_empty() {
                self.choice_index = (self.choice_index + 1) % choices.len();
            }
        }
    }

    pub fn prev_choice(&mut self) {
        if let Some(choices) = &self.choices {
            if !choices.is_empty() {
                self.choice_index = if self.choice_index == 0 {
                    choices.len() - 1
                } else {
                    self.choice_index - 1
                };
            }
        }
    }

    pub fn display_flag(&self) -> String {
        match (&self.short, &self.long) {
            (Some(s), Some(l)) => format!("-{}, --{}", s, l),
            (Some(s), None) => format!("-{}", s),
            (None, Some(l)) => format!("--{}", l),
            (None, None) => String::new(),
        }
    }

    pub fn to_arg(&self) -> Option<String> {
        if !self.selected {
            return None;
        }

        let flag = if let Some(l) = &self.long {
            format!("--{}", l)
        } else if let Some(s) = self.short {
            format!("-{}", s)
        } else {
            return None;
        };

        // Use choice if available, otherwise use free-form value
        if let Some(choice) = self.current_choice() {
            Some(format!("{} {}", flag, choice))
        } else if self.takes_value && !self.value.is_empty() {
            Some(format!("{} {}", flag, self.value))
        } else if self.takes_value {
            None // Don't include flags that need values but don't have them
        } else {
            Some(flag)
        }
    }
}
