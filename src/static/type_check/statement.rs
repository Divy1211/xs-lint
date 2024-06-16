use std::collections::HashMap;
use chumsky::container::Container;
use crate::parsing::ast::astree::ASTreeNode;
use crate::parsing::ast::expr::Expr;
use crate::parsing::ast::identifier::Identifier;
use crate::parsing::ast::literal::Literal;
use crate::parsing::ast::type_::Type;
use crate::parsing::span::Spanned;
use crate::r#static::type_check::expression::xs_tc_expr;
use crate::r#static::type_check::TypeEnv;
use crate::r#static::type_check::util::type_cmp;
use crate::r#static::xs_error::{name_err, syntax_err, warn, XSError};

pub fn xs_tc_stmt<'src>(
    (stmt, span): &'src Spanned<ASTreeNode>,
    type_env: &'src mut TypeEnv,
    errs: &mut Vec<XSError>,
    is_top_level: bool,
) { match stmt {
    // an include statement is always parsed with a string literal
    ASTreeNode::Include(_) => {
        if !is_top_level {
            errs.push(syntax_err(
                "An include statement may only appear at the top of the module", span
            ))
        }
    }
    ASTreeNode::VarDef {
        is_extern,
        is_static,
        is_const,
        type_,
        name: spanned_name,
        value
    } => {
        let (name, name_span) = spanned_name;
        match type_env.get(name) {
            Some(_) => {
                errs.push(name_err(
                    "Variable name is already in use", name_span
                ))
            }
            None => {
                type_env.push((name.clone(), type_.clone()));
            }
        };

        if !is_top_level && *is_extern {
            errs.push(syntax_err(
                "Local variables cannot be declared as `extern`", name_span
            ))
        }
        let Some(spanned_expr) = value else {
            if *is_const {
                errs.push(syntax_err(
                    "Variable declared as `const` must be initialised with a value", name_span
                ))
            }

            return;
        };

        let (expr, expr_span) = spanned_expr;

        if is_top_level || *is_const {
            match expr {
                Expr::Literal(Literal::Str(_)) if is_top_level => {
                    errs.push(warn(
                        "Top level string initializers do not work correctly. yES",
                        expr_span
                    ));
                }
                Expr::Literal(_) => {}
                _ => {
                    errs.push(syntax_err(
                        "Top level or `const` variable initializers must be literals",
                        expr_span
                    ));
                }
            }
        }

        let Some(init_type) = xs_tc_expr(spanned_expr, type_env, errs) else {
            return;
        };

        type_cmp(type_, init_type, expr_span, errs, false);

    }
    ASTreeNode::VarAssign {
        name: spanned_name,
        value: spanned_expr
    } => {
        if is_top_level {
            errs.push(syntax_err(
                "Assignments are not allowed at the top level", span
            ));
        }
        
        let (name, name_span) = spanned_name;

        let Some(type_) = type_env.get(name) else {
            errs.push(name_err(
                "Undefined variable", name_span
            ));
            return;
        };

        let Some(init_type) = xs_tc_expr(spanned_expr, type_env, errs) else {
            // An invalid expr will generate its own error
            return;
        };

        type_cmp(type_, init_type, &spanned_expr.1, errs, false);
    },
    ASTreeNode::RuleDef {
        name: (name, name_span),
        rule_opts: _rule_opts,
        body: (body, _body_span)
    } => {
        match type_env.get(name) {
            Some(_) => {
                errs.push(name_err(
                    "Variable name is already in use", name_span
                ))
            }
            None => {
                type_env.push((name.clone(), Type::Rule));
            }
        };
        
        let mut local_type_env = type_env.clone();
        for spanned_stmt in body.0.iter() {
            xs_tc_stmt(spanned_stmt, &mut local_type_env, errs, false);
        }
    }
    ASTreeNode::FnDef {
        is_mutable,
        return_type,
        name: (name, name_span),
        params,
        body: (body, _body_span)
    } => {
        let mut local_type_env = HashMap::with_capacity(params.len());
        for param in params {
            let (param_name, param_name_span) = &param.name;
            if let (Some(_), _) | (_, Some(_)) = (
                type_env.get(param_name), local_type_env.get(param_name)
            ) {
                errs.push(name_err(
                    "Variable name is already in use", param_name_span
                ))
            }
            local_type_env.push((param_name.clone(), param.type_.clone()));

            let (expr, expr_span) = &param.default;
            if let Expr::Literal(_) = expr {} else {
                errs.push(syntax_err(
                    "Parameter defaults must be literals",
                    expr_span,
                ));
            };
            
            // expr will generate its own error when it returns None
            let Some(param_default_value_type) = xs_tc_expr(&param.default, type_env, errs)
                else { continue; };
            type_cmp(
                &param.type_,
                param_default_value_type,
                expr_span,
                errs,
                false
            );
        }

        if let Some(_) = type_env.get(name) {
            // todo: check mutability
            errs.push(name_err(
                "Variable name is already in use", name_span
            ))
        } else {
            let mut param_types = local_type_env.values().cloned().collect::<Vec<Type>>();
            param_types.push(return_type.clone());
            type_env.push((name.clone(), Type::Func(param_types)));
        }

        local_type_env.extend(type_env.clone());
        local_type_env.push((Identifier::new("return"), return_type.clone()));

        // todo: figure out how to check returns on all fn paths
        for spanned_stmt in body.0.iter() {
            xs_tc_stmt(spanned_stmt, &mut local_type_env, errs, false);
        }
    },
    ASTreeNode::Return(spanned_expr) => {
        let Some(return_type) = type_env.get(&Identifier::new("return")) else {
            errs.push(syntax_err(
                "`return` statement is not allowed here",
                span
            ));
            return;
        };
        
        let Some(spanned_expr) = spanned_expr else {
            if *return_type != Type::Void {
                errs.push(syntax_err(
                    &format!("This function must return a value of type `{:}`", return_type),
                    span,
                ));
            }
            return;
        };
        if *return_type == Type::Void {
            errs.push(syntax_err("This function cannot return a value", span));
            return;
        }
        
        let (expr, expr_span) = spanned_expr;
        if let Expr::Paren(_) = expr {} else {
            errs.push(syntax_err(
                "`return` statement expressions must be enclosed in parenthesis. yES",
                expr_span,
            ));
        };

        // if expr returns None, it'll generate its own error
        let Some(return_expr_type) = xs_tc_expr(spanned_expr, type_env, errs) else {
            return;
        };
        
        type_cmp(return_type, return_expr_type, expr_span, errs, false);
    },
    ASTreeNode::IfElse { .. } => {},
    ASTreeNode::While { .. } => {},
    ASTreeNode::For { .. } => {},
    ASTreeNode::Switch { .. } => {},
    ASTreeNode::PostDPlus(_) => {},
    ASTreeNode::PostDMinus(_) => {},
    ASTreeNode::Break => {},
    ASTreeNode::Continue => {},
    ASTreeNode::LabelDef(_) => {},
    ASTreeNode::Goto(_) => {},
    ASTreeNode::Discarded(_) => {},
    ASTreeNode::Debug(_) => {},
    ASTreeNode::Breakpoint => {},
    ASTreeNode::Class { .. } => {},
}}