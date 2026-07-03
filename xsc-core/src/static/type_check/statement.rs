use std::collections::{HashMap};
use std::path::PathBuf;

use chumsky::container::Container;
use crate::doxygen::Doc;
use crate::parsing::ast::{AstNode, RuleOpt, Expr, Identifier, Literal, Type};
use crate::parsing::span::{Span, Spanned};
use crate::r#static::info::{
    gen_errs_from_path,
    AstCacheRef,
    Error,
    FnInfo,
    IdInfo,
    Modifiers,
    SrcCacheRef,
    SrcLoc,
    TypeEnv,
    WarningKind,
    XsError,
};
use crate::r#static::type_check::expression::xs_tc_expr;
use crate::r#static::type_check::util::{chk_rule_opt, combine_results, get_broken_path_name, type_cmp};

#[allow(clippy::too_many_arguments)]
pub fn xs_tc_stmt(
    path: &PathBuf,
    (stmt, span): &Spanned<AstNode>,
    type_env: &mut TypeEnv,
    ast_cache: AstCacheRef,
    src_cache: SrcCacheRef,
    comments: &Vec<Spanned<String>>,
    comment_pos: &mut usize,
    is_top_level: bool,
    is_breakable: bool,
    is_continuable: bool,
) -> Result<(), Vec<Error>> {
    let mut docstr = None;
    loop { match comments.get(*comment_pos) {
        Some((com, com_span)) if com.starts_with("/** @extern") => {
            *comment_pos += 1;
            let decl = com
                .trim_start_matches("/**")
                .trim_end_matches("*/")
                .trim()
                .trim_start_matches("@extern")
                .trim_end_matches(";")
                .trim();

            let mut decl = decl.split_whitespace().filter(|val| !val.is_empty()).collect::<Vec<&str>>();
            if decl.len() == 3 && decl[0] != "const" || decl.len() != 3 && decl.len() != 2 {
                type_env.add_err(path, XsError::warning(
                    com_span,
                    "Invalid syntax for extern declaration",
                    vec![],
                    WarningKind::InvalidExternDecl,
                ));
                break;
            }
            let mut is_const = false;
            if decl[0] == "const" {
                decl.remove(0);
                is_const = true;
            }
            let type_ = match decl[0] {
                "int"    => Type::Int,
                "float"  => Type::Float,
                "string" => Type::Str,
                "vector" => Type::Vec,
                "bool" => Type::Bool,
                _ => {
                    type_env.add_err(path, XsError::warning(
                        com_span,
                        &format!("Unrecognised type '{}'", decl[0]),
                        vec![],
                        WarningKind::InvalidExternDecl,
                    ));
                    break;
                }
            };

            let name = Identifier(decl[1].to_string());
            match type_env.get(&name) {
                Some(IdInfo { src_loc, modifiers, ..})
                if modifiers.is_extern() || get_broken_path_name(path) == get_broken_path_name(&src_loc.file_path)
                => {
                    type_env.add_err(path, XsError::redefined_name(
                        &name,
                        span,
                        &src_loc,
                        None,
                    ))
                }
                _ => {
                    type_env.set(&name, IdInfo::from_with_mods(
                        &type_,
                        SrcLoc::from(path, com_span),
                        Doc::None,
                        Modifiers::var(false, is_const, true, false)
                    ));
                }
            };
        }
        Some((com, com_span)) if com_span.end <= span.start => {
            *comment_pos += 1;
            docstr = Some((com, com_span));
        }
        _ => break,
    }};
    let (doc, _temp_ignore) = docstr
        .map(|(com, span)| {
            match Doc::parse(com) {
                Err(err) => {
                    type_env.add_err(path, XsError::warning(
                        span,
                        &format!("Unrecognised warning name '{}'", err),
                        vec![],
                        WarningKind::UnknownWarningName,
                    ));
                    (Doc::None, None)
                }
                Ok(Doc::Ignore(ignores)) => {
                    (Doc::None, Some(type_env.temp_ignore(ignores)))
                }
                Ok(doc) => (doc, None)
            }
        })
        .unwrap_or((Doc::None, None));

match stmt {
    AstNode::Error => { Ok(()) },
    // an include statement is always parsed with a string literal
    AstNode::Include((filename, _span)) => {
        if !is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "An {0} statement is only allowed at the top level",
                vec!["include"],
            ));
            return Ok(());
        }
        
        let include_dirs = type_env.include_dirs.clone();
        let deps = type_env.dependencies.as_mut()
            .expect("Re-using type_env is not supported")
            .entry(path.clone())
            .or_default();
        
        let mut result = None;
        for inc_path in include_dirs.iter() {
            let mut inc_path = inc_path.clone();
            inc_path.push(&filename[1..(filename.len()-1)]);
            if inc_path.is_file() {
                deps.push(inc_path.clone());
                drop(_temp_ignore);
                result = Some(gen_errs_from_path(&inc_path, type_env, ast_cache, src_cache));
                break
            }
        }

        let Some(result) = result else {
            type_env.add_err(path, XsError::unresolved_include(
                filename,
                span,
            ));
            return Ok(())
        };
        result
    }
    AstNode::VarDef {
        is_export,
        is_extern,
        is_static,
        is_const,
        type_,
        name: spanned_name,
        value
    } => {
        let (name, name_span) = spanned_name;
        match type_env.get(name) {
            Some(IdInfo { src_loc, modifiers, .. })
            if modifiers.is_extern() || get_broken_path_name(path) == get_broken_path_name(&src_loc.file_path)
            => {
                type_env.add_err(path, XsError::redefined_name(
                    name,
                    name_span,
                    &src_loc,
                    None,
                ))
            }
            _ => {
                type_env.set(name, IdInfo::from_with_mods(
                    type_,
                    SrcLoc::from(path, name_span),
                    doc,
                    Modifiers::var(*is_static, *is_const, *is_extern, *is_export)
                ));
            }
        };

        if !is_top_level && *is_extern {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "Local variables cannot be declared as {0}",
                vec!["extern"],
            ));
        }

        if !is_top_level && *is_export {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "Local variables cannot be declared as {0}",
                vec!["export"],
            ));
        }

        if is_top_level && *is_static {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "Global variables declared as {0} cause a silent XS crash. yES",
                vec!["static"],
            ));
        }

        if *is_export && *is_const {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "Variables declared as both {0} and {1} cause a silent XS crash. yES",
                vec!["export", "const"],
            ));
        }

        if *is_static && *is_const {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "{0} variables cannot be declared as {1}",
                vec!["static", "const"],
            ));
        }

        let Some(spanned_expr) = value else {
            if *is_const {
                type_env.add_err(path, XsError::syntax(
                    name_span,
                    "Variable declared as {0} must be initialised with a value",
                    vec!["const"],
                ));
            }
            return Ok(());
        };

        let (expr, expr_span) = spanned_expr;

        if is_top_level || *is_const || *is_static {
            let mut gen_err = false;
            match expr {
                Expr::Literal(Literal::Str(_)) if is_top_level => {
                    type_env.add_err(path, XsError::warning(
                        expr_span,
                        "Top level initialized {0} do not work correctly. yES",
                        vec!["string"],
                        WarningKind::TopStrInit,
                    ));
                }
                Expr::Literal(_) | Expr::Neg(_) | Expr::Vec { .. } => { },
                Expr::Identifier(id) => {
                    match type_env.get(id) {
                        Some(IdInfo { modifiers, .. }) if !modifiers.is_const() => {
                            gen_err = true;
                        }
                        _ => {}
                    }
                }
                _ => {
                    gen_err = true;
                }
            }

            if gen_err {
                type_env.add_err(path, XsError::syntax(
                    expr_span,
                    "Top level, {0}, or, {1} variable initializers must be literals or consts",
                    vec!["const", "static"],
                ));
            } else if *is_const {
                let info = type_env.get_mut(name).expect("Value inserted above");
                info.init = Some(expr.clone());
            }
        }

        let mut gen_err = false;
        if *is_static {
            match expr {
                Expr::Literal(_) | Expr::Neg(_) | Expr::Vec { .. } => { }
                Expr::Identifier(id) => 'consts: {
                    let Some(id_info) = type_env.get(id) else {
                        break 'consts;
                    };
                    if !id_info.modifiers.is_const() {
                        gen_err = true;
                    }
                }
                _ => {
                    gen_err = true;
                }
            }
        }

        if gen_err {
            type_env.add_err(path, XsError::syntax(
                expr_span,
                "{0} variable initializers must be literals or consts",
                vec!["static"],
            ));
        }

        let info = type_env.pop(name).expect("Value inserted above");
        let tc_result = xs_tc_expr(path, spanned_expr, type_env);
        type_env.set(name, info);
        let Some(init_type) = tc_result else {
            return Ok(());
        };

        type_env.add_errs(path, type_cmp(type_, &init_type, expr_span, false, false, false));
        
        Ok(())
    }
    AstNode::VarAssign {
        name: spanned_name,
        value: spanned_expr
    } => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "Assignments are only allowed in a local scope",
                vec![],
            ));
        }

        let (name, name_span) = spanned_name;

        match name.0.as_str() {
            "infiniteLoopLimit" => {
                type_env.add_err(path, XsError::warning(
                    span,
                    "Loops will run for one extra iteration than the defined limit. yES",
                    vec![],
                    WarningKind::InfLoopLim,
                ));
            }
            "infiniteRecursionLimit" => {
                type_env.add_err(path, XsError::warning(
                    span,
                    "Exceeding this recursion limit in a recursive function will cause a silent XS crash. yES",
                    vec![],
                    WarningKind::InfRecLim,
                ));
            }
            _ => {}
        }

        let Some(IdInfo { type_, modifiers, src_loc, .. }) = type_env.get(name) else {
            type_env.add_err(path, XsError::undefined_name(
                name,
                name_span,
            ));
            return Ok(());
        };

        if type_.is_concrete() {
            let current_file = get_broken_path_name(path);
            let def_file = get_broken_path_name(&src_loc.file_path);
            if current_file != def_file && !modifiers.is_extern() {
                type_env.add_err(path, XsError::private_name(
                    name,
                    name_span,
                    &src_loc,
                ));
            }
        }

        if modifiers.is_const() {
            type_env.add_err(path, XsError::syntax(
                span,
                "Cannot re-assign a value to a {0} variable",
                vec!["const"],
            ));
        }

        let Some(init_type) = xs_tc_expr(path, spanned_expr, type_env) else {
            // An invalid expr will generate its own error
            return Ok(());
        };

        type_env.add_errs(path, type_cmp(&type_, &init_type, &spanned_expr.1, false, false, false));

        Ok(())
    },
    AstNode::RuleDef {
        name: (name, name_span),
        rule_opts,
        body: (body, body_span)
    } => {
        if !is_top_level {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "A rule definition is only allowed at the top level",
                vec![],
            ));
        }

        let mut opt_spans: HashMap<&str, &Span> = HashMap::with_capacity(rule_opts.len());

        for (opt, opt_span) in rule_opts {
            let mut opt_expr_name = None;
            match opt {
                RuleOpt::Active | RuleOpt::Inactive => {
                    chk_rule_opt("activity", opt_span, &mut opt_spans, path, type_env);
                }
                RuleOpt::RunImmediately => {
                    chk_rule_opt("run immediately", opt_span, &mut opt_spans, path, type_env);
                }
                RuleOpt::HighFrequency => {
                    chk_rule_opt("min interval", opt_span, &mut opt_spans, path, type_env);
                    chk_rule_opt("max interval", opt_span, &mut opt_spans, path, type_env);
                }
                RuleOpt::MinInterval(expr) => {
                    opt_expr_name = Some((expr, "minInterval"));
                    chk_rule_opt("min interval", opt_span, &mut opt_spans, path, type_env);
                }
                RuleOpt::MaxInterval(expr) => {
                    opt_expr_name = Some((expr, "maxInterval"));
                    chk_rule_opt("max interval", opt_span, &mut opt_spans, path, type_env);
                }
                RuleOpt::Priority(expr) => {
                    opt_expr_name = Some((expr, "priority"));
                    chk_rule_opt("priority", opt_span, &mut opt_spans, path, type_env);
                }
                RuleOpt::Group((grp, _grp_span)) => {
                    if chk_rule_opt("group", opt_span, &mut opt_spans, path, type_env) {
                        type_env.add_group(grp)
                    }
                }
            }
            if let Some(((expr, span), name)) = opt_expr_name && let Expr::Identifier(id) = expr { 'consts: {
                let Some(id_info) = type_env.get(id) else {
                    break 'consts;
                };
                if !id_info.modifiers.is_const() {
                    type_env.add_err(path, XsError::syntax(
                        span,
                        "{0} values must be literals or consts",
                        vec![name],
                    ));
                }
            }}
        }

        match type_env.get(name) {
            Some(IdInfo { src_loc: og_src_loc, ..}) => {
                type_env.add_err(path, XsError::redefined_name(
                    name,
                    name_span,
                    &og_src_loc,
                    None,
                ))
            }
            None => {
                type_env.set_global(
                    name,
                    IdInfo::rule(
                        SrcLoc::from(path, name_span),
                        rule_opts.iter().map(|opt| opt.0.clone()).collect(),
                        doc
                    )
                );
            }
        };

        let mut fn_info = FnInfo::new(name, SrcLoc::from(path, body_span));
        fn_info.set(Identifier::new("return"), IdInfo::dummy(Type::Void));

        let old_env = type_env.get_fn_env();
        type_env.set_fn_env(fn_info);
        // this essentially treats nested fn defs like they're global fns.
        // nested fns aren't allowed in XS so this is fine because we
        // can't close over values
        
        let results = combine_results(body.iter()
            .map(|spanned_stmt| {
                xs_tc_stmt(
                    path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                    false, is_breakable, is_continuable,
                )
            })
        );
        
        type_env.save_fn_env(name);
        
        // restore the old fn env if it existed
        if let Some(env) = old_env {
            type_env.set_fn_env(env);
        };

        results
    }
    AstNode::FnDef {
        is_mutable,
        return_type,
        name: (name, name_span),
        params,
        body: (body, body_span)
    } => {
        if !is_top_level {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "A function definition is only allowed at the top level",
                vec![],
            ));
        }

        let mut fn_info = FnInfo::new(name, SrcLoc::from(path, body_span));
        fn_info.set(Identifier::new("return"), IdInfo::dummy(return_type.clone()));
        
        let old_env = type_env.get_fn_env();
        type_env.set_fn_env(fn_info);
        // this essentially treats nested fn defs like they're global fns.
        // nested fns aren't allowed in XS so this is fine because we
        // can't close over values
        
        if params.len() > 12 {
            type_env.add_err(path, XsError::syntax(
                name_span,
                "XS functions cannot have more than 12 parameters.",
                vec![],
            ));
        }
        
        for param in params {
            let (param_name, param_name_span) = &param.name;
            if let Some(IdInfo {type_: _type, src_loc: og_src_loc, ..}) = type_env.get(param_name) {
                type_env.add_err(path, XsError::redefined_name(
                    param_name,
                    param_name_span,
                    &og_src_loc,
                    None,
                ))
            }
            
            type_env.set(param_name, IdInfo::from(&param.type_, SrcLoc::from(path, param_name_span)));

            let (expr, expr_span) = &param.default;

            let mut gen_err = false;
            match expr {
                Expr::Literal(_)  | Expr::Neg(_) | Expr::Vec { .. } => { }
                Expr::Identifier(id) => {
                    match type_env.get(id) {
                        Some(IdInfo { modifiers, .. }) if !modifiers.is_const() => {
                            gen_err = true;
                        }
                        _ => {}
                    }
                }
                _ => {
                    gen_err = true;
                }
            }

            if gen_err {
                type_env.add_err(path, XsError::syntax(
                    expr_span,
                    "Parameter defaults must be literals or consts",
                    vec![],
                ));
            }

            // expr will generate its own error when it returns None
            let Some(param_default_value_type) = xs_tc_expr(path, &param.default, type_env) else {
                continue;
            };
            type_env.add_errs(path, type_cmp(
                &param.type_,
                &param_default_value_type,
                expr_span,
                false,
                false,
                false
            ));
        }

        let mut new_type_sign = params
            .iter()
            .map(|param| (param.name.0.clone(), param.type_.clone()))
            .collect::<Vec<_>>();
        new_type_sign.push(("return".into(), return_type.clone()));

        // Nested fns are not allowed. If someone has accidentally defined a nested fn, pretend it
        // exists in the global space, an error was already issued for this above.
        match type_env.get(name) {
            Some(IdInfo{ type_: Type::Fn {
                is_mutable: was_mutable,
                type_sign
            }, src_loc: og_src_loc, .. }) => if !was_mutable {
                type_env.add_err(path, XsError::redefined_name(
                    name,
                    name_span,
                    &og_src_loc,
                    Some("Only mutable functions may be overridden"),
                ))
            } else if new_type_sign != *type_sign {
                type_env.add_err(path, XsError::redefined_name(
                    name,
                    name_span,
                    &og_src_loc,
                    Some("Type signatures of mutable functions must be the same"),
                ))
            } else {
                type_env.set_global(name, IdInfo::new(
                    Type::Fn { is_mutable: *is_mutable, type_sign: new_type_sign },
                    SrcLoc::from(path, name_span),
                    doc,
                ))
            },
            Some(IdInfo { src_loc: og_src_loc, .. }) => {
                type_env.add_err(path, XsError::redefined_name(
                    name,
                    name_span,
                    &og_src_loc,
                    None,
                ))
            },
            _ => {
                type_env.set_global(name, IdInfo::new(
                    Type::Fn { is_mutable: *is_mutable, type_sign: new_type_sign },
                    SrcLoc::from(path, name_span), doc
                ))
            }
        }

        let results = combine_results(body.iter()
            .map(|spanned_stmt| {
                xs_tc_stmt(
                    path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                    false, is_breakable, is_continuable,
                )
            })
        );
        
        type_env.save_fn_env(name);

        // restore the old fn env if it existed
        if let Some(env) = old_env {
            type_env.set_fn_env(env);
        };
        
        results
    },
    AstNode::Return(spanned_expr) => {
        let Some(IdInfo { type_: return_type, .. }) = type_env.get_return() else {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed inside functions or rules",
                vec!["return"],
            ));
            return Ok(());
        };

        let Some(spanned_expr) = spanned_expr else {
            if return_type != Type::Void {
                type_env.add_err(path, XsError::type_mismatch(
                    "void",
                    &return_type.to_string(),
                    span,
                    Some(&format!("This function's return type was declared as '{}'", return_type)),
                ));
            }
            return Ok(());
        };
        if return_type == Type::Void {
            type_env.add_err(path, XsError::syntax(
                span,
                "This function's return type was declared as {0}",
                vec!["void"]
            ));
            return Ok(());
        }

        let (expr, expr_span) = spanned_expr;
        if let Expr::Paren(_) = expr {} else {
            type_env.add_err(path, XsError::syntax(
                expr_span,
                "A {0} statement's expression must be enclosed in parenthesis. yES",
                vec!["return"]
            ));
        };

        // if expr returns None, it'll generate its own error
        let Some(return_expr_type) = xs_tc_expr(path, spanned_expr, type_env) else {
            return Ok(());
        };

        type_env.add_errs(path, type_cmp(&return_type, &return_expr_type, expr_span, false, false, false));

        Ok(())
    },
    AstNode::IfElse {
        condition,
        consequent,
        alternate
    } => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "An {0} statement is only allowed in a local scope",
                vec!["if"]
            ));
        }

        if let Some(type_) = xs_tc_expr(path, condition, type_env) && type_ != Type::Bool {
            type_env.add_err(path, XsError::type_mismatch(
                &type_.to_string(),
                "bool",
                &condition.1,
                None,
            ));
        }

        let results = consequent.0.iter()
            .map(|spanned_stmt| {
                xs_tc_stmt(
                    path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                    false, is_breakable, is_continuable,
                )
            })
            .collect::<Vec<_>>();

        if alternate.is_none() {
            return combine_results(results)
        }
        let alternate = alternate.as_ref().expect("Infallible: see above");

        combine_results(results.into_iter().chain(alternate.0.iter()
            .map(|spanned_stmt| {
                xs_tc_stmt(
                    path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                    false, is_breakable, is_continuable,
                )
            })
        ))
    },
    AstNode::While { condition, body } => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed in a local scope",
                vec!["while"]
            ));
        }

        if let Some(type_) = xs_tc_expr(path, condition, type_env) && type_ != Type::Bool {
            type_env.add_err(path, XsError::type_mismatch(
                &type_.to_string(),
                "bool",
                &condition.1,
                None,
            ));
        }
        
        combine_results(body.0.iter()
            .map(|spanned_stmt| {
                xs_tc_stmt(
                    path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                    false, true, true,
                )
            })
        )
    },
    AstNode::For { var, condition, body } => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed in a local scope",
                vec!["for"]
            ));
        }

        let (AstNode::VarAssign { name: (name, name_span), value }, assign_span) = var.as_ref()
        else { unreachable!("XSC Internal Error while type checking For at {}", var.as_ref().1) };

        let mut do_err = false;
        let mut actual_type = Type::Int;
        let mut do_set = true;
        if let Some(id_info) = type_env.get_mut(name) {
            if id_info.modifiers.is_const() {
                do_err = true;
            } else {
                id_info.make_const();
            }
            actual_type = id_info.type_.clone();
            do_set = false;
        };
        if actual_type != Type::Int {
            type_env.add_err(path, XsError::type_mismatch(
                &actual_type.to_string(),
                "int",
                name_span,
                None,
            ));
        }
        if do_err {
            type_env.add_err(path, XsError::syntax(
                assign_span,
                "Cannot re-assign a value to a {0} variable",
                vec!["const"],
            ));
        }

        if let Some(value_type) = xs_tc_expr(path, value, type_env) {
            type_env.add_errs(path, type_cmp(&Type::Int, &value_type, &value.1, false, false, false));
        }

        if do_set {
            type_env.set(name, IdInfo::from_with_mods(
                &Type::Int,
                SrcLoc::from(path, name_span),
                doc,
                Modifiers::var(false, true, false, false)),
            );
        }
        if let Some(type_) = xs_tc_expr(path, condition, type_env) && type_ != Type::Bool {
            type_env.add_err(path, XsError::type_mismatch(
                &type_.to_string(),
                "bool",
                &condition.1,
                None,
            ));
        }

        let result = combine_results(body.0.iter()
            .map(|spanned_stmt| {
                xs_tc_stmt(
                    path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                    false, true, true,
                )
            })
        );

        if let Some(id_info) = type_env.get_mut(name) {
            id_info.make_mut();
        };
        result
    },
    AstNode::Switch { clause, cases } => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed in a local scope",
                vec!["switch"]
            ));
        }

        // expression generates its own error for a None return
        if let Some(clause_type) = xs_tc_expr(path, clause, type_env) {
            type_env.add_errs(path, type_cmp(&Type::Int, &clause_type, &clause.1, false, false, false));
        }

        let mut default_span: Option<&Span> = None;
        let mut case_spans: HashMap<&Expr, &Span> = HashMap::with_capacity(cases.len());

        let mut results = Vec::with_capacity(cases.len());
        
        for (case_clause, (body, body_span)) in cases {
            // expression generates its own error for a None return
            results.push(combine_results(body.iter()
                .map(|spanned_stmt| {
                    xs_tc_stmt(
                        path, spanned_stmt, type_env, ast_cache, src_cache, comments, comment_pos,
                        false, true, is_continuable,
                    )
                })
            ));
            let Some(spanned_case_expr) = case_clause else {
                let Some(og_span) = default_span else {
                    default_span = Some(body_span);
                    continue;
                };
                type_env.add_err(path, XsError::warning(
                    og_span,
                    "Only the first default block will run when case matching fails",
                    vec![],
                    WarningKind::DupCase,
                ));
                type_env.add_err(path, XsError::warning(
                    body_span,
                    "Only the first default block will run when case matching fails",
                    vec![],
                    WarningKind::DupCase,
                ));
                continue;
            };
            let (case_expr, case_expr_span) = spanned_case_expr;
            if let Some(clause_type) = xs_tc_expr(path, spanned_case_expr, type_env) {
                type_env.add_errs(path, type_cmp(&Type::Int, &clause_type, case_expr_span, false, true, false));
            }
            if let Some(&og_span) = case_spans.get(case_expr) {
                type_env.add_err(path, XsError::warning(
                    og_span,
                    "Only the first case will run on a match",
                    vec![],
                    WarningKind::DupCase,
                ));
                type_env.add_err(path, XsError::warning(
                    &spanned_case_expr.1,
                    "Only the first case will run on a match",
                    vec![],
                    WarningKind::DupCase,
                ));
            } else {
                case_spans.push((case_expr, case_expr_span));
            }
        };
        
        combine_results(results)
    },
    AstNode::PostDPlus((id, id_span)) => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A postfix increment ({0}) statement is only allowed in a local scope",
                vec!["++"]
            ));
        }

        let Some(IdInfo { type_: id_type, .. }) = type_env.get(id) else {
            type_env.add_err(path, XsError::undefined_name(id, id_span));
            return Ok(());
        };

        if let Type::Int | Type::Float = id_type {
            return Ok(());
        }
        type_env.add_err(path, XsError::syntax(
            span,
            "A postfix increment ({0}) statement is only allowed on {1} values",
            vec!["++", "int | float"]
        ));
        
        Ok(())
    },
    AstNode::PostDMinus((id, id_span)) => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A postfix decrement ({0}) statement is only allowed in a local scope",
                vec!["--"]
            ));
        }

        let Some(IdInfo { type_: id_type, .. }) = type_env.get(id) else {
            type_env.add_err(path, XsError::undefined_name(id, id_span));
            return Ok(());
        };

        if let Type::Int | Type::Float = id_type {
            return Ok(());
        }
        type_env.add_err(path, XsError::syntax(
            span,
            "A postfix decrement ({0}) statement is only allowed on {1} values",
            vec!["--", "int | float"]
        ));
        
        Ok(())
    },
    AstNode::Break => {
        if !is_breakable {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed inside loops or switch cases",
                vec!["return"],
            ));
        }
        
        Ok(())
    },
    AstNode::Continue => {
        if !is_continuable {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed inside loops",
                vec!["continue"],
            ));
        }

        Ok(())
    },
    AstNode::LabelDef((id, id_span)) => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} definition is only allowed inside a local scope",
                vec!["label"],
            ));
        }

        if let Some(IdInfo { src_loc: og_src_loc, .. }) = type_env.get(id) {
            type_env.add_err(path, XsError::redefined_name(
                id,
                id_span,
                &og_src_loc,
                None,
            ));
            return Ok(());
        };
        type_env.set(id, IdInfo::new(Type::Label, SrcLoc::from(path, id_span), doc));

        Ok(())
    },
    AstNode::Goto((id, id_span)) => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed inside functions or rules",
                vec!["goto"],
            ));
        }
        let Some(IdInfo { type_: id_type, .. }) = type_env.get(id) else {
            type_env.add_err(path, XsError::undefined_name(id, id_span));
            return Ok(());
        };

        type_env.add_errs(path, type_cmp(&Type::Label, &id_type, id_span, false, false, false));

        Ok(())
    },
    AstNode::Discarded(spanned_expr) => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A discarded expression is only allowed in a local scope",
                vec![]
            ));
        }

        let (expr, expr_span) = spanned_expr;
        let Expr::FnCall { name, .. } = expr else {
            type_env.add_err(path, XsError::syntax(
                expr_span,
                "Only function calls can be discarded",
                vec![],
            ));
            return Ok(());
        };

        let nodiscard = type_env.get(&name.0).map(|id| id.doc.is_nodiscard()).unwrap_or(true);

        // an unknown identifier/deprecation error will be issued by xs_tc_expr when needed
        let return_value_type = xs_tc_expr(path, spanned_expr, type_env).unwrap_or(Type::Void);

        if Type::Void == return_value_type || !nodiscard {
            return Ok(());
        }
        type_env.add_err(path, XsError::warning(
            expr_span,
            "The return value of this function call is being ignored",
            vec![],
            WarningKind::DiscardedFn,
        ));

        Ok(())
    },
    AstNode::Debug((id, id_span)) => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed inside functions or rules",
                vec!["dbg"],
            ));
        }
        let Some(IdInfo { type_: id_type, .. }) = type_env.get(id) else {
            type_env.add_err(path, XsError::undefined_name(id, id_span));
            return Ok(());
        };

        let (Type::Fn { .. } | Type::Rule | Type::Class | Type::Label) = id_type else {
            return Ok(());
        };

        type_env.add_err(path, XsError::syntax(
            id_span,
            "A {0} statement can only be given {1} values",
            vec!["dbg", "int | float | bool | string | vector"],
        ));

        Ok(())
    },
    AstNode::Breakpoint => {
        if is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} statement is only allowed inside a local scope",
                vec!["breakpoint"],
            ));
        }

        type_env.add_err(path, XsError::warning(
            span,
            "Breakpoints cause XS execution to pause irrecoverably",
            vec![],
            WarningKind::BreakPt,
        ));

        Ok(())
    },
    AstNode::Class { name: (id, id_span), member_vars } => {
        if !is_top_level {
            type_env.add_err(path, XsError::syntax(
                span,
                "A {0} definition is only allowed at the top level",
                vec!["class"],
            ));
        }
        if let Some(IdInfo { src_loc: og_src_loc, .. }) = type_env.get(id) {
            type_env.add_err(path, XsError::redefined_name(
                id,
                id_span,
                &og_src_loc,
                None,
            ))
        } else {
            type_env.set(id, IdInfo::new(Type::Class, SrcLoc::from(path, id_span), doc));
        }

        let mut mem_name: HashMap<&Identifier, &Span> = HashMap::with_capacity(member_vars.len());
        for (member_var, _var_span) in member_vars {
            let AstNode::VarDef {
                type_,
                name: (id, id_span),
                value,
                is_export,
                is_extern,
                is_const,
                is_static
            } = member_var
            else { unreachable!("XSC Internal Error while type checking Class {}", id_span) };

            if *is_export {
                type_env.add_err(path, XsError::syntax(
                    id_span,
                    "Member variables cannot be declared as {0}",
                    vec!["export"],
                ));
            }
            if *is_extern {
                type_env.add_err(path, XsError::syntax(
                    id_span,
                    "Member variables cannot be declared as {0}",
                    vec!["extern"],
                ));
            }
            if *is_const {
                type_env.add_err(path, XsError::syntax(
                    id_span,
                    "Member variables cannot be declared as {0}",
                    vec!["const"],
                ));
            }
            if *is_static {
                type_env.add_err(path, XsError::syntax(
                    id_span,
                    "Member variables cannot be declared as {0}",
                    vec!["static"],
                ));
            }

            if let Some(&og_span) = mem_name.get(id) {
                type_env.add_err(path, XsError::redefined_name(
                    id,
                    id_span,
                    &SrcLoc::from(path, og_span),
                    None,
                ))
            } else {
                mem_name.push((id, id_span));
            }
            let init_value = match value.as_ref() {
                Some(val) => val,
                None => continue,
            };
            let (_init_value_expr, init_value_span) = init_value;

            let Some(init_value_type) = xs_tc_expr(path, init_value, type_env) else {
                continue;
            };
            type_env.add_errs(path, type_cmp(type_, &init_value_type, init_value_span, false, false, false));
        }

        type_env.add_err(path, XsError::warning(
            span,
            "Classes are unusable in XS",
            vec![],
            WarningKind::UnusableClasses,
        ));
        Ok(())
    },
}}
