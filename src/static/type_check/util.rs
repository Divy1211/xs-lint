use std::collections::HashMap;
use crate::parsing::ast::expr::Expr;
use crate::parsing::ast::identifier::Identifier;
use crate::parsing::ast::literal::Literal;
use crate::parsing::ast::type_::Type;
use crate::parsing::span::{Span, Spanned};
use crate::r#static::type_check::expression::xs_tc_expr;
use crate::r#static::xs_error::{type_err, warn, XSError};

pub fn chk_int_lit(val: &i64, span: &Span) -> Vec<XSError> {
    if *val < -999_999_999 || 999_999_999 < *val {
        vec!(type_err("`int` literals cannot have more than 9 digits", span))
    } else {
        vec![]
    }
}

pub fn chk_num_lit((expr, span): &Spanned<Expr>, is_neg: bool) -> Vec<XSError> {
    match expr {
        Expr::Neg(expr) => if is_neg {
            vec![type_err("Unary negative may only be used with `int | float` literals", span)]
        } else {
            chk_num_lit(expr, true)
        }
        Expr::Literal(lit) => match lit {
            Literal::Int(val) => { chk_int_lit(val, span) }
            Literal::Float(_) => { vec![] }
            _ => { vec![type_err("Expected a value of type `int | float`", span)] }
        }
        _ => {
            vec![type_err("Only `int | float` literals are allowed in vector initialisations", span)]
        }
    }
}

pub fn arith_op<'src>(
    span: &'src Span,
    expr1: &'src Spanned<Expr>,
    expr2: &'src Spanned<Expr>,
    type_env: &'src HashMap<Identifier, Type>,
    errs: &mut Vec<XSError>,
    op_name: &str
) -> Option<&'src Type> {
    // no error is returned specifically because if None is returned, an error will have
    // been generated already
    let (Some(type1), Some(type2)) = (
        xs_tc_expr(expr1, type_env, errs), xs_tc_expr(expr2, type_env, errs)
    ) else {
        return None;
    };

    match (type1, type2) {
        (Type::Int, Type::Int) => { Some(&Type::Int) }
        (Type::Int, Type::Float) => {
            errs.push(warn(
                "This expression yields an `int`, not a `float`.\n\nThe resulting type of an arithmetic operation depends on its first operand. yES.",
                span
            ));
            Some(&Type::Int)
        }
        
        (Type::Float, Type::Int | Type::Float) => { Some(&Type::Float) }

        (Type::Str, _) | (_, Type::Str) if op_name == "add" => { Some(&Type::Str) }
        
        _ => {
            errs.push(type_err(
                &format!("Cannot {:} types `{:}` and `{:}`", op_name, type1, type2), span
            ));
            None
        }
    }
}

pub fn reln_op<'src>(
    span: &'src Span,
    expr1: &'src Spanned<Expr>,
    expr2: &'src Spanned<Expr>,
    type_env: &'src HashMap<Identifier, Type>,
    errs: &mut Vec<XSError>,
    op_name: &str
) -> Option<&'src Type> {
    // no error is returned specifically because if None is returned, an error will have
    // been generated already
    let (Some(type1), Some(type2)) = (
        xs_tc_expr(expr1, type_env, errs), xs_tc_expr(expr2, type_env, errs)
    ) else {
        return None;
    };

    match (type1, type2) {
        (Type::Int | Type::Float, Type::Int | Type::Float) => { Some(&Type::Bool) }
        (Type::Str, Type::Str) => { Some(&Type::Bool) }
        (Type::Vec, Type::Vec) | (Type::Bool, Type::Bool) => {
            if op_name != "eq" || op_name != "ne" {
                errs.push(warn(
                    "This comparison will cause a silent XS crash!",
                    span,
                ));
            }
            Some(&Type::Bool)
        }
        
        _ => {
            errs.push(type_err(
                &format!("Cannot compare types `{:}` and `{:}`", type1, type2), span
            ));
            None
        }
    }
}

pub fn logical_op<'src>(
    span: &'src Span,
    expr1: &'src Spanned<Expr>,
    expr2: &'src Spanned<Expr>,
    type_env: &'src HashMap<Identifier, Type>,
    errs: &mut Vec<XSError>,
    op_name: &str
) -> Option<&'src Type> {
    // no error is returned specifically because if None is returned, an error will have
    // been generated already
    let (Some(type1), Some(type2)) = (
        xs_tc_expr(expr1, type_env, errs), xs_tc_expr(expr2, type_env, errs)
    ) else {
        return None;
    };

    match (type1, type2) {
        (Type::Bool, Type::Bool) => { Some(&Type::Bool) }
        _ => {
            errs.push(type_err(
                &format!("Cannot {:} types `{:}` and `{:}`", op_name, type1, type2), span
            ));
            None
        }
    }
}

pub fn type_cmp(
    expected: &Type,
    actual: &Type,
    actual_span: &Span,
    errs: &mut Vec<XSError>,
    is_fn_call: bool,
) {
    match (expected, actual) {
        (_, _) if *expected == *actual => {},
        (Type::Int, Type::Float) => {
            errs.push(warn(
                "Possible loss of precision due to downcast from `float` to an `int`",
                actual_span
            ))
        }
        (Type::Float, Type::Int) => if is_fn_call {
            errs.push(warn(
                "`int` value may not get promoted to `float` automatically in a function call, floating point operations may not work correctly",
                actual_span
            ))
        }
        _ => {
            errs.push(type_err(
                &format!("Expected `{:}` found `{:}`", expected, actual), actual_span
            ))
        }
    }
}