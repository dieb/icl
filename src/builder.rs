use crate::option::CliOption;

pub fn build_command(base_command: &[String], options: &[CliOption]) -> String {
    let mut parts: Vec<String> = base_command.to_vec();

    for opt in options {
        if let Some(arg) = opt.to_arg() {
            parts.push(arg);
        }
    }

    parts.join(" ")
}
