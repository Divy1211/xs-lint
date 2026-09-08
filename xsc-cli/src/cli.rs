use std::collections::HashSet;
use std::path::PathBuf;
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

pub fn parse_args() -> Result<Option<(PathBuf, HashSet<u32>, Option<PathBuf>, Vec<PathBuf>)>, String> {
    let opt = Opt::from_args();
    if opt.version {
        print_info();
        return Ok(None);
    }
    
    match opt.filepath {
        None => {
            Opt::clap().print_help().unwrap();
            println!();
            Ok(None)
        }
        Some(rel_path) => {
            let filepath = match canonicalize(&rel_path) {
                Ok(filepath) => { filepath }
                Err(err) => {
                    return Err(format!("Failed to open file '{}': {err}", rel_path.display()));
                }
            };
            
            Ok(Some((
                filepath,
                opt.ignores.unwrap_or_else(HashSet::new),
                opt.extra_prelude_path,
                opt.include_dirs
            )))
        }
    }
}
