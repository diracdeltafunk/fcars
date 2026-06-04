use std::env;
use std::fmt::Debug;
use std::fs::File;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use fcars::FormalContext;

const USAGE: &str = "\
Usage: fcars [-n] [-V] [-o file] [--dat | --cxt] [file_in]
       fcars --hpc-jobs jobs [--hpc-job job | --hpc-job-env env_var] [--hpc-one-based] [-o file] [--dat | --cxt] [file_in]

Options:
  -n                By default, all concepts are printed (one per line). If this flag is given, only the number of concepts is printed.
  -V                Verbose output: print the context, whether it is reduced, and the number of concepts.
  -o file           Write output to file instead of stdout.
  [--dat | --cxt]   Specifies input format. By default, .dat format is assumed. If more than one format flag is specified, the last one takes precedence.
  --hpc-jobs jobs   Split PCbO counting into many independent jobs. Without a job index, print the Slurm array plan.
  --hpc-job job     Count one zero-based HPC job partition and print: <job><tab><concept_count>.
  --hpc-job-env var Read the HPC job index from an environment variable. Defaults to SLURM_ARRAY_TASK_ID when it is set.
  --hpc-one-based   Interpret the HPC job index as one-based, useful with Slurm arrays like --array=1-N.
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
    hpc: Option<HpcConfig>,
    input_path: Option<String>,
}

struct HpcConfig {
    target_frontier: usize,
    job_index: Option<usize>,
    job_env: Option<String>,
    one_based: bool,
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
            if let Some(hpc) = &config.hpc {
                return write_hpc_result(context, hpc, config.verbose, &mut output);
            }
            write_result(context, config.count_only, config.verbose, &mut output)
        }
        InputFormat::CXT => {
            let context = FormalContext::from_cxt(input);
            if let Some(hpc) = &config.hpc {
                return write_hpc_result(context, hpc, config.verbose, &mut output);
            }
            write_result(context, config.count_only, config.verbose, &mut output)
        }
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> io::Result<Option<Config>> {
    let mut count_only = false;
    let mut verbose = false;
    let mut output_path = None;
    let mut input_format = InputFormat::DAT;
    let mut hpc_target_frontier = None;
    let mut hpc_job_index = None;
    let mut hpc_job_env = None;
    let mut hpc_one_based = false;
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
            "--hpc-jobs" | "--hpc-frontier" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_input(format!("{arg} requires a positive integer")))?;
                let jobs = parse_positive_usize(&value, &arg)?;
                if hpc_target_frontier.replace(jobs).is_some() {
                    return Err(invalid_input(
                        "--hpc-jobs/--hpc-frontier may only be specified once",
                    ));
                }
            }
            "--hpc-job" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_input("--hpc-job requires an integer"))?;
                let job = parse_usize(&value, "--hpc-job")?;
                if hpc_job_index.replace(job).is_some() {
                    return Err(invalid_input("--hpc-job may only be specified once"));
                }
            }
            "--hpc-job-env" => {
                let env_var = args
                    .next()
                    .ok_or_else(|| invalid_input("--hpc-job-env requires an environment name"))?;
                if hpc_job_env.replace(env_var).is_some() {
                    return Err(invalid_input("--hpc-job-env may only be specified once"));
                }
            }
            "--hpc-one-based" => hpc_one_based = true,
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

    if hpc_job_index.is_some() && hpc_job_env.is_some() {
        return Err(invalid_input(
            "--hpc-job and --hpc-job-env may not be used together",
        ));
    }

    let hpc_flags_without_target = hpc_target_frontier.is_none()
        && (hpc_job_index.is_some() || hpc_job_env.is_some() || hpc_one_based);
    if hpc_flags_without_target {
        return Err(invalid_input(
            "--hpc-job, --hpc-job-env, and --hpc-one-based require --hpc-jobs",
        ));
    }

    let hpc = hpc_target_frontier.map(|target_frontier| HpcConfig {
        target_frontier,
        job_index: hpc_job_index,
        job_env: hpc_job_env,
        one_based: hpc_one_based,
    });

    Ok(Some(Config {
        count_only,
        verbose,
        output_path,
        input_format,
        hpc,
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

fn write_hpc_result<A, B>(
    context: FormalContext<A, B>,
    hpc: &HpcConfig,
    verbose: bool,
    output: &mut dyn Write,
) -> io::Result<()>
where
    A: Clone + Send + Sync + Debug + std::fmt::Display,
    B: Clone + Send + Sync + Debug + std::fmt::Display,
{
    if verbose {
        writeln!(output, "{context}")?;
        writeln!(output, "Reduced? {}", context.is_reduced())?;
    }

    let plan = context.pcbo_job_plan(hpc.target_frontier);
    if let Some(job_index) = hpc_job_index(hpc)? {
        if job_index >= plan.total_jobs() {
            return Err(invalid_input(format!(
                "HPC job index {job_index} is out of range for {} jobs",
                plan.total_jobs()
            )));
        }

        let count = context
            .num_concepts_in_pcbo_job(hpc.target_frontier, job_index)
            .expect("job index was checked against the PCbO job plan");
        return writeln!(output, "{job_index}\t{count}");
    }

    writeln!(output, "HPC PCbO count plan")?;
    writeln!(output, "target_frontier\t{}", plan.target_frontier)?;
    writeln!(output, "singleton_jobs\t{}", plan.singleton_jobs)?;
    writeln!(output, "subtree_jobs\t{}", plan.subtree_jobs)?;
    writeln!(output, "array_jobs\t{}", plan.total_jobs())?;
    writeln!(
        output,
        "slurm_array\t0-{}",
        plan.total_jobs().saturating_sub(1)
    )?;
    writeln!(output, "default_job_env\tSLURM_ARRAY_TASK_ID")
}

fn hpc_job_index(hpc: &HpcConfig) -> io::Result<Option<usize>> {
    let raw_index = match hpc.job_index {
        Some(index) => Some(index),
        None => match &hpc.job_env {
            Some(env_var) => Some(read_hpc_job_env(env_var)?),
            None => match env::var("SLURM_ARRAY_TASK_ID") {
                Ok(value) => Some(parse_usize(&value, "SLURM_ARRAY_TASK_ID")?),
                Err(env::VarError::NotPresent) => None,
                Err(env::VarError::NotUnicode(_)) => {
                    return Err(invalid_input("SLURM_ARRAY_TASK_ID is not valid Unicode"));
                }
            },
        },
    };

    match (raw_index, hpc.one_based) {
        (Some(0), true) => Err(invalid_input(
            "one-based HPC job indices must be greater than zero",
        )),
        (Some(index), true) => Ok(Some(index - 1)),
        (Some(index), false) => Ok(Some(index)),
        (None, _) => Ok(None),
    }
}

fn read_hpc_job_env(env_var: &str) -> io::Result<usize> {
    match env::var(env_var) {
        Ok(value) => parse_usize(&value, env_var),
        Err(env::VarError::NotPresent) => Err(invalid_input(format!(
            "environment variable {env_var} is not set"
        ))),
        Err(env::VarError::NotUnicode(_)) => Err(invalid_input(format!(
            "environment variable {env_var} is not valid Unicode"
        ))),
    }
}

fn parse_positive_usize(value: &str, option: &str) -> io::Result<usize> {
    let parsed = parse_usize(value, option)?;
    if parsed == 0 {
        return Err(invalid_input(format!("{option} must be greater than zero")));
    }
    Ok(parsed)
}

fn parse_usize(value: &str, option: &str) -> io::Result<usize> {
    value
        .parse()
        .map_err(|_| invalid_input(format!("{option} must be an integer")))
}

fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
