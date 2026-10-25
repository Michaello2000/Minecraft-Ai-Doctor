fn main() {
    if let Err(error) = minecraft_ai_doctor::cli::run() {
        eprintln!("{}", minecraft_ai_doctor::privacy::redact(&error));
        std::process::exit(1);
    }
}
