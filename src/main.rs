fn main() {
    let code = enzyme_installer::run_cli();
    if code != 0 {
        std::process::exit(code);
    }
}
