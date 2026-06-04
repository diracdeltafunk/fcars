use std::env;
use std::fmt::Debug;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use fcars::FormalContext;

const USAGE: &str = "\
Usage: fcars [-n] [-o file] [--dat | --cxt] [file_in]

Options:
  -n                By default, all concepts are printed (one per line). If this flag is given, only the number of concepts is printed.
  -V                Verbose output: print the context, whether it is reduced, and the number of concepts.
  -o file           Write output to file instead of stdout.
  [--dat | --cxt]   Specifies input format. By default, .dat format is assumed. If more than one format flag is specified, the last one takes precedence.
  -h, --help        Print this help message. Disregard all other options and arguments.

Arguments:
    file_in         Path to input file. If not specified, input is read from stdin.
";

#[derive(Clone, Copy)]
enum InputFormat {
    DAT,
    CXT,
}

struct Config {
    count_only: bool,
    verbose: bool,
    output_path: Option<String>,
    input_format: InputFormat,
    input_path: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("fcars: {err}");
            ExitCode::from(1)
        }
    }
}

fn run() -> io::Result<()> {
    let Some(config) = parse_args(env::args().skip(1))? else {
        print!("{USAGE}");
        return Ok(());
    };

    let input = open_input(config.input_path.as_deref())?;
    let mut output = open_output(config.output_path.as_deref())?;

    match config.input_format {
        InputFormat::DAT => {
            let context = FormalContext::from_dat(input);
            write_result(context, config.count_only, config.verbose, &mut output)
        }
        InputFormat::CXT => {
            let context = FormalContext::from_cxt(input);
            write_result(context, config.count_only, config.verbose, &mut output)
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> io::Result<Option<Config>> {
    let mut count_only = false;
    let mut verbose = false;
    let mut output_path = None;
    let mut input_format = InputFormat::DAT;
    let mut input_path = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "-n" => count_only = true,
            "-V" => verbose = true,
            "-o" => {
                let path = args
                    .next()
                    .ok_or_else(|| invalid_input("-o requires an output file path"))?;
                if output_path.replace(path).is_some() {
                    return Err(invalid_input("-o may only be specified once"));
                }
            }
            "--dat" => input_format = InputFormat::DAT,
            "--cxt" => input_format = InputFormat::CXT,
            _ if arg.starts_with('-') => {
                return Err(invalid_input(format!("unknown option: {arg}")));
            }
            _ => {
                if input_path.replace(arg).is_some() {
                    return Err(invalid_input("expected at most one input file"));
                }
            }
        }
    }

    Ok(Some(Config {
        count_only,
        verbose,
        output_path,
        input_format,
        input_path,
    }))
}

fn open_input(path: Option<&str>) -> io::Result<Box<dyn Read>> {
    match path {
        Some(path) => File::open(path).map(|file| Box::new(file) as Box<dyn Read>),
        None => Ok(Box::new(io::stdin())),
    }
}

fn open_output(path: Option<&str>) -> io::Result<Box<dyn Write>> {
    match path {
        Some(path) => File::create(path).map(|file| Box::new(file) as Box<dyn Write>),
        None => Ok(Box::new(io::stdout())),
    }
}

fn write_result<A, B>(
    context: FormalContext<A, B>,
    count_only: bool,
    verbose: bool,
    output: &mut dyn Write,
) -> io::Result<()>
where
    A: Clone + Send + Sync + Debug + std::fmt::Display,
    B: Clone + Send + Sync + Debug + std::fmt::Display,
{
    if count_only {
        if verbose {
            writeln!(output, "{context}")?;
            writeln!(output, "Reduced? {}", context.is_reduced())?;
        }
        return writeln!(output, "{}", context.num_concepts());
    }

    if verbose {
        writeln!(output, "{context}")?;
        writeln!(output, "Reduced? {}", context.is_reduced())?;
        let concepts = context.all_concepts();
        writeln!(output, "{}", concepts.len())?;
        for concept in concepts {
            writeln!(output, "{concept}")?;
        }
        return Ok(());
    }

    for concept in context.all_concepts() {
        writeln!(output, "{concept}")?;
    }
    Ok(())
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
