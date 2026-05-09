mod app;
mod platform;

use std::io::Write;

fn main() {
    let code = app::run();
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    std::process::exit(code);
}
