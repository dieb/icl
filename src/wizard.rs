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
}

impl Wizard {
    pub fn new(config: Config, base_command: Vec<String>) -> Self {
        Self {
            config,
            base_command,
            answers: HashMap::new(),
            current_step: 0,
            phase: Phase::Menu,
            menu_index: 0,
            choice_index: 0,
            toggle_value: false,
            text_buffer: String::new(),
            multi_selected: Vec::new(),
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

    fn next_step(&mut self) {
        self.save_answer();

        let visible = self.visible_steps();
        if self.current_step + 1 >= visible.len() {
            self.phase = Phase::Confirm;
        } else {
            self.current_step += 1;
            self.init_step();
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
        let mut crumbs = Vec::new();

        for step in &self.config.steps {
            let Some(answer) = self.answers.get(&step.id) else {
                continue;
            };

            let label = match (&step.step_type, answer) {
                (StepType::Choice, Answer::Choice(idx)) => {
                    step.options.get(*idx).map(|opt| opt.label.clone())
                }
                (StepType::Toggle, Answer::Toggle(val)) => {
                    if *val {
                        Some("Yes".to_string())
                    } else {
                        Some("No".to_string())
                    }
                }
                (StepType::Text, Answer::Text(text)) => {
                    if text.is_empty() {
                        None
                    } else {
                        Some(text.clone())
                    }
                }
                (StepType::Multi, Answer::Multi(indices)) => {
                    let labels: Vec<&str> = indices
                        .iter()
                        .filter_map(|i| step.options.get(*i).map(|o| o.label.as_str()))
                        .collect();
                    if labels.is_empty() {
                        None
                    } else {
                        Some(labels.join(", "))
                    }
                }
                _ => None,
            };

            if let Some(l) = label {
                crumbs.push(l);
            }
        }

        crumbs
    }
}

pub fn run(config: Config, base_command: Vec<String>) -> io::Result<Option<(String, OutputMode)>> {
    if config.steps.is_empty() {
        eprintln!("Config has no steps defined");
        return Ok(None);
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
                    KeyCode::Esc | KeyCode::Char('q') => break Ok(None),
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
                        }
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(cmd) = wizard.build_preset_command() {
                            break Ok(Some((cmd, OutputMode::Clipboard)));
                        }
                    }
                    KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if let Some(cmd) = wizard.build_preset_command() {
                            break Ok(Some((cmd, OutputMode::Execute)));
                        }
                    }
                    _ => {}
                },
                Phase::Steps => {
                    let step_type = wizard.current_step().map(|s| s.step_type.clone());

                    match key.code {
                        KeyCode::Esc => {
                            if wizard.current_step == 0 {
                                wizard.phase = Phase::Menu;
                                wizard.answers.clear();
                            } else {
                                wizard.prev_step();
                            }
                        }
                        KeyCode::Char('q') => break Ok(None),
                        KeyCode::Enter => wizard.next_step(),
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
                    KeyCode::Char('q') => break Ok(None),
                    KeyCode::Enter => {
                        let cmd = if wizard.menu_index == 0 {
                            wizard.build_command()
                        } else {
                            wizard.build_preset_command().unwrap_or_default()
                        };
                        break Ok(Some((cmd, OutputMode::Execute)));
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let cmd = if wizard.menu_index == 0 {
                            wizard.build_command()
                        } else {
                            wizard.build_preset_command().unwrap_or_default()
                        };
                        break Ok(Some((cmd, OutputMode::Clipboard)));
                    }
                    KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let cmd = if wizard.menu_index == 0 {
                            wizard.build_command()
                        } else {
                            wizard.build_preset_command().unwrap_or_default()
                        };
                        break Ok(Some((cmd, OutputMode::Print)));
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
            let cmd = if wizard.menu_index == 0 {
                wizard.build_command()
            } else {
                wizard.build_preset_command().unwrap_or_default()
            };
            let content = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "Your command:",
                    Style::default().fg(Color::DarkGray),
                )),
                Line::from(""),
                Line::from(Span::styled(cmd, Style::default().fg(Color::Green).bold())),
                Line::from(""),
            ];

            let block = Block::default().borders(Borders::ALL).title(title);
            let paragraph = Paragraph::new(content).block(block).alignment(Alignment::Center);
            f.render_widget(paragraph, chunks[0]);

            let help = Paragraph::new("Enter run  ^C copy  ^P print  Esc back  q quit")
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
