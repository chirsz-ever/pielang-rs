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
                for Id(_, id) in args {
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
                for (Id(_, id), e_ty) in args {
                    check_syntax(e_ty, &new_env)?;
                    new_env = new_env.insert(Some(*id), ());
                }
                check_syntax(body, &new_env)?;
            }
            SigmaExpr(_, args, body) => {
                let mut new_env = env.clone();
                for (Id(_, id), e_ty) in args {
                    check_syntax(e_ty, &new_env)?;
                    new_env = new_env.insert(Some(*id), ());
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

#[cfg(test)]
mod unit_test {
    thread_local! {
        static EXPR_PARSER: crate::syntax::ExprParser = crate::syntax::ExprParser::new();
        static STATEMENT_PARSER: crate::syntax::GlobalStatemantParser = crate::syntax::GlobalStatemantParser::new();
    }

    #[test]
    fn test_parse_statement() {
        fn parse_stat(s: &str) -> String {
            STATEMENT_PARSER
                .with(|parser| parser.parse(s))
                .map_or_else(|err| format!("Error: {}", err), |_| "OK".to_string())
        }

        // (claim varname type)
        insta::assert_snapshot!(parse_stat("(claim x Nat)"), @"OK");
        insta::assert_snapshot!(parse_stat("(claim x)"), @"Error: 0:9: claim: expect 2 arguments, got 1");
        insta::assert_snapshot!(parse_stat("(claim x y z)"), @"Error: 0:13: claim: expect 2 arguments, got 3");
        insta::assert_snapshot!(parse_stat("(claim claim Nat)"), @"OK");
        insta::assert_snapshot!(parse_stat("(claim U Nat)"), @"Error: 7:8: claim: U is not a valid Pie name");
        // (define varname expression)
        insta::assert_snapshot!(parse_stat("(define x 0)"), @"OK");
        insta::assert_snapshot!(parse_stat("(define x)"), @"Error: 0:10: define: expect 2 arguments, got 1");
        insta::assert_snapshot!(parse_stat("(define x y z)"), @"Error: 0:14: define: expect 2 arguments, got 3");
        insta::assert_snapshot!(parse_stat("(define define 0)"), @"OK");
        insta::assert_snapshot!(parse_stat("(define check-same 0)"), @"OK");
        insta::assert_snapshot!(parse_stat("(define f (λ (U) 0))"), @"Error: 15:16: U is not a valid Pie name");
        insta::assert_snapshot!(parse_stat("(define f (λ (sole) 0))"), @"Error: 15:19: sole is not a valid Pie name");
        insta::assert_snapshot!(parse_stat("(define f (λ (Pair) 0))"), @"Error: 15:19: Pair is not a valid Pie name");
        insta::assert_snapshot!(parse_stat("(define f (λ (claim) 0))"), @"OK");
        insta::assert_snapshot!(parse_stat("(define f (λ (define) 0))"), @"OK");
        insta::assert_snapshot!(parse_stat("(define f (Pi ((U Nat)) Atom))"), @"Error: 16:17: U is not a valid Pie name");
        insta::assert_snapshot!(parse_stat("(define f (Pi ((x Nat)(U Nat)) Atom))"), @"Error: 23:24: U is not a valid Pie name");
        insta::assert_snapshot!(parse_stat("(define f (Sigma ((U Nat)) Atom))"), @"Error: 19:20: U is not a valid Pie name");
        insta::assert_snapshot!(parse_stat("(define f (Sigma ((x Nat)(U Nat)) Atom))"), @"Error: 26:27: U is not a valid Pie name");
        // (check-same type expression expression)
        insta::assert_snapshot!(parse_stat("(check-same Nat 0 0)"), @"OK");
        insta::assert_snapshot!(parse_stat("(check-same a b c)"), @"OK");
        insta::assert_snapshot!(parse_stat("(check-same a)"), @"Error: 0:14: check-same: expect 3 arguments, got 1");
        insta::assert_snapshot!(parse_stat("(check-same a b)"), @"Error: 0:16: check-same: expect 3 arguments, got 2");
        insta::assert_snapshot!(parse_stat("(check-same a b c d)"), @"Error: 0:20: check-same: expect 3 arguments, got 4");
    }

    #[test]
    fn test_parse_expression() {
        fn parse_expr(s: &str) -> String {
            EXPR_PARSER.with(|parser| parser.parse(s)).map_or_else(
                |err| format!("Error: {}", err),
                |expr| format!("{:?}", expr),
            )
        }

        // Nat literals
        insta::assert_snapshot!(parse_expr("0"), @"NatLit(Span(0, 1), 0)");
        insta::assert_snapshot!(parse_expr("1"), @"NatLit(Span(0, 1), 1)");
        insta::assert_snapshot!(parse_expr("9876"), @"NatLit(Span(0, 4), 9876)");
        insta::assert_snapshot!(parse_expr("01"), @"NatLit(Span(0, 2), 1)");
        // Atom literals
        insta::assert_snapshot!(parse_expr("'a"), @r#"AtomLit(Span(0, 2), "a")"#);
        insta::assert_snapshot!(parse_expr("'-a"), @r#"AtomLit(Span(0, 3), "-a")"#);
        insta::assert_snapshot!(parse_expr("'a-"), @r#"AtomLit(Span(0, 3), "a-")"#);
        insta::assert_snapshot!(parse_expr("'atom"), @r#"AtomLit(Span(0, 5), "atom")"#);
        insta::assert_snapshot!(parse_expr("'this-is-a-symbol"), @r#"AtomLit(Span(0, 17), "this-is-a-symbol")"#);
        insta::assert_snapshot!(parse_expr("'  btom"), @r#"AtomLit(Span(0, 7), "btom")"#);
        insta::assert_snapshot!(parse_expr("(quote ctom)"), @r#"AtomLit(Span(0, 12), "ctom")"#);
        insta::assert_snapshot!(parse_expr("(quote this-is-a-symbol)"), @r#"AtomLit(Span(0, 24), "this-is-a-symbol")"#);
        // symbols
        insta::assert_snapshot!(parse_expr("nil"), @r#"Ident(Span(0, 3), "nil")"#);
        insta::assert_snapshot!(parse_expr("x"), @r#"Ident(Span(0, 1), "x")"#);
        insta::assert_snapshot!(parse_expr("类型"), @r#"Ident(Span(0, 6), "类型")"#);
        // S-expressions
        insta::assert_snapshot!(parse_expr("(the (List Nat) nil)"), @r#"AppExpr(Span(0, 20), [Ident(Span(1, 4), "the"), AppExpr(Span(5, 15), [Ident(Span(6, 10), "List"), Ident(Span(11, 14), "Nat")]), Ident(Span(16, 19), "nil")])"#);
        insta::assert_snapshot!(parse_expr("(the(List Nat)nil)"), @r#"AppExpr(Span(0, 18), [Ident(Span(1, 4), "the"), AppExpr(Span(4, 14), [Ident(Span(5, 9), "List"), Ident(Span(10, 13), "Nat")]), Ident(Span(14, 17), "nil")])"#);
        insta::assert_snapshot!(parse_expr("(cons 2 (same 2))"), @r#"AppExpr(Span(0, 17), [Ident(Span(1, 5), "cons"), NatLit(Span(6, 7), 2), AppExpr(Span(8, 16), [Ident(Span(9, 13), "same"), NatLit(Span(14, 15), 2)])])"#);
        insta::assert_snapshot!(parse_expr("(lambda (x) x)"), @r#"LambdaExpr(Span(0, 14), [Id(Span(9, 10), "x")], Ident(Span(12, 13), "x"))"#);
        insta::assert_snapshot!(parse_expr("(lambda (x y) x)"), @r#"LambdaExpr(Span(0, 16), [Id(Span(9, 10), "x"), Id(Span(11, 12), "y")], Ident(Span(14, 15), "x"))"#);
        insta::assert_snapshot!(parse_expr("(Pi ((x Nat)) Atom)"), @r#"PiExpr(Span(0, 19), [(Id(Span(6, 7), "x"), Ident(Span(8, 11), "Nat"))], Ident(Span(14, 18), "Atom"))"#);
        insta::assert_snapshot!(parse_expr("(Pi ((x Nat)(y Atom)) Atom)"), @r#"PiExpr(Span(0, 27), [(Id(Span(6, 7), "x"), Ident(Span(8, 11), "Nat")), (Id(Span(13, 14), "y"), Ident(Span(15, 19), "Atom"))], Ident(Span(22, 26), "Atom"))"#);
        insta::assert_snapshot!(parse_expr("(Sigma ((x Nat)) Atom)"), @r#"SigmaExpr(Span(0, 22), [(Id(Span(9, 10), "x"), Ident(Span(11, 14), "Nat"))], Ident(Span(17, 21), "Atom"))"#);
        insta::assert_snapshot!(parse_expr("(Sigma ((x Nat)(y Atom)) Atom)"), @r#"SigmaExpr(Span(0, 30), [(Id(Span(9, 10), "x"), Ident(Span(11, 14), "Nat")), (Id(Span(16, 17), "y"), Ident(Span(18, 22), "Atom"))], Ident(Span(25, 29), "Atom"))"#);
        insta::assert_snapshot!(parse_expr("(λ (x) (add1 x))"), @r#"LambdaExpr(Span(0, 17), [Id(Span(5, 6), "x")], AppExpr(Span(8, 16), [Ident(Span(9, 13), "add1"), Ident(Span(14, 15), "x")]))"#);
        insta::assert_snapshot!(parse_expr(r"(the (Σ ((n Nat)) (= Nat n n)) (cons 2 (same 2)))"), @r#"AppExpr(Span(0, 50), [Ident(Span(1, 4), "the"), SigmaExpr(Span(5, 31), [(Id(Span(11, 12), "n"), Ident(Span(13, 16), "Nat"))], AppExpr(Span(19, 30), [Ident(Span(20, 21), "="), Ident(Span(22, 25), "Nat"), Ident(Span(26, 27), "n"), Ident(Span(28, 29), "n")])), AppExpr(Span(32, 49), [Ident(Span(33, 37), "cons"), NatLit(Span(38, 39), 2), AppExpr(Span(40, 48), [Ident(Span(41, 45), "same"), NatLit(Span(46, 47), 2)])])])"#);
        // brackets and braces
        insta::assert_snapshot!(parse_expr("[the Nat 1]"), @r#"AppExpr(Span(0, 11), [Ident(Span(1, 4), "the"), Ident(Span(5, 8), "Nat"), NatLit(Span(9, 10), 1)])"#);
        insta::assert_snapshot!(parse_expr("{the Nat 1}"), @r#"AppExpr(Span(0, 11), [Ident(Span(1, 4), "the"), Ident(Span(5, 8), "Nat"), NatLit(Span(9, 10), 1)])"#);
        // error cases
        insta::assert_snapshot!(parse_expr("("), @r#"
        Error: Unrecognized EOF found at 1
        Expected one of IDENT, NAT_LIT, PI, SIGMA, LAMBDA, FARROW, "'", "(", "[", "quote" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(add1 zero))"), @"Error: Unrecognized token `)` found at 11:12");
        insta::assert_snapshot!(parse_expr("(quote 'a)"), @r#"
        Error: Unrecognized token `'` found at 7:8
        Expected one of IDENT, PI, SIGMA, LAMBDA or "quote"
        "#);
        insta::assert_snapshot!(parse_expr("'a1"), @"Error: 1:3: Atoms can only consist of letters and hyphens");
        insta::assert_snapshot!(parse_expr("99999999999999999999999999999"), @"Error: 0:29: parse natural number failed");
        // FIXME: Pie should reject "-1"
        // insta::assert_snapshot!(parse_expr("-1"), @r#"Ident(Span(0, 2), "-1")"#);
        insta::assert_snapshot!(parse_expr("(lambda)"), @r#"
        Error: Unrecognized token `)` found at 7:8
        Expected one of "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(lambda 0)"), @r#"
        Error: Unrecognized token `0` found at 8:9
        Expected one of "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(lambda () 0)"), @"
        Error: Unrecognized token `)` found at 9:10
        Expected one of IDENT
        ");
        insta::assert_snapshot!(parse_expr("(lambda (zero) 0)"), @"Error: 9:13: zero is not a valid Pie name");
        insta::assert_snapshot!(parse_expr("(Pi () Nat)"), @r#"
        Error: Unrecognized token `)` found at 5:6
        Expected one of "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(Pi ((x)) Nat)"), @r#"
        Error: Unrecognized token `)` found at 7:8
        Expected one of IDENT, NAT_LIT, "'", "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(Pi ((zero Nat)) Nat)"), @"Error: 6:10: zero is not a valid Pie name");
        insta::assert_snapshot!(parse_expr("(Sigma () Nat)"), @r#"
        Error: Unrecognized token `)` found at 8:9
        Expected one of "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(Sigma ((x)) Nat)"), @r#"
        Error: Unrecognized token `)` found at 10:11
        Expected one of IDENT, NAT_LIT, "'", "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(Sigma ((zero Nat)) Nat)"), @"Error: 9:13: zero is not a valid Pie name");
        insta::assert_snapshot!(parse_expr("(Sigma 0 0)"), @r#"
        Error: Unrecognized token `0` found at 7:8
        Expected one of "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(->)"), @r#"
        Error: Unrecognized token `)` found at 3:4
        Expected one of IDENT, NAT_LIT, "'", "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(-> Nat)"), @r#"
        Error: Unrecognized token `)` found at 7:8
        Expected one of IDENT, NAT_LIT, "'", "(", "[" or "{"
        "#);
        insta::assert_snapshot!(parse_expr("(a)"), @r#"
        Error: Unrecognized token `)` found at 2:3
        Expected one of IDENT, NAT_LIT, "'", "(", "[" or "{"
        "#);
    }

    #[test]
    fn test_check_syntax() {
        fn parse_expression(s: &str) -> String {
            let expr = EXPR_PARSER.with(|parser| parser.parse(s)).unwrap();
            let env = crate::utils::StackMap::new();
            super::check_syntax(&expr, &env)
                .map_or_else(|err| format!("Error: {}", err), |_| "OK".to_string())
        }

        // checking built-in names have correct number of arguments
        insta::assert_snapshot!(parse_expression("(the Nat 0)"), @"OK");
        insta::assert_snapshot!(parse_expression("(the Nat)"), @"Error: 0:9: the need 2 arguments, got 1");
        insta::assert_snapshot!(parse_expression("(the Nat 0 1)"), @"Error: 0:13: the need 2 arguments, got 3");
        insta::assert_snapshot!(parse_expression("(add1 0)"), @"OK");
        insta::assert_snapshot!(parse_expression("add1"), @"Error: 0:4: add1 need 1 arguments");
        insta::assert_snapshot!(parse_expression("(zero 0)"), @"Error: 1:5: zero cannot be caller");
        insta::assert_snapshot!(parse_expression("(λ (x) add1)"), @"Error: 8:12: add1 need 1 arguments");
        // checking no unbound variables
        insta::assert_snapshot!(parse_expression("x"), @"Error: 0:1: undefined identifier: x");
        insta::assert_snapshot!(parse_expression("(λ (x) x)"), @"OK");
        insta::assert_snapshot!(parse_expression("(λ (x) y)"), @"Error: 8:9: undefined identifier: y");
        insta::assert_snapshot!(parse_expression("(λ (x) (λ (y) x))"), @"OK");
        insta::assert_snapshot!(parse_expression("(λ (x) (λ (y) z))"), @"Error: 16:17: undefined identifier: z");
        insta::assert_snapshot!(parse_expression("(λ (x) (λ (y) (λ (z) x)))"), @"OK");
        insta::assert_snapshot!(parse_expression("(λ (x) (λ (y) (λ (x) x)))"), @"OK");
    }
}
