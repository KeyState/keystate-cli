//! Binary entry point: parse arguments, run, map outcomes to exit codes.

use keystate_cli::{Exit, main_argv};

fn main() {
    let code = match main_argv() {
        Ok(Exit::Ok) => 0,
        Ok(Exit::Drift) => 3,
        Err(error) => {
            eprintln!("error: {error}");
            1
        }
    };
    std::process::exit(code);
}
