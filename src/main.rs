mod config;
mod output;
mod wizard;

use std::io;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "i")]
#[command(about = "Interactive TUI for CLI commands")]
struct Args {
    /// The command to make interactive (e.g., "ls" or "git commit")
    #[arg(required = true)]
    command: Vec<String>,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // Load config for this command
    let config = match config::Config::load(&args.command) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    // Run wizard
    match wizard::run(config, args.command)? {
        Some((command, mode)) => {
            if let Err(e) = output::handle_output(&command, mode) {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        None => {
            // User quit without selecting
        }
    }

    Ok(())
}
