use std::io::{self, stdout};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

use crate::option::CliOption;
use crate::output::OutputMode;
use crate::builder::build_command;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Normal,
    Input,
}

pub struct App {
    options: Vec<CliOption>,
    base_command: Vec<String>,
    list_state: ListState,
    mode: Mode,
    input_buffer: String,
    should_quit: bool,
    output_mode: Option<OutputMode>,
}

impl App {
    pub fn new(options: Vec<CliOption>, base_command: Vec<String>) -> Self {
        let mut list_state = ListState::default();
        if !options.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            options,
            base_command,
            list_state,
            mode: Mode::Normal,
            input_buffer: String::new(),
            should_quit: false,
            output_mode: None,
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }

    fn selected_option(&self) -> Option<&CliOption> {
        self.selected_index().and_then(|i| self.options.get(i))
    }

    fn move_up(&mut self) {
        if let Some(i) = self.selected_index() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    fn move_down(&mut self) {
        if let Some(i) = self.selected_index() {
            if i < self.options.len().saturating_sub(1) {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(i) = self.selected_index() {
            if let Some(opt) = self.options.get_mut(i) {
                opt.selected = !opt.selected;
                // Only enter input mode if takes_value AND doesn't have predefined choices
                let needs_input = opt.selected && opt.takes_value && !opt.has_choices();
                let value = opt.value.clone();
                if needs_input {
                    self.mode = Mode::Input;
                    self.input_buffer = value;
                }
            }
        }
    }

    fn cycle_choice_next(&mut self) {
        if let Some(i) = self.selected_index() {
            if let Some(opt) = self.options.get_mut(i) {
                if opt.has_choices() && opt.selected {
                    opt.next_choice();
                }
            }
        }
    }

    fn cycle_choice_prev(&mut self) {
        if let Some(i) = self.selected_index() {
            if let Some(opt) = self.options.get_mut(i) {
                if opt.has_choices() && opt.selected {
                    opt.prev_choice();
                }
            }
        }
    }

    fn handle_input_key(&mut self, key: KeyCode) {
        match key {
            KeyCode::Enter | KeyCode::Esc => {
                let new_value = self.input_buffer.clone();
                let is_empty = new_value.is_empty();
                if let Some(i) = self.selected_index() {
                    if let Some(opt) = self.options.get_mut(i) {
                        opt.value = new_value;
                        if is_empty {
                            opt.selected = false;
                        }
                    }
                }
                self.mode = Mode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Char(c) => {
                self.input_buffer.push(c);
            }
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn build_command(&self) -> String {
        build_command(&self.base_command, &self.options)
    }
}

pub fn run(options: Vec<CliOption>, base_command: Vec<String>) -> io::Result<Option<(String, OutputMode)>> {
    if options.is_empty() {
        eprintln!("No options parsed from help text");
        return Ok(None);
    }

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;

    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut app = App::new(options, base_command);

    let result = loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            match app.mode {
                Mode::Normal => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                        break Ok(None);
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.output_mode = Some(OutputMode::Clipboard);
                        break Ok(Some((app.build_command(), OutputMode::Clipboard)));
                    }
                    KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.output_mode = Some(OutputMode::Execute);
                        break Ok(Some((app.build_command(), OutputMode::Execute)));
                    }
                    KeyCode::Enter => {
                        app.output_mode = Some(OutputMode::Print);
                        break Ok(Some((app.build_command(), OutputMode::Print)));
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::Left | KeyCode::Char('h') => app.cycle_choice_prev(),
                    KeyCode::Right | KeyCode::Char('l') => app.cycle_choice_next(),
                    KeyCode::Char(' ') => app.toggle_selected(),
                    KeyCode::Char('e') => {
                        let should_edit = app.selected_option()
                            .map(|opt| opt.takes_value && opt.selected)
                            .unwrap_or(false);
                        let value = app.selected_option()
                            .map(|opt| opt.value.clone())
                            .unwrap_or_default();
                        if should_edit {
                            app.mode = Mode::Input;
                            app.input_buffer = value;
                        }
                    }
                    _ => {}
                },
                Mode::Input => {
                    app.handle_input_key(key.code);
                }
            }
        }
    };

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(f.area());

    // Options list
    let items: Vec<ListItem> = app
        .options
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            let checkbox = if opt.selected { "[x]" } else { "[ ]" };
            let flag = opt.display_flag();

            let value_str = if opt.has_choices() {
                // Show choices with current one highlighted
                if opt.selected {
                    format!(" = {}", opt.current_choice().unwrap_or("?"))
                } else {
                    let choices = opt.choices.as_ref().unwrap();
                    format!(" [{}]", choices.join("|"))
                }
            } else if opt.takes_value && opt.selected && !opt.value.is_empty() {
                format!(" = {}", opt.value)
            } else if opt.takes_value {
                format!(" <{}>", opt.value_hint.as_deref().unwrap_or("VALUE"))
            } else {
                String::new()
            };

            let is_selected = app.selected_index() == Some(i);
            let line = format!("{} {}{}", checkbox, flag, value_str);

            let style = if is_selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else if opt.selected {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            ListItem::new(Line::from(vec![
                Span::styled(line, style),
                Span::styled(format!("  {}", opt.description), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Options"))
        .highlight_style(Style::default());

    f.render_stateful_widget(list, chunks[0], &mut app.list_state);

    // Command preview
    let cmd = app.build_command();
    let preview = Paragraph::new(cmd)
        .block(Block::default().borders(Borders::ALL).title("Command"));
    f.render_widget(preview, chunks[1]);

    // Help / Input
    let bottom_content = match app.mode {
        Mode::Normal => {
            Paragraph::new("Space: toggle  ←/→: cycle choice  Enter: print  Ctrl+C: copy  Ctrl+X: exec  e: edit  q: quit")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL).title("Help"))
        }
        Mode::Input => {
            Paragraph::new(format!("Value: {}█", app.input_buffer))
                .block(Block::default().borders(Borders::ALL).title("Enter value (Enter to confirm, Esc to cancel)"))
        }
    };
    f.render_widget(bottom_content, chunks[2]);
}
