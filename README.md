# enzyme-installer

Adaptive, cross-platform installer CLI that turns declarative manifests into deterministic installation plans for macOS and Windows machines.

## Features

- Environment detection that captures OS family/version, CPU architecture, RAM, common package managers, and a machine fingerprint for license hooks.
- Manifest-driven planning with deterministic mode selection and clear failure reasons.
- Plan execution via platform-appropriate shells with streaming stdout/stderr and rich step primitives.
- Extensible data model with downloads, archive extraction, and templated config rendering.
- Persistent install-state tracking so you can see what was attempted on the machine.

## Getting started

### Build

```bash
cargo build --release
```

### Test

Run the validation and planner unit tests to ensure parsing and compatibility logic remain sound:

```bash
cargo test
```

### Commands

- Detect the current machine profile:

```bash
enzyme-installer detect
```

- Produce an installation plan without executing steps:

```bash
enzyme-installer plan examples/keanu.manifest.json
```

- Execute an installation plan end-to-end:

```bash
enzyme-installer install examples/keanu.manifest.json
```

- View recorded installs on this machine:

```bash
enzyme-installer list-installed
```

Pass `--json` to any subcommand to receive machine-readable output (errors included). JSON payloads are emitted to stdout.

## Manifest shape

Manifests describe a single application with multiple installation modes. Each mode declares requirements and per-OS steps. See `examples/keanu.manifest.json` for a concrete example.

Supported steps:

- `{"run": "echo hi"}` – executes the command in a platform-appropriate shell.
- `{"download": {"url": "https://example.com/file", "dest": "artifacts/file.zip"}}` – downloads a file to the provided relative path.
- `{"extract": {"archive": "artifacts/file.zip", "dest": "workdir"}}` – extracts a `.zip` archive into the destination directory.
- ```json
  {
    "template_config": {
      "source": "config/app.env.template",
      "dest": "workdir/.env",
      "vars": { "APP_NAME": "demo", "PORT": "3000" }
    }
  }
  ```
  Renders a text template by substituting `{{VAR}}` placeholders with provided values.

Existing manifests that only contain `run` steps continue to work without modification.

### Virtual runtimes (v3)

Each mode can declare a `runtime_env` describing an isolated runtime to prepare before running steps. The feature is additive and optional:

```json
{
  "modes": {
    "full": {
      "runtime_env": {
        "type": "node_local",
        "root": ".enzyme_env",
        "node": {
          "version": "20.11.0",
          "install_strategy": "local_bundle_or_global"
        }
      },
      "steps": {
        "macos": [ {"run": "node --version"} ]
      }
    },
    "light": {
      "runtime_env": {
        "type": "python_venv",
        "root": ".enzyme_env",
        "python": {
          "version": "3.11",
          "install_strategy": "venv_or_global"
        }
      },
      "steps": {
        "macos": [ {"run": "python -m pip --version"} ]
      }
    }
  }
}
```

- `node_local` prepares a per-app Node.js directory under `root/node` and prefers a locally provisioned binary. If none is present, the installer falls back to a compatible global `node` when allowed by `install_strategy`.
- `python_venv` creates a virtual environment at `root/venv` using `python3` (or `py` on Windows) and runs subsequent steps inside it.

### Fingerprints

Environment detection now surfaces a fingerprint containing OS, version, architecture, RAM, hostname (when available), and a stable SHA-256 hash over those fields. Use `enzyme-installer detect --json` to inspect the structure when integrating licensing or per-machine bundle logic.

## Install state and reporting

Successful and failed installs are recorded per app version and mode. State is stored at:

- macOS: `$HOME/Library/Application Support/enzyme-installer/state.json`
- Windows: `%APPDATA%\enzyme-installer\state.json`

Use `enzyme-installer list-installed` (or `--json` for structured output) to view historical records. Each record includes the app name, version, mode, OS, CPU architecture, status, and timestamp.

## JSON output

All subcommands accept `--json` to emit a single JSON object to stdout. On failure, an error object is returned instead of human text. Examples:

- `enzyme-installer detect --json` → `{ "ok": true, "environment": { ... } }`
- `enzyme-installer plan manifest.json --json` → `{ "ok": true, "plan": { ... } }` or `{ "ok": false, "error": { "message": "...", "details": ["..."], "environment": { ... } } }`
- `enzyme-installer install manifest.json --json` → success response includes the plan and step counts; failures include the plan (when available) and the zero-based `failed_step_index`.
- `enzyme-installer list-installed --json` → `{ "ok": true, "installs": [ ... ] }`

## Extensibility

The codebase intentionally isolates manifest parsing, environment detection, planning, execution, and persistence. New step types or requirement kinds can be added without breaking existing manifests or CLI contracts.
