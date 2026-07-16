use crate::ast;
use crate::utils::*;
use crate::Never;
use std::fmt;

macro_rules! throw {
    ($e:expr) => {
        return Err($e)
    };
}

macro_rules! claim_array {
    ($id:ident $name:ident: [$ty: ty; _] = $value:expr $(;)?) => {
        $id $name: [$ty; $value.len()] = $value;
    }
}

// TODO: 嵌入源代码位置信息，定义合适的错误类型

pub type ULevel = u64;

/// 表达式包含位置信息和元信息（类型等）
#[derive(Debug, Clone)]
pub enum Expr<MetaInfo, Variable = DBI> {
    /// 用于在抽象代码树中插入信息的中间层。
    Info(MetaInfo, Ref<Expr<MetaInfo, Variable>>),

    /// 自然数字面量
    NatLiteral(u64),

    /// 原子符号字面量
    AtomLiteral(Ref<str>),

    /// 标识符，表示变量、函数、类型等
    Identifier(Variable),

    /// `(λ (ident) expr)`，被转换为单层
    LambdaExpr(Argument, Ref<Self>),

    /// `(Π ((ident expr)) expr)`，被转换为单层
    /// 并将箭头表达式转换为 Π 表达式
    PiExpr(Argument, Ref<Self>, Ref<Self>),

    /// `(Σ ((ident expr)) expr)`，被转换为单层
    SigmaExpr(Argument, Ref<Self>, Ref<Self>),

    /// 函数调用，经过柯里化转换为只有一个参数
    Apply(Ref<Self>, Ref<Self>),

    /// 内建标识符，如 `Atom`、`Nat`、`zero`、`nil`
    BuiltinId(&'static str),

    /// 内建调用，如 `(the Type expr)`、`(cons expr expr)`、`(add1 expr)`
    BuiltinApply(&'static str, Vec<Self>),
}

// FIXME: 为了通过编译
impl Default for Expr<Never> {
    fn default() -> Self {
        Expr::NatLiteral(0)
    }
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
            AtomLiteral(atom) => {
                write!(f, "'{}", atom)
            }
            NatLiteral(n) => {
                write!(f, "{}", n)
            }
            Identifier(id) => {
                write!(f, "{}", id.as_ref())
            }
            LambdaExpr(arg, body) => {
                write!(f, "(λ ({}) {})", arg, body)
            }
            PiExpr(arg, ty, body) => {
                if matches!(arg, Argument::Dummy) {
                    write!(f, "(→ {}", ty)?;
                    let mut current: &Self = &**body;
                    loop {
                        match current {
                            PiExpr(next_arg, next_ty, next_body)
                                if matches!(next_arg, Argument::Dummy) =>
                            {
                                write!(f, " {}", next_ty)?;
                                current = &**next_body;
                            }
                            _ => {
                                write!(f, " {})", current)?;
                                break;
                            }
                        }
                    }
                    Ok(())
                } else {
                    write!(f, "(Π (({} {})) {})", arg, ty, body)
                }
            }
            SigmaExpr(arg, ty, body) => {
                if matches!(arg, Argument::Dummy) {
                    write!(f, "(Pair {} {})", ty, body)
                } else {
                    write!(f, "(Σ (({} {})) {})", arg, ty, body)
                }
            }
            Apply(fun, arg) => {
                write!(f, "({} {})", fun, arg)
            }
            BuiltinId(id) => {
                write!(f, "{}", id)
            }
            BuiltinApply(bf, args) => match (*bf, args.as_slice()) {
                ("U", [NatLiteral(0) | BuiltinId("zero")]) => {
                    write!(f, "U")
                }
                _ => {
                    write!(f, "({}", bf)?;
                    for arg in args {
                        write!(f, " {}", arg)?;
                    }
                    write!(f, ")")
                }
            },
        }
    }
}

type Env<V> = StackMap<Option<Ref<str>>, V>;

/// 包装器，为 De Bruijn 索引表示实现 pretty print
pub struct DBIPPrint<'a, M, V>(pub &'a Expr<M>, pub &'a Env<V>);

impl<'a, M, V> fmt::Display for DBIPPrint<'a, M, V>
where
    M: fmt::Display,
    V: Default,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    where
        M: fmt::Display,
    {
        use Expr::*;
        let DBIPPrint(expr, env) = self;

        fn fetch_fresh_name<V>(arg: Option<Ref<str>>, env: &Env<V>) -> Ref<str> {
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

        fn ext_env<V>(env: &Env<V>, name: &str) -> Env<V>
        where
            V: Default,
        {
            env.insert(Some(name.into()), <V as Default>::default())
        }

        match expr {
            Info(info, inner) => {
                write!(f, "[{}: {}]", info, DBIPPrint(&**inner, *env))
            }
            AtomLiteral(atom) => {
                write!(f, "'{}", atom)
            }
            NatLiteral(n) => {
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
                if matches!(arg, Argument::Dummy) {
                    let ty = DBIPPrint(&**ty, &new_env);
                    write!(f, "(→ {}", ty)?;
                    let mut current: &Expr<_> = &**body;
                    loop {
                        match current {
                            PiExpr(next_arg, next_ty, next_body)
                                if matches!(next_arg, Argument::Dummy) =>
                            {
                                let next_arg_name = fetch_fresh_name(next_arg.into(), &new_env);
                                let next_env = ext_env(&new_env, &next_arg_name);
                                let next_ty = DBIPPrint(&**next_ty, &next_env);
                                write!(f, " {}", next_ty)?;
                                current = &**next_body;
                            }
                            _ => {
                                write!(f, " {})", DBIPPrint(current, &new_env))?;
                                break;
                            }
                        }
                    }
                    Ok(())
                } else {
                    let ty = DBIPPrint(&**ty, &new_env);
                    let body = DBIPPrint(&**body, &new_env);
                    write!(f, "(Π (({} {})) {})", arg_name, ty, body)
                }
            }
            SigmaExpr(arg, ty, body) => {
                let arg_name = fetch_fresh_name(arg.into(), env);
                let new_env = ext_env(env, &arg_name);
                if matches!(arg, Argument::Dummy) {
                    let ty = DBIPPrint(&**ty, &new_env);
                    let body = DBIPPrint(&**body, &new_env);
                    write!(f, "(Pair {} {})", ty, body)
                } else {
                    let ty = DBIPPrint(&**ty, &new_env);
                    let body = DBIPPrint(&**body, &new_env);
                    write!(f, "(Σ (({} {})) {})", arg_name, ty, body)
                }
            }
            Apply(fun, arg) => {
                let fun = DBIPPrint(&**fun, &env);
                let arg = DBIPPrint(&**arg, &env);
                write!(f, "({} {})", fun, arg)
            }
            BuiltinId(id) => {
                write!(f, "{}", id)
            }
            BuiltinApply(bf, args) => match (*bf, args.as_slice()) {
                ("U", [NatLiteral(0) | BuiltinId("zero")]) => {
                    write!(f, "U")
                }
                _ => {
                    write!(f, "({}", bf)?;
                    for arg in args {
                        write!(f, " {}", DBIPPrint(arg, &env))?;
                    }
                    write!(f, ")")
                }
            },
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
    IllegalArgumentType {
        caller: String,
        valid_argt: String,
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
            IllegalArgumentType { caller, valid_argt } => write!(
                f,
                "`{}` should take argument of type {}, but here is not.",
                caller, valid_argt
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
/// 调用分别转化为函数调用和内建调用，并检查内建调用的合法性
pub fn unfold(e: &ast::Expr) -> Result<Expr<Never, Ref<str>>, Error> {
    use ast::Expr::*;
    let ret = match e {
        Literal(_, ast::Literal::Nat(n)) => Expr::NatLiteral(*n),
        Literal(_, ast::Literal::Atom(atom)) => Expr::AtomLiteral(atom.clone()),
        Identifier(_, id) => match &**id {
            "U" => Expr::BuiltinApply("U", vec![Expr::NatLiteral(0)]),
            _ if let Some(id) = PIE_BUILTIN_SINGLETONS.iter().find(|d| **d == &**id) => {
                Expr::BuiltinId(id)
            }
            _ => Expr::Identifier(id.clone()),
        },
        List(loc, exprs) => match &**exprs {
            [Identifier(_, f), args @ ..]
                if let Some((bid, argc)) =
                    PIE_BUILTIN_FUNCTIONS.iter().find(|(bid, _)| **bid == **f) =>
            {
                if args.len() == *argc {
                    Expr::BuiltinApply(bid, map_result(args, unfold)?)
                } else {
                    throw!(Error {
                        loc: Some(loc.clone()),
                        erk: ErrorKind::IllegalArgumentNumber {
                            caller: f.to_string(),
                            valid_argc: *argc,
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
            let body = unfold(body)?;
            let types: Vec<_> = args
                .iter()
                .map(|(_, ty)| unfold(ty))
                .collect::<Result<_, Error>>()?;
            let mut e = body;
            for (i, (ast::Symbol(_, sym), _)) in args.iter().enumerate().rev() {
                let has_body_ref = occurs(&e, sym) || types[i + 1..].iter().any(|t| occurs(t, sym));
                let arg = if has_body_ref {
                    self::Argument::Symbol(sym.clone())
                } else {
                    self::Argument::Dummy
                };
                e = Expr::PiExpr(arg, Ref::new(types[i].clone()), Ref::new(e));
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
            let body = unfold(body)?;
            let types: Vec<_> = args
                .iter()
                .map(|(_, ty)| unfold(ty))
                .collect::<Result<_, Error>>()?;
            let mut e = body;
            for (i, (ast::Symbol(_, sym), _)) in args.iter().enumerate().rev() {
                let has_body_ref = occurs(&e, sym) || types[i + 1..].iter().any(|t| occurs(t, sym));
                let arg = if has_body_ref {
                    self::Argument::Symbol(sym.clone())
                } else {
                    self::Argument::Dummy
                };
                e = Expr::SigmaExpr(arg, Ref::new(types[i].clone()), Ref::new(e));
            }
            e
        }
    };
    Ok(ret)
}

/// 将列表经过柯里化转换为函数调用
fn unfold_list(exprs: &[ast::Expr]) -> Result<Expr<Never, Ref<str>>, Error> {
    let mut es = exprs.iter();
    let mut f = unfold(es.next().unwrap())?;
    for e in es {
        f = Expr::Apply(Ref::new(f), Ref::new(unfold(e)?));
    }
    Ok(f)
}

/// 检查 expr 中是否直接出现标识符 `var`（不考虑遮蔽）
fn occurs(e: &Expr<Never, Ref<str>>, var: &str) -> bool {
    use Expr::*;
    match e {
        Info(_, inner) => occurs(inner, var),
        Identifier(id) => **id == *var,
        NatLiteral(_) | AtomLiteral(_) | BuiltinId(_) => false,
        LambdaExpr(arg, body) => !match_arg(var, arg) && occurs(body, var),
        PiExpr(arg, ty, body) => !match_arg(var, arg) && (occurs(ty, var) || occurs(body, var)),
        SigmaExpr(arg, ty, body) => !match_arg(var, arg) && (occurs(ty, var) || occurs(body, var)),
        Apply(f, a) => occurs(f, var) || occurs(a, var),
        BuiltinApply(_, args) => args.iter().any(|arg| occurs(arg, var)),
    }
}

fn match_arg(var: &str, arg: &Argument) -> bool {
    match arg {
        Argument::Dummy => true,
        Argument::Symbol(sym) => **sym == *var,
    }
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
    // convert Pair to Sigma, so no need to define Pair as builtin function
    // ("Pair", 2),
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
}

// 内建无参数类型
#[allow(non_upper_case_globals)]
pub mod builtin_type {
    use super::*;

    macro_rules! claim_builtin_types {
        ($(($tynm:ident, $tyf:ident)),+ $(,)?) => {
            $(
                #[inline]
                pub fn $tyf() -> Expr<Never> {
                    Expr::BuiltinId(stringify!($tynm))
                }
            )+
        };
    }

    macro_rules! claim_builtin_types_with_args {
        ($(($tynm:ident, $tyf:ident, $($arg:ident),+)),+ $(,)?) => {
            $(
                #[inline]
                pub fn $tyf($($arg: Expr<Never>),+) -> Expr<Never> {
                    Expr::BuiltinApply(stringify!($tynm), vec![$($arg),+])
                }
            )+
        };
    }

    claim_builtin_types! {
        (Absurd, absurd),
        (Trivial, trivial),
        (Atom, atom),
        (Nat, nat),
        (zero, zero),
        (nil, nil),
        (vecnil, vecnil),
        (sole, sole),
    }

    claim_builtin_types_with_args! {
        (List, list, t),
        (Vec, vec, t, l),
        (Either, either, l, r),
        (U, u_l, l),
    }

    #[inline]
    pub fn equal(t: Expr<Never>, l: Expr<Never>, r: Expr<Never>) -> Expr<Never> {
        Expr::BuiltinApply("=", vec![t, l, r])
    }

    pub fn u() -> Expr<Never> {
        u_l(Expr::NatLiteral(0))
    }
}
