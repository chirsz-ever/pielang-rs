use crate::utils::{LocatedError, Ref, Span};

/// 顶层语句允许 define 语句、claim 语句, check-same 语句和表达式。
#[derive(Debug, Clone)]
pub enum GlobalStatemant<'a> {
    /// `(claim varname type)`
    Claim(Span, Ident<'a>, Type<'a>),

    /// `(define varname expression)`
    Define(Span, Ident<'a>, Expr<'a>),

    /// `(check-same type expression expression)`
    CheckSame(Span, Expr<'a>, Expr<'a>, Expr<'a>),

    /// 表达式
    Expression(Expr<'a>),
}

/// 包含位置信息的一个符号
#[derive(Debug, Clone)]
pub struct Ident<'a>(pub Span, pub &'a str);

/// 表达式包含位置信息
#[derive(Debug, Clone)]
pub enum Expr<'a> {
    /// 字面量，表示一个值
    NatLit(Span, u64),

    AtomLit(Span, &'a str),

    /// 标识符，可以绑定到变量、函数、类型等
    Ident(Span, &'a str),

    /// 函数调用、值的构造（introduce）、解构（eliminate），以及 the 表达式
    App(Span, Vec<Expr<'a>>),

    /// 以下为一些特殊语法项

    /// `(λ (ident+) expr)`
    LambdaExpr(Span, Vec<Ident<'a>>, Ref<Expr<'a>>),

    /// `(Π ((ident expr)+) expr)`
    PiExpr(Span, Vec<(Ident<'a>, Type<'a>)>, Ref<Expr<'a>>),

    /// `(→ expr+ expr)`
    ArrowExpr(Span, Vec<Type<'a>>),

    /// `(Σ ((ident expr)+) expr)`
    SigmaExpr(Span, Vec<(Ident<'a>, Type<'a>)>, Ref<Expr<'a>>),
}

impl<'a> From<Ident<'a>> for Expr<'a> {
    fn from(value: Ident<'a>) -> Self {
        let Ident(span, id) = value;
        Expr::Ident(span, id)
    }
}

/// 类型也是表达式
pub type Type<'a> = Expr<'a>;

/// Pie 的 Atom 由字母或者横线组成
pub static RE_ATOM_IDENT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[-\w--\d]+$").unwrap());

/// 内建单例对象
pub const PIE_BUILTIN_SINGLETONS: [&str; 8] = [
    "Atom", "Nat", "zero", "nil", "vecnil", "Trivial", "sole", "Absurd",
];

/// 内建函数名及参数数
pub const PIE_BUILTIN_FUNCTIONS: [(&str, usize); 32] = [
    // `(the Type expr)`
    ("the", 2),
    // Pair
    ("Pair", 2),
    ("cons", 2),
    ("car", 1),
    ("cdr", 1),
    // Nat
    ("add1", 1),
    ("which-Nat", 3),
    ("iter-Nat", 3),
    ("rec-Nat", 3),
    ("ind-Nat", 4),
    // List
    ("List", 1),
    ("::", 2),
    ("rec-List", 3),
    ("ind-List", 4),
    // Vec
    ("Vec", 2),
    ("vec::", 2),
    ("head", 1),
    ("tail", 1),
    ("ind-Vec", 5),
    // Equality
    ("=", 3),
    ("same", 1),
    ("symm", 1),
    ("cong", 2),
    ("replace", 3),
    ("trans", 2),
    ("ind-=", 3),
    // Either
    ("Either", 2),
    ("left", 1),
    ("right", 1),
    ("ind-Either", 4),
    // Absurd
    ("ind-Absurd", 2),
    // U
    ("U", 1),
];

/// 关键字
pub const PIE_KEYWORDS: [&str; 8] = ["quote", "Π", "Pi", "∏", "Σ", "Sigma", "λ", "lambda"];

pub fn to_statement<'a>(e: Expr<'a>) -> Result<GlobalStatemant<'a>, LocatedError<String>> {
    use Expr::*;
    use GlobalStatemant::*;
    let stat = match e {
        App(span, exprs) => match &exprs[0] {
            Ident(_, "claim") => {
                let args = exprs.len() - 1;
                let Ok([_, id, ty]): Result<[Expr<'_>; _], _> = exprs.try_into() else {
                    return Err(LocatedError {
                        loc: Some(span),
                        erk: format!("claim: expect 2 arguments, got {}", args),
                    });
                };
                let Ident(span_id, id) = id else {
                    return Err(LocatedError {
                        loc: Some(get_span(&id)),
                        erk: "claim: expect identifier".to_string(),
                    });
                };
                if is_builtin_name(id) {
                    return Err(LocatedError {
                        loc: Some(span_id),
                        erk: format!("claim: {} is not a valid Pie name", id),
                    });
                }
                Claim(span, crate::ast::Ident(span_id, id), ty)
            }
            Ident(_, "define") => {
                let args = exprs.len() - 1;
                let Ok([_, id, body]): Result<[Expr<'_>; _], _> = exprs.try_into() else {
                    return Err(LocatedError {
                        loc: Some(span),
                        erk: format!("define: expect 2 arguments, got {}", args),
                    });
                };
                let Ident(span_id, id) = id else {
                    return Err(LocatedError {
                        loc: Some(get_span(&id)),
                        erk: "define: expect identifier".to_string(),
                    });
                };
                if is_builtin_name(id) {
                    return Err(LocatedError {
                        loc: Some(span_id),
                        erk: format!("define: {} is not a valid Pie name", id),
                    });
                }
                Define(span, crate::ast::Ident(span_id, id), body)
            }
            Ident(_, "check-same") => {
                let args = exprs.len() - 1;
                let Ok([_, ty, e1, e2]): Result<[Expr<'_>; _], _> = exprs.try_into() else {
                    return Err(LocatedError {
                        loc: Some(span),
                        erk: format!("check-same: expect 3 arguments, got {}", args),
                    });
                };
                CheckSame(span, ty, e1, e2)
            }
            _ => Expression(App(span, exprs)),
        },
        _ => Expression(e),
    };
    Ok(stat)
}

pub fn get_span(e: &Expr) -> Span {
    match e {
        Expr::NatLit(span, _) => *span,
        Expr::AtomLit(span, _) => *span,
        Expr::Ident(span, _) => *span,
        Expr::App(span, _) => *span,
        Expr::LambdaExpr(span, _, _) => *span,
        Expr::PiExpr(span, _, _) => *span,
        Expr::ArrowExpr(span, _) => *span,
        Expr::SigmaExpr(span, _, _) => *span,
    }
}

pub fn is_builtin_name(name: &str) -> bool {
    PIE_BUILTIN_SINGLETONS.contains(&name)
        || PIE_BUILTIN_FUNCTIONS.iter().any(|(n, _)| n == &name)
        || PIE_KEYWORDS.contains(&name)
}

pub fn check_builtin_names<'a>(args: impl IntoIterator<Item = &'a Ident<'a>>) -> Result<(), LocatedError<String>> {
    for Ident(span, id) in args {
        if is_builtin_name(id) {
            return Err(LocatedError {
                loc: Some(*span),
                erk: format!("{} is not a valid Pie name", id),
            });
        }
    }
    Ok(())
}
