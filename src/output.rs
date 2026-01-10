use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OutputMode {
    Print,
    Clipboard,
    Execute,
}

pub fn handle_output(command: &str, mode: OutputMode) -> Result<(), String> {
    match mode {
        OutputMode::Print => {
            println!("{}", command);
            Ok(())
        }
        OutputMode::Clipboard => {
            use arboard::Clipboard;
            let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
            clipboard.set_text(command).map_err(|e| e.to_string())?;
            eprintln!("Command copied to clipboard");
            Ok(())
        }
        OutputMode::Execute => {
            let status = if cfg!(target_os = "windows") {
                Command::new("cmd")
                    .args(["/C", command])
                    .status()
            } else {
                Command::new("sh")
                    .args(["-c", command])
                    .status()
            };

            match status {
                Ok(s) => {
                    if !s.success() {
                        Err(format!("Command exited with status: {}", s))
                    } else {
                        Ok(())
                    }
                }
                Err(e) => Err(e.to_string()),
            }
        }
    }
}
