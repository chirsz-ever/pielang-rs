use crate::{ast, core, utils};
use ast::to_builtin_name as bn;
use core::{Argument, DBIPPrint as dpp, Expr::Nat};
use std::{
    cell::{Cell, RefCell},
    fmt,
};
use utils::{LocatedError, Ref};

thread_local! {
    static INDENT: Cell<usize> = const { Cell::new(0) };
    /// 入口字符串栈，None 表示该帧已被 tc_log_end! 消费
    static TC_LOG_ENTRYS: RefCell<Vec<Option<String>>> = const { RefCell::new(Vec::new()) };
}

/// 仿函数宏：在函数体内展开入口日志并创建 IndentGuard。
///
/// 用法（仅入口日志）：
/// ```notest
/// tc_log!("fmt {}", args...)
/// ```
///
/// 搭配 tc_log_end! 使用入口+退出日志：
/// ```notest
/// tc_log!("entry fmt", args...);
/// let ret = ...;
/// tc_log_end!("=> ret", ret);
/// ```
macro_rules! tc_log {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {
        {
            let _tc_log_entry = format!($fmt $(, $arg)*);
            log::trace!(
                "{}{}{}",
                "│".repeat(crate::type_check::INDENT.get()),
                "┌",
                _tc_log_entry
            );
            TC_LOG_ENTRYS.with(|v| v.borrow_mut().push(Some(_tc_log_entry)));
        }
        let _tc_log_guard = crate::type_check::IndentGuard::new();
    };
}

/// 搭配 tc_log! 使用的出口日志宏。
///
/// 用法：在捕获返回值 ret 后调用，需要再次传入入口 fmt 与入口 args。
/// ```notest
/// tc_log!("entry {}", a);
/// let ret = body;
/// tc_log_end!("entry {}", a; "exit {}", ret);
/// ```
/// 打印出口日志并标记当前帧的入口字符串已消费（抑制 IndentGuard::drop 的兜底打印）。
macro_rules! tc_log_end {
    ($exit_fmt:literal $(, $exit_arg:expr)* $(,)?) => {{
        let _tc_log_entry = TC_LOG_ENTRYS.with(|v| {
            v.borrow_mut().last_mut()
                .and_then(|slot| slot.take())
        }).expect("tc_log_end! must be called after tc_log!");
        log::trace!(
            "{}{}{} {}",
            "│".repeat(crate::type_check::INDENT.get() - 1),
            "└",
            _tc_log_entry,
            format_args!($exit_fmt $(, $exit_arg)*),
        );
    }};
}

// TODO: 移除 Option
/// 变量名 -> (类型, 表达式)
pub type Env = crate::utils::StackMap<Option<Ref<str>>, (core::Expr, RefCell<Option<core::Expr>>)>;

type Error = LocatedError<ErrorKind>;

macro_rules! throw {
    ($e:expr) => {
        return Err(Error::from($e))
    };
}

#[derive(Debug, Clone)]
pub enum ErrorKind {
    TypeNotMatch { expected: String, given: String },
    CannotInferType { expr: String },
    NotSame(String, String, String),
    NotType(String),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::*;
        match self {
            TypeNotMatch { expected, given } => {
                write!(f, "Expected {} but given {}", expected, given)
            }
            CannotInferType { expr } => {
                write!(f, "Can't determine the type of {}", expr)
            }
            NotSame(x, y, t) => {
                write!(f, "The expressions {} and {} are not the same {}", x, y, t)
            }
            NotType(x) => {
                write!(f, "{} is not a type", x)
            }
        }
    }
}

macro_rules! try_match {
    (let S($bf:literal , [$($i:ident),+ $(,)?]) = $e:expr; $env:expr) => {
        let ($($i,)+) = if let S(bf, args) = $e {
            if let ($bf, [$($i),+]) = (&**bf, &**args) {
                ($($i,)+)
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

macro_rules! no_else {
    (let $p:pat = $expr:expr $(;)?) => {
        let $p = $expr else { unreachable!() };
    };
}

macro_rules! pi {
    ($ty_a:expr, $ty_r:expr $(,)?) => {
        core::Expr::Pi(Argument::Dummy, Ref::new($ty_a), Ref::new($ty_r))
    };
    (ref $ty_a:expr, $ty_r:expr $(,)?) => {
        core::Expr::Pi(Argument::Dummy, Ref::clone(&$ty_a), Ref::new($ty_r))
    };
    ($ty_a:expr, ref $ty_r:expr $(,)?) => {
        core::Expr::Pi(Argument::Dummy, Ref::new($ty_a), Ref::clone(&$ty_r))
    };
    (ref $ty_a:expr, ref $ty_r:expr $(,)?) => {
        core::Expr::Pi(Argument::Dummy, Ref::clone(&$ty_a), Ref::clone(&$ty_r))
    };
    ($ty_a:expr, $($e:tt)+) => {
        core::Expr::Pi(
            Argument::Dummy,
            Ref::new($ty_a),
            Ref::new(pi!($($e)+)))
    };
    (ref $ty_a:expr, $($e:tt)+) => {
        core::Expr::Pi(
            Argument::Dummy,
            Ref::clone(&$ty_a),
            Ref::new(pi!($($e)+)))
    };
}

macro_rules! app {
    ($f:expr, $a:expr $(,)?) => {
        core::Expr::App(Ref::new($f), Ref::new($a))
    };
    (ref $f:expr, $a:expr $(,)?) => {
        core::Expr::App(Ref::clone(&$f), Ref::new($a))
    };
    ($f:expr, ref $a:expr $(,)?) => {
        core::Expr::App(Ref::new($f), Ref::clone(&$a))
    };
    (ref $f:expr, ref $a:expr $(,)?) => {
        core::Expr::App(Ref::clone(&$f), Ref::clone(&$a))
    };
    ($f:expr, $a:expr, $($tt:tt)+) => {
        app!(core::Expr::App(Ref::new($f), Ref::new($a)), $($tt)+)
    };
    (ref $f:expr, $a:expr, $($tt:tt)+) => {
        app!(core::Expr::App(Ref::clone(&$f), Ref::new($a)), $($tt)+)
    };
    ($f:expr, ref $a:expr, $($tt:tt)+) => {
        app!(core::Expr::App(Ref::new($f), Ref::clone(&$a)), $($tt)+)
    };
    (ref $f:expr, ref $a:expr, $($tt:tt)+) => {
        app!(core::Expr::App(Ref::clone(&$f), Ref::clone(&$a)), $($tt)+)
    };
}

macro_rules! bapp {
    ($bf:expr $(,$a:expr)+ $(,)?) => {
        core::Expr::S($bf, vec![$($a),*])
    };
}

macro_rules! U {
    () => {
        core::Expr::S("U", vec![Nat(0)])
    };
    ($e:expr) => {
        core::Expr::S("U", vec![$e])
    };
}

macro_rules! B {
    ($lit:literal) => {
        core::Expr::I($lit)
    };
}

/// 缩进守卫，进入时增加缩进，退出时自动恢复。
struct IndentGuard;

impl IndentGuard {
    fn new() -> Self {
        INDENT.set(INDENT.get() + 1);
        IndentGuard
    }
}

impl std::ops::Drop for IndentGuard {
    fn drop(&mut self) {
        INDENT.set(INDENT.get() - 1);
        // pop 本帧的槽；Some(entry) 说明 tc_log_end! 未被调用，打印兜底出口日志
        if let Some(entry) = TC_LOG_ENTRYS.with(|v| v.borrow_mut().pop().flatten()) {
            log::trace!(
                "{}{}{}",
                "│".repeat(crate::type_check::INDENT.get()),
                "└",
                entry
            );
        }
    }
}

// TODO: 使用 De Bruijn 方法解决变量名、作用域的各种问题

/// 执行 expr[var/e]，将 expr 中自由出现的 var 替换为 e，e 应当是没有自由变量的。
fn substitute(expr: &core::Expr, var: &str, e: &core::Expr, env: &Env) -> core::Expr {
    tc_log!(
        "substitute `{}` to `{}` in `{}`",
        var,
        dpp(e, env),
        dpp(expr, env)
    );
    todo!()
}

/// 对常用的 Argument 模式的简写
#[inline]
fn substitute_arg(body: &core::Expr, arg: &Argument, e: &core::Expr, env: &Env) -> core::Expr {
    match arg {
        Argument::Symbol(sym) => substitute(body, sym, e, env),
        Argument::Dummy => body.clone(),
    }
}

#[inline]
fn env_ext(env: &Env, name: &Ref<str>, ty: &core::Expr) -> Env {
    env.insert(name.clone().into(), (ty.clone(), Default::default()))
}

fn env_ext_arg(env: &Env, name: &Argument, ty: &core::Expr) -> Env {
    match name {
        Argument::Dummy => env.clone(),
        Argument::Symbol(n) => env_ext(env, n, ty),
    }
}

// TODO: debruijn
// fn env_get_nth_type(env: &Env, n: usize) -> &core::Expr {
//     // 经过作用域检查，保证不会 panic
//     &env.iter().nth(n).unwrap().1.0
// }

/// 先综合出 e 的类型，再检查其是否与 ty 相同
#[inline]
fn switch_rule(e: &ast::Expr, ty: &core::Expr, env: &Env) -> Result<core::Expr, Error> {
    let (ty_e_o, e_o) = synthesize(e, env)?;
    // TODO: 改为 context
    type_check_same(&ty_e_o, ty, env).map_err(|_| ErrorKind::TypeNotMatch {
        expected: dpp(ty, env).to_string(),
        given: dpp(&ty_e_o, env).to_string(),
    })?;
    Ok(e_o)
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
/// 对于构造式，有唯一相关的类型与之匹配；
/// 其它表达式则应用 Which 规则：试图综合得出其类型，再将结果与所给类型比较。
pub fn synthesize_with_type(
    e: &ast::Expr,
    ty: &core::Expr,
    env: &Env,
) -> Result<core::Expr, Error> {
    tc_log!("check `{}` is a `{}`", e, dpp(ty, env));

    use ast::Expr::AppExpr as A;
    use ast::Expr::*;
    use ast::Id;
    use core::Expr::*;

    let ret = match (e, ty) {
        // 简单情况优化
        (Ident(_, "sole"), I("Trivial")) => I("sole"),
        (AtomLit(_, a), I("Atom")) => Atom((*a).into()),
        (Ident(_, "zero"), I("Nat")) => Nat(0),
        (NatLit(_, n), I("Nat")) => Nat(*n),
        (Ident(_, ty @ ("Nat" | "Atom" | "Trivial" | "Absurd")), S("U", args))
            if let [Nat(0)] = **args =>
        {
            I(bn(ty))
        }
        // FunI-1, FunI-2
        (LambdaExpr(sp, args, r), Pi(pi_arg, ty_arg, ty_ret)) => {
            no_else! { let [Id(_, arg), rargs @ ..] = &args[..] }
            let arg = (*arg).into();
            if rargs.is_empty() {
                // FunI-1
                // FIXME: variable scope
                let r_o = synthesize_with_type(r, ty_ret, &env_ext(env, &arg, ty_arg))?;
                Lambda(Argument::Symbol(arg), Ref::new(r_o))
            } else {
                // FunI-2
                let r_o = synthesize_with_type(
                    &LambdaExpr(*sp, rargs.to_vec(), r.clone()),
                    ty_ret,
                    &env_ext_arg(env, pi_arg, ty_arg),
                )?;
                Lambda(Argument::Symbol(arg), Ref::new(r_o))
            }
        }
        // ΣI
        (A(_, args), Sigma(arg, ty_a, ty_d)) if let Ident(_, "cons") = args[0] => {
            let [_, a, d] = &**args else { unreachable!() };
            let a_o = synthesize_with_type(a, ty_a, env)?;
            let d_o = synthesize_with_type(d, &substitute_arg(ty_d, arg, &a_o, env), env)?;
            S("cons", vec![a_o, d_o])
        }
        // ListI-1
        (Ident(_, "nil"), S("List", _ty_args)) => I("nil"),
        // VecI-1
        (Ident(_, "vecnil"), S("Vec", ty_args)) => {
            no_else! { let [_ty_e, l] = &ty_args[..] }
            if let Nat(0) = l {
                I("vecnil")
            } else {
                throw!(ErrorKind::TypeNotMatch {
                    expected: format!("{}", dpp(ty, env)),
                    given: "vecnil".to_string(),
                })
            }
        }
        (A(_, args), S(ty_bf, ty_args)) => {
            match (args.as_slice(), &**ty_bf, &**ty_args) {
                // ListI-3，TLT 中不存在，我自己加的，使 (the (List (-> Nat Nat)) (:: (lambda (x) x) nil)) 这样的
                // 表达式能推导出类型。
                ([Ident(_, "::"), e, es], "List", [ty_1]) => {
                    let e_o = synthesize_with_type(e, ty_1, env)?;
                    let es_o = synthesize_with_type(es, ty, env)?;
                    S("::", vec![e_o, es_o])
                }
                // VecI-2
                ([Ident(_, "vec::"), e, es], "Vec", [ty_e, len]) if is_literal_add1(len) => {
                    let e_o = synthesize_with_type(e, ty_e, env)?;
                    let sublen = literal_sub1(len);
                    let ty_subvec = S(ty_bf, vec![ty_e.clone(), sublen]);
                    let es_o = synthesize_with_type(es, &ty_subvec, env)?;
                    S("vec::", vec![e_o, es_o])
                }
                // EitehrI-1
                ([Ident(_, "left"), lt], "Either", [ty_l, _ty_r]) => {
                    let lt_o = synthesize_with_type(lt, ty_l, env)?;
                    S("left", vec![lt_o])
                }
                // EitehrI-2
                ([Ident(_, "right"), rt], "Either", [_ty_l, ty_r]) => {
                    let rt_o = synthesize_with_type(rt, ty_r, env)?;
                    S("right", vec![rt_o])
                }
                // EqI
                ([Ident(_, "same"), mid], "=", [ty_x, from, to]) => {
                    let mid_o = synthesize_with_type(mid, ty_x, env)?;
                    expr_check_same(from, &mid_o, ty_x, env)?;
                    expr_check_same(&mid_o, to, ty_x, env)?;
                    S("same", vec![mid_o])
                }
                _ => switch_rule(e, ty, env)?,
            }
        }
        // Switch
        _ => switch_rule(e, ty, env)?,
    };

    tc_log_end!("=> {}", dpp(&ret, env));
    Ok(ret)
}

fn is_literal_add1(e: &core::Expr) -> bool {
    use core::Expr::*;
    match e {
        Nat(0) => false,
        Nat(_) => true,
        S(bf, _) if &**bf == "add1" => true,
        _ => false,
    }
}

fn literal_sub1(e: &core::Expr) -> core::Expr {
    use core::Expr::*;
    match e {
        Nat(n) => {
            debug_assert_ne!(*n, 0);
            Nat(n - 1)
        }
        S(bf, args) => match (&**bf, &**args) {
            ("add1", [n]) => n.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
pub fn synthesize(e: &ast::Expr, env: &Env) -> Result<(core::Expr, core::Expr), Error> {
    tc_log!("synthesize `{}`", e);

    use ast::Expr::*;
    use core::Expr::*;

    let ret = match e {
        // NatI-3, NatI-4
        NatLit(_, n) => (I("Nat"), Nat(*n)),
        // AtomI
        AtomLit(_, a) => (I("Atom"), Atom((*a).into())),
        // NatI-1
        Ident(_, "zero") => (I("Nat"), Nat(0)),
        // TrivI
        Ident(_, "sole") => (I("Trivial"), I("sole")),
        // UI-1, UI-9, UI-14, UI-15
        Ident(_, ty @ ("Atom" | "Nat" | "Trivial" | "Absurd")) => {
            (U!(), I(ast::to_builtin_name(ty)))
        }
        Ident(_, "U") => (U!(Nat(1)), U!(Nat(0))),
        // Hypothesis
        // TODO: de bruijn
        Ident(_, id) => {
            let (ty, _idx) = env.get(&Some((*id).into())).unwrap();
            (ty.clone(), Identifier((*id).into(), 0))
        }
        LambdaExpr(_, _args, _body) => {
            throw!(ErrorKind::CannotInferType {
                expr: e.to_string()
            })
        }
        PiExpr(..) | SigmaExpr(..) | ArrowExpr(..) => resolve_type_rule(e, env)?,
        AppExpr(_, args) => {
            match args.as_slice() {
                // (U n): (U (add1 n))
                [Ident(_, "U"), NatLit(_, n)] => (U!(Nat(*n + 1)), U!(Nat(*n))),
                [Ident(_, "U"), n] => {
                    let n_o = synthesize_with_type(n, &B!("Nat"), env)?;
                    match n_o {
                        Nat(n) => (U!(Nat(n + 1)), S("U", vec![n_o])),
                        _ => throw!(ErrorKind::CannotInferType {
                            expr: format!("{}", e)
                        }),
                    }
                }
                // 内建类型
                [Ident(_, "List" | "Vec" | "Either" | "=" | "Pair"), ..] => {
                    resolve_type_rule(e, env)?
                }
                // nil 和 vecnil 必须附加类型
                [Ident(_, s)] => throw!(ErrorKind::CannotInferType {
                    expr: s.to_string()
                }),
                // "The" 规则
                [Ident(_, "the"), ty, expr] => {
                    let (_, ty_o) = resolve_type(ty, env)?;
                    let expr_o = synthesize_with_type(expr, &ty_o, env)?;
                    (ty_o, expr_o)
                }
                // ListI-2
                [Ident(_, "::"), e, es] => {
                    let (ty_e_o, e_o) = synthesize(e, env)?;
                    let ty_list = bapp!("List", ty_e_o);
                    let es_o = synthesize_with_type(es, &ty_list, env)?;
                    (ty_list, S("::", vec![e_o, es_o]))
                }
                // NatI-2
                [Ident(_, "add1"), n] => {
                    let n_o = synthesize_with_type(n, &B!("Nat"), env)?;
                    (B!("Nat"), S("add1", vec![n_o]))
                }
                // VecE-1
                [Ident(_, "head"), v] => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let S("Vec", [ty_e, len]) = &ty_v; env };
                    if is_literal_add1(len) {
                        (ty_e.clone(), S("head", vec![v_o]))
                    } else {
                        throw!(ErrorKind::TypeNotMatch {
                            expected: "Vec longer than 1".to_owned(),
                            given: format!("{}", v),
                        })
                    }
                }
                // VecE-2
                [Ident(_, "tail"), v] => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let S("Vec", [ty_e, len]) = &ty_v; env };
                    if is_literal_add1(len) {
                        let ty_subv = bapp!("Vec", ty_e.clone(), literal_sub1(len));
                        (ty_subv, S("tail", vec![v_o]))
                    } else {
                        throw!(ErrorKind::TypeNotMatch {
                            expected: "Vec longer than 1".to_owned(),
                            given: format!("{}", v),
                        })
                    }
                }
                // SigmaE-1
                [Ident(_, "car"), pr] => {
                    let (ty_pr, pr_o) = synthesize(pr, env)?;
                    try_match! { let Sigma(_x, ty_a, _ty_d) = &ty_pr; env };
                    ((**ty_a).clone(), S("car", vec![pr_o]))
                }
                // SigmaE-2
                [Ident(_, "cdr"), pr] => {
                    let (ty_pr, pr_o) = synthesize(pr, env)?;
                    try_match! { let Sigma(_x, ty_a, ty_d) = &ty_pr; env };
                    // FIXME: 在此需要编译期计算
                    let car_pr = bapp!("car", pr_o.clone());
                    // FIXME!
                    let _ty_d_o = substitute(ty_d, "", &car_pr, env);
                    ((**ty_a).clone(), S("cdr", vec![pr_o]))
                }
                // NatE-1
                [Ident(_, "which-Nat"), t, b, s] => {
                    let t_o = synthesize_with_type(t, &B!("Nat"), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let s_o = synthesize_with_type(s, &ty_b, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b, S("which-Nat", vec![t_o, b_o, s_o]))
                }
                // NatE-2
                [Ident(_, "iter-Nat"), t, b, s] => {
                    let t_o = synthesize_with_type(t, &B!("Nat"), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = pi!(ref ty_b, ref ty_b);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), S("iter-Nat", vec![t_o, b_o, s_o]))
                }
                // NatE-3
                [Ident(_, "rec-Nat"), t, b, s] => {
                    let t_o = synthesize_with_type(t, &B!("Nat"), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = pi!(B!("Nat"), ref ty_b, ref ty_b);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), S("rec-Nat", vec![t_o, b_o, s_o]))
                }
                // NatE-4
                // [Ident(_, "ind-Nat"), t, m, b, s] => {
                //     let t_o = synthesize_with_type(t, &B!("Nat"), env)?;
                //     let ty_m = pi!(B!("Nat"), U!());
                //     let m_o = synthesize_with_type(m, &ty_m, env)?;
                //     let m_o = Ref::new(m_o);
                //     // FIXME: 在此需要编译期计算
                //     let ty_b = app!(ref m_o, Nat(0));
                //     let b_o = synthesize_with_type(b, &ty_b, env)?;
                //     // s : (k : Nat) -> (m k) -> (m (add1 k))
                //     let ty_s = pi!(
                //         B!("Nat"),
                //         app!(ref m_o, Identifier(0)),
                //         app!(ref m_o, bapp!("add1", Identifier(1))),
                //     );
                //     let s_o = synthesize_with_type(s, &ty_s, env)?;
                //     let ty_o = app!(ref m_o, t_o.clone());
                //     (
                //         ty_o,
                //         S("ind-Nat", vec![t_o, m_o.as_ref().clone(), b_o, s_o]),
                //     )
                // }
                // ListE-1
                [Ident(_, "rec-List"), t, b, s] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("List", [ty_e]) = &ty_t; env }
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = pi!(ty_e.clone(), ty_t, ref ty_b, ref ty_b,);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), S("rec-List", vec![t_o, b_o, s_o]))
                }
                // ListE-2
                // [Ident(_, "ind-List"), t, m, b, s] => {
                //     let (ty_t, t_o) = synthesize(t, env)?;
                //     try_match! { let S("List", [ty_e]) = &ty_t; env }
                //     let ty_m = pi!(ty_t.clone(), U!());
                //     let m_o = synthesize_with_type(m, &ty_m, env)?;
                //     let m_o = Ref::new(m_o);
                //     // FIXME: 在此需要编译期计算
                //     let ty_b = app!(ref m_o, bty::nil());
                //     let b_o = synthesize_with_type(b, &ty_b, env)?;
                //     let ty_s = pi!(
                //         ty_e.clone(),
                //         ty_t,
                //         app!(ref m_o, Identifier(0)),
                //         app!(ref m_o, bapp!("::", Identifier(2), Identifier(1)))
                //     );
                //     let s_o = synthesize_with_type(s, &ty_s, env)?;
                //     (
                //         app!(ref m_o, t_o.clone()),
                //         S("ind-List", vec![t_o, m_o.as_ref().clone(), b_o, s_o]),
                //     )
                // }
                // VecE-3
                // [Ident(_, "ind-Vec"), l, t, m, b, s] => {
                //     let l_o = synthesize_with_type(l, &B!("Nat"), env)?;
                //     let (ty_t, t_o) = synthesize(t, env)?;
                //     try_match! { let S("Vec", [ty_e, n]) = &ty_t; env }
                //     expr_check_same(&l_o, n, &B!("Nat"), env)?;
                //     let ty_m = pi!(
                //         B!("Nat"),
                //         bapp!("Vec", ty_e.clone(), Identifier(0)),
                //         U!()
                //     );
                //     let m_o = synthesize_with_type(m, &ty_m, env)?;
                //     let m_o = Ref::new(m_o);
                //     // FIXME: 在此需要编译期计算
                //     let ty_b = app!(ref m_o, bty::zero(), bty::vecnil());
                //     let b_o = synthesize_with_type(b, &ty_b, env)?;
                //     let ty_s = pi!(
                //         B!("Nat"),
                //         ty_e.clone(),
                //         bapp!("Vec", ty_e.clone(), Identifier(1)),
                //         app!(ref m_o, Identifier(2), Identifier(0)),
                //         app!(
                //             ref m_o,
                //             bapp!("add1", Identifier(3)),
                //             bapp!("vec::", Identifier(2), Identifier(1))
                //         )
                //     );
                //     let s_o = synthesize_with_type(s, &ty_s, env)?;
                //     (
                //         app!(ref m_o, l_o.clone(), t_o.clone()),
                //         S("ind-Vec", vec![l_o, t_o, m_o.as_ref().clone(), b_o, s_o]),
                //     )
                // }
                // EitherE
                // [Ident(_, "ind-Either"), t, m, bl, br] => {
                //     let (ty_t, t_o) = synthesize(t, env)?;
                //     try_match! { let S("Either", [ty_p, ty_s]) = &ty_t; env }
                //     let ty_m = pi!(ty_t.clone(), U!());
                //     let m_o = synthesize_with_type(m, &ty_m, env)?;
                //     let m_o = Ref::new(m_o);
                //     // FIXME: 在此需要编译期计算
                //     let ty_bl = pi!(ty_p.clone(), app!(ref m_o, bapp!("left", Identifier(0))));
                //     let bl_o = synthesize_with_type(bl, &ty_bl, env)?;
                //     let ty_br = pi!(ty_s.clone(), app!(ref m_o, bapp!("right", Identifier(0))));
                //     let br_o = synthesize_with_type(br, &ty_br, env)?;
                //     (
                //         app!(ref m_o, t_o.clone()),
                //         S("ind-Either", vec![t_o, m_o.as_ref().clone(), bl_o, br_o]),
                //     )
                // }
                // AbsE
                [Ident(_, "ind-Absurd"), t, m] => {
                    let t_o = synthesize_with_type(t, &I("Absurd"), env)?;
                    let (_lm, m_o) = resolve_type(m, env)?;
                    (m_o.clone(), S("ind-Absurd", vec![t_o, m_o]))
                }
                // EqE-1
                [Ident(_, "replace"), t, _m, b] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("=", [ty_x, from, to]) = &ty_t; env }
                    let m_o = pi!(ty_x.clone(), U!());
                    let m_o = Ref::new(m_o);
                    let b_o = synthesize_with_type(b, &app!(ref m_o, from.clone()), env)?;
                    (
                        app!(ref m_o, to.clone()),
                        bapp!("replace", t_o, m_o.as_ref().clone(), b_o),
                    )
                }
                // EqE-2
                [Ident(_, "cong"), t, f] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("=", [ty_x1, from, to]) = &ty_t; env }
                    let (ty_f, f_o) = synthesize(f, env)?;
                    try_match! { let Pi(_arg, ty_x2, ty_y) = &ty_f; env }
                    type_check_same(ty_x1, ty_x2, env)?;
                    let f_o = Ref::new(f_o);
                    let ty = bapp!(
                        "=",
                        ty_y.as_ref().clone(),
                        app!(ref f_o, from.clone()),
                        app!(ref f_o, to.clone())
                    );
                    // FIXME: TLT 中需要多一个参数
                    (ty, bapp!("cong", t_o, f_o.as_ref().clone()))
                }
                // EqE-3
                [Ident(_, "symm"), t] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("=", [ty_x, from, to]) = &ty_t; env }
                    (
                        bapp!("=", ty_x.clone(), to.clone(), from.clone()),
                        bapp!("symm", t_o),
                    )
                }
                // EqE-4
                [Ident(_, "trans"), t1, t2] => {
                    let (ty_t1, t1_o) = synthesize(t1, env)?;
                    try_match! { let S("=", [ty_x, from, mid1]) = &ty_t1; env }
                    let (ty_t2, t2_o) = synthesize(t2, env)?;
                    try_match! { let S("=", [ty_y, mid2, to]) = &ty_t2; env }
                    type_check_same(ty_x, ty_y, env)?;
                    expr_check_same(mid1, mid2, ty_x, env)?;
                    (
                        bapp!("=", ty_x.clone(), from.clone(), to.clone()),
                        bapp!("trans", t1_o, t2_o),
                    )
                }
                // EqE-5
                // [Ident(_, "ind-="), t, m, b] => {
                //     let (ty_t, t_o) = synthesize(t, env)?;
                //     try_match! { let S("=", [ty_x, from, to]) = &ty_t; env }
                //     let ty_m = pi!(
                //         ty_x.clone(),
                //         bapp!("=", ty_x.clone(), from.clone(), Identifier(0)),
                //         U!()
                //     );
                //     let m_o = synthesize_with_type(m, &ty_m, env)?;
                //     let m_o = Ref::new(m_o);
                //     let ty_b = app!(ref m_o, from.clone(), bapp!("same", from.clone()));
                //     let b_o = synthesize_with_type(b, &ty_b, env)?;
                //     (
                //         app!(ref m_o, to.clone(), t_o.clone()),
                //         bapp!("ind-=", t_o, m_o.as_ref().clone(), b_o),
                //     )
                // }
                _ => throw!(ErrorKind::CannotInferType {
                    expr: format!("{}", e)
                }),
            }
        }
    };

    tc_log_end!("=> (the {} {})", dpp(&ret.0, env), dpp(&ret.1, env));
    Ok(ret)
}

/// 判断并计算表达式是一个类型或 U(n)，返回其类型层级，相当于为 U(n) 特化的 synthesize。
/// 改进的第四种 Judgement，见 Figure B.1。
pub fn resolve_type(e: &ast::Expr, env: &Env) -> Result<(u64, core::Expr), Error> {
    tc_log!("resolve `{}` is a type", e);

    use ast::Expr::*;
    use core::Expr::*;

    // TODO: 改进 El 规则
    let ret = match e {
        // FunF-1, FunF-2
        PiExpr(_, _args, _body) => {
            todo!("resolve_type: PiExpr")
        }
        // FunF->1, FunF->2
        ArrowExpr(sp, args) => {
            match args.as_slice() {
                // FunF->1
                [ty_a, ty_r] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let (l_r, ty_r_o) = resolve_type(ty_r, env)?;
                    (std::cmp::max(l_a, l_r), pi!(ty_a_o, ty_r_o))
                }
                // FunF->2
                [ty_a, rargs @ ..] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let (l_r, ty_r_o) = resolve_type(&AppExpr(*sp, rargs.to_vec()), env)?;
                    (std::cmp::max(l_a, l_r), pi!(ty_a_o, ty_r_o))
                }
                _ => unreachable!(),
            }
        }
        // SigmaF-1
        SigmaExpr(_, _args, _body) => todo!("resolve_type: SigmaExpr"),
        NatLit(_, _) | AtomLit(_, _) => {
            return Err(ErrorKind::NotType(format!("{}", e)).into());
        }
        AppExpr(_, args) => {
            match args.as_slice() {
                // ListF
                [Ident(_, "List"), ty_e] => {
                    let (l, ty_e_o) = resolve_type(ty_e, env)?;
                    (l, bapp!("List", ty_e_o))
                }
                // ΣF-Pair
                [Ident(_, "Pair"), ty_a, ty_d] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let (l_d, ty_d_o) = resolve_type(ty_d, env)?;
                    (
                        std::cmp::max(l_a, l_d),
                        Sigma(Argument::Dummy, Ref::new(ty_a_o), Ref::new(ty_d_o)),
                    )
                }
                // VecF
                [Ident(_, "Vec"), ty, len] => {
                    let (l, ty_o) = resolve_type(ty, env)?;
                    let len_o = synthesize_with_type(len, &B!("Nat"), env)?;
                    (l, bapp!("Vec", ty_o, len_o))
                }
                // EitherF
                [Ident(_, "Either"), ty_l, ty_r] => {
                    let (l_l, ty_l_o) = resolve_type(ty_l, env)?;
                    let (l_r, ty_r_o) = resolve_type(ty_r, env)?;
                    (std::cmp::max(l_l, l_r), bapp!("Either", ty_l_o, ty_r_o))
                }
                // EqF
                [Ident(_, "="), ty, from, to] => {
                    let (l, ty_o) = resolve_type(ty, env)?;
                    let from_o = synthesize_with_type(from, &ty_o, env)?;
                    let to_o = synthesize_with_type(to, &ty_o, env)?;
                    (l, bapp!("=", ty_o, from_o, to_o))
                }
                // UF
                [Ident(_, "U"), NatLit(_, n)] => (*n + 1, U!(Nat(*n))),
                [Ident(_, "U"), n] => {
                    let n_o = synthesize_with_type(n, &B!("Nat"), env)?;
                    match n_o {
                        Nat(n) => (n + 1, U!(n_o)),
                        _ => throw!(ErrorKind::CannotInferType {
                            expr: format!("{}", e)
                        }),
                    }
                }
                [Ident(_, "add1"), ..] => throw!(ErrorKind::NotType("(add1 ...)".into())),
                _ => unreachable!(),
            }
        }
        // 内建单例对象
        Ident(_, ty @ ("Atom" | "Nat" | "Trivial" | "Absurd")) => (0, I(ast::to_builtin_name(ty))),
        Ident(_, "U") => (1, U!(Nat(0))),
        // 非类型单例对象
        Ident(_, e @ ("zero" | "sole" | "nil" | "vecnil")) => {
            return Err(ErrorKind::NotType(e.to_string()).into());
        }
        //Literal, Lambda, Identifier, Apply
        // El
        _ => (0, synthesize_with_type(e, &U!(), env)?),
    };

    tc_log_end!("=> (the (U {}) {})", ret.0, dpp(&ret.1, env));
    Ok(ret)
}

// 将 resolve_type 的返回值包装为 (U(n), t_o)
#[inline]
fn resolve_type_rule(ty: &ast::Expr, env: &Env) -> Result<(core::Expr, core::Expr), Error> {
    let (l, t_o) = resolve_type(ty, env)?;
    Ok((U!(Nat(l)), t_o))
}

/// 检查是否相同类型
/// 第五种 Judgement，见 Figure B.1。
#[inline]
fn type_check_same(ty1: &core::Expr, ty2: &core::Expr, env: &Env) -> Result<(), Error> {
    if !is_type_check_same(ty1, ty2, env) {
        throw!(ErrorKind::NotSame(
            dpp(ty1, env).to_string(),
            dpp(ty2, env).to_string(),
            "(U _)".to_owned(),
        ));
    }
    Ok(())
}

fn is_type_check_same(ty1: &core::Expr, ty2: &core::Expr, env: &Env) -> bool {
    tc_log!(
        "check `{}` and `{}` are the same type",
        dpp(ty1, env),
        dpp(ty2, env)
    );

    use core::Expr::*;
    // TODO: 比较前充分计算 ty1 和 ty2
    let ret = match (ty1, ty2) {
        // TODO: de bruijn
        (Identifier(id1, _idx1), Identifier(id2, _idx2)) => id1 == id2,
        (I(ty1), I(ty2)) => ty1 == ty2,
        (Sigma(a1, ty_a1, ty_r1), Sigma(_a2, ty_a2, ty_r2)) => {
            // FIXME: variable scope
            is_type_check_same(ty_a1, ty_a2, env)
                && is_type_check_same(ty_r1, ty_r2, &env_ext_arg(env, a1, ty_a1))
        }
        (S(f1, args1), S(f2, args2)) => match (&**f1, &**args1, &**f2, &**args2) {
            ("List", [ty_e1], "List", [ty_e2]) => is_type_check_same(ty_e1, ty_e2, env),
            ("Vec", [ty_e1, len1], "Vec", [ty_e2, len2]) => {
                is_type_check_same(ty_e1, ty_e2, env)
                    && is_expr_check_same(len1, len2, &B!("Nat"), env)
            }
            ("Either", [ty_l1, ty_r1], "Either", [ty_l2, ty_r2]) => {
                is_type_check_same(ty_l1, ty_l2, env) && is_type_check_same(ty_r1, ty_r2, env)
            }
            ("=", [ty_x1, from1, to1], "=", [ty_x2, from2, to2]) => {
                is_type_check_same(ty_x1, ty_x2, env)
                    && is_expr_check_same(from1, from2, ty_x1, env)
                    && is_expr_check_same(to1, to2, ty_x1, env)
            }
            ("U", [n1], "U", [n2]) => is_expr_check_same(n1, n2, &B!("Nat"), env),
            ("U", _, "List" | "Vec" | "=" | "Either", _) => false,
            ("List" | "Vec" | "=" | "Either", _, "U", _) => false,
            _ => {
                todo!(
                    "is_type_check_same: unhandled case: {} and {}",
                    dpp(ty1, env),
                    dpp(ty2, env)
                )
            }
        },
        (S("U", _), I("Atom" | "Nat" | "Trivial" | "Absurd")) => false,
        (I("Atom" | "Nat" | "Trivial" | "Absurd"), S("U", _)) => false,
        _ => {
            todo!(
                "is_type_check_same: unhandled case: {} and {}",
                dpp(ty1, env),
                dpp(ty2, env)
            )
        }
    };
    tc_log_end!("=> {}", ret);
    ret
}

/// 检查是否相同表达式
/// 认为 `c1: ct` 与 `c2: ct` 已满足
/// 第八种 Judgement，见 Figure B.1。
pub fn expr_check_same(
    c1: &core::Expr,
    c2: &core::Expr,
    ct: &core::Expr,
    env: &Env,
) -> Result<(), Error> {
    if !is_expr_check_same(c1, c2, ct, env) {
        throw!(ErrorKind::NotSame(
            dpp(c1, env).to_string(),
            dpp(c2, env).to_string(),
            dpp(ct, env).to_string(),
        ));
    }
    Ok(())
}

fn is_expr_check_same(c1: &core::Expr, c2: &core::Expr, ct: &core::Expr, env: &Env) -> bool {
    tc_log!(
        "check `{}` and `{}` are the same `{}`",
        dpp(c1, env),
        dpp(c2, env),
        dpp(ct, env)
    );

    use core::Expr::*;
    // TODO: 比较前充分计算 c1、c2、ct
    let ret = match (c1, c2) {
        // HypothesisSame
        // TODO: de bruijn
        (Identifier(id1, _idx1), Identifier(id2, _idx2)) => id1 == id2,
        (S("U", l1), S("U", l2)) => is_expr_check_same(&l1[0], &l2[0], &B!("Nat"), env),
        // 比较自然数，考虑字面量和构造器表示
        // NatSame-zero, NatSame-literal
        (Nat(m), Nat(n)) => m == n,
        // NatSame-add1
        (S("add1", args), Nat(n)) | (Nat(n), S("add1", args)) => {
            *n > 0 && is_expr_check_same(&args[0], &Nat(n - 1), ct, env)
        }
        (S("add1", args), S("add1", args2)) => is_expr_check_same(&args[0], &args2[0], ct, env),
        // NatSame-Nat, AtomSame-Atom, ListSame-nil ...
        (I(ty1), I(ty2)) => ty1 == ty2,
        // AtomSame-tick
        (Atom(a1), Atom(a2)) => a1 == a2,
        // ΣSame-Σ
        (Sigma(arg1, ty_a1, ty_d1), Sigma(_arg2, ty_a2, ty_d2)) => {
            is_type_check_same(ty_a1, ty_a2, env)
                && is_type_check_same(ty_d1, ty_d2, &env_ext_arg(env, arg1, ty_a1))
        }
        // ΣSame-cons
        (S("cons", args1), S("cons", args2)) => {
            no_else! { let ([a1, d1], [a2, d2], Sigma(arg, ty_a, ty_d)) = (&**args1, &**args2, ct) }
            is_expr_check_same(a1, a2, ty_a, env)
                && is_expr_check_same(d1, d2, &substitute_arg(ty_d, arg, a1, env), env)
        }
        // FunSame-λ
        (Lambda(_, r1), Lambda(_, r2)) => {
            no_else! { let Pi(a, ty_a, ty_r) = ct }
            is_expr_check_same(r1, r2, ty_r, &env_ext_arg(env, a, ty_a))
        }
        _ => false,
    };
    tc_log_end!("=> {}", ret);
    ret
}

pub fn default_environment() -> Env {
    Env::new()
}
