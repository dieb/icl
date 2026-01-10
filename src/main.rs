mod option;
mod parser;
mod builder;
mod output;
mod tui;

use std::process::Command;
use std::io;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "i")]
#[command(about = "Transform any CLI into an interactive TUI")]
struct Args {
    /// The command to make interactive (e.g., "cargo" or "git commit")
    #[arg(required = true)]
    command: Vec<String>,

    /// Print debug info and exit
    #[arg(long)]
    debug: bool,
}

fn get_help_text(command: &[String]) -> Option<String> {
    // Try --help first
    let output = Command::new(&command[0])
        .args(&command[1..])
        .arg("--help")
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if !text.is_empty() {
        return Some(text);
    }

    // Some commands output help to stderr
    let text = String::from_utf8_lossy(&output.stderr).to_string();
    if !text.is_empty() {
        return Some(text);
    }

    // Try -h as fallback
    let output = Command::new(&command[0])
        .args(&command[1..])
        .arg("-h")
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if !text.is_empty() {
        return Some(text);
    }

    let text = String::from_utf8_lossy(&output.stderr).to_string();
    if !text.is_empty() {
        return Some(text);
    }

    None
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    // Get help text by running the command with --help
    let help_text = match get_help_text(&args.command) {
        Some(text) => text,
        None => {
            eprintln!("Could not get help for '{}'", args.command.join(" "));
            std::process::exit(1);
        }
    };

    // Parse options from help text
    let options = parser::parse_help(&help_text);

    if args.debug {
        eprintln!("=== Help text ({} bytes) ===", help_text.len());
        eprintln!("{}", help_text);
        eprintln!("=== Parsed {} options ===", options.len());
        for opt in &options {
            eprintln!("  {:?} {:?} - {} | choices: {:?}", opt.short, opt.long, opt.description, opt.choices);
        }
        return Ok(());
    }

    if options.is_empty() {
        eprintln!("Could not parse any options from the help text.");
        eprintln!("The help text format may not be supported.");
        std::process::exit(1);
    }

    // Run TUI
    match tui::run(options, args.command)? {
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
