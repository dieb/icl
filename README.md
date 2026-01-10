# icl

Transform any CLI into a little interactive wizard. The binary is called `i` for quick access.

![CleanShot 2026-01-10 at 10 18 48](https://github.com/user-attachments/assets/00c6ccbb-8ffe-477f-907c-9b70c3d280fd)



## Use Cases

**Forgot which flags to use?** Run `i <command>` and pick your way through the wizard.

**Same commands every day?** Add presets for one-keystroke access to common flag combinations.

## Features

- Step-by-step prompts
- Keyboard navigation (vim keys supported)
- Presets for common flag combinations
- Chaining (top-level configs can link to subcommands)
- Breadcrumbs showing your choices
- Run, copy to clipboard, or print to stdout

## Installation

```bash
cargo install --path .
```

## Development

```bash
cargo run -- ls
cargo run -- docker
```

## Usage

```bash
i <command>
```

```bash
i ls              # Interactive ls wizard
i docker          # Docker command picker
i docker run      # Jump directly to docker run
```

## Keybindings

| Key | Action |
|-----|--------|
| `↑` / `k` | Move up |
| `↓` / `j` | Move down |
| `Space` | Toggle option |
| `Enter` | Confirm / Run command |
| `Ctrl+C` | Copy to clipboard |
| `Ctrl+P` | Print to stdout |
| `Esc` | Go back |
| `q` | Quit |

## Available Configs

See the [.i](.i/) directory for included configs (ls, docker, etc).

## Creating Your Own Configs

Configs are JSON files that define the wizard steps. Place them in:

- `./.i/<command>.json` — Project-local
- `~/.config/i/<command>.json` — User-global

For subcommands like `git commit`, name the file `git-commit.json`.

### Basic Structure

```json
{
  "command": "mytool",
  "description": "What this tool does",
  "presets": [
    { "label": "Quick option", "flags": "--fast --quiet" }
  ],
  "steps": [
    {
      "id": "output",
      "prompt": "Where should output go?",
      "type": "choice",
      "options": [
        { "label": "Terminal", "flag": null },
        { "label": "File", "flag": "-o output.txt" }
      ]
    }
  ]
}
```

### Step Types

#### `choice` — Single selection

```json
{
  "id": "format",
  "prompt": "Output format?",
  "type": "choice",
  "options": [
    { "label": "JSON", "flag": "--format json" },
    { "label": "YAML", "flag": "--format yaml" }
  ]
}
```

#### `toggle` — Yes/No

```json
{
  "id": "verbose",
  "prompt": "Verbose output?",
  "type": "toggle",
  "flag": "-v"
}
```

#### `text` — Free-form input

```json
{
  "id": "name",
  "prompt": "Container name",
  "type": "text",
  "flag": "--name",
  "placeholder": "my-container"
}
```

#### `multi` — Multiple selections

```json
{
  "id": "features",
  "prompt": "Enable features",
  "type": "multi",
  "options": [
    { "label": "Logging", "flag": "--enable-logging" },
    { "label": "Metrics", "flag": "--enable-metrics" }
  ]
}
```

### Conditional Steps

Show a step only when a previous answer matches:

```json
{
  "id": "filename",
  "prompt": "Output filename?",
  "type": "text",
  "flag": "-o",
  "when": { "output": "File" }
}
```

### Chaining

Link to another config:

```json
{
  "id": "subcommand",
  "prompt": "What do you want to do?",
  "type": "choice",
  "options": [
    { "label": "Run a container", "chain": "docker-run" },
    { "label": "List containers", "chain": "docker-ps" }
  ]
}
```

### Presets

```json
{
  "presets": [
    { "label": "List all", "flags": "-la" },
    { "label": "Human-readable", "flags": "-lah" }
  ]
}
```

## License

MIT
