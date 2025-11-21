pub mod cli;
pub mod env_detect;
pub mod executor;
pub mod manifest;
pub mod planner;
pub mod runtime_env;
pub mod state;

/// Run the command line interface and return an exit code.
pub fn run_cli() -> i32 {
    cli::run()
}
