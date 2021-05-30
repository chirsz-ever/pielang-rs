use crate::utils::{Ref, Span};

/// 顶层语句允许 define 语句、claim 语句和表达式。
#[derive(Debug, Clone)]
pub enum GlobalStatemant {
    /// `(claim varname type)`
    Claim(Span, Symbol, Type),

    /// `(define varname expression)`
    Define(Span, Symbol, Expr),

    /// `(check-same type expression expression)`
    CheckSame(Span, Expr, Expr, Expr),

    /// 表达式
    Expression(Expr),
}

/// 字面量，目前有整数和原子
#[derive(Debug, Clone)]
pub enum Literal {
    /// 自然数
    Nat(u64),

    /// 原子
    Atom(Ref<str>),
}

/// 包含位置信息的一个符号
#[derive(Debug, Clone)]
pub struct Symbol(pub Span, pub Ref<str>);

/// 表达式包含位置信息
#[derive(Debug, Clone)]
pub enum Expr {
    /// 字面量，表示一个值
    Literal(Span, Literal),

    /// 标识符，可以绑定到变量、函数、类型等
    Identifier(Span, Ref<str>),

    /// 函数调用、值的构造（introduce）、解构（eliminate），以及 the 表达式
    List(Span, Vec<Expr>),

    /// 以下为一些特殊语法项

    /// `(λ (ident+) expr)`
    LambdaExpr(Span, Vec<Symbol>, Ref<Expr>),

    /// `(Π ((ident expr)+) expr)`
    PiExpr(Span, Vec<(Symbol, Type)>, Ref<Expr>),

    /// `(→ expr+ expr)`
    ArrowExpr(Span, Vec<Type>),

    /// `(Σ ((ident expr)+) expr)`
    SigmaExpr(Span, Vec<(Symbol, Type)>, Ref<Expr>),
}

/// 类型也是表达式
pub type Type = Expr;
