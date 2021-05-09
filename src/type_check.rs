use crate::{ast, core_ast, utils};
use ast::Literal;
use core_ast::{builtin_type as bty, Argument, DBIPPrint as dpp, Expr, Type, ULevel};
use fehler::{throw, throws};
use std::fmt;
use thiserror::Error;
use utils::{LocatedError, Ref, Span, DBI};

// TODO: 改进打印方式，将这里改成 StackMap<Option<Ref<str>>, Type<!>>
pub type Env = crate::utils::StackMap<Option<Ref<str>>, Option<Type<!>>>;

type Error = LocatedError<ErrorKind>;

#[derive(Debug, Clone)]
pub enum ErrorKind {
    TypeNotMatch { expected: String, given: String },
    CannotInferType { expr: String },
    NotSame(String, String),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::*;
        match self {
            TypeNotMatch { expected, given } => {
                write!(f, "expect a {}, but get `{}`", expected, given)
            }
            CannotInferType { expr } => {
                write!(f, "cannot infer the type of `{}`", expr)
            }
            NotSame(x, y) => {
                write!(f, "`{}` and `{}` are not the same", x, y)
            }
        }
    }
}

macro_rules! try_match {
    (let BuiltinApply($bf:literal , [$($i:ident),+ $(,)?]) = $e:expr; $env:expr) => {
        let ($($i),+) = if let BuiltinApply(ref bf, ref args) = $e {
            if let ($bf, [$($i),+]) = (&**bf, &**args) {
                ($($i),+)
            } else {
                throw!(ErrorKind::TypeNotMatch {
                    expected: format!($bf),
                    given: format!("{}", dpp($e, $env)),
                })
            }
        } else {
            throw!(ErrorKind::TypeNotMatch {
                expected: format!($bf),
                given: format!("{}", dpp($e, $env)),
            })
        };
    };
    (let $p:tt($($i:ident),+) = $e:expr; $env:expr) => {
        let ($($i),+) = if let $p($($i),+) = $e {
            ($($i),+)
        } else {
            throw!(ErrorKind::TypeNotMatch {
                expected: format!(stringify!($p)),
                given: format!("{}", dpp($e, $env)),
            })
        };
    };
}

macro_rules! match_array {
    (let [$($i:ident),+ $(,)?] = $e:expr, $($on_fail:tt)+) => {
        let ($($i),+) = if let [$($i),+] = $e {
            ($($i),+)
        } else {
            $($on_fail)+
        };
    };
}

macro_rules! match_builtin_args {
    (let [$($i:ident),+ $(,)?] = $e:expr) => {
        match_array!(let [$($i),+] = $e, panic!("Error: expect "))
    };
}

// TODO: 使用 De Bruijn 方法解决变量名、作用域的各种问题

/// 执行 expr[var/e]，将 expr 中自由出现的 var 替换为 e，e 应当是没有自由变量的。
fn substitute(expr: &Expr<!>, var: &str, e: &Expr<!>, env: &Env) -> Expr<!> {
    log::trace!(
        "substitute {:?} to {} in {}",
        var,
        dpp(e, env),
        dpp(expr, env)
    );
    todo!()
}

/// 对常用的 Argument 模式的简写
#[inline]
fn substitute_arg(expr: &Expr<!>, arg: &Argument, e: &Expr<!>, env: &Env) -> Expr<!> {
    match arg {
        Argument::Symbol(sym) => substitute(expr, sym, e, env),
        Argument::Dummy => expr.clone(),
    }
}

#[inline]
fn env_ext(env: &Env, name: Option<Ref<str>>, ty: &Type<!>) -> Env {
    env.insert(name, Some(ty.clone()))
}

fn env_get_nth_type(env: &Env, n: usize) -> &Type<!> {
    // 经过作用域检查，保证不会 panic
    env.iter().nth(n).and_then(|(_, ty)| ty.as_ref()).unwrap()
}

#[inline]
#[throws]
fn switch_rule<M: fmt::Display>(e: &Expr<M>, ty: &Type<!>, env: &Env) -> Expr<!> {
    let (ty_e_o, e_o) = synthesize(e, env)?;
    type_check_same(&ty_e_o, &ty, env)?;
    e_o
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
/// 对于构造式，有唯一相关的类型与之匹配；
/// 其它表达式则应用 Which 规则：试图综合得出其类型，再将结果与所给类型比较。
#[throws]
pub fn synthesize_with_type<M: fmt::Display>(e: &Expr<M>, ty: &Type<!>, env: &Env) -> Expr<!> {
    use Expr::*;
    log::trace!("check {} is a {}", dpp(e, env), dpp(ty, env));
    if let Info(_, e) = e {
        return synthesize_with_type(e, ty, env)?;
    }
    match (e, ty) {
        // FunI-1
        (LambdaExpr(arg, r), PiExpr(pi_arg, ty_arg, ty_ret)) => {
            let r_o = synthesize_with_type(r, ty_ret, &env_ext(env, arg.into(), &ty_arg))?;
            let new_arg = match (arg, pi_arg) {
                (Argument::Symbol(sym), _) => Argument::Symbol(sym.clone()),
                (Argument::Dummy, Argument::Symbol(sym)) => Argument::Symbol(sym.clone()),
                _ => Argument::Dummy,
            };
            LambdaExpr(new_arg, Ref::new(r_o))
        }
        (BuiltinApply(bf, args), SigmaExpr(arg, ty_a, ty_d)) if &**bf == "cons" => {
            match_builtin_args!(let [a, d] = &**args);
            let a_o = synthesize_with_type(a, ty_a, env)?;
            let d_o = synthesize_with_type(d, &substitute_arg(ty_d, arg, &a_o, env), env)?;
            BuiltinApply(bf.clone(), vec![a_o, d_o])
        }
        (BuiltinApply(bf, args), BuiltinApply(ty_bf, ty_args)) => {
            match (&**bf, &**args, &**ty_bf, &**ty_args) {
                // ListI-1
                ("nil", [], "List", [_ty]) => BuiltinApply(bf.clone(), vec![]),
                // ListI-3，TLY 中不存在，我自己加的，使 (the (List (-> Nat Nat)) (:: (lambda (x) x) nil)) 这样的
                // 表达式能推导出类型。
                ("::", [e, es], "List", [ty_1]) => {
                    let e_o = synthesize_with_type(e, ty_1, env)?;
                    let es_o = synthesize_with_type(es, ty, env)?;
                    BuiltinApply(bf.clone(), vec![e_o, es_o])
                }
                // VecI-1
                ("vecnil", [], "Vec", [_ty, len]) => {
                    if is_literal_zero(len) {
                        BuiltinApply(bf.clone(), vec![])
                    } else {
                        throw!(ErrorKind::TypeNotMatch {
                            expected: format!("{}", dpp(ty, env)),
                            given: bf.to_string(),
                        })
                    }
                }
                // VecI-2
                ("vec::", [e, es], "Vec", [ty_e, len]) if is_literal_add1(len) => {
                    let e_o = synthesize_with_type(e, ty_e, env)?;
                    let sublen = literal_sub1(len);
                    let ty_subvec = BuiltinApply(ty_bf.clone(), vec![ty_e.clone(), sublen]);
                    let es_o = synthesize_with_type(es, &ty_subvec, env)?;
                    BuiltinApply(bf.clone(), vec![e_o, es_o])
                }
                _ => switch_rule(e, ty, env)?,
            }
        }
        // Switch
        _ => switch_rule(e, ty, env)?,
    }
}

fn is_literal_zero<M>(e: &Expr<M>) -> bool {
    use Expr::*;
    match e {
        Literal(ast::Literal::Nat(0)) => true,
        BuiltinApply(bf, _) if &**bf == "zero" => true,
        _ => false,
    }
}

fn is_literal_add1<M>(e: &Expr<M>) -> bool {
    use Expr::*;
    match e {
        Literal(ast::Literal::Nat(n)) if *n > 0 => true,
        BuiltinApply(bf, _) if &**bf == "add1" => true,
        _ => false,
    }
}

fn literal_sub1(e: &Expr<!>) -> Expr<!> {
    use Expr::*;
    match e {
        Literal(ast::Literal::Nat(n)) => Literal(ast::Literal::Nat(n - 1)),
        BuiltinApply(bf, args) => match (&**bf, &**args) {
            ("add1", [n]) => n.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
#[throws]
pub fn synthesize<M: fmt::Display>(e: &Expr<M>, env: &Env) -> (Type<!>, Expr<!>) {
    use Expr::*;
    log::trace!("synthesize {}", dpp(e, env));
    match e {
        Info(_, e) => synthesize(e, env)?,
        Literal(lit) => synthesize_literal(lit),
        // Hypothesis
        Identifier(ident) => {
            let ty = env_get_nth_type(env, *ident).clone();
            (ty, Identifier(ident.clone()))
        }
        PiExpr(arg, ty_a, ty_r) => resolve_type_rule(ty_a, env)?,
        SigmaExpr(arg, ty_a, ty_d) => resolve_type_rule(ty_a, env)?,
        // FunE-1
        Apply(f, arg) => {
            let (ty_f, f_o) = synthesize(f, env)?;
            try_match!(let PiExpr(var, ty_arg, ty_ret) = &ty_f; &env);
            let arg_o = synthesize_with_type(arg, &ty_arg, env)?;
            let ty = substitute_arg(&ty_ret, &var, &arg_o, env);
            (ty, Apply(Ref::new(f_o), Ref::new(arg_o)))
        }
        // 目前还未引入 Universe Hierarchy，但这条规则似乎没有问题
        U(n) => (U(n + 1), U(*n)),
        BuiltinApply(bf, args) => {
            match (&**bf, &**args) {
                // 内建单例对象
                ("zero", []) => (bty::nat(), BuiltinApply(bf.clone(), vec![])),
                ("sole", []) => (bty::trivial(), BuiltinApply(bf.clone(), vec![])),
                // 内建类型
                ("Atom" | "Nat" | "Trivial" | "Absurd" | "List" | "Vec" | "Either" | "=", _) => {
                    resolve_type_rule(e, env)?
                }
                // nil 和 vecnil 必须附加类型
                (s, []) => throw!(ErrorKind::CannotInferType { expr: s.to_owned() }),
                // "The" 规则
                ("the", [ty, expr]) => {
                    let (_, ty_o) = resolve_type(ty, env)?;
                    let expr_o = synthesize_with_type(expr, &ty_o, env)?;
                    (ty_o, expr_o)
                }
                // ListI-2
                ("::", [e, es]) => {
                    let (ty_e_o, e_o) = synthesize(e, env)?;
                    let ty_list = bty::list(ty_e_o);
                    let es_o = synthesize_with_type(es, &ty_list, env)?;
                    (ty_list, BuiltinApply(bf.clone(), vec![e_o, es_o]))
                }
                // NatI-2
                ("add1", [n]) => {
                    let n_o = synthesize_with_type(n, &bty::nat(), env)?;
                    (bty::nat(), BuiltinApply(bf.clone(), vec![n_o]))
                }
                // VecE-1
                ("head", [v]) => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let BuiltinApply("Vec", [ty_e, len]) = &ty_v; env };
                    if is_literal_add1(&len) {
                        (ty_e.clone(), BuiltinApply(bf.clone(), vec![v_o]))
                    } else {
                        throw!(ErrorKind::TypeNotMatch {
                            expected: "Vec longer than 1".to_owned(),
                            given: format!("{}", dpp(v, env)),
                        })
                    }
                }
                // VecE-2
                ("tail", [v]) => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let BuiltinApply("Vec", [ty_e, len]) = &ty_v; env };
                    if is_literal_add1(&len) {
                        let ty_subv = bty::vec(ty_e.clone(), literal_sub1(len));
                        (ty_subv, BuiltinApply(bf.clone(), vec![v_o]))
                    } else {
                        throw!(ErrorKind::TypeNotMatch {
                            expected: "Vec longer than 1".to_owned(),
                            given: format!("{}", dpp(v, env)),
                        })
                    }
                }
                _ => unreachable!(),
            }
        }
        _ => throw!(ErrorKind::CannotInferType {
            expr: format!("{}", dpp(e, env))
        }),
    }
}

/// 判断并计算表达式是一个类型或 U(n)，返回其类型层级，相当于为 U(n) 特化的 synthesize。
/// 改进的第四种 Judgement，见 Figure B.1。
#[throws]
fn resolve_type<M: fmt::Display>(e: &Expr<M>, env: &Env) -> (ULevel, Type<!>) {
    use Expr::*;
    log::trace!("resolve {0} is a type", dpp(e, env));
    // TODO: 改进 El 规则
    match e {
        Info(_, e) => resolve_type(e, env)?,
        // FunF-1
        PiExpr(arg, ty_a, ty_r) => {
            let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
            let (l_r, ty_r_o) = resolve_type(ty_r, &env_ext(&env, arg.into(), &ty_a_o))?;
            (
                std::cmp::max(l_a, l_r),
                PiExpr(arg.clone(), Ref::new(ty_a_o), Ref::new(ty_r_o)),
            )
        }
        // SigmaF-1
        SigmaExpr(arg, ty_a, ty_d) => {
            let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
            let (l_d, ty_d_o) = resolve_type(ty_d, &env_ext(&env, arg.into(), &ty_a_o))?;
            (
                std::cmp::max(l_a, l_d),
                SigmaExpr(arg.clone(), Ref::new(ty_a_o), Ref::new(ty_d_o)),
            )
        }
        BuiltinApply(bf, args) => {
            match (&**bf, &**args) {
                // 内建单例对象
                (s, []) => match s {
                    "Atom" | "Nat" | "Trivial" | "Absurd" => (0, BuiltinApply(bf.clone(), vec![])),
                    _ => throw!(ErrorKind::TypeNotMatch {
                        expected: "type".to_owned(),
                        given: bf.to_string()
                    }),
                },
                // ListF
                ("List", [ty_e]) => {
                    let (l, ty_e_o) = resolve_type(ty_e, env)?;
                    (l, bty::list(ty_e_o))
                }
                // VecF
                ("Vec", [ty, len]) => {
                    let (l, ty_o) = resolve_type(ty, env)?;
                    let len_o = synthesize_with_type(len, &bty::nat(), env)?;
                    (l, bty::vec(ty_o, len_o))
                }
                _ => unreachable!(),
            }
        }
        // UF
        U(n) => (n + 1, U(*n)),
        //Literal, Lambda, Identifier, Apply
        // El
        _ => (0, synthesize_with_type(e, &U(0), env)?),
    }
}

// 将 resolve_type 的返回值包装为 (U(n), t_o)
#[inline]
#[throws]
fn resolve_type_rule<M: fmt::Display>(ty: &Expr<M>, env: &Env) -> (Type<!>, Type<!>) {
    let (l, t_o) = resolve_type(ty, env)?;
    (Expr::U(l), t_o)
}

/// 检查是否相同类型
/// 第五种 Judgement，见 Figure B.1。
#[inline]
#[throws]
fn type_check_same(ty1: &Type<!>, ty2: &Type<!>, env: &Env) {
    use Expr::*;
    log::trace!(
        "check if {} and {} are the same type",
        dpp(ty1, env),
        dpp(ty2, env)
    );
    if !is_type_check_same(ty1, ty2, env) {
        throw!(ErrorKind::NotSame(
            dpp(ty1, env).to_string(),
            dpp(ty2, env).to_string()
        ));
    }
}

fn is_type_check_same(ty1: &Type<!>, ty2: &Type<!>, env: &Env) -> bool {
    use Expr::*;
    log::trace!(
        "check {} and {} are the same type",
        dpp(ty1, env),
        dpp(ty2, env)
    );
    // TODO: 比较前充分计算 ty1 和 ty2
    match (ty1, ty2) {
        (Identifier(id1), Identifier(id2)) => id1 == id2,
        (U(m), U(n)) => m == n,
        (BuiltinApply(f1, args1), BuiltinApply(f2, args2))
            if args1.len() == 0 && args2.len() == 0 =>
        {
            f1 == f2
        }
        _ => {
            todo!()
        }
    }
}

/// 检查是否相同表达式
/// 第八种 Judgement，见 Figure B.1。
#[throws]
fn expr_check_same(c1: &Expr<!>, c2: &Expr<!>, ct: &Type<!>, env: &Env) {
    log::trace!(
        "check {} and {} are the same expression",
        dpp(c1, env),
        dpp(c2, env)
    );
    todo!()
}

/// 直接从字面量推导类型
fn synthesize_literal(lit: &Literal) -> (Type<!>, Expr<!>) {
    let ty = match lit {
        Literal::Nat(_) => bty::nat(),
        Literal::Atom(_) => bty::atom(),
    };
    (ty, Expr::Literal(lit.clone()))
}

pub fn default_environment() -> Env {
    Env::new()
}
