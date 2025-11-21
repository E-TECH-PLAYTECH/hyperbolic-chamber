mod cli;
mod env_detect;
mod executor;
mod manifest;
mod planner;

fn main() {
    if let Err(err) = cli::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
