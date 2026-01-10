use std::collections::HashMap;
use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph},
};

use crate::config::{Answer, Config, Step, StepType};
use crate::output::OutputMode;

pub enum WizardResult {
    Command(String, OutputMode),
    Chain(String),  // Chain to another config
    Back,           // Go back to previous wizard in chain
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Menu,    // Initial menu: wizard vs presets
    Steps,   // Step-by-step wizard
    Confirm, // Final confirmation
}

pub struct Wizard {
    config: Config,
    base_command: Vec<String>,
    answers: HashMap<String, Answer>,
    current_step: usize,
    phase: Phase,
    // Menu state: 0 = wizard, 1+ = presets
    menu_index: usize,
    // State for current widget
    choice_index: usize,
    toggle_value: bool,
    text_buffer: String,
    multi_selected: Vec<bool>,
    // Dynamic placeholder options (e.g., container selection)
    placeholder_values: Vec<(String, String)>, // (name, id) pairs
    placeholder_index: usize,
    active_placeholder: Option<String>, // The placeholder key being resolved
}

impl Wizard {
    pub fn new(config: Config, base_command: Vec<String>) -> Self {
        // Skip menu phase if there are no presets - go directly to steps
        let phase = if config.presets.is_empty() {
            Phase::Steps
        } else {
            Phase::Menu
        };

        Self {
            config,
            base_command,
            answers: HashMap::new(),
            current_step: 0,
            phase,
            menu_index: 0,
            choice_index: 0,
            toggle_value: false,
            text_buffer: String::new(),
            multi_selected: Vec::new(),
            placeholder_values: Vec::new(),
            placeholder_index: 0,
            active_placeholder: None,
        }
    }

    fn menu_item_count(&self) -> usize {
        1 + self.config.presets.len() // wizard + presets
    }

    fn selected_preset(&self) -> Option<&crate::config::Preset> {
        if self.menu_index > 0 {
            self.config.presets.get(self.menu_index - 1)
        } else {
            None
        }
    }

    fn build_preset_command(&self) -> Option<String> {
        self.selected_preset().map(|preset| {
            format!("{} {}", self.base_command.join(" "), preset.flags)
        })
    }

    fn current_command(&self) -> String {
        if self.menu_index == 0 {
            self.build_command()
        } else {
            self.build_preset_command().unwrap_or_default()
        }
    }

    fn current_step(&self) -> Option<&Step> {
        self.visible_steps().get(self.current_step).copied()
    }

    fn visible_steps(&self) -> Vec<&Step> {
        self.config
            .steps
            .iter()
            .filter(|step| self.should_show_step(step))
            .collect()
    }

    fn should_show_step(&self, step: &Step) -> bool {
        let Some(when) = &step.when else {
            return true;
        };

        for (step_id, expected_value) in when {
            let Some(answer) = self.answers.get(step_id) else {
                return false;
            };

            // Find the referenced step to get option labels
            let ref_step = self.config.steps.iter().find(|s| &s.id == step_id);

            match answer {
                Answer::Choice(idx) => {
                    if let Some(ref_step) = ref_step {
                        let label = ref_step
                            .options
                            .get(*idx)
                            .map(|o| o.label.as_str())
                            .unwrap_or("");
                        if label != expected_value {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                Answer::Toggle(val) => {
                    let matches = (*val && expected_value == "true")
                        || (!*val && expected_value == "false");
                    if !matches {
                        return false;
                    }
                }
                Answer::Text(text) => {
                    if text != expected_value {
                        return false;
                    }
                }
                Answer::Multi(indices) => {
                    // Check if expected_value is in the selected options
                    if let Some(ref_step) = ref_step {
                        let selected_labels: Vec<&str> = indices
                            .iter()
                            .filter_map(|i| ref_step.options.get(*i).map(|o| o.label.as_str()))
                            .collect();
                        if !selected_labels.contains(&expected_value.as_str()) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
        }

        true
    }

    fn init_step(&mut self) {
        let Some(step) = self.current_step().cloned() else {
            return;
        };

        match step.step_type {
            StepType::Choice => {
                self.choice_index = step.default.unwrap_or(0);
            }
            StepType::Toggle => {
                self.toggle_value = false;
            }
            StepType::Text => {
                self.text_buffer.clear();
            }
            StepType::Multi => {
                self.multi_selected = vec![false; step.options.len()];
            }
        }
    }

    fn save_answer(&mut self) {
        let Some(step) = self.current_step().cloned() else {
            return;
        };

        let answer = match step.step_type {
            StepType::Choice => Answer::Choice(self.choice_index),
            StepType::Toggle => Answer::Toggle(self.toggle_value),
            StepType::Text => Answer::Text(self.text_buffer.clone()),
            StepType::Multi => {
                let indices: Vec<usize> = self
                    .multi_selected
                    .iter()
                    .enumerate()
                    .filter(|(_, &selected)| selected)
                    .map(|(i, _)| i)
                    .collect();
                Answer::Multi(indices)
            }
        };

        self.answers.insert(step.id.clone(), answer);
    }

    fn get_current_chain(&self) -> Option<String> {
        let step = self.current_step()?;
        if step.step_type != StepType::Choice {
            return None;
        }
        step.options.get(self.choice_index)?.chain.clone()
    }

    fn next_step(&mut self) -> Option<String> {
        // Check for chain before saving
        let chain = self.get_current_chain();
        if chain.is_some() {
            return chain;
        }

        self.save_answer();

        let visible = self.visible_steps();
        if self.current_step + 1 >= visible.len() {
            self.phase = Phase::Confirm;
            self.prepare_confirm_phase();
        } else {
            self.current_step += 1;
            self.init_step();
        }
        None
    }

    fn prepare_confirm_phase(&mut self) {
        if self.has_placeholder_options() {
            self.fetch_placeholder_values();
        }
    }

    fn prev_step(&mut self) {
        if self.phase == Phase::Confirm {
            self.phase = Phase::Steps;
            // Re-init the current step
            self.init_step();
        } else if self.current_step > 0 {
            self.current_step -= 1;
            self.init_step();
            // Restore previous answer
            if let Some(step) = self.current_step() {
                if let Some(answer) = self.answers.get(&step.id) {
                    match answer {
                        Answer::Choice(idx) => self.choice_index = *idx,
                        Answer::Toggle(val) => self.toggle_value = *val,
                        Answer::Text(text) => self.text_buffer = text.clone(),
                        Answer::Multi(indices) => {
                            if let Some(step) = self.current_step() {
                                self.multi_selected = vec![false; step.options.len()];
                                for &idx in indices {
                                    if idx < self.multi_selected.len() {
                                        self.multi_selected[idx] = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn build_command(&self) -> String {
        let mut parts: Vec<String> = self.base_command.clone();

        for step in &self.config.steps {
            let Some(answer) = self.answers.get(&step.id) else {
                continue;
            };

            match (&step.step_type, answer) {
                (StepType::Choice, Answer::Choice(idx)) => {
                    if let Some(opt) = step.options.get(*idx) {
                        if let Some(flag) = &opt.flag {
                            parts.push(flag.clone());
                        }
                    }
                }
                (StepType::Toggle, Answer::Toggle(true)) => {
                    if let Some(flag) = &step.flag {
                        parts.push(flag.clone());
                    }
                }
                (StepType::Text, Answer::Text(text)) => {
                    if !text.is_empty() {
                        if let Some(flag) = &step.flag {
                            parts.push(format!("{} {}", flag, text));
                        } else {
                            parts.push(text.clone());
                        }
                    }
                }
                (StepType::Multi, Answer::Multi(indices)) => {
                    for &idx in indices {
                        if let Some(opt) = step.options.get(idx) {
                            if let Some(flag) = &opt.flag {
                                parts.push(flag.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        parts.join(" ")
    }

    fn build_breadcrumb(&self) -> Vec<String> {
        self.config
            .steps
            .iter()
            .filter_map(|step| {
                let answer = self.answers.get(&step.id)?;
                match (&step.step_type, answer) {
                    (StepType::Choice, Answer::Choice(idx)) => {
                        step.options.get(*idx).map(|opt| opt.label.clone())
                    }
                    (StepType::Toggle, Answer::Toggle(val)) => {
                        Some(if *val { "Yes" } else { "No" }.to_string())
                    }
                    (StepType::Text, Answer::Text(text)) => {
                        (!text.is_empty()).then(|| text.clone())
                    }
                    (StepType::Multi, Answer::Multi(indices)) => {
                        let labels: Vec<&str> = indices
                            .iter()
                            .filter_map(|i| step.options.get(*i).map(|o| o.label.as_str()))
                            .collect();
                        (!labels.is_empty()).then(|| labels.join(", "))
                    }
                    _ => None,
                }
            })
            .collect()
    }

    fn has_placeholder_options(&self) -> bool {
        let cmd = self.current_command();
        self.config
            .placeholder_options
            .keys()
            .any(|p| cmd.contains(p))
    }

    fn get_active_placeholder(&self) -> Option<(&str, &str)> {
        let cmd = self.current_command();
        self.config
            .placeholder_options
            .iter()
            .find(|(placeholder, _)| cmd.contains(*placeholder))
            .map(|(p, c)| (p.as_str(), c.as_str()))
    }

    fn fetch_placeholder_values(&mut self) {
        self.placeholder_values.clear();
        self.placeholder_index = 0;
        self.active_placeholder = None;

        let Some((placeholder, fetch_cmd)) = self.get_active_placeholder() else {
            return;
        };

        // Copy values to avoid borrow issues
        let placeholder = placeholder.to_string();
        let fetch_cmd = fetch_cmd.to_string();

        self.active_placeholder = Some(placeholder);

        let output = std::process::Command::new("sh")
            .args(["-c", &fetch_cmd])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() == 2 {
                        self.placeholder_values
                            .push((parts[0].to_string(), parts[1].to_string()));
                    }
                }
            }
        }
    }

    fn command_with_placeholder(&self, value: &str) -> String {
        if let Some(placeholder) = &self.active_placeholder {
            self.current_command().replace(placeholder, value)
        } else {
            self.current_command()
        }
    }
}

pub fn run(config: Config, base_command: Vec<String>) -> io::Result<WizardResult> {
    if config.steps.is_empty() {
        eprintln!("Config has no steps defined");
        return Ok(WizardResult::Quit);
    }

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut wizard = Wizard::new(config, base_command);
    wizard.init_step();

    let result = loop {
        terminal.draw(|f| ui(f, &wizard))?;

        if let Event::Key(key) = event::read()? {
            match wizard.phase {
                Phase::Menu => match key.code {
                    KeyCode::Esc => break Ok(WizardResult::Back),
                    KeyCode::Char('q') => break Ok(WizardResult::Quit),
                    KeyCode::Up | KeyCode::Char('k') => {
                        if wizard.menu_index > 0 {
                            wizard.menu_index -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if wizard.menu_index + 1 < wizard.menu_item_count() {
                            wizard.menu_index += 1;
                        }
                    }
                    KeyCode::Enter => {
                        if wizard.menu_index == 0 {
                            // Start wizard
                            wizard.phase = Phase::Steps;
                            wizard.init_step();
                        } else {
                            // Preset selected - go straight to confirm
                            wizard.phase = Phase::Confirm;
                            wizard.prepare_confirm_phase();
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(cmd) = wizard.build_preset_command() {
                            break Ok(WizardResult::Command(cmd, OutputMode::Clipboard));
                        }
                    }
                    KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(cmd) = wizard.build_preset_command() {
                            break Ok(WizardResult::Command(cmd, OutputMode::Execute));
                        }
                    }
                    _ => {}
                },
                Phase::Steps => {
                    let step_type = wizard.current_step().map(|s| s.step_type.clone());

                    match key.code {
                        KeyCode::Esc => {
                            if wizard.current_step == 0 {
                                if wizard.config.presets.is_empty() {
                                    // No menu to go back to, go back to previous wizard
                                    break Ok(WizardResult::Back);
                                } else {
                                    wizard.phase = Phase::Menu;
                                    wizard.answers.clear();
                                }
                            } else {
                                wizard.prev_step();
                            }
                        }
                        KeyCode::Char('q') => break Ok(WizardResult::Quit),
                        KeyCode::Enter => {
                            if let Some(chain) = wizard.next_step() {
                                break Ok(WizardResult::Chain(chain));
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => match step_type {
                            Some(StepType::Choice) | Some(StepType::Multi) => {
                                if wizard.choice_index > 0 {
                                    wizard.choice_index -= 1;
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Down | KeyCode::Char('j') => match step_type {
                            Some(StepType::Choice) | Some(StepType::Multi) => {
                                let len = wizard
                                    .current_step()
                                    .map(|s| s.options.len())
                                    .unwrap_or(0);
                                if wizard.choice_index + 1 < len {
                                    wizard.choice_index += 1;
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => match step_type {
                            Some(StepType::Toggle) => {
                                wizard.toggle_value = !wizard.toggle_value;
                            }
                            Some(StepType::Multi) => {
                                let idx = wizard.choice_index;
                                if idx < wizard.multi_selected.len() {
                                    wizard.multi_selected[idx] = !wizard.multi_selected[idx];
                                }
                            }
                            _ => {}
                        },
                        KeyCode::Char(c) => {
                            if step_type == Some(StepType::Text) {
                                wizard.text_buffer.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            if step_type == Some(StepType::Text) {
                                wizard.text_buffer.pop();
                            }
                        }
                        _ => {}
                    }
                }
                Phase::Confirm => match key.code {
                    KeyCode::Esc => {
                        if wizard.menu_index == 0 {
                            wizard.prev_step();
                        } else {
                            wizard.phase = Phase::Menu;
                        }
                    }
                    KeyCode::Char('q') => break Ok(WizardResult::Quit),
                    KeyCode::Up | KeyCode::Char('k') => {
                        if !wizard.placeholder_values.is_empty() && wizard.placeholder_index > 0 {
                            wizard.placeholder_index -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if wizard.placeholder_index + 1 < wizard.placeholder_values.len() {
                            wizard.placeholder_index += 1;
                        }
                    }
                    KeyCode::Enter => {
                        let cmd = if wizard.has_placeholder_options() && !wizard.placeholder_values.is_empty() {
                            let value = &wizard.placeholder_values[wizard.placeholder_index].0;
                            wizard.command_with_placeholder(value)
                        } else {
                            wizard.current_command()
                        };
                        break Ok(WizardResult::Command(cmd, OutputMode::Execute));
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break Ok(WizardResult::Command(wizard.current_command(), OutputMode::Clipboard));
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        break Ok(WizardResult::Command(wizard.current_command(), OutputMode::Print));
                    }
                    _ => {}
                },
            }
        }
    };

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

fn ui(f: &mut Frame, wizard: &Wizard) {
    // Center a box of fixed size
    let box_width = 60u16;
    let box_height = 16u16;
    let centered = centered_rect(box_width, box_height, f.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(centered);

    let title = format!(" {} ", wizard.base_command.join(" "));

    match wizard.phase {
        Phase::Menu => {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(""));

            // Interactive wizard option
            let wizard_style = if wizard.menu_index == 0 {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default()
            };
            let marker = if wizard.menu_index == 0 { "● " } else { "○ " };
            lines.push(Line::from(Span::styled(
                format!("{}Interactive wizard...", marker),
                wizard_style,
            )));

            // Separator if there are presets
            if !wizard.config.presets.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "  Quick presets:",
                    Style::default().fg(Color::DarkGray),
                )));
            }

            // Preset options
            for (i, preset) in wizard.config.presets.iter().enumerate() {
                let idx = i + 1;
                let is_selected = wizard.menu_index == idx;
                let style = if is_selected {
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default()
                };
                let marker = if is_selected { "● " } else { "○ " };
                lines.push(Line::from(vec![
                    Span::styled(format!("{}{}", marker, preset.label), style),
                    Span::styled(
                        format!("  ({})", preset.flags),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }

            let block = Block::default().borders(Borders::ALL).title(title);
            let paragraph = Paragraph::new(lines).block(block);
            f.render_widget(paragraph, chunks[0]);

            let help = Paragraph::new("↑↓ select  Enter confirm  q quit")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, chunks[1]);
        }
        Phase::Steps => {
            if let Some(step) = wizard.current_step() {
                render_step(f, chunks[0], step, wizard, &title);
            }

            let help = Paragraph::new("↑↓ select  Enter confirm  Esc back  q quit")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, chunks[1]);
        }
        Phase::Confirm => {
            let cmd = wizard.current_command();
            let show_placeholder_options = wizard.has_placeholder_options();

            let mut content = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Your command:",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(cmd, Style::default().fg(Color::Green).bold())),
                Line::from(""),
            ];

            if show_placeholder_options {
                if wizard.placeholder_values.is_empty() {
                    content.push(Line::from(Span::styled(
                        "No options found.",
                        Style::default().fg(Color::Yellow),
                    )));
                } else {
                    content.push(Line::from(Span::styled(
                        "Run on:",
                        Style::default().fg(Color::DarkGray),
                    )));
                    for (i, (name, id)) in wizard.placeholder_values.iter().enumerate() {
                        let is_selected = i == wizard.placeholder_index;
                        let marker = if is_selected { "● " } else { "○ " };
                        let style = if is_selected {
                            Style::default().fg(Color::Cyan).bold()
                        } else {
                            Style::default()
                        };
                        let short_id = &id[..id.len().min(12)];
                        content.push(Line::from(Span::styled(
                            format!("{}{} ({})", marker, name, short_id),
                            style,
                        )));
                    }
                }
            }

            let block = Block::default().borders(Borders::ALL).title(title);
            let paragraph = Paragraph::new(content).block(block);
            f.render_widget(paragraph, chunks[0]);

            let help_text = if show_placeholder_options && !wizard.placeholder_values.is_empty() {
                "↑↓ select  Enter run  ^C copy  Esc back  q quit"
            } else {
                "^C copy  Esc back  q quit"
            };
            let help = Paragraph::new(help_text)
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(help, chunks[1]);
        }
    }
}

fn render_step(f: &mut Frame, area: Rect, step: &Step, wizard: &Wizard, title: &str) {
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2),  // Prompt
            Constraint::Min(1),     // Options
            Constraint::Length(1),  // Breadcrumb
        ])
        .split(area);

    // Draw border
    let block = Block::default().borders(Borders::ALL).title(title.to_string());
    f.render_widget(block, area);

    // Prompt
    let prompt = Paragraph::new(Line::from(Span::styled(
        &step.prompt,
        Style::default().bold(),
    )));
    f.render_widget(prompt, inner_chunks[0]);

    // Breadcrumb at the bottom
    let crumbs = wizard.build_breadcrumb();
    if !crumbs.is_empty() {
        let breadcrumb_text = crumbs.join(" › ");
        let breadcrumb = Paragraph::new(Line::from(Span::styled(
            breadcrumb_text,
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(breadcrumb, inner_chunks[2]);
    }

    // Widget based on type
    match step.step_type {
        StepType::Choice => {
            let mut lines: Vec<Line> = Vec::new();
            for (i, opt) in step.options.iter().enumerate() {
                let marker = if i == wizard.choice_index {
                    "● "
                } else {
                    "○ "
                };
                let style = if i == wizard.choice_index {
                    Style::default().fg(Color::Cyan).bold()
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", marker, opt.label),
                    style,
                )));
            }
            let list = Paragraph::new(lines);
            f.render_widget(list, inner_chunks[1]);
        }
        StepType::Toggle => {
            let yes_style = if wizard.toggle_value {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let no_style = if !wizard.toggle_value {
                Style::default().fg(Color::Cyan).bold()
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let line = Line::from(vec![
                Span::styled(if wizard.toggle_value { "● " } else { "○ " }, yes_style),
                Span::styled("Yes", yes_style),
                Span::raw("   "),
                Span::styled(if !wizard.toggle_value { "● " } else { "○ " }, no_style),
                Span::styled("No", no_style),
            ]);
            let toggle = Paragraph::new(line);
            f.render_widget(toggle, inner_chunks[1]);
        }
        StepType::Text => {
            let placeholder = step.placeholder.as_deref().unwrap_or("Type here...");
            let display = if wizard.text_buffer.is_empty() {
                Span::styled(placeholder, Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(&wizard.text_buffer, Style::default())
            };
            let input = Paragraph::new(Line::from(vec![display, Span::raw("█")]));
            f.render_widget(input, inner_chunks[1]);
        }
        StepType::Multi => {
            let mut lines: Vec<Line> = Vec::new();
            for (i, opt) in step.options.iter().enumerate() {
                let selected = wizard.multi_selected.get(i).copied().unwrap_or(false);
                let is_cursor = i == wizard.choice_index;

                let checkbox = if selected { "[x] " } else { "[ ] " };
                let style = if is_cursor {
                    Style::default().fg(Color::Cyan).bold()
                } else if selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                lines.push(Line::from(Span::styled(
                    format!("{}{}", checkbox, opt.label),
                    style,
                )));
            }
            let list = Paragraph::new(lines);
            f.render_widget(list, inner_chunks[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, Preset, Step, StepOption, StepType};

    fn make_config(steps: Vec<Step>) -> Config {
        Config {
            _command: "test".to_string(),
            _description: "".to_string(),
            steps,
            presets: vec![],
            placeholder_options: HashMap::new(),
        }
    }

    fn make_choice_step(id: &str, options: Vec<(&str, Option<&str>)>) -> Step {
        Step {
            id: id.to_string(),
            prompt: format!("Choose {}:", id),
            step_type: StepType::Choice,
            options: options
                .into_iter()
                .map(|(label, flag)| StepOption {
                    label: label.to_string(),
                    flag: flag.map(|f| f.to_string()),
                    chain: None,
                })
                .collect(),
            flag: None,
            default: None,
            when: None,
            placeholder: None,
        }
    }

    fn make_toggle_step(id: &str, flag: &str) -> Step {
        Step {
            id: id.to_string(),
            prompt: format!("Enable {}?", id),
            step_type: StepType::Toggle,
            options: vec![],
            flag: Some(flag.to_string()),
            default: None,
            when: None,
            placeholder: None,
        }
    }

    fn make_text_step(id: &str, flag: Option<&str>) -> Step {
        Step {
            id: id.to_string(),
            prompt: format!("Enter {}:", id),
            step_type: StepType::Text,
            options: vec![],
            flag: flag.map(|f| f.to_string()),
            default: None,
            when: None,
            placeholder: None,
        }
    }

    fn make_multi_step(id: &str, options: Vec<(&str, &str)>) -> Step {
        Step {
            id: id.to_string(),
            prompt: format!("Select {}:", id),
            step_type: StepType::Multi,
            options: options
                .into_iter()
                .map(|(label, flag)| StepOption {
                    label: label.to_string(),
                    flag: Some(flag.to_string()),
                    chain: None,
                })
                .collect(),
            flag: None,
            default: None,
            when: None,
            placeholder: None,
        }
    }

    // ===================
    // build_command tests
    // ===================

    #[test]
    fn test_build_command_empty() {
        let config = make_config(vec![]);
        let wizard = Wizard::new(config, vec!["ls".to_string()]);
        assert_eq!(wizard.build_command(), "ls");
    }

    #[test]
    fn test_build_command_choice_with_flag() {
        let config = make_config(vec![make_choice_step(
            "format",
            vec![("List", Some("-l")), ("Grid", None)],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        // Select first option (has flag)
        wizard.choice_index = 0;
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls -l");
    }

    #[test]
    fn test_build_command_choice_without_flag() {
        let config = make_config(vec![make_choice_step(
            "format",
            vec![("List", Some("-l")), ("Grid", None)],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        // Select second option (no flag)
        wizard.choice_index = 1;
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls");
    }

    #[test]
    fn test_build_command_toggle_enabled() {
        let config = make_config(vec![make_toggle_step("hidden", "-a")]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.toggle_value = true;
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls -a");
    }

    #[test]
    fn test_build_command_toggle_disabled() {
        let config = make_config(vec![make_toggle_step("hidden", "-a")]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.toggle_value = false;
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls");
    }

    #[test]
    fn test_build_command_text_with_flag() {
        let config = make_config(vec![make_text_step("message", Some("-m"))]);
        let mut wizard = Wizard::new(config, vec!["git".to_string(), "commit".to_string()]);

        wizard.text_buffer = "my commit".to_string();
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "git commit -m my commit");
    }

    #[test]
    fn test_build_command_text_without_flag() {
        let config = make_config(vec![make_text_step("path", None)]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.text_buffer = "/tmp".to_string();
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls /tmp");
    }

    #[test]
    fn test_build_command_text_empty() {
        let config = make_config(vec![make_text_step("path", None)]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.text_buffer = "".to_string();
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls");
    }

    #[test]
    fn test_build_command_multi_none_selected() {
        let config = make_config(vec![make_multi_step(
            "options",
            vec![("Long", "-l"), ("All", "-a"), ("Human", "-h")],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.multi_selected = vec![false, false, false];
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls");
    }

    #[test]
    fn test_build_command_multi_some_selected() {
        let config = make_config(vec![make_multi_step(
            "options",
            vec![("Long", "-l"), ("All", "-a"), ("Human", "-h")],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.multi_selected = vec![true, false, true];
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls -l -h");
    }

    #[test]
    fn test_build_command_multi_all_selected() {
        let config = make_config(vec![make_multi_step(
            "options",
            vec![("Long", "-l"), ("All", "-a"), ("Human", "-h")],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.multi_selected = vec![true, true, true];
        wizard.save_answer();
        assert_eq!(wizard.build_command(), "ls -l -a -h");
    }

    #[test]
    fn test_build_command_multiple_steps() {
        let config = make_config(vec![
            make_choice_step("format", vec![("List", Some("-l")), ("Grid", None)]),
            make_toggle_step("hidden", "-a"),
            make_toggle_step("human", "-h"),
        ]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        // Select List format
        wizard.choice_index = 0;
        wizard.save_answer();

        // Enable hidden
        wizard.current_step = 1;
        wizard.toggle_value = true;
        wizard.save_answer();

        // Disable human
        wizard.current_step = 2;
        wizard.toggle_value = false;
        wizard.save_answer();

        assert_eq!(wizard.build_command(), "ls -l -a");
    }

    // ========================
    // should_show_step tests
    // ========================

    #[test]
    fn test_should_show_step_no_condition() {
        let step = make_toggle_step("hidden", "-a");
        let config = make_config(vec![step.clone()]);
        let wizard = Wizard::new(config, vec!["ls".to_string()]);

        assert!(wizard.should_show_step(&step));
    }

    #[test]
    fn test_should_show_step_choice_condition_met() {
        let mut conditional_step = make_toggle_step("verbose", "-v");
        let mut when = HashMap::new();
        when.insert("mode".to_string(), "Advanced".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_choice_step("mode", vec![("Simple", None), ("Advanced", None)]),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        // Select "Advanced" (index 1)
        wizard.choice_index = 1;
        wizard.save_answer();

        assert!(wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_choice_condition_not_met() {
        let mut conditional_step = make_toggle_step("verbose", "-v");
        let mut when = HashMap::new();
        when.insert("mode".to_string(), "Advanced".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_choice_step("mode", vec![("Simple", None), ("Advanced", None)]),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        // Select "Simple" (index 0)
        wizard.choice_index = 0;
        wizard.save_answer();

        assert!(!wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_toggle_condition_true() {
        let mut conditional_step = make_text_step("level", Some("--level"));
        let mut when = HashMap::new();
        when.insert("debug".to_string(), "true".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_toggle_step("debug", "-d"),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        wizard.toggle_value = true;
        wizard.save_answer();

        assert!(wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_toggle_condition_false() {
        let mut conditional_step = make_text_step("level", Some("--level"));
        let mut when = HashMap::new();
        when.insert("debug".to_string(), "false".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_toggle_step("debug", "-d"),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        wizard.toggle_value = false;
        wizard.save_answer();

        assert!(wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_missing_dependency() {
        let mut conditional_step = make_toggle_step("verbose", "-v");
        let mut when = HashMap::new();
        when.insert("mode".to_string(), "Advanced".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![conditional_step.clone()]);
        let wizard = Wizard::new(config, vec!["test".to_string()]);

        // No answer for "mode" exists
        assert!(!wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_text_condition() {
        let mut conditional_step = make_toggle_step("confirm", "-y");
        let mut when = HashMap::new();
        when.insert("name".to_string(), "admin".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_text_step("name", None),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        wizard.text_buffer = "admin".to_string();
        wizard.save_answer();

        assert!(wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_text_condition_not_met() {
        let mut conditional_step = make_toggle_step("confirm", "-y");
        let mut when = HashMap::new();
        when.insert("name".to_string(), "admin".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_text_step("name", None),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        wizard.text_buffer = "user".to_string();
        wizard.save_answer();

        assert!(!wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_multi_condition_met() {
        let mut conditional_step = make_toggle_step("confirm", "-y");
        let mut when = HashMap::new();
        when.insert("features".to_string(), "Logging".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_multi_step("features", vec![("Logging", "--log"), ("Debug", "--debug")]),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        wizard.multi_selected = vec![true, false]; // Logging selected
        wizard.save_answer();

        assert!(wizard.should_show_step(&conditional_step));
    }

    #[test]
    fn test_should_show_step_multi_condition_not_met() {
        let mut conditional_step = make_toggle_step("confirm", "-y");
        let mut when = HashMap::new();
        when.insert("features".to_string(), "Logging".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_multi_step("features", vec![("Logging", "--log"), ("Debug", "--debug")]),
            conditional_step.clone(),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        wizard.multi_selected = vec![false, true]; // Only Debug selected
        wizard.save_answer();

        assert!(!wizard.should_show_step(&conditional_step));
    }

    // ====================
    // visible_steps tests
    // ====================

    #[test]
    fn test_visible_steps_all_unconditional() {
        let config = make_config(vec![
            make_toggle_step("a", "-a"),
            make_toggle_step("b", "-b"),
            make_toggle_step("c", "-c"),
        ]);
        let wizard = Wizard::new(config, vec!["test".to_string()]);

        assert_eq!(wizard.visible_steps().len(), 3);
    }

    #[test]
    fn test_visible_steps_filters_conditional() {
        let mut conditional_step = make_toggle_step("verbose", "-v");
        let mut when = HashMap::new();
        when.insert("mode".to_string(), "Advanced".to_string());
        conditional_step.when = Some(when);

        let config = make_config(vec![
            make_choice_step("mode", vec![("Simple", None), ("Advanced", None)]),
            conditional_step,
            make_toggle_step("other", "-o"),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);

        // Before answering, conditional step is hidden (dependency not met)
        assert_eq!(wizard.visible_steps().len(), 2);

        // Select "Simple" - conditional still hidden
        wizard.choice_index = 0;
        wizard.save_answer();
        assert_eq!(wizard.visible_steps().len(), 2);

        // Change to "Advanced" - conditional now visible
        wizard.choice_index = 1;
        wizard.save_answer();
        assert_eq!(wizard.visible_steps().len(), 3);
    }

    // ====================
    // build_breadcrumb tests
    // ====================

    #[test]
    fn test_build_breadcrumb_empty() {
        let config = make_config(vec![make_toggle_step("a", "-a")]);
        let wizard = Wizard::new(config, vec!["test".to_string()]);

        assert!(wizard.build_breadcrumb().is_empty());
    }

    #[test]
    fn test_build_breadcrumb_choice() {
        let config = make_config(vec![make_choice_step(
            "format",
            vec![("List", Some("-l")), ("Grid", None)],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.choice_index = 0;
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert_eq!(crumbs, vec!["List"]);
    }

    #[test]
    fn test_build_breadcrumb_toggle() {
        let config = make_config(vec![make_toggle_step("hidden", "-a")]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.toggle_value = true;
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert_eq!(crumbs, vec!["Yes"]);

        wizard.toggle_value = false;
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert_eq!(crumbs, vec!["No"]);
    }

    #[test]
    fn test_build_breadcrumb_text() {
        let config = make_config(vec![make_text_step("path", None)]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.text_buffer = "/tmp".to_string();
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert_eq!(crumbs, vec!["/tmp"]);
    }

    #[test]
    fn test_build_breadcrumb_text_empty_excluded() {
        let config = make_config(vec![make_text_step("path", None)]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.text_buffer = "".to_string();
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert!(crumbs.is_empty());
    }

    #[test]
    fn test_build_breadcrumb_multi() {
        let config = make_config(vec![make_multi_step(
            "options",
            vec![("Long", "-l"), ("All", "-a"), ("Human", "-h")],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.multi_selected = vec![true, false, true];
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert_eq!(crumbs, vec!["Long, Human"]);
    }

    #[test]
    fn test_build_breadcrumb_multi_empty_excluded() {
        let config = make_config(vec![make_multi_step(
            "options",
            vec![("Long", "-l"), ("All", "-a")],
        )]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.multi_selected = vec![false, false];
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert!(crumbs.is_empty());
    }

    #[test]
    fn test_build_breadcrumb_multiple_steps() {
        let config = make_config(vec![
            make_choice_step("format", vec![("List", Some("-l")), ("Grid", None)]),
            make_toggle_step("hidden", "-a"),
        ]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        wizard.choice_index = 1;
        wizard.save_answer();

        wizard.current_step = 1;
        wizard.toggle_value = true;
        wizard.save_answer();

        let crumbs = wizard.build_breadcrumb();
        assert_eq!(crumbs, vec!["Grid", "Yes"]);
    }

    // ====================
    // Navigation tests
    // ====================

    #[test]
    fn test_init_step_choice_default() {
        let mut step = make_choice_step("opt", vec![("A", None), ("B", None), ("C", None)]);
        step.default = Some(2);

        let config = make_config(vec![step]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.init_step();

        assert_eq!(wizard.choice_index, 2);
    }

    #[test]
    fn test_init_step_toggle() {
        let config = make_config(vec![make_toggle_step("opt", "-o")]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.toggle_value = true; // Set to true first
        wizard.init_step();

        assert!(!wizard.toggle_value); // Should be reset to false
    }

    #[test]
    fn test_init_step_text() {
        let config = make_config(vec![make_text_step("opt", None)]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.text_buffer = "something".to_string();
        wizard.init_step();

        assert!(wizard.text_buffer.is_empty());
    }

    #[test]
    fn test_init_step_multi() {
        let config = make_config(vec![make_multi_step(
            "opt",
            vec![("A", "-a"), ("B", "-b"), ("C", "-c")],
        )]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.init_step();

        assert_eq!(wizard.multi_selected, vec![false, false, false]);
    }

    #[test]
    fn test_next_step_advances() {
        let config = make_config(vec![
            make_toggle_step("a", "-a"),
            make_toggle_step("b", "-b"),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.init_step();

        assert_eq!(wizard.current_step, 0);
        wizard.next_step();
        assert_eq!(wizard.current_step, 1);
    }

    #[test]
    fn test_next_step_goes_to_confirm() {
        let config = make_config(vec![make_toggle_step("a", "-a")]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.init_step();

        wizard.next_step();
        assert_eq!(wizard.phase, Phase::Confirm);
    }

    #[test]
    fn test_prev_step_goes_back() {
        let config = make_config(vec![
            make_toggle_step("a", "-a"),
            make_toggle_step("b", "-b"),
        ]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.init_step();
        wizard.next_step();

        assert_eq!(wizard.current_step, 1);
        wizard.prev_step();
        assert_eq!(wizard.current_step, 0);
    }

    #[test]
    fn test_prev_step_from_confirm() {
        let config = make_config(vec![make_toggle_step("a", "-a")]);
        let mut wizard = Wizard::new(config, vec!["test".to_string()]);
        wizard.init_step();
        wizard.next_step(); // Goes to confirm

        assert_eq!(wizard.phase, Phase::Confirm);
        wizard.prev_step();
        assert_eq!(wizard.phase, Phase::Steps);
    }

    // ====================
    // Placeholder tests
    // ====================

    #[test]
    fn test_has_placeholder_options() {
        let mut config = make_config(vec![make_text_step("container", None)]);
        config.placeholder_options.insert(
            "<container>".to_string(),
            "docker ps --format '{{.Names}}'".to_string(),
        );

        let mut wizard = Wizard::new(config, vec!["docker".to_string(), "logs".to_string()]);
        wizard.text_buffer = "<container>".to_string();
        wizard.save_answer();

        assert!(wizard.has_placeholder_options());
    }

    #[test]
    fn test_has_placeholder_options_none() {
        let config = make_config(vec![make_text_step("path", None)]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);
        wizard.text_buffer = "/tmp".to_string();
        wizard.save_answer();

        assert!(!wizard.has_placeholder_options());
    }

    #[test]
    fn test_command_with_placeholder() {
        let mut config = make_config(vec![make_text_step("container", None)]);
        config.placeholder_options.insert(
            "<container>".to_string(),
            "docker ps".to_string(),
        );

        let mut wizard = Wizard::new(config, vec!["docker".to_string(), "logs".to_string()]);
        wizard.text_buffer = "<container>".to_string();
        wizard.save_answer();
        wizard.active_placeholder = Some("<container>".to_string());

        let cmd = wizard.command_with_placeholder("my_container");
        assert_eq!(cmd, "docker logs my_container");
    }

    #[test]
    fn test_command_with_placeholder_no_active() {
        let config = make_config(vec![make_text_step("path", None)]);
        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);
        wizard.text_buffer = "/tmp".to_string();
        wizard.save_answer();

        let cmd = wizard.command_with_placeholder("ignored");
        assert_eq!(cmd, "ls /tmp");
    }

    // ====================
    // Preset tests
    // ====================

    #[test]
    fn test_wizard_with_presets_starts_in_menu() {
        let mut config = make_config(vec![make_toggle_step("a", "-a")]);
        config.presets = vec![Preset {
            label: "Quick".to_string(),
            flags: "-la".to_string(),
        }];

        let wizard = Wizard::new(config, vec!["ls".to_string()]);
        assert_eq!(wizard.phase, Phase::Menu);
    }

    #[test]
    fn test_wizard_without_presets_starts_in_steps() {
        let config = make_config(vec![make_toggle_step("a", "-a")]);
        let wizard = Wizard::new(config, vec!["ls".to_string()]);
        assert_eq!(wizard.phase, Phase::Steps);
    }

    #[test]
    fn test_build_preset_command() {
        let mut config = make_config(vec![]);
        config.presets = vec![
            Preset {
                label: "Quick".to_string(),
                flags: "-la".to_string(),
            },
            Preset {
                label: "Verbose".to_string(),
                flags: "-lah".to_string(),
            },
        ];

        let mut wizard = Wizard::new(config, vec!["ls".to_string()]);

        // Select first preset
        wizard.menu_index = 1;
        assert_eq!(wizard.build_preset_command(), Some("ls -la".to_string()));

        // Select second preset
        wizard.menu_index = 2;
        assert_eq!(wizard.build_preset_command(), Some("ls -lah".to_string()));
    }

    #[test]
    fn test_menu_item_count() {
        let mut config = make_config(vec![]);
        config.presets = vec![
            Preset {
                label: "A".to_string(),
                flags: "-a".to_string(),
            },
            Preset {
                label: "B".to_string(),
                flags: "-b".to_string(),
            },
        ];

        let wizard = Wizard::new(config, vec!["ls".to_string()]);
        assert_eq!(wizard.menu_item_count(), 3); // wizard + 2 presets
    }

    // ====================
    // Chain tests
    // ====================

    #[test]
    fn test_get_current_chain() {
        let mut step = make_choice_step("action", vec![("Run", None), ("Build", None)]);
        step.options[0].chain = Some("docker-run".to_string());

        let config = make_config(vec![step]);
        let mut wizard = Wizard::new(config, vec!["docker".to_string()]);
        wizard.init_step();

        // Select "Run" which has a chain
        wizard.choice_index = 0;
        assert_eq!(wizard.get_current_chain(), Some("docker-run".to_string()));

        // Select "Build" which has no chain
        wizard.choice_index = 1;
        assert_eq!(wizard.get_current_chain(), None);
    }

    #[test]
    fn test_next_step_returns_chain() {
        let mut step = make_choice_step("action", vec![("Run", None), ("Build", None)]);
        step.options[0].chain = Some("docker-run".to_string());

        let config = make_config(vec![step]);
        let mut wizard = Wizard::new(config, vec!["docker".to_string()]);
        wizard.init_step();
        wizard.choice_index = 0;

        let chain = wizard.next_step();
        assert_eq!(chain, Some("docker-run".to_string()));
    }
}
