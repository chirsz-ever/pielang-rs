use crate::utils::{Ref, Span};

/// 顶层语句允许 define 语句、claim 语句和表达式。
#[derive(Debug, Clone)]
pub enum GlobalStatemant {
    /// `(claim varname type)`
    Claim(Span, Ident, Type),

    /// `(define varname expression)`
    Define(Span, Ident, Expr),

    /// `(check-same type expression expression)`
    CheckSame(Span, Expr, Expr, Expr),

    /// 表达式
    Expression(Expr),
}

/// 包含位置信息的一个符号
#[derive(Debug, Clone)]
pub struct Ident(pub Span, pub Ref<str>);

/// 表达式包含位置信息
#[derive(Debug, Clone)]
pub enum Expr {
    /// 字面量，表示一个值
    NatLit(Span, u64),

    AtomLit(Span, Ref<str>),

    /// 标识符，可以绑定到变量、函数、类型等
    Ident(Span, Ref<str>),

    /// 函数调用、值的构造（introduce）、解构（eliminate），以及 the 表达式
    App(Span, Vec<Expr>),

    /// 以下为一些特殊语法项

    /// `(λ (ident+) expr)`
    LambdaExpr(Span, Vec<Ident>, Ref<Expr>),

    /// `(Π ((ident expr)+) expr)`
    PiExpr(Span, Vec<(Ident, Type)>, Ref<Expr>),

    /// `(→ expr+ expr)`
    ArrowExpr(Span, Vec<Type>),

    /// `(Σ ((ident expr)+) expr)`
    SigmaExpr(Span, Vec<(Ident, Type)>, Ref<Expr>),
}

impl From<Ident> for Expr {
    fn from(value: Ident) -> Self {
        let Ident(span, id) = value;
        Expr::Ident(span, id)
    }
}

/// 类型也是表达式
pub type Type = Expr;

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
