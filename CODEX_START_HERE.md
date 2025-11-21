You are an expert Rust systems programmer and DevOps engineer.
Your task is to implement a first working version of a cross-platform CLI tool called enzyme-installer.
High-level goal
enzyme-installer is a standalone CLI program that:
Reads a JSON “app manifest” describing how to install a program on different platforms and in different “modes” (full/light/legacy).
Detects the current machine’s environment (OS, CPU arch, RAM, basic package managers where possible).
Chooses the best compatible mode for this machine.
Expands platform-specific install steps into a plan.
Executes the plan (by running shell commands) and reports success/failure in a structured way.
The target platforms for v1 are:
macOS (Intel + Apple Silicon)
Windows 10+ (x64; older but still widely used hardware is expected)
You will:
Create a complete, compilable Rust CLI project.
Implement environment detection, manifest parsing, planning, and execution.
Provide clear README instructions and basic error handling.
Keep the design modular so it can later be embedded into a virtual OS (Enzyme-VOS) or wrapped by a GUI.
PROJECT SCOPE AND REQUIREMENTS
Language and tooling:
Language: Rust (stable).
Use common crates where appropriate (for example: clap for CLI, serde/serde_json for JSON, sysinfo or similar for environment info).
The project must build on macOS and Windows without nightly features.
Core CLI behavior:
Implement a binary enzyme-installer with at least these subcommands:
detect
Detects the environment and prints a JSON description to stdout.
plan <manifest-path>
Loads the app manifest JSON file.
Runs detection.
Chooses the best compatible mode for this machine.
Prints a JSON install plan to stdout (but does NOT run it).
install <manifest-path>
Same as plan, but then executes the plan (runs the steps).
Streams user-friendly logs to stdout.
Returns non-zero exit code on failure.
You may optionally add --json flags for machine-readable output; if you do, ensure human-readable output remains the default.
MANIFEST FORMAT
Implement a strongly-typed Rust representation of the manifest and define the JSON schema implicitly through types + examples. The manifest models a single app/program.
Use this shape as ground truth:
{
  "name": "keanu-chronicle",
  "version": "1.0.0",

  "modes": {
    "full": {
      "requirements": {
        "os": ["windows>=10", "macos>=13"],
        "cpu_arch": ["x64", "arm64"],
        "ram_gb": 8
      },
      "steps": {
        "windows": [
          { "run": "winget install --id PostgreSQL.PostgreSQL --source winget" },
          { "run": "winget install --id OpenJS.NodeJS.LTS" },
          { "run": "git clone https://github.com/example/keanu.git" },
          { "run": "cd keanu && npm install && npm run build" }
        ],
        "macos": [
          { "run": "brew install postgresql@16 node" },
          { "run": "git clone https://github.com/example/keanu.git" },
          { "run": "cd keanu && npm install && npm run build" }
        ]
      }
    },

    "light": {
      "requirements": {
        "os": ["windows>=8.1", "macos>=12"],
        "ram_gb": 4
      },
      "steps": {
        "windows": [
          { "run": "winget install --id OpenJS.NodeJS.LTS" },
          { "run": "git clone https://github.com/example/keanu.git" },
          { "run": "cd keanu && npm install && npm run dev -- --no-chronicle" }
        ],
        "macos": [
          { "run": "brew install node" },
          { "run": "git clone https://github.com/example/keanu.git" },
          { "run": "cd keanu && npm install && npm run dev -- --no-chronicle" }
        ]
      }
    }
  }
}
Design notes:
name and version are strings.
modes is a map mode_name -> Mode.
Each Mode contains:
requirements (see below).
steps, keyed by OS family ("windows", "macos"). You can treat unknown OS families as unsupported for that mode.
Each step is at v1 just a {"run": "<shell command>"} object. Keep this extensible so new step types can be added later (e.g. download, extract).
requirements:
Represent the following:
os: array of constraints like "windows>=10", "macos>=13". You should:
Parse into {family: "windows" | "macos", min_version: Option<String>}.
Compare against detected OS + version using a simple string-based comparison for v1 (it doesn’t have to be perfect; just stable and deterministic).
cpu_arch: array of allowed architectures, e.g. ["x64", "arm64"].
ram_gb: minimum integer RAM in gigabytes.
In Rust, define typed structs for:
Manifest
Mode
Requirements
OsConstraint
InstallSteps
Step (initially just RunCommand(String))
Derive Serialize/Deserialize as appropriate so manifests and plans can be easily consumed.
ENVIRONMENT DETECTION
Implement a module (e.g. env_detect) that exposes:
#[derive(Debug, Serialize)]
pub struct Environment {
    pub os: String,          // "windows" or "macos" (lowercase)
    pub os_version: String,  // e.g. "10.0.19045" or "14.5"
    pub cpu_arch: String,    // "x64" or "arm64" (normalize common cases)
    pub ram_gb: u64,
    pub pkg_managers: Vec<String>  // e.g. ["winget", "choco"] or ["brew"]
}
Behavior:
On Windows:
os = "windows".
Use any appropriate crate or Windows API wrapper to obtain version (approximate is acceptable).
Detect cpu_arch from std::env::consts::ARCH and normalize "x86_64" to "x64".
Use sysinfo (or similar) to detect RAM in bytes and convert to gigabytes (round down).
For pkg_managers, best-effort:
Check if winget is on PATH.
Optionally also choco.
On macOS:
os = "macos".
Use sysinfo or OS-specific API/crate to get version string (approximate is acceptable).
Normalize ARCH to "x64" or "arm64".
RAM as above.
For pkg_managers, check for brew on PATH.
The detect subcommand:
Calls this module.
Prints pretty JSON to stdout (for human) by default.
You may optionally add a --raw flag to print compact JSON.
PLANNER (MODE SELECTION + PLAN OBJECT)
Implement a planner module (e.g. planner) with core logic:
Inputs:
env: Environment
manifest: Manifest
Outputs:
On success: an InstallPlan.
On failure: a structured error with reasons.
Define a Rust struct for the plan:
#[derive(Debug, Serialize)]
pub struct InstallPlan {
    pub app_name: String,
    pub app_version: String,
    pub chosen_mode: String,
    pub os: String,
    pub steps: Vec<PlannedStep>
}

#[derive(Debug, Serialize)]
pub struct PlannedStep {
    pub description: String,
    pub command: String
}
Planner behavior:
For each mode in manifest.modes:
Check whether env satisfies the requirements:
OS family matches one of the os constraints.
OS version is >= the minimum (use a simple split-on-dots numeric comparison; if parsing fails, treat as not meeting the requirement unless equal).
CPU arch matches one of the cpu_arch entries (if cpu_arch is omitted, accept all).
ram_gb >= required ram_gb (if present).
Also ensure that steps has an entry for env.os for that mode.
Collect all compatible modes.
Select the “best” by a deterministic rule, for example:
Prefer full if compatible.
Otherwise pick the mode with the highest ram_gb requirement (most “capable” mode that still fits).
If no modes compatible:
Return an error that includes:
The machine’s environment.
A vector of strings describing why each mode failed (e.g. ["full: requires windows>=10, found windows 8.1"]).
If successful:
For the chosen mode, read steps[env.os].
Convert each manifest step into a PlannedStep:
description: human-readable explanation like "Run: brew install node".
command: the actual string to pass to the shell.
The plan subcommand:
Loads manifest from given path.
Runs detection.
Runs planner.
Prints JSON of InstallPlan on success (pretty).
On failure:
Prints a human-readable error to stderr.
Returns non-zero exit code.
EXECUTOR
Implement a simple executor module (e.g. executor) that:
Takes an InstallPlan.
For each step:
Prints a header like: ==> [1/4] Run: <description>.
Executes the command using the appropriate shell:
On Windows: invoke cmd.exe /C <command>.
On macOS: invoke /bin/sh -c <command>.
Stream stdout/stderr to the user.
If any command returns a non-zero code:
Stop execution.
Print a clear message like Step 2 failed with exit code X.
Return an error to caller.
The install subcommand:
Loads manifest.
Detects environment.
Runs planner.
Prints the plan briefly (e.g. mode + number of steps).
Asks no interactive questions for v1 (assume the user wants to proceed).
Executes the plan.
Returns exit code 0 on full success, non-zero on failure.
PROJECT STRUCTURE
Create a standard Cargo project layout:
Cargo.toml
src/main.rs
src/cli.rs – CLI arg parsing and entrypoints for subcommands.
src/manifest.rs – Manifest types + JSON parsing.
src/env_detect.rs – Environment detection.
src/planner.rs – Mode selection + InstallPlan.
src/executor.rs – Running the steps.
main.rs should be as small as possible, delegating to cli.rs.
ERROR HANDLING AND LOGGING
Use Result and a simple error type (or anyhow/thiserror if you prefer).
All subcommands should:
Print a concise message on error.
Return non-zero exit code.
Do not panic for expected failures (e.g., “no compatible mode,” “manifest parsing failed,” “command not found”).
README AND EXAMPLES
Create a README.md with:
Project description (“adaptive program installer for heterogeneous machines”).
How to build:
cargo build --release
How to run:
enzyme-installer detect
enzyme-installer plan examples/keanu.manifest.json
enzyme-installer install examples/keanu.manifest.json
Example manifest file in an examples/ directory that matches the JSON shape specified above.
EXTENSIBILITY HOOKS (DESIGN ONLY, NOT FULL IMPLEMENTATION)
In your code structure and types, leave obvious, well-commented extension points for:
New step types beyond "run" (e.g. Download, Extract, TemplateConfigFile).
Additional requirement types (e.g. “needs Docker”, “needs GPU,” “needs Postgres client”).
Optional JSON output mode for all subcommands (--json flag).
You do not need to fully implement these extra types for v1, but design your enums and modules so they can be added without breaking everything.
DELIVERABLE EXPECTATIONS
Produce:
All source files (main.rs, cli.rs, manifest.rs, env_detect.rs, planner.rs, executor.rs).
A complete Cargo.toml with reasonable crate choices.
An examples/keanu.manifest.json file.
A README.md that explains usage.
The project must:
Compile with cargo build on both macOS and Windows (assuming Rust toolchain is installed).
Run enzyme-installer detect successfully.
Correctly parse the example manifest and generate a valid plan for both a macOS and a Windows environment (you can mock or approximate environment in tests if needed).
Begin by generating the full Cargo project (all source files and Cargo.toml content), then the README and the example manifest.
