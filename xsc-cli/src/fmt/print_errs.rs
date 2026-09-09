use std::collections::HashSet;
use std::fs;
use std::ops::{BitOrAssign};
use std::path::PathBuf;

use ariadne::{Color, Config, Fmt, IndexType, Label, Report, ReportKind, Source};

use crate::fmt::msg_fmt::msg_fmt;
use xsc_core::r#static::info::{ParseError, XsError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorsPresent {
    None,
    OnlyWarnings,
    HasErrors,
}

impl BitOrAssign<ErrorsPresent> for ErrorsPresent {
    fn bitor_assign(&mut self, rhs: ErrorsPresent) {
        match (*self, rhs) {
            (ErrorsPresent::None, other)
            | (other, ErrorsPresent::None) => { *self = other },
            (ErrorsPresent::OnlyWarnings, ErrorsPresent::HasErrors)
            | (ErrorsPresent::HasErrors, ErrorsPresent::OnlyWarnings) => { *self = ErrorsPresent::HasErrors }
            _ => {},
        }
    }
}

pub fn print_xs_errs(path: &PathBuf, errs: &Vec<XsError>, ignores: &HashSet<u32>) -> ErrorsPresent {
    let filename = &path.display().to_string();
    let src = &fs::read_to_string(&path).expect("Infallible: If we are here, the file was read previously");
    
    let kwds = Color::Fixed(5);
    let highlight = Color::Fixed(12);
    let names = Color::Fixed(13);
    let types = Color::Fixed(14);

    let mut found_errs = ErrorsPresent::None;
    for error in errs.iter() {
        if ignores.contains(&error.code()) || error.is_ignored() {
            continue;
        }
        found_errs |= if !error.is_warning() { ErrorsPresent::HasErrors } else { ErrorsPresent::OnlyWarnings };
        let report = Report::build(error.report_kind(), filename, error.span().start)
            .with_config(Config::default().with_index_type(IndexType::Byte))
            .with_code(error.code())
            .with_message(error.kind());
        let report = match error {
            XsError::ExtraArg { fn_name, span } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("Extra argument provided to function {}", fn_name.fg(names)))
                        .with_color(highlight)
                )
            }
            XsError::TypeMismatch { actual, expected, span, note } => {
                let report = report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("Expected type {} but found {}", expected.fg(types), actual.fg(types)))
                        .with_color(highlight)
                );
                match note {
                    None => { report }
                    Some(note) => {
                        report.with_help(note)
                    }
                }
            }
            XsError::NotCallable { name, actual, span } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("The variable {} is of type {} and not a function", name.fg(names), actual.fg(types)))
                        .with_color(highlight)
                )
            }
            XsError::OpMismatch { op, type1, type2, span, note } => {
                let report = report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("Cannot {} types {} and {}", op, type1.fg(types), type2.fg(types)))
                        .with_color(highlight)
                );
                match note {
                    None => { report }
                    Some(note) => {
                        report.with_help(note)
                    }
                }
            }
            XsError::UndefinedName { name, span } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("Name {} is not defined", name.fg(names)))
                        .with_color(highlight)
                )
            }
            XsError::PrivateName { name, span, src_loc } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!(
                            "Name {} is private in file {}. Consider using {} if needed",
                            name.fg(names),
                            src_loc.file_path.file_name().unwrap_or_default().to_string_lossy(),
                            "extern".fg(kwds),
                        ))
                        .with_color(highlight)
                )
            }
            XsError::RedefinedName { name, span, note, .. } => {
                let report = report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("Name {} is already defined", name.fg(names)))
                        .with_color(highlight)
                );
                match note {
                    None => { report }
                    Some(note) => {
                        report.with_help(note)
                    }
                }
            }
            XsError::UnresolvedInclude { inc_filename, span } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(format!("Failed to resolve included file {}", inc_filename.fg(names)))
                        .with_color(highlight)
                )
            }
            XsError::Syntax { span, msg, keywords } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(msg_fmt(msg, keywords, &kwds))
                        .with_color(highlight)
                )
            }

            XsError::Warning { span, msg, keywords, .. } => {
                report.with_label(
                    Label::new((filename, span.start..span.end))
                        .with_message(msg_fmt(msg, keywords, &types))
                        .with_color(highlight)
                )
            }
        };
        report
            .finish()
            .print((filename, Source::from(src)))
            .unwrap();
    }
    found_errs
}

pub fn print_parse_errs(path: &PathBuf, errs: &Vec<ParseError>) {
    let src = fs::read_to_string(&path).expect("Infallible: If we are here, the file was read previously");
    let filename = &path.display().to_string();
    let highlight = Color::Fixed(12);
    
    for error in errs {
        let kind = error.kind();
        let (span, msg) = (error.span(), error.msg());
        
        Report::build(ReportKind::Error, filename, span.start)
            .with_config(Config::default().with_index_type(IndexType::Byte))
            .with_message(kind)
            .with_label(
                Label::new((filename, span.start..span.end))
                    .with_message(msg)
                    .with_color(highlight)
            )
            .finish()
            .print((filename, Source::from(&src)))
            .unwrap();
    }
}