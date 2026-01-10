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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_mode_equality() {
        assert_eq!(OutputMode::Print, OutputMode::Print);
        assert_eq!(OutputMode::Clipboard, OutputMode::Clipboard);
        assert_eq!(OutputMode::Execute, OutputMode::Execute);

        assert_ne!(OutputMode::Print, OutputMode::Clipboard);
        assert_ne!(OutputMode::Print, OutputMode::Execute);
        assert_ne!(OutputMode::Clipboard, OutputMode::Execute);
    }

    #[test]
    fn test_output_mode_clone() {
        let mode = OutputMode::Execute;
        let cloned = mode.clone();
        assert_eq!(mode, cloned);
    }

    #[test]
    fn test_output_mode_copy() {
        let mode = OutputMode::Print;
        let copied = mode; // Copy semantics
        assert_eq!(mode, copied);
    }

    #[test]
    fn test_output_mode_debug() {
        assert_eq!(format!("{:?}", OutputMode::Print), "Print");
        assert_eq!(format!("{:?}", OutputMode::Clipboard), "Clipboard");
        assert_eq!(format!("{:?}", OutputMode::Execute), "Execute");
    }

    #[test]
    fn test_handle_output_print() {
        // Print mode should succeed (writes to stdout)
        let result = handle_output("echo test", OutputMode::Print);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_output_execute_success() {
        // Execute a simple command that should succeed
        let result = handle_output("true", OutputMode::Execute);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_output_execute_failure() {
        // Execute a command that returns non-zero exit status
        let result = handle_output("false", OutputMode::Execute);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Command exited with status"));
    }
}
