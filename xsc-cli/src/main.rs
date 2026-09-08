use std::path::PathBuf;
use std::process::ExitCode;

use xsc_core::r#static::info::{gen_errs_from_path, gen_errs_from_src, AstCache, AstMap, Error, SrcCache, TypeEnv};

use crate::cli::parse_args;
use crate::fmt::{print_parse_errs, print_xs_errs};

mod cli;
mod fmt;

fn main() -> ExitCode {
    let (filepath, ignores, extra_prelude_path, include_dirs) = match parse_args() {
        Ok(Some(filepath)) => { filepath }
        Ok(None) => { return ExitCode::SUCCESS; },
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    
    let mut type_env= TypeEnv::new(include_dirs);
    let mut ast_cache = AstMap::new();
    let mut src_cache = AstMap::new();
    
    let prelude_path = PathBuf::from(r"prelude.xs");
    let prelude = include_str!(r"../../xsc-core/prelude.xs");

    gen_errs_from_src(&prelude_path, prelude, &mut type_env, &mut ast_cache, &mut src_cache).expect("Prelude can't produce parse errors");

    let mut has_errors = false;
    if let Some(extra_prelude_path) = extra_prelude_path {
        let new_errs = check_file(&extra_prelude_path, &mut type_env, &mut ast_cache, &mut src_cache);
        has_errors = new_errs;
    }
    let new_errs = check_file(&filepath, &mut type_env, &mut ast_cache, &mut src_cache);
    has_errors =  has_errors || new_errs;

    // Keep the existing diagnostic-summary behavior, but warnings alone must
    // not make the command fail. File and parse errors are always fatal.
    let mut has_fatal_errors = has_errors;

    for (filepath, errs) in type_env.errs() {
        if errs.is_empty() {
            continue;
        } else if filepath == &prelude_path {
            if errs.iter().any(|err| !err.is_warning()) {
                panic!("Prelude can't produce errors")
            } else {
                continue;
            }
        }
        let new_errs = print_xs_errs(filepath, errs, &ignores);
        has_errors = has_errors || new_errs;
        has_fatal_errors = has_fatal_errors || errs.iter().any(|err| !err.is_warning());
    }

    if !has_errors {
        println!(
            "No errors found in file '{}'! Your code is free of the pitfalls of XS' quirks =)",
            filepath.display()
        );
    }
    println!("Finished analysing file '{}'.", filepath.display());
    if has_fatal_errors { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

fn check_file(filepath: &PathBuf, type_env: &mut TypeEnv, ast_cache: &mut AstCache, src_cache: &SrcCache) -> bool {
    let mut has_errors = false;
    if let Err(errs) = gen_errs_from_path(&filepath, type_env, ast_cache, src_cache) {
        has_errors = true;
        for err in errs {
            match err {
                Error::FileErr(_path, msg) => {
                    println!("{}", msg);
                }
                Error::ParseErrs { path, errs } => {
                    print_parse_errs(&path, &errs);
                }
            }
        }
    }
    has_errors
}
