use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(input) = args.next() else {
        eprintln!("usage: bubble-compiler <input.tsx> <output.js>");
        return ExitCode::from(2);
    };
    let Some(output) = args.next() else {
        eprintln!("usage: bubble-compiler <input.tsx> <output.js>");
        return ExitCode::from(2);
    };
    let source = match fs::read_to_string(&input) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{input}: {err}");
            return ExitCode::from(1);
        }
    };
    match bubble_compiler::compile(&source) {
        Ok(js) => {
            if let Err(err) = fs::write(&output, js) {
                eprintln!("{output}: {err}");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("{err}");
            ExitCode::from(1)
        }
    }
}
