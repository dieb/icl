# i

Interactive TUI for CLI commands.

`i` provides a wizard-style interface for building command-line commands. Each command has a JSON config that defines the steps and options, making it easy to create curated experiences for any CLI tool.

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
# Interactive wizard for ls
i ls

# Configure a git commit
i git commit
```

## How It Works

When you run `i ls`, it:

1. Loads the config file for `ls` (from `.i/ls.json`, `~/.config/i/ls.json`, or bundled)
2. Shows a menu with quick presets and the interactive wizard
3. Guides you through each option with a step-by-step wizard
4. Outputs the final command (print, clipboard, or execute)

## Config File Format

Config files are JSON and define the command's options and presets:

```json
{
  "command": "ls",
  "description": "List directory contents",
  "presets": [
    {
      "label": "Detailed list with sizes",
      "flags": "-lah"
    },
    {
      "label": "Recently modified first",
      "flags": "-lt"
    }
  ],
  "steps": [
    {
      "id": "format",
      "prompt": "How do you want to see files?",
      "type": "choice",
      "options": [
        { "label": "Detailed list", "flag": "-l" },
        { "label": "Grid (default)", "flag": null },
        { "label": "One per line", "flag": "-1" }
      ]
    },
    {
      "id": "hidden",
      "prompt": "Show hidden files?",
      "type": "toggle",
      "flag": "-a"
    },
    {
      "id": "human_sizes",
      "prompt": "Human-readable sizes?",
      "type": "toggle",
      "flag": "-h",
      "when": { "format": "Detailed list" }
    }
  ]
}
```

### Step Types

| Type | Description |
|------|-------------|
| `choice` | Single-select from a list of options |
| `toggle` | Yes/No question |
| `text` | Free-form text input |
| `multi` | Multi-select from a list of options |

### Conditional Steps

Use `when` to show a step only when a previous answer matches:

```json
{
  "id": "human_sizes",
  "prompt": "Human-readable sizes?",
  "type": "toggle",
  "flag": "-h",
  "when": { "format": "Detailed list" }
}
```

### Presets

Presets appear in the main menu for quick access to common flag combinations:

```json
"presets": [
  { "label": "List all with sizes", "flags": "-lah" },
  { "label": "Just names", "flags": "-1" }
]
```

## Config Lookup Order

1. `./.i/<command>.json` - Project-local configs
2. `~/.config/i/<command>.json` - User configs
3. Bundled configs

For subcommands like `git commit`, use `git-commit.json`.

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `Space` | Toggle (for toggle/multi types) |
| `Enter` | Confirm / Run command |
| `Ctrl+C` | Copy command to clipboard |
| `Ctrl+P` | Print command to stdout |
| `Esc` | Go back |
| `q` | Quit |

## Creating Configs

To add support for a new command:

1. Create `.i/<command>.json` in your project, or `~/.config/i/<command>.json` for global use
2. Define the steps based on the command's options
3. Add common presets for quick access
4. Run `i <command>` to test

## License

MIT
