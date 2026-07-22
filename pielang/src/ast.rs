use crate::utils::{LocatedError, Ref, Span, StackMap};
use core::fmt;

/// 顶层语句允许 define 语句、claim 语句, check-same 语句和表达式。
#[derive(Debug, Clone)]
pub enum GlobalStatemant<'a> {
    /// `(claim varname type)`
    Claim(Span, Id<'a>, Type<'a>),

    /// `(define varname expression)`
    Define(Span, Id<'a>, Expr<'a>),

    /// `(check-same type expression expression)`
    CheckSame(Span, Expr<'a>, Expr<'a>, Expr<'a>),

    /// 表达式
    Expression(Expr<'a>),
}

/// 包含位置信息的一个符号
#[derive(Debug, Clone)]
pub struct Id<'a>(pub Span, pub &'a str);

impl<'a> fmt::Display for Id<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.1)
    }
}

/// 表达式包含位置信息
#[derive(Debug, Clone)]
pub enum Expr<'a> {
    /// 字面量，表示一个值
    NatLit(Span, u64),

    AtomLit(Span, &'a str),

    /// 标识符，可以绑定到变量、函数、类型等
    Ident(Span, &'a str),

    /// 函数调用、值的构造（introduce）、解构（eliminate），以及 the 表达式
    AppExpr(Span, Vec<Expr<'a>>),

    // 以下为一些特殊语法项
    /// `(λ (ident+) expr)`
    LambdaExpr(Span, Vec<Id<'a>>, Ref<Expr<'a>>),

    /// `(Π ((ident expr)+) expr)`
    PiExpr(Span, Vec<(Id<'a>, Type<'a>)>, Ref<Expr<'a>>),

    /// `(→ expr+ expr)`
    ArrowExpr(Span, Vec<Type<'a>>),

    /// `(Σ ((ident expr)+) expr)`
    SigmaExpr(Span, Vec<(Id<'a>, Type<'a>)>, Ref<Expr<'a>>),
}

impl Expr<'_> {
    pub fn span(&self) -> Span {
        match self {
            Expr::NatLit(span, _) => *span,
            Expr::AtomLit(span, _) => *span,
            Expr::Ident(span, _) => *span,
            Expr::AppExpr(span, _) => *span,
            Expr::LambdaExpr(span, _, _) => *span,
            Expr::PiExpr(span, _, _) => *span,
            Expr::ArrowExpr(span, _) => *span,
            Expr::SigmaExpr(span, _, _) => *span,
        }
    }
}

impl<'a> fmt::Display for Expr<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;
        match self {
            NatLit(_, n) => write!(f, "{n}")?,
            AtomLit(_, a) => write!(f, "'{a}")?,
            Ident(_, id) => write!(f, "{id}")?,
            AppExpr(_, args) => {
                write!(f, "(")?;
                fmt_args(f, &args[..])?;
                write!(f, ")")?;
            }
            LambdaExpr(_, args, body) => {
                write!(f, "(λ (")?;
                fmt_args(f, &args[..])?;
                write!(f, ") {})", body)?;
            }
            PiExpr(_, args, body) => {
                write!(f, "(Π (")?;
                fmt_args_compact(f, &args[..])?;
                write!(f, ") {})", body)?;
            }
            ArrowExpr(_, args) => {
                write!(f, "(→ ")?;
                fmt_args(f, &args[..])?;
                write!(f, ")")?;
            }
            SigmaExpr(_, args, body) => {
                write!(f, "(Π (")?;
                fmt_args_compact(f, &args[..])?;
                write!(f, ") {})", body)?;
            }
        }
        Ok(())
    }
}

fn fmt_args(f: &mut fmt::Formatter<'_>, args: &[impl fmt::Display]) -> fmt::Result {
    write!(f, "{}", args[0])?;
    for a in &args[1..] {
        write!(f, " {}", a)?;
    }
    Ok(())
}

fn fmt_args_compact(
    f: &mut fmt::Formatter<'_>,
    args: &[(impl fmt::Display, impl fmt::Display)],
) -> fmt::Result {
    let (a, b) = &args[0];
    write!(f, "({} {})", a, b)?;
    for arg in &args[1..] {
        let (a, b) = arg;
        write!(f, "({} {})", a, b)?;
    }
    Ok(())
}

impl<'a> From<Id<'a>> for Expr<'a> {
    fn from(value: Id<'a>) -> Self {
        let Id(span, id) = value;
        Expr::Ident(span, id)
    }
}

/// 类型也是表达式
pub type Type<'a> = Expr<'a>;

/// Pie 的 Atom 由字母或者横线组成
pub static RE_ATOM_IDENT: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^[-\w--\d]+$").unwrap());

/// 内建单例对象
pub const PIE_BUILTIN_SINGLETONS: [&str; 9] = [
    "Atom", "Nat", "zero", "nil", "vecnil", "Trivial", "sole", "Absurd", "U",
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
        AppExpr(span, exprs) => match &exprs[0] {
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
                        loc: Some(id.span()),
                        erk: "claim: expect identifier".to_string(),
                    });
                };
                if is_builtin_name(id) {
                    return Err(LocatedError {
                        loc: Some(span_id),
                        erk: format!("claim: {} is not a valid Pie name", id),
                    });
                }
                Claim(span, crate::ast::Id(span_id, id), ty)
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
                        loc: Some(id.span()),
                        erk: "define: expect identifier".to_string(),
                    });
                };
                if is_builtin_name(id) {
                    return Err(LocatedError {
                        loc: Some(span_id),
                        erk: format!("define: {} is not a valid Pie name", id),
                    });
                }
                Define(span, crate::ast::Id(span_id, id), body)
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
            _ => Expression(AppExpr(span, exprs)),
        },
        _ => Expression(e),
    };
    Ok(stat)
}

pub fn is_builtin_name(name: &str) -> bool {
    PIE_BUILTIN_SINGLETONS.contains(&name)
        || PIE_BUILTIN_FUNCTIONS.iter().any(|(n, _)| n == &name)
        || PIE_KEYWORDS.contains(&name)
}

pub fn check_builtin_names<'a>(
    args: impl IntoIterator<Item = &'a Id<'a>>,
) -> Result<(), LocatedError<String>> {
    for Id(span, id) in args {
        if is_builtin_name(id) {
            return Err(LocatedError {
                loc: Some(*span),
                erk: format!("{} is not a valid Pie name", id),
            });
        }
    }
    Ok(())
}

/// - checking the λ-expressions do not use built-in names as variable names
/// - checking built-in names have correct number of arguments
/// - checking no unbound variables
pub fn check_syntax<'a>(
    expr: &'a Expr<'a>,
    env: &StackMap<Option<&'a str>, ()>,
) -> Result<(), LocatedError<String>> {
    use crate::ast::Id;
    use Expr::*;
    'm: {
        match expr {
            NatLit(_, _) | AtomLit(_, _) => {}
            Ident(sp, id) => {
                if PIE_BUILTIN_SINGLETONS.contains(id) {
                    break 'm;
                }
                if let Some((_, argc)) = PIE_BUILTIN_FUNCTIONS.iter().find(|(i, _)| i == id) {
                    return Err(LocatedError {
                        loc: Some(*sp),
                        erk: format!("{} need {} arguments", id, argc),
                    });
                }
                if !env
                    .iter()
                    .any(|(k, _)| k.as_deref().is_some_and(|k| k == *id))
                {
                    return Err(LocatedError {
                        loc: Some(*sp),
                        erk: format!("undefined identifier: {}", id),
                    });
                }
            }
            AppExpr(sp, exprs) => {
                let exprs_to_check;
                match &**exprs {
                    [Ident(sp_id, id), args @ ..] => {
                        // TODO: check Universe Hierarchy extension
                        // (add1 e), (= e e e), (same e), ...
                        if let Some((_, argn)) = PIE_BUILTIN_FUNCTIONS.iter().find(|(i, _)| i == id)
                        {
                            if args.len() != *argn {
                                return Err(LocatedError {
                                    loc: Some(*sp),
                                    erk: format!(
                                        "{} need {} arguments, got {}",
                                        id,
                                        argn,
                                        args.len()
                                    ),
                                });
                            }
                            exprs_to_check = args;
                        }
                        // zero, nil, ...
                        else if PIE_BUILTIN_SINGLETONS.contains(id) {
                            return Err(LocatedError {
                                loc: Some(*sp_id),
                                erk: format!("{} cannot be caller", id),
                            });
                        } else {
                            exprs_to_check = &exprs[..];
                        }
                    }
                    _ => {
                        exprs_to_check = &exprs[..];
                    }
                }
                for e in exprs_to_check {
                    check_syntax(e, env)?;
                }
            }
            LambdaExpr(_, args, body) => {
                let mut new_env = env.clone();
                for Id(sp, id) in args {
                    if is_builtin_name(id) {
                        return Err(LocatedError {
                            loc: Some(*sp),
                            erk: format!("lambda: {} is not a valid name for arguments", id),
                        });
                    }
                    new_env = new_env.insert(Some(*id), ());
                }
                check_syntax(body, &new_env)?;
            }
            ArrowExpr(_, args) => {
                for e in args {
                    check_syntax(e, env)?;
                }
            }
            PiExpr(_, args, body) => {
                let mut new_env = env.clone();
                for (Id(sp, id), e_ty) in args {
                    if is_builtin_name(id) {
                        return Err(LocatedError {
                            loc: Some(*sp),
                            erk: format!("Pi: {} is not a valid name for arguments", id),
                        });
                    }
                    new_env = new_env.insert(Some(*id), ());
                    check_syntax(e_ty, &new_env)?;
                }
                check_syntax(body, &new_env)?;
            }
            SigmaExpr(_, args, body) => {
                let mut new_env = env.clone();
                for (Id(sp, id), e_ty) in args {
                    if is_builtin_name(id) {
                        return Err(LocatedError {
                            loc: Some(*sp),
                            erk: format!("Pi: {} is not a valid name for arguments", id),
                        });
                    }
                    new_env = new_env.insert(Some(*id), ());
                    check_syntax(e_ty, &new_env)?;
                }
                check_syntax(body, &new_env)?;
            }
        }
    }
    Ok(())
}

pub fn to_builtin_name(x: &str) -> &'static str {
    for n in PIE_BUILTIN_SINGLETONS {
        if x == n {
            return n;
        }
    }
    for n in PIE_BUILTIN_FUNCTIONS.map(|x| x.0) {
        if x == n {
            return n;
        }
    }
    panic!("{x} is not a builtin name")
}
