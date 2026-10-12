//! Headless simulation CLI. See `sim --help`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match sim::parse_args(&args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{}", sim::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n\n{}", sim::USAGE);
            return ExitCode::FAILURE;
        }
    };
    match sim::run_output(&options) {
        Ok(output) => {
            print!("{}", output.report);
            if !output.timing.is_empty() {
                print!("\n{}", output.timing);
            }
            if output.failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}
