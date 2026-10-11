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
    match sim::run_report(&options) {
        Ok((report, failed)) => {
            print!("{report}");
            if failed {
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
