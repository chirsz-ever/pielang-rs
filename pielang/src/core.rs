use crate::utils::*;
use std::fmt;

/// 表达式，最终的正则化的值和中性表达式
#[derive(Debug, Clone)]
pub enum Expr {
    /// 自然数
    /// zero 会被计算为 Nat(0)
    Nat(u64),

    /// 原子符号
    Atom(Ref<str>),

    /// 标识符，表示变量、函数、类型等
    Identifier(Ref<str>, usize),

    /// `(λ (ident) expr)`，单层
    Lambda(Argument, Ref<Self>),

    /// `(Π ((ident expr)) expr)`，单层
    /// 并将箭头表达式转换为 Π 表达式
    Pi(Argument, Ref<Self>, Ref<Self>),

    /// `(Σ ((ident expr)) expr)`，单层
    Sigma(Argument, Ref<Self>, Ref<Self>),

    /// 函数调用，去柯里化，只有一个参数
    App(Ref<Self>, Ref<Self>),

    /// 内建标识符，如 `Atom`、`Nat`、`zero`、`nil`
    I(&'static str),

    /// 内建调用，如 `(the Type expr)`、`(cons expr expr)`、`(add1 expr)`
    S(&'static str, Vec<Self>),
}

// FIXME: 为了通过编译
impl Default for Expr {
    fn default() -> Self {
        Expr::Nat(0)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;
        match self {
            Atom(atom) => {
                write!(f, "'{}", atom)
            }
            Nat(n) => {
                write!(f, "{}", n)
            }
            Identifier(id, _) => {
                write!(f, "{}", id)
            }
            Lambda(arg, body) => {
                write!(f, "(λ ({}", arg)?;
                let mut current: &Self = body;
                loop {
                    match current {
                        Lambda(next_arg, next_body) => {
                            write!(f, " {}", next_arg)?;
                            current = &**next_body;
                        }
                        _ => {
                            write!(f, ") {})", current)?;
                            break;
                        }
                    }
                }
                Ok(())
            }
            Pi(arg, ty, body) => {
                if matches!(arg, Argument::Dummy) {
                    write!(f, "(→ {}", ty)?;
                    let mut current: &Self = body;
                    loop {
                        match current {
                            Pi(Argument::Dummy, next_ty, next_body) => {
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
            Sigma(arg, ty, body) => {
                if matches!(arg, Argument::Dummy) {
                    write!(f, "(Pair {} {})", ty, body)
                } else {
                    write!(f, "(Σ (({} {})) {})", arg, ty, body)
                }
            }
            App(fun, arg) => {
                write!(f, "(")?;
                let mut args = vec![&**arg];
                let mut current: &Self = fun;
                while let App(inner_fun, inner_arg) = current {
                    args.push(&**inner_arg);
                    current = &**inner_fun;
                }
                write!(f, "{}", current)?;
                for a in args.iter().rev() {
                    write!(f, " {}", a)?;
                }
                write!(f, ")")
            }
            I(id) => {
                write!(f, "{}", id)
            }
            S(bf, args) => match (*bf, args.as_slice()) {
                ("U", [Nat(0)]) => {
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
pub struct DBIPPrint<'a, V>(pub &'a Expr, pub &'a Env<V>);

impl<'a, V> fmt::Display for DBIPPrint<'a, V>
where
    V: Default,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Expr::*;
        let DBIPPrint(expr, env) = self;

        fn has_name_conflict<V>(env: &Env<V>, id: &str, idx: usize) -> bool {
            for (i, (k, _)) in env.iter().enumerate() {
                let same_name = k.as_ref().is_some_and(|k| &**k == id);
                let same_idx = i == idx;
                if same_name && same_idx {
                    return false;
                } else if same_name && !same_idx {
                    return true;
                } else if !same_name && same_idx {
                    return true;
                }
            }
            false
        }

        fn ext_env_arg<V>(env: &Env<V>, arg: &Argument) -> Env<V>
        where
            V: Default,
        {
            env.insert(arg.into(), <V as Default>::default())
        }

        match expr {
            Atom(atom) => {
                write!(f, "'{}", atom)
            }
            Nat(n) => {
                write!(f, "{}", n)
            }
            Identifier(id, n) => {
                if has_name_conflict(env, id, *n) {
                    // TODO: more pretty print
                    write!(f, "{}:{}", id, n)
                } else {
                    write!(f, "{}", id)
                }
            }
            Lambda(arg, body) => {
                let mut current_env = ext_env_arg(env, arg);
                write!(f, "(λ ({}", arg)?;
                let mut current: &Expr = body;
                loop {
                    match current {
                        Lambda(next_arg, next_body) => {
                            write!(f, " {}", next_arg)?;
                            current_env = ext_env_arg(&current_env, next_arg);
                            current = &**next_body;
                        }
                        _ => {
                            write!(f, ") {})", DBIPPrint(current, &current_env))?;
                            break;
                        }
                    }
                }
                Ok(())
            }
            Pi(arg, ty, body) => {
                let new_env = ext_env_arg(env, arg);
                match arg {
                    Argument::Dummy => {
                        let ty = DBIPPrint(ty, &new_env);
                        write!(f, "(→ {}", ty)?;
                        let mut current: &Expr = body;
                        loop {
                            match current {
                                Pi(Argument::Dummy, next_ty, next_body) => {
                                    let next_env = env.insert(None, Default::default());
                                    let next_ty = DBIPPrint(next_ty, &next_env);
                                    write!(f, " {}", next_ty)?;
                                    current = &**next_body;
                                }
                                _ => {
                                    write!(f, " {})", DBIPPrint(current, &new_env))?;
                                    break;
                                }
                            }
                        }
                    }
                    Argument::Symbol(arg) => {
                        let ty = DBIPPrint(ty, &new_env);
                        let body = DBIPPrint(body, &new_env);
                        write!(f, "(Π (({} {})) {})", arg, ty, body)?;
                    }
                }
                Ok(())
            }
            Sigma(arg, ty, body) => {
                let new_env = ext_env_arg(env, arg);
                match arg {
                    Argument::Dummy => {
                        let ty = DBIPPrint(ty, &new_env);
                        let body = DBIPPrint(body, &new_env);
                        write!(f, "(Pair {} {})", ty, body)
                    }
                    Argument::Symbol(arg) => {
                        let ty = DBIPPrint(ty, &new_env);
                        let body = DBIPPrint(body, &new_env);
                        write!(f, "(Σ (({} {})) {})", arg, ty, body)
                    }
                }
            }
            App(fun, arg) => {
                write!(f, "(")?;
                let mut args: Vec<&Expr> = vec![&**arg];
                let mut current: &Expr = fun;
                while let App(inner_fun, inner_arg) = current {
                    args.push(&**inner_arg);
                    current = &**inner_fun;
                }
                write!(f, "{}", DBIPPrint(current, env))?;
                for a in args.iter().rev() {
                    write!(f, " {}", DBIPPrint(a, env))?;
                }
                write!(f, ")")
            }
            I(id) => {
                write!(f, "{}", id)
            }
            S(bf, args) => match (*bf, args.as_slice()) {
                ("U", [Nat(0)]) => {
                    write!(f, "U")
                }
                _ => {
                    write!(f, "({}", bf)?;
                    for arg in args {
                        write!(f, " {}", DBIPPrint(arg, env))?;
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

/// 标识符，`Dummy` 用于表示参数不在之后出现，例如从 `→` `Pair` 转换为 `Π` `Σ` 时。
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
