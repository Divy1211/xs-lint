use std::collections::HashSet;
use std::path::PathBuf;
use std::process::ExitCode;
use dunce::canonicalize;
use xsc_core::utils::warnings_from_str;

use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "xs-check", about = env!("CARGO_PKG_DESCRIPTION"))]
struct Opt {
    #[structopt(parse(from_os_str))]
    filepath: Option<PathBuf>,
    
    #[structopt(short, long, help = "Show binary version & info")]
    version: bool,
    
    #[structopt(
        short,
        long,
        help = "Comma separated list of names of warnings to ignore",
        parse(try_from_str = warnings_from_str)
    )]
    ignores: Option<HashSet<u32>>,

    #[structopt(short, long, help = "Treat warnings as errors (A non zero exit code is produced when errors are encountered)")]
    warnings_are_errors: bool,

    #[structopt(
        short,
        long,
        help = "Specify an additional prelude file",
        parse(from_os_str)
    )]
    extra_prelude_path: Option<PathBuf>,

    #[structopt(
        short = "I",
        long,
        help = "Additional directories to search for includes. Comma or space delimited",
        parse(from_os_str)
    )]
    include_dirs: Vec<PathBuf>,
}

include!(concat!(env!("OUT_DIR"), "/build_date.rs"));

fn print_info() {
    let name = "xs-check";
    let version = env!("CARGO_PKG_VERSION");
    let authors = env!("CARGO_PKG_AUTHORS");
    let description = env!("CARGO_PKG_DESCRIPTION");

    println!("{name} v{version}: {description}");
    println!("Author: {authors}");
    println!("Compiled: {BUILD_DATE}");
}

pub fn parse_args() -> Result<(PathBuf, HashSet<u32>, Option<PathBuf>, Vec<PathBuf>, bool), ExitCode> {
    let opt = Opt::from_args();
    if opt.version {
        print_info();
        return Err(ExitCode::SUCCESS);
    }
    
    match opt.filepath {
        None => {
            Opt::clap().print_help().unwrap();
            println!();
            Err(ExitCode::FAILURE)
        }
        Some(rel_path) => {
            let filepath = match canonicalize(&rel_path) {
                Ok(filepath) => { filepath }
                Err(err) => {
                    println!("Failed to open file '{}': {err}", rel_path.display());
                    return Err(ExitCode::FAILURE);
                }
            };
            
            Ok((
                filepath,
                opt.ignores.unwrap_or_else(HashSet::new),
                opt.extra_prelude_path,
                opt.include_dirs,
                opt.warnings_are_errors,
            ))
        }
    }
}