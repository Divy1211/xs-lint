use crate::lang::ast::literal::{Identifier, Literal};
use crate::lang::span::Span;

#[derive(Debug, Clone)]
pub enum Token {
    Plus,
    Minus,
    Star,
    FSlash,
    PCent,
    DPlus,
    DMinus,
    LT,
    GT,
    LE,
    GE,
    Deq,
    Neq,
    And,
    Or,
    
    Eq,
    LBrace,
    RBrace,
    LParen,
    RParen,
    SColon,
    Colon,
    Comma,
    Dot,
    
    Literal(Literal),
    Identifier(Identifier),
    
    Comment(String),

    Vector,
    Include,
    Switch,
    Case,
    While,
    Break,
    Default,
    Rule,
    If,
    Then,
    Else,
    Goto,
    Label,
    For,
    Dbg,
    Return,
    Void,
    Int,
    Float,
    String,
    Const,
    Priority,
    MinInterval,
    MaxInterval,
    HighFrequency,
    Active,
    Inactive,
    Group,
    InfiniteLoopLimit,
    InfiniteRecursionLimit,
    Breakpoint,
    Static,
    Continue,
    Extern,
    Export,
    RunImmediately,
    Mutable,
    Class,
}