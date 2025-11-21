mod cli;
mod env_detect;
mod executor;
mod manifest;
mod planner;
mod state;

fn main() {
    let code = cli::run();
    if code != 0 {
        std::process::exit(code);
    }
}
