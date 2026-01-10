# icl

Transform any CLI into a little interactive wizard.

![output](https://github.com/user-attachments/assets/989b1208-9a9a-4baf-aad3-59c60bccdbf0)

Run things like `i docker`, `i ls` to open their wizard. See the [.i](.i/) directory for included configs.

## Use Cases

**Forgot which flags to use?** Run `i <command>` and pick your way through the wizard.

**Same commands every day?** Add presets for one-keystroke access to common flag combinations.

## Features

- Step-by-step prompts
- Keyboard navigation (vim keys supported)
- Presets for common flag combinations
- Chaining (top-level configs can link to subcommands)
- Dynamic placeholder options (e.g., select from running containers)
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

### Dynamic Placeholder Options

When your command includes a placeholder like `<container>`, you can configure a command to fetch available options dynamically. At runtime, the wizard will execute the command and present the results as a selectable list.

```json
{
  "command": "docker logs",
  "placeholder_options": {
    "<container>": "docker ps --format '{{.Names}}\t{{.ID}}'"
  },
  "presets": [
    { "label": "Follow logs", "flags": "-f <container>" }
  ]
}
```

The fetch command should output one option per line in `name\tid` format (tab-separated). The name is used for substitution and display, while the id is shown in parentheses as additional context.

When you select "Follow logs", instead of just showing `docker logs -f <container>`, the wizard will:
1. Run `docker ps --format '{{.Names}}\t{{.ID}}'`
2. Show a list of running containers to choose from
3. Replace `<container>` with your selection and execute

This works for any placeholder — not just containers. For example, you could fetch git branches, kubernetes pods, or any other dynamic list.

## License

MIT
