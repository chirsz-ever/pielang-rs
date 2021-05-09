use crate::ast;
use crate::utils::*;
use fehler::{throw, throws};
use std::fmt;
use thiserror::Error;

macro_rules! claim_array {
    ($id:ident $name:ident: [$ty: ty; _] = $value:expr $(;)?) => {
        $id $name: [$ty; $value.len()] = $value;
    }
}

// TODO: 嵌入源代码位置信息，定义合适的错误类型

pub type ULevel = u32;

/// 表达式包含位置信息和元信息（类型等）
#[derive(Debug, Clone)]
pub enum Expr<MetaInfo, Variable = DBI> {
    /// 用于在抽象代码树中插入信息的中间层。
    Info(MetaInfo, Ref<Expr<MetaInfo, Variable>>),

    /// 字面量
    Literal(ast::Literal),

    /// 标识符，表示变量、函数、类型等
    Identifier(Variable),

    /// `(λ (ident) expr)`，被转换为单层
    LambdaExpr(Argument, Ref<Expr<MetaInfo, Variable>>),

    /// `(Π ((ident expr)) expr)`，被转换为单层
    /// 并将箭头表达式转换为 Π 表达式
    PiExpr(
        Argument,
        Ref<Type<MetaInfo, Variable>>,
        Ref<Expr<MetaInfo, Variable>>,
    ),

    /// `(Σ ((ident expr)) expr)`，被转换为单层
    SigmaExpr(
        Argument,
        Ref<Type<MetaInfo, Variable>>,
        Ref<Expr<MetaInfo, Variable>>,
    ),

    /// 函数调用，经过柯里化转换为只有一个参数
    Apply(Ref<Expr<MetaInfo, Variable>>, Ref<Expr<MetaInfo, Variable>>),

    /// 内建调用，用长度为 0 的 [`Vec`] 表示单例内建对象如 `nil`。
    // 这里或许可以用两层 `Info` 给第一参数加上元信息
    BuiltinApply(Ref<str>, Vec<Expr<MetaInfo, Variable>>),

    /// 类型的类型，后面的数字为 Universe Hierarchy 准备，目前统一是 0
    U(ULevel),
}

pub type Type<M, V = DBI> = Expr<M, V>;

impl<M, V> fmt::Display for Expr<M, V>
where
    M: fmt::Display,
    //V: fmt::Display,
    V: AsRef<str>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;
        match self {
            Info(info, inner) => {
                write!(f, "[{}: {}]", info, inner)
            }
            Literal(ast::Literal::Atom(atom)) => {
                write!(f, "'{}", atom)
            }
            Literal(ast::Literal::Nat(n)) => {
                write!(f, "{}", n)
            }
            Identifier(id) => {
                write!(f, "{}", id.as_ref())
            }
            LambdaExpr(arg, body) => {
                write!(f, "(λ ({}) {})", arg, body)
            }
            PiExpr(arg, ty, body) => {
                write!(f, "(Π (({} {})) {})", arg, ty, body)
            }
            SigmaExpr(arg, ty, body) => {
                write!(f, "(Σ (({} {})) {})", arg, ty, body)
            }
            Apply(fun, arg) => {
                write!(f, "({} {})", fun, arg)
            }
            BuiltinApply(bf, args) => {
                if args.len() == 0 {
                    return write!(f, "{}", bf);
                }
                write!(f, "({}", bf)?;
                for arg in args {
                    write!(f, " {}", arg)?;
                }
                write!(f, ")")
            }
            U(0) => {
                write!(f, "U")
            }
            U(n) => {
                write!(f, "(U {})", n)
            }
        }
    }
}

type Env<V> = StackMap<Option<Ref<str>>, Option<V>>;

/// 包装器，为 De Bruijn 索引表示实现 pretty print
pub struct DBIPPrint<'a, M, V>(pub &'a Expr<M>, pub &'a Env<V>);

impl<'a, M, V> fmt::Display for DBIPPrint<'a, M, V>
where
    M: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    where
        M: fmt::Display,
    {
        use Expr::*;
        let DBIPPrint(expr, env) = self;

        fn fetch_fresh_name<'a, V>(arg: Option<Ref<str>>, env: &'a Env<V>) -> Ref<str> {
            if let Some(sym) = arg {
                sym
            } else {
                let mut x = "x".to_owned();
                let mut n = 0;
                while env
                    .iter()
                    .any(|(y, _)| y.as_ref().map_or(false, |y| **y == *x))
                {
                    x = format!("x{}", n);
                    n += 1;
                }
                x.into()
            }
        }

        fn ext_env<V>(env: &Env<V>, name: &str) -> Env<V> {
            env.insert(Some(name.into()), None)
        }

        match expr {
            Info(info, inner) => {
                write!(f, "[{}: {}]", info, DBIPPrint(&**inner, *env))
            }
            Literal(ast::Literal::Atom(atom)) => {
                write!(f, "'{}", atom)
            }
            Literal(ast::Literal::Nat(n)) => {
                write!(f, "{}", n)
            }
            Identifier(n) => write!(
                f,
                "{}",
                fetch_fresh_name(
                    env.iter().nth(*n).and_then(|(k, _)| k.as_ref()).cloned(),
                    env
                )
            ),
            LambdaExpr(arg, body) => {
                let arg_name = fetch_fresh_name(arg.into(), env);
                write!(
                    f,
                    "(λ ({}) {})",
                    arg_name,
                    DBIPPrint(&**body, &ext_env(env, &arg_name))
                )
            }
            PiExpr(arg, ty, body) => {
                let arg_name = fetch_fresh_name(arg.into(), env);
                let new_env = ext_env(env, &arg_name);
                let ty = DBIPPrint(&**ty, &new_env);
                let body = DBIPPrint(&**body, &new_env);
                write!(f, "(Π (({} {})) {})", arg_name, ty, body)
            }
            SigmaExpr(arg, ty, body) => {
                let arg_name = fetch_fresh_name(arg.into(), env);
                let new_env = ext_env(env, &arg_name);
                let ty = DBIPPrint(&**ty, &new_env);
                let body = DBIPPrint(&**body, &new_env);
                write!(f, "(Σ (({} {})) {})", arg_name, ty, body)
            }
            Apply(fun, arg) => {
                let fun = DBIPPrint(&**fun, &env);
                let arg = DBIPPrint(&**arg, &env);
                write!(f, "({} {})", fun, arg)
            }
            BuiltinApply(bf, args) => {
                if args.len() == 0 {
                    return write!(f, "{}", bf);
                }
                write!(f, "({}", bf)?;
                for arg in args {
                    write!(f, " {}", DBIPPrint(&arg, &env))?;
                }
                write!(f, ")")
            }
            U(0) => {
                write!(f, "U")
            }
            U(n) => {
                write!(f, "(U {})", n)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ErrorKind {
    IllegalArgumentNumber {
        caller: String,
        valid_argc: usize,
        current_argc: usize,
    },
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use ErrorKind::*;
        match self {
            IllegalArgumentNumber {
                caller,
                valid_argc,
                current_argc,
            } => write!(
                f,
                "`{}` should take {} arguments, but here is {} arguments.",
                caller, valid_argc, current_argc
            ),
        }
    }
}

pub type Error = LocatedError<ErrorKind>;

/// 标识符，`Dummy` 用于将普通函数类型转换为 Pi 类型时，
/// 未来或可用于 `_` 语法
#[derive(Debug, Clone)]
pub enum Argument {
    Dummy,
    Symbol(Ref<str>),
}

impl From<Argument> for Option<Ref<str>> {
    fn from(arg: Argument) -> Self {
        match arg {
            Argument::Dummy => None,
            Argument::Symbol(sym) => Some(sym),
        }
    }
}

impl From<&Argument> for Option<Ref<str>> {
    fn from(arg: &Argument) -> Self {
        match arg {
            Argument::Dummy => None,
            Argument::Symbol(sym) => Some(sym.clone()),
        }
    }
}

impl fmt::Display for Argument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Argument::Dummy => write!(f, "_"),
            Argument::Symbol(sym) => write!(f, "{}", sym),
        }
    }
}

/// 将 Pi 表达式、Sigma 表达式展开为单层，箭头表达式转换为 Pi 表达式，
/// Pair 表达式转换为 Sigma 表达式，
/// 调用分别转化为函数调用和内建调用，并检查内建调用的合法性，将标识符 U 转换为
/// core_ast::Expr::U。
#[throws]
pub fn unfold(e: &ast::Expr) -> Expr<!, Ref<str>> {
    use ast::Expr::*;
    match e {
        Literal(_, lit) => Expr::Literal(lit.clone()),
        Identifier(_, id) => match &**id {
            "U" => Expr::U(0),
            _ if PIE_BUILTIN_SINGLETONS.contains(&&**id) => Expr::BuiltinApply(id.clone(), vec![]),
            _ => Expr::Identifier(id.clone()),
        },
        List(loc, exprs) => match &**exprs {
            [Identifier(_, f), args @ ..] if get_builtin_argument_number(f).is_some() => {
                let valid_argc = get_builtin_argument_number(f).unwrap();
                if args.len() == valid_argc {
                    Expr::BuiltinApply(f.clone(), map_result(args, unfold)?)
                } else {
                    throw!(Error {
                        loc: Some(loc.clone()),
                        erk: ErrorKind::IllegalArgumentNumber {
                            caller: f.to_string(),
                            valid_argc,
                            current_argc: args.len(),
                        }
                    });
                }
            }
            [Identifier(_, f), ty_a, ty_d] if &**f == "Pair" => Expr::SigmaExpr(
                Argument::Dummy,
                Ref::new(unfold(ty_a)?),
                Ref::new(unfold(ty_d)?),
            ),
            [Identifier(loc, f), args @ ..] if &**f == "Pair" => {
                throw!(Error {
                    loc: Some(loc.clone()),
                    erk: ErrorKind::IllegalArgumentNumber {
                        caller: f.to_string(),
                        valid_argc: 2,
                        current_argc: args.len(),
                    }
                })
            }
            _ => unfold_list(exprs)?,
        },
        LambdaExpr(_, args, body) => {
            let mut e = unfold(body)?;
            // 注意从后向前的顺序
            for ast::Symbol(_, sym) in args.iter().rev() {
                e = Expr::LambdaExpr(self::Argument::Symbol(sym.clone()), Ref::new(e));
            }
            e
        }
        PiExpr(_, args, body) => {
            let mut e = unfold(body)?;
            // 注意从后向前的顺序
            for (ast::Symbol(_, sym), ty) in args.iter().rev() {
                e = Expr::PiExpr(
                    self::Argument::Symbol(sym.clone()),
                    Ref::new(unfold(ty)?),
                    Ref::new(e),
                );
            }
            e
        }
        ArrowExpr(_, types) => {
            let mut tys = map_result(types, unfold)?.into_iter().rev();
            // syntax.lalrpop 中的规则保证至少有两项，所以以下 `unwrap` 不会有问题
            // 注意从后向前的顺序
            let mut e = tys.next().unwrap();
            for ty in tys {
                e = Expr::PiExpr(self::Argument::Dummy, Ref::new(ty), Ref::new(e));
            }
            e
        }
        SigmaExpr(_, args, body) => {
            let mut e = unfold(body)?;
            // 注意从后向前的顺序
            for (ast::Symbol(_, sym), ty) in args.iter().rev() {
                e = Expr::SigmaExpr(
                    self::Argument::Symbol(sym.clone()),
                    Ref::new(unfold(ty)?),
                    Ref::new(e),
                );
            }
            e
        }
    }
}

/// 将列表经过柯里化转换为函数调用
#[throws]
fn unfold_list(exprs: &[ast::Expr]) -> Expr<!, Ref<str>> {
    let mut es = exprs.iter();
    let mut f = unfold(es.next().unwrap())?;
    for e in es {
        f = Expr::Apply(Ref::new(f), Ref::new(unfold(e)?));
    }
    f
}

/// 通过内建函数名获取其应有的参数数量，如果传入的不是内建函数名，返回 `None`。
fn get_builtin_argument_number(fname: &str) -> Option<usize> {
    for (bf, n) in std::array::IntoIter::new(PIE_BUILTIN_FUNCTIONS) {
        if bf == fname {
            return Some(n);
        }
    }
    None
}

// 内建单例对象，其后是类型等级
claim_array! {
const PIE_BUILTIN_SINGLETONS: [&str; _] = [
    "Atom",
    "Nat",
    "zero",
    "nil",
    "vecnil",
    "Trivial",
    "sole",
    "Absurd",
];
}

// 内建函数名及参数数
claim_array! {
const PIE_BUILTIN_FUNCTIONS: [(&str, usize); _] = [
    // `(the Type expr)`
    ("the", 2),
    // Pair
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
];
}

// 内建无参数类型
#[allow(non_upper_case_globals)]
pub mod builtin_type {
    use super::Expr;

    macro_rules! claim_builtin_types {
        ($(($tynm:ident, $tyf:ident)),+ $(,)?) => {
            thread_local! {
                $(
                pub static $tynm: Expr<!> = Expr::BuiltinApply(stringify!($tynm).into(), vec![]);
                )+
            }
            $(
                pub fn $tyf() -> Expr<!> {
                    $tynm.with(Clone::clone)
                }
            )+
        };
    }

    claim_builtin_types! {
        (Absurd, absurd),
        (Trivial, trivial),
        (Atom, atom),
        (Nat, nat),
    }
}
