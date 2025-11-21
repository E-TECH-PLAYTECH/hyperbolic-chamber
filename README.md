# enzyme-installer

Adaptive, cross-platform installer CLI that turns declarative manifests into deterministic installation plans for macOS and Windows machines.

## Features

- Environment detection that captures OS family/version, CPU architecture, RAM, and common package managers.
- Manifest-driven planning with deterministic mode selection and clear failure reasons.
- Plan execution via platform-appropriate shells with streaming stdout/stderr.
- Extensible data model to support richer step types and requirement classes in future iterations.

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

Add `--raw` to any subcommand for compact JSON output suitable for piping into other tools.

## Manifest shape

Manifests describe a single application with multiple installation modes. Each mode declares requirements and per-OS steps. See `examples/keanu.manifest.json` for a concrete example that follows the schema outlined in the project brief.

## Extensibility

The codebase intentionally isolates manifest parsing, environment detection, planning, and execution. `Step` is currently an untagged enum with a single `Run` variant, leaving room for additional step types like downloads or template rendering without breaking compatibility. The planner can also be expanded to evaluate new requirement kinds (GPU availability, external services) while keeping existing consumers stable.
