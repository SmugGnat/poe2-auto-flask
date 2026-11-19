#[cfg(windows)]
fn main() {
    use poe2_auto_flask::{backend, launch::LaunchOptions, runtime::BackendControl};
    let result = LaunchOptions::parse(std::env::args().skip(1)).and_then(|options| {
        if options.help {
            println!("{}", LaunchOptions::HELP);
            return Ok(());
        }
        backend::run(options.backend(), BackendControl::standalone())
    });
    if let Err(error) = result {
        eprintln!("FAIL: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Use the Linux launcher to run the Windows application under Proton.");
    std::process::exit(1);
}
