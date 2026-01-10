mod config;
mod output;
mod wizard;

use clap::Parser;
use wizard::WizardResult;

#[derive(Parser, Debug)]
#[command(name = "i")]
#[command(about = "Interactive TUI for CLI commands")]
struct Args {
    /// The command to make interactive (e.g., "ls" or "git commit")
    #[arg(required = true)]
    command: Vec<String>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut command = args.command;
    let mut history: Vec<Vec<String>> = Vec::new();

    loop {
        let config = config::Config::load(&command)?;

        match wizard::run(config, command.clone())? {
            WizardResult::Command(cmd, mode) => {
                output::handle_output(&cmd, mode)?;
                break;
            }
            WizardResult::Chain(next_config) => {
                history.push(command.clone());
                command = next_config.split('-').map(String::from).collect();
            }
            WizardResult::Back => {
                if let Some(prev_command) = history.pop() {
                    command = prev_command;
                } else {
                    break;
                }
            }
            WizardResult::Quit => break,
        }
    }

    Ok(())
}
