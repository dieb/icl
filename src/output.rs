use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputMode {
    Print,
    Clipboard,
    Execute,
}

pub fn handle_output(command: &str, mode: OutputMode) -> Result<(), Box<dyn std::error::Error>> {
    match mode {
        OutputMode::Print => {
            println!("{}", command);
        }
        OutputMode::Clipboard => {
            let mut clipboard = arboard::Clipboard::new()?;
            clipboard.set_text(command)?;
            eprintln!("Command copied to clipboard");
        }
        OutputMode::Execute => {
            let status = if cfg!(target_os = "windows") {
                Command::new("cmd").args(["/C", command]).status()?
            } else {
                Command::new("sh").args(["-c", command]).status()?
            };

            if !status.success() {
                return Err(format!("Command exited with status: {}", status).into());
            }
        }
    }
    Ok(())
}
