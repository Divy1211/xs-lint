use ropey::Rope;
use tower_lsp::lsp_types::{InlayHint, InlayHintLabel, InlayHintTooltip, MarkupContent, MarkupKind, Range};
use xsc_core::parsing::ast::{AstNode, Expr, Type};
use xsc_core::parsing::span::Spanned;
use xsc_core::r#static::info::{IdInfo, TypeEnv};
use xsc_core::r#static::type_check::{dist, get_arg_name, get_param_name};
use crate::fmt::pos_info::{pos_from_span, span_from_pos};

fn xs_hints_expr(
    (expr, _span): &Spanned<Expr>,
    hints: &mut Vec<InlayHint>,
    src: &Rope,
    env: &TypeEnv,
) { match expr {
    Expr::Literal(_) => {}
    Expr::Identifier(_) => {}
    Expr::Paren(inner) => {
        xs_hints_expr(inner, hints, src, env);
    }
    Expr::Vec { x, y, z } => {
        xs_hints_expr(x, hints, src, env);
        xs_hints_expr(y, hints, src, env);
        xs_hints_expr(z, hints, src, env);
    }
    Expr::FnCall { name, args, .. } => {
        let Some(id_info @ IdInfo { type_: Type::Fn { type_sign, .. }, doc, .. }) = &env.get(&name.0) else {
            return;
        };

        for ((param_name, _param_type), arg) in type_sign[..type_sign.len()-1].iter().zip(args) {
            if let (Some((arg_name, _type)), Some(param_name)) = (get_arg_name(arg, env, Some(Type::Void)), get_param_name(param_name)) {
                // don't inlay low quality names like i, j, v1, v2, etc.
                if param_name.len() < 3 {
                    continue;
                }
                let di = dist(&param_name, &arg_name);
                if di < 0.2 {
                    continue;
                }
            }

            let range = pos_from_span(src, &arg.1);

            hints.push(InlayHint {
                position: range.0,
                label: InlayHintLabel::String(format!("{}:", param_name.0.clone())),
                kind: None,
                text_edits: None,
                tooltip: Some(InlayHintTooltip::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: doc.render(&name.0, id_info),
                })),
                padding_left: None,
                padding_right: Some(true),
                data: None,
            });
        }
    }
    Expr::Neg(inner) | Expr::Not(inner) => {
        xs_hints_expr(inner, hints, src, env);
    }
    Expr::Star(expr1, expr2) |
    Expr::FSlash(expr1, expr2) |
    Expr::PCent(expr1, expr2) |
    Expr::Plus(expr1, expr2) |
    Expr::Minus(expr1, expr2) |
    Expr::Lt(expr1, expr2) |
    Expr::Gt(expr1, expr2) |
    Expr::Le(expr1, expr2) |
    Expr::Ge(expr1, expr2) |
    Expr::Eq(expr1, expr2) |
    Expr::Ne(expr1, expr2) |
    Expr::And(expr1, expr2) |
    Expr::Or(expr1, expr2) => {
        xs_hints_expr(expr1, hints, src, env);
        xs_hints_expr(expr2, hints, src, env);
    }
}}

fn xs_hints(
    (node, _span): &Spanned<AstNode>,
    hints: &mut Vec<InlayHint>,
    src: &Rope,
    env: &TypeEnv,
) { match node {
    AstNode::Error => {}
    AstNode::Include(_) => {}
    AstNode::VarDef { value, .. } => {
        if let Some(value) = value {
            xs_hints_expr(value, hints, src, env);
        }
    }
    AstNode::VarAssign { value, .. } => {
        xs_hints_expr(value, hints, src, env);
    }
    AstNode::RuleDef { body, .. } => {
        for stmt in body.0.iter() {
            xs_hints(stmt, hints, src, env);
        }
    }
    AstNode::FnDef { body, .. } => {
        for stmt in body.0.iter() {
            xs_hints(stmt, hints, src, env);
        }
    }
    AstNode::Return(_) => {}
    AstNode::IfElse { condition, consequent, alternate } => {
        xs_hints_expr(condition, hints, src, env);
        for stmt in consequent.0.iter() {
            xs_hints(stmt, hints, src, env);
        }
        if let Some(alternate) = alternate {
            for stmt in alternate.0.iter() {
                xs_hints(stmt, hints, src, env);
            }
        }
    }
    AstNode::While { condition, body } => {
        xs_hints_expr(condition, hints, src, env);
        for stmt in body.0.iter() {
            xs_hints(stmt, hints, src, env);
        }
    }
    AstNode::For { var, condition, body } => {
        xs_hints(var, hints, src, env);
        xs_hints_expr(condition, hints, src, env);
        for stmt in body.0.iter() {
            xs_hints(stmt, hints, src, env);
        }
    }
    AstNode::Switch { clause, cases } => {
        xs_hints_expr(clause, hints, src, env);
        for (case, body) in cases {
            if let Some(case) = case {
                xs_hints_expr(case, hints, src, env);
            }
            for stmt in body.0.iter() {
                xs_hints(stmt, hints, src, env);
            }
        }
    }
    AstNode::PostDPlus(_) => {}
    AstNode::PostDMinus(_) => {}
    AstNode::Break => {}
    AstNode::Continue => {}
    AstNode::LabelDef(_) => {}
    AstNode::Goto(_) => {}
    AstNode::Discarded(expr) => {
        xs_hints_expr(expr, hints, src, env);
    }
    AstNode::Debug(_) => {}
    AstNode::Breakpoint => {}
    AstNode::Class { .. } => {}
}}

pub fn gen_inlay_hints(src: &Rope, ast: &Vec<Spanned<AstNode>>, env: &TypeEnv, range: Range) -> Vec<InlayHint> {
    let range_span = span_from_pos(src, &range.start, &range.end);

    let mut hints = Vec::new();
    for node in ast {
        if node.1.end < range_span.start || range_span.end < node.1.start {
            continue;
        }
        xs_hints(node, &mut hints, src, env);
    }

    hints
}