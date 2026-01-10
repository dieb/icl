# i

Transform any CLI into an interactive TUI.

`i` parses the `--help` output of any command and presents its options in an interactive terminal interface. Select the options you need, provide values, and get your command ready to run.

## Installation

```bash
cargo install --path .
```

## Usage

```bash
i <command>
```

### Examples

```bash
# Interactively build a cargo command
i cargo build

# Explore git commit options
i git commit

# Configure an rsync command
i rsync
```

## How It Works

1. Runs `<command> --help` (or `-h` as fallback) to get the help text
2. Parses options, flags, and their descriptions from the output
3. Displays them in a TUI where you can:
   - Toggle options on/off
   - Enter values for options that require them
   - Cycle through predefined choices (e.g., `--color auto|always|never`)
4. Outputs the final command via print, clipboard, or direct execution

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Space` | Toggle option |
| `h` / `←` | Previous choice |
| `l` / `→` | Next choice |
| `e` | Edit value |
| `Enter` | Print command to stdout |
| `Ctrl+C` | Copy command to clipboard |
| `Ctrl+X` | Execute command |
| `q` / `Esc` | Quit |

## Debugging

Use the `--debug` flag to see the parsed help text and extracted options:

```bash
i cargo build --debug
```

## Dependencies

- [ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) - Terminal manipulation
- [arboard](https://github.com/1Password/arboard) - Clipboard access
- [clap](https://github.com/clap-rs/clap) - Argument parsing
- [regex](https://github.com/rust-lang/regex) - Help text parsing

## License

MIT
