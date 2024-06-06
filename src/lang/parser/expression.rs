use chumsky::prelude::*;
use crate::lang::ast::expr::Expr;
use crate::lang::ast::identifier::Identifier;
use crate::lang::lexer::token::Token;
use crate::lang::parser::parser_input::ParserInput;
use crate::lang::span::{Span, Spanned};

pub fn expression<'tokens>(
) -> impl Parser<
    'tokens,
    ParserInput<'tokens>,
    Spanned<Expr>,
    extra::Err<Rich<'tokens, Token, Span>>,
> + Clone {
    recursive(|expr| {
        let paren_expr = expr.clone()
            .delimited_by(just(Token::LParen), just(Token::RParen))
            .map_with(|exp, info| (Expr::Paren(Box::new(exp)), info.span()));
        
        let lit = select! { Token::Literal(lit) => Expr::Literal(lit) }
            .map_with(|exp, info| (exp, info.span()));

        let id = select! {
            Token::Identifier(id) => Expr::Identifier(id),
            Token::Vector         => Expr::Identifier(Identifier("vector".to_string())),
        }.map_with(|exp, info| (exp, info.span()));
        
        let fn_call = id.clone().then(
            expr.clone()
            .separated_by(just(Token::Comma))
            // .allow_trailing() // todo: does XS allow trailing commas?
            .collect::<Vec<Spanned<Expr>>>()
            .delimited_by(just(Token::LParen), just(Token::RParen))
        ).map_with(|(name, args), info| {
            (Expr::FnCall { name: Box::new(name), args }, info.span())
        });

        let expr7 = choice((
            fn_call,
            paren_expr,
            lit,
            id,
        )).boxed();

        let unary = just(Token::Minus).or_not()
            .then(expr7)
            .map_with(|(sign, exp), info| match sign {
                Some(_) => (Expr::UMinus(Box::new(exp)), info.span()),
                None    => exp,
            }).boxed();

        let expr6 = unary.clone()
            .foldl_with(
                one_of([Token::Star, Token::FSlash, Token::PCent]).then(unary).repeated(),
                |a, (op, b), info| (match op {
                    Token::Star   => Expr::Star(Box::new(a), Box::new(b)),
                    Token::FSlash => Expr::FSlash(Box::new(a), Box::new(b)),
                    Token::PCent  => Expr::PCent(Box::new(a), Box::new(b)),
                    _             => Expr::Error("E6Unreachable".to_string()),
                }, info.span())
            ).boxed();

        let expr5 = expr6.clone()
            .foldl_with(
                one_of([Token::Plus, Token::Minus]).then(expr6).repeated(),
                |a, (op, b), info| (match op {
                    Token::Plus  => Expr::Plus(Box::new(a), Box::new(b)),
                    Token::Minus => Expr::Minus(Box::new(a), Box::new(b)),
                    _            => Expr::Error("E5Unreachable".to_string()),
                }, info.span())
            ).boxed();

        let expr4 = expr5.clone()
            .foldl_with(
                one_of([Token::Lt, Token::Le, Token::Gt, Token::Ge]).then(expr5).repeated(),
                |a, (op, b), info| (match op {
                    Token::Lt => Expr::Lt(Box::new(a), Box::new(b)),
                    Token::Le => Expr::Le(Box::new(a), Box::new(b)),
                    Token::Gt => Expr::Gt(Box::new(a), Box::new(b)),
                    Token::Ge => Expr::Ge(Box::new(a), Box::new(b)),
                    _         => Expr::Error("E4Unreachable".to_string()),
                }, info.span())
            ).boxed();

        let expr3 = expr4.clone()
            .foldl_with(
                one_of([Token::Deq, Token::Neq]).then(expr4).repeated(),
                |a, (op, b), info| (match op {
                    Token::Deq => Expr::Eq(Box::new(a), Box::new(b)),
                    Token::Neq => Expr::Ne(Box::new(a), Box::new(b)),
                    _          => Expr::Error("E3Unreachable".to_string()),
                }, info.span())
            ).boxed();

        let expr2 = expr3.clone()
            .foldl_with(
                just(Token::And).ignore_then(expr3).repeated(),
                |a, b, info| {
                    (Expr::And(Box::new(a), Box::new(b)), info.span())
                }
            ).boxed();

        let expr1 = expr2.clone()
            .foldl_with(
                just(Token::Or).ignore_then(expr2).repeated(),
                |a, b, info| {
                    (Expr::Or(Box::new(a), Box::new(b)), info.span())
                }
            ).boxed();

        expr1
    })
}