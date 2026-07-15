use crate::{core_ast, utils, Never};
use core_ast::{
    builtin_type as bty, Argument, DBIPPrint as dpp, Expr, Expr::NatLiteral, Type, ULevel,
};
use fehler::{throw, throws};
use pielang_macros::tc_log;
use std::{cell::Cell, fmt};
use utils::{LocatedError, Ref};

// TODO: 改进打印方式，将这里改成 StackMap<Option<Ref<str>>, Type<Never>>
pub type Env = crate::utils::StackMap<Option<Ref<str>>, Option<Type<Never>>>;

type Error = LocatedError<ErrorKind>;

#[derive(Debug, Clone)]
pub enum ErrorKind {
    TypeNotMatch { expected: String, given: String },
    CannotInferType { expr: String },
    NotSame(String, String, String),
}

impl fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorKind::*;
        match self {
            TypeNotMatch { expected, given } => {
                write!(f, "expect a `{}`, but get `{}`", expected, given)
            }
            CannotInferType { expr } => {
                write!(f, "cannot infer the type of `{}`", expr)
            }
            NotSame(x, y, t) => {
                write!(f, "`{}` and `{}` are not the same `{}`", x, y, t)
            }
        }
    }
}

macro_rules! try_match {
    (let BuiltinApply($bf:literal , [$($i:ident),+ $(,)?]) = $e:expr; $env:expr) => {
        let ($($i,)+) = if let BuiltinApply(ref bf, ref args) = $e {
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

macro_rules! match_array {
    (let [$($i:ident),+ $(,)?] = $e:expr, $($on_fail:tt)+) => {
        let ($($i,)+) = if let [$($i),+] = $e {
            ($($i,)+)
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

macro_rules! pi {
    ($ty_a:expr, $ty_r:expr $(,)?) => {
        Expr::PiExpr(Argument::Dummy, Ref::new($ty_a), Ref::new($ty_r))
    };
    (ref $ty_a:expr, $ty_r:expr $(,)?) => {
        Expr::PiExpr(Argument::Dummy, Ref::clone(&$ty_a), Ref::new($ty_r))
    };
    ($ty_a:expr, ref $ty_r:expr $(,)?) => {
        Expr::PiExpr(Argument::Dummy, Ref::new($ty_a), Ref::clone(&$ty_r))
    };
    (ref $ty_a:expr, ref $ty_r:expr $(,)?) => {
        Expr::PiExpr(Argument::Dummy, Ref::clone(&$ty_a), Ref::clone(&$ty_r))
    };
    ($ty_a:expr, $($e:tt)+) => {
        Expr::PiExpr(
            Argument::Dummy,
            Ref::new($ty_a),
            Ref::new(pi!($($e)+)))
    };
    (ref $ty_a:expr, $($e:tt)+) => {
        Expr::PiExpr(
            Argument::Dummy,
            Ref::clone(&$ty_a),
            Ref::new(pi!($($e)+)))
    };
}

macro_rules! app {
    ($f:expr, $a:expr $(,)?) => {
        Expr::Apply(Ref::new($f), Ref::new($a))
    };
    (ref $f:expr, $a:expr $(,)?) => {
        Expr::Apply(Ref::clone(&$f), Ref::new($a))
    };
    ($f:expr, ref $a:expr $(,)?) => {
        Expr::Apply(Ref::new($f), Ref::clone(&$a))
    };
    (ref $f:expr, ref $a:expr $(,)?) => {
        Expr::Apply(Ref::clone(&$f), Ref::clone(&$a))
    };
    ($f:expr, $a:expr, $($tt:tt)+) => {
        app!(Expr::Apply(Ref::new($f), Ref::new($a)), $($tt)+)
    };
    (ref $f:expr, $a:expr, $($tt:tt)+) => {
        app!(Expr::Apply(Ref::clone(&$f), Ref::new($a)), $($tt)+)
    };
    ($f:expr, ref $a:expr, $($tt:tt)+) => {
        app!(Expr::Apply(Ref::new($f), Ref::clone(&$a)), $($tt)+)
    };
    (ref $f:expr, ref $a:expr, $($tt:tt)+) => {
        app!(Expr::Apply(Ref::clone(&$f), Ref::clone(&$a)), $($tt)+)
    };
}

macro_rules! bapp {
    ($bf:expr $(,$a:expr)+ $(,)?) => {
        Expr::BuiltinApply($bf, vec![$($a),*])
    };
}

thread_local! {
    pub static INDENT: Cell<usize> = const { Cell::new(0) };
}

/// 缩进守卫，进入时增加缩进，退出时自动恢复
pub struct IndentGuard;

impl IndentGuard {
    pub fn new() -> Self {
        INDENT.set(INDENT.get() + 1);
        IndentGuard
    }
}

impl std::ops::Drop for IndentGuard {
    fn drop(&mut self) {
        INDENT.set(INDENT.get() - 1);
    }
}

// TODO: 使用 De Bruijn 方法解决变量名、作用域的各种问题

/// 执行 expr[var/e]，将 expr 中自由出现的 var 替换为 e，e 应当是没有自由变量的。
#[tc_log("substitute `{}` to `{}` in `{}`", var, dpp(e, env), dpp(expr, env))]
fn substitute(expr: &Expr<Never>, var: &str, e: &Expr<Never>, env: &Env) -> Expr<Never> {
    todo!()
}

/// 对常用的 Argument 模式的简写
#[inline]
fn substitute_arg(body: &Expr<Never>, arg: &Argument, e: &Expr<Never>, env: &Env) -> Expr<Never> {
    match arg {
        Argument::Symbol(sym) => substitute(body, sym, e, env),
        Argument::Dummy => body.clone(),
    }
}

#[inline]
fn env_ext(env: &Env, name: Option<Ref<str>>, ty: &Type<Never>) -> Env {
    env.insert(name, Some(ty.clone()))
}

fn env_get_nth_type(env: &Env, n: usize) -> &Type<Never> {
    // 经过作用域检查，保证不会 panic
    env.iter().nth(n).and_then(|(_, ty)| ty.as_ref()).unwrap()
}

/// 先综合出 e 的类型，再检查其是否与 ty 相同
#[inline]
#[throws]
fn switch_rule<M: fmt::Display>(e: &Expr<M>, ty: &Type<Never>, env: &Env) -> Expr<Never> {
    let (ty_e_o, e_o) = synthesize(e, env)?;
    // TODO: 改为 context
    type_check_same(&ty_e_o, &ty, env).map_err(|_| ErrorKind::TypeNotMatch {
        expected: dpp(ty, env).to_string(),
        given: dpp(&ty_e_o, env).to_string(),
    })?;
    e_o
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
/// 对于构造式，有唯一相关的类型与之匹配；
/// 其它表达式则应用 Which 规则：试图综合得出其类型，再将结果与所给类型比较。
#[tc_log(
    "check `{}` is a `{}`", dpp(e, env), dpp(ty, env);
    "=> {}", dpp(&ret, env)
)]
#[throws]
pub fn synthesize_with_type<M: fmt::Display>(
    e: &Expr<M>,
    ty: &Type<Never>,
    env: &Env,
) -> Expr<Never> {
    use Expr::*;
    if let Info(_, e) = e {
        return synthesize_with_type(e, ty, env)?;
    }
    match (e, ty) {
        // 简单情况优化
        (BuiltinId("sole"), BuiltinId("Trivial")) => BuiltinId("sole"),
        (AtomLiteral(a), BuiltinId("Atom")) => AtomLiteral(a.clone()),
        (BuiltinId("zero"), BuiltinId("Nat")) => BuiltinId("zero"),
        (NatLiteral(n), BuiltinId("Nat")) => NatLiteral(*n),
        (BuiltinApply("add1", args), BuiltinId("Nat")) => {
            match_builtin_args!(let [n] = &**args);
            let n_o = synthesize_with_type(n, &bty::nat(), env)?;
            BuiltinApply("add1", vec![n_o])
        }
        (BuiltinId(ty @ ("Nat" | "Atom" | "Trivial" | "Absurd")), BuiltinApply("U", args))
            if let [NatLiteral(0)] = **args =>
        {
            BuiltinId(ty)
        }
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
        (BuiltinApply("cons", args), SigmaExpr(arg, ty_a, ty_d)) => {
            match_builtin_args!(let [a, d] = &**args);
            let a_o = synthesize_with_type(a, ty_a, env)?;
            let d_o = synthesize_with_type(d, &substitute_arg(ty_d, arg, &a_o, env), env)?;
            BuiltinApply("cons", vec![a_o, d_o])
        }
        (BuiltinApply(bf, args), BuiltinApply(ty_bf, ty_args)) => {
            match (&**bf, &**args, &**ty_bf, &**ty_args) {
                // ListI-1
                ("nil", [], "List", [_ty]) => BuiltinApply(bf, vec![]),
                // ListI-3，TLY 中不存在，我自己加的，使 (the (List (-> Nat Nat)) (:: (lambda (x) x) nil)) 这样的
                // 表达式能推导出类型。
                ("::", [e, es], "List", [ty_1]) => {
                    let e_o = synthesize_with_type(e, ty_1, env)?;
                    let es_o = synthesize_with_type(es, ty, env)?;
                    BuiltinApply(bf, vec![e_o, es_o])
                }
                // VecI-1
                ("vecnil", [], "Vec", [_ty, len]) => {
                    if is_literal_zero(len) {
                        BuiltinApply(bf, vec![])
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
                    let ty_subvec = BuiltinApply(ty_bf, vec![ty_e.clone(), sublen]);
                    let es_o = synthesize_with_type(es, &ty_subvec, env)?;
                    BuiltinApply(bf, vec![e_o, es_o])
                }
                // EitehrI-1
                ("left", [lt], "Either", [ty_l, _ty_r]) => {
                    let lt_o = synthesize_with_type(lt, ty_l, env)?;
                    BuiltinApply(bf, vec![lt_o])
                }
                // EitehrI-2
                ("right", [rt], "Either", [_ty_l, ty_r]) => {
                    let rt_o = synthesize_with_type(rt, ty_r, env)?;
                    BuiltinApply(bf, vec![rt_o])
                }
                // EqI
                ("same", [mid], "=", [ty_x, from, to]) => {
                    let mid_o = synthesize_with_type(mid, ty_x, env)?;
                    expr_check_same(from, &mid_o, ty_x, env)?;
                    expr_check_same(&mid_o, to, ty_x, env)?;
                    BuiltinApply(bf, vec![mid_o])
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
        NatLiteral(0) => true,
        BuiltinId(bid) if *bid == "zero" => true,
        _ => false,
    }
}

fn is_literal_add1<M>(e: &Expr<M>) -> bool {
    use Expr::*;
    match e {
        NatLiteral(0) => false,
        NatLiteral(_) => true,
        BuiltinApply(bf, _) if &**bf == "add1" => true,
        _ => false,
    }
}

fn literal_sub1(e: &Expr<Never>) -> Expr<Never> {
    use Expr::*;
    match e {
        NatLiteral(n) => {
            debug_assert_ne!(*n, 0);
            NatLiteral(n - 1)
        }
        BuiltinApply(bf, args) => match (&**bf, &**args) {
            ("add1", [n]) => n.clone(),
            _ => unreachable!(),
        },
        _ => unreachable!(),
    }
}

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
#[tc_log(
    "synthesize `{}`", dpp(e, env);
    "=> (the {} {})", dpp(&ret.0, env), dpp(&ret.1, env)
)]
#[throws]
pub fn synthesize<M: fmt::Display>(e: &Expr<M>, env: &Env) -> (Type<Never>, Expr<Never>) {
    use Expr::BuiltinId as Id;
    use Expr::*;
    match e {
        Info(_, e) => synthesize(e, env)?,
        NatLiteral(n) => (bty::nat(), NatLiteral(*n)),
        AtomLiteral(a) => (bty::atom(), AtomLiteral(a.clone())),
        // Hypothesis
        Identifier(ident) => {
            let ty = env_get_nth_type(env, *ident).clone();
            (ty, Identifier(ident.clone()))
        }
        PiExpr(_arg, ty_a, _ty_r) => resolve_type_rule(e, env)?,
        SigmaExpr(_arg, ty_a, _ty_d) => resolve_type_rule(e, env)?,
        // FunE-1
        Apply(f, arg) => {
            let (ty_f, f_o) = synthesize(f, env)?;
            try_match!(let PiExpr(var, ty_arg, ty_ret) = &ty_f; &env);
            let arg_o = synthesize_with_type(arg, &ty_arg, env)?;
            let ty = substitute_arg(&ty_ret, &var, &arg_o, env);
            (ty, app!(f_o, arg_o))
        }
        Id(ty @ ("Atom" | "Nat" | "Trivial" | "Absurd")) => (bty::u(), Id(ty)),
        Id("zero") => (bty::nat(), Id("zero")),
        Id("sole") => (bty::trivial(), Id("sole")),
        BuiltinApply(bf, args) => {
            match (&**bf, &**args) {
                // (U n): (U (add1 n))
                ("U", [NatLiteral(n)]) => (bty::u_l(NatLiteral(*n + 1)), bty::u_l(NatLiteral(*n))),
                ("U", [n]) => {
                    let n_o = synthesize_with_type(n, &bty::nat(), env)?;
                    match n_o {
                        NatLiteral(n) => (bty::u_l(NatLiteral(n + 1)), BuiltinApply(bf, vec![n_o])),
                        _ => throw!(ErrorKind::CannotInferType {
                            expr: format!("{}", dpp(e, env))
                        }),
                    }
                }
                // 内建类型
                ("List" | "Vec" | "Either" | "=", _) => resolve_type_rule(e, env)?,
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
                    (ty_list, BuiltinApply(bf, vec![e_o, es_o]))
                }
                // NatI-2
                ("add1", [n]) => {
                    let n_o = synthesize_with_type(n, &bty::nat(), env)?;
                    (bty::nat(), BuiltinApply(bf, vec![n_o]))
                }
                // VecE-1
                ("head", [v]) => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let BuiltinApply("Vec", [ty_e, len]) = &ty_v; env };
                    if is_literal_add1(&len) {
                        (ty_e.clone(), BuiltinApply(bf, vec![v_o]))
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
                        (ty_subv, BuiltinApply(bf, vec![v_o]))
                    } else {
                        throw!(ErrorKind::TypeNotMatch {
                            expected: "Vec longer than 1".to_owned(),
                            given: format!("{}", dpp(v, env)),
                        })
                    }
                }
                // SigmaE-1
                ("car", [pr]) => {
                    let (ty_pr, pr_o) = synthesize(pr, env)?;
                    try_match! { let SigmaExpr(_x, ty_a, _ty_d) = &ty_pr; env };
                    (Expr::clone(ty_a), BuiltinApply(bf, vec![pr_o]))
                }
                // SigmaE-2
                ("cdr", [pr]) => {
                    let (ty_pr, pr_o) = synthesize(pr, env)?;
                    try_match! { let SigmaExpr(_x, ty_a, ty_d) = &ty_pr; env };
                    // FIXME: 在此需要编译期计算
                    let car_pr = bapp!("car", pr_o.clone());
                    // FIXME!
                    let _ty_d_o = substitute(ty_d, "", &car_pr, env);
                    (Expr::clone(ty_a), BuiltinApply(bf, vec![pr_o]))
                }
                // NatE-1
                ("which-Nat", [t, b, s]) => {
                    let t_o = synthesize_with_type(t, &bty::nat(), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let s_o = synthesize_with_type(s, &ty_b, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b, BuiltinApply(bf, vec![t_o, b_o, s_o]))
                }
                // NatE-2
                ("iter-Nat", [t, b, s]) => {
                    let t_o = synthesize_with_type(t, &bty::nat(), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = pi!(ref ty_b, ref ty_b);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), BuiltinApply(bf, vec![t_o, b_o, s_o]))
                }
                // NatE-3
                ("rec-Nat", [t, b, s]) => {
                    let t_o = synthesize_with_type(t, &bty::nat(), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = pi!(bty::nat(), ref ty_b, ref ty_b);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), BuiltinApply(bf, vec![t_o, b_o, s_o]))
                }
                // NatE-4
                ("ind-Nat", [t, m, b, s]) => {
                    let t_o = synthesize_with_type(t, &bty::nat(), env)?;
                    let ty_m = pi!(bty::nat(), bty::u());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    // FIXME: 在此需要编译期计算
                    let ty_b = app!(ref m_o, NatLiteral(0));
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    // s : (k : Nat) -> (m k) -> (m (add1 k))
                    let ty_s = pi!(
                        bty::nat(),
                        app!(ref m_o, Identifier(0)),
                        app!(ref m_o, bapp!("add1", Identifier(1))),
                    );
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    let ty_o = app!(ref m_o, t_o.clone());
                    (
                        ty_o,
                        BuiltinApply(bf, vec![t_o, m_o.as_ref().clone(), b_o, s_o]),
                    )
                }
                // ListE-1
                ("rec-List", [t, b, s]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("List", [ty_e]) = &ty_t; env }
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = pi!(ty_e.clone(), ty_t, ref ty_b, ref ty_b,);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), BuiltinApply(bf, vec![t_o, b_o, s_o]))
                }
                // ListE-2
                ("ind-List", [t, m, b, s]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("List", [ty_e]) = &ty_t; env }
                    let ty_m = pi!(ty_t.clone(), bty::u());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    // FIXME: 在此需要编译期计算
                    let ty_b = app!(ref m_o, bty::nil());
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    let ty_s = pi!(
                        ty_e.clone(),
                        ty_t,
                        app!(ref m_o, Identifier(0)),
                        app!(ref m_o, bapp!("::", Identifier(2), Identifier(1)))
                    );
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    (
                        app!(ref m_o, t_o.clone()),
                        BuiltinApply(bf, vec![t_o, m_o.as_ref().clone(), b_o, s_o]),
                    )
                }
                // VecE-3
                ("ind-Vec", [l, t, m, b, s]) => {
                    let l_o = synthesize_with_type(l, &bty::nat(), env)?;
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("Vec", [ty_e, n]) = &ty_t; env }
                    expr_check_same(&l_o, &n, &bty::nat(), env)?;
                    let ty_m = pi!(
                        bty::nat(),
                        bapp!("Vec", ty_e.clone(), Identifier(0)),
                        bty::u()
                    );
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    // FIXME: 在此需要编译期计算
                    let ty_b = app!(ref m_o, bty::zero(), bty::vecnil());
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    let ty_s = pi!(
                        bty::nat(),
                        ty_e.clone(),
                        bapp!("Vec", ty_e.clone(), Identifier(1)),
                        app!(ref m_o, Identifier(2), Identifier(0)),
                        app!(
                            ref m_o,
                            bapp!("add1", Identifier(3)),
                            bapp!("vec::", Identifier(2), Identifier(1))
                        )
                    );
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    (
                        app!(ref m_o, l_o.clone(), t_o.clone()),
                        BuiltinApply(bf, vec![l_o, t_o, m_o.as_ref().clone(), b_o, s_o]),
                    )
                }
                // EitherE
                ("ind-Either", [t, m, bl, br]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("Either", [ty_p, ty_s]) = &ty_t; env }
                    let ty_m = pi!(ty_t.clone(), bty::u());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    // FIXME: 在此需要编译期计算
                    let ty_bl = pi!(ty_p.clone(), app!(ref m_o, bapp!("left", Identifier(0))));
                    let bl_o = synthesize_with_type(bl, &ty_bl, env)?;
                    let ty_br = pi!(ty_s.clone(), app!(ref m_o, bapp!("right", Identifier(0))));
                    let br_o = synthesize_with_type(br, &ty_br, env)?;
                    (
                        app!(ref m_o, t_o.clone()),
                        BuiltinApply(bf, vec![t_o, m_o.as_ref().clone(), bl_o, br_o]),
                    )
                }
                // AbsE
                ("ind-Absurd", [t, m]) => {
                    let t_o = synthesize_with_type(t, &bty::absurd(), env)?;
                    let (_lm, m_o) = resolve_type(m, env)?;
                    (m_o.clone(), BuiltinApply(bf, vec![t_o, m_o]))
                }
                // EqE-1
                ("replace", [t, _m, b]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("=", [ty_x, from, to]) = &ty_t; env }
                    let m_o = pi!(ty_x.clone(), bty::u());
                    let m_o = Ref::new(m_o);
                    let b_o = synthesize_with_type(b, &app!(ref m_o, from.clone()), env)?;
                    (
                        app!(ref m_o, to.clone()),
                        bapp!(bf, t_o, m_o.as_ref().clone(), b_o),
                    )
                }
                // EqE-2
                ("cong", [t, f]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("=", [ty_x1, from, to]) = &ty_t; env }
                    let (ty_f, f_o) = synthesize(f, env)?;
                    try_match! { let PiExpr(_arg, ty_x2, ty_y) = &ty_f; env }
                    type_check_same(ty_x1, ty_x2, env)?;
                    let f_o = Ref::new(f_o);
                    let ty = bapp!(
                        "=",
                        ty_y.as_ref().clone(),
                        app!(ref f_o, from.clone()),
                        app!(ref f_o, to.clone())
                    );
                    // FIXME: TLT 中需要多一个参数
                    (ty, bapp!(bf, t_o, f_o.as_ref().clone()))
                }
                // EqE-3
                ("symm", [t]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("=", [ty_x, from, to]) = &ty_t; env }
                    (
                        bapp!("=", ty_x.clone(), to.clone(), from.clone()),
                        bapp!(bf, t_o),
                    )
                }
                // EqE-4
                ("trans", [t1, t2]) => {
                    let (ty_t1, t1_o) = synthesize(t1, env)?;
                    try_match! { let BuiltinApply("=", [ty_x, from, mid1]) = &ty_t1; env }
                    let (ty_t2, t2_o) = synthesize(t2, env)?;
                    try_match! { let BuiltinApply("=", [ty_y, mid2, to]) = &ty_t2; env }
                    type_check_same(ty_x, ty_y, env)?;
                    expr_check_same(mid1, mid2, ty_x, env)?;
                    (
                        bapp!("=", ty_x.clone(), from.clone(), to.clone()),
                        bapp!(bf, t1_o, t2_o),
                    )
                }
                // EqE-5
                ("ind-=", [t, m, b]) => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let BuiltinApply("=", [ty_x, from, to]) = &ty_t; env }
                    let ty_m = pi!(
                        ty_x.clone(),
                        bapp!("=", ty_x.clone(), from.clone(), Identifier(0)),
                        bty::u()
                    );
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    let ty_b = app!(ref m_o, from.clone(), bapp!("same", from.clone()));
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    (
                        app!(ref m_o, to.clone(), t_o.clone()),
                        bapp!(bf, t_o, m_o.as_ref().clone(), b_o),
                    )
                }
                _ => throw!(ErrorKind::CannotInferType {
                    expr: format!("{}", dpp(e, env))
                }),
            }
        }
        _ => throw!(ErrorKind::CannotInferType {
            expr: format!("{}", dpp(e, env))
        }),
    }
}

/// 判断并计算表达式是一个类型或 U(n)，返回其类型层级，相当于为 U(n) 特化的 synthesize。
/// 改进的第四种 Judgement，见 Figure B.1。
#[tc_log(
    "resolve `{}` is a type", dpp(e, env);
    "=> (the (U {}) {})", ret.0, dpp(&ret.1, env)
)]
#[throws]
pub fn resolve_type<M: fmt::Display>(e: &Expr<M>, env: &Env) -> (ULevel, Type<Never>) {
    use Expr::*;
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
                // EitherF
                ("Either", [ty_l, ty_r]) => {
                    let (l_l, ty_l_o) = resolve_type(ty_l, env)?;
                    let (l_r, ty_r_o) = resolve_type(ty_r, env)?;
                    (std::cmp::max(l_l, l_r), bty::either(ty_l_o, ty_r_o))
                }
                // EqF
                ("=", [ty, from, to]) => {
                    let (l, ty_o) = resolve_type(ty, env)?;
                    let from_o = synthesize_with_type(from, &ty_o, env)?;
                    let to_o = synthesize_with_type(to, &ty_o, env)?;
                    (l, bty::equal(ty_o, from_o, to_o))
                }
                // UF
                ("U", [NatLiteral(n)]) => (n + 1, bty::u_l(NatLiteral(*n))),
                ("U", [n]) => {
                    let n_o = synthesize_with_type(n, &bty::nat(), env)?;
                    match n_o {
                        NatLiteral(n) => (n + 1, bty::u_l(n_o)),
                        _ => throw!(ErrorKind::CannotInferType {
                            expr: format!("{}", dpp(e, env))
                        }),
                    }
                }
                _ => unreachable!(),
            }
        }
        // 内建单例对象
        BuiltinId(ty @ ("Atom" | "Nat" | "Trivial" | "Absurd")) => (0, BuiltinId(ty)),
        //Literal, Lambda, Identifier, Apply
        // El
        _ => (0, synthesize_with_type(e, &bty::u(), env)?),
    }
}

// 将 resolve_type 的返回值包装为 (U(n), t_o)
#[inline]
#[throws]
fn resolve_type_rule<M: fmt::Display>(ty: &Expr<M>, env: &Env) -> (Type<Never>, Type<Never>) {
    let (l, t_o) = resolve_type(ty, env)?;
    (bty::u_l(NatLiteral(l)), t_o)
}

/// 检查是否相同类型
/// 第五种 Judgement，见 Figure B.1。
#[inline]
#[throws]
fn type_check_same(ty1: &Type<Never>, ty2: &Type<Never>, env: &Env) {
    if !is_type_check_same(ty1, ty2, env) {
        throw!(ErrorKind::NotSame(
            dpp(ty1, env).to_string(),
            dpp(ty2, env).to_string(),
            "(U _)".to_owned(),
        ));
    }
}

#[tc_log("check `{}` and `{}` are the same type", dpp(ty1, env), dpp(ty2, env);"=> {}", ret)]
fn is_type_check_same(ty1: &Type<Never>, ty2: &Type<Never>, env: &Env) -> bool {
    use Expr::*;
    // TODO: 比较前充分计算 ty1 和 ty2
    match (ty1, ty2) {
        (Identifier(id1), Identifier(id2)) => id1 == id2,
        (BuiltinId(ty1), BuiltinId(ty2)) => ty1 == ty2,
        (BuiltinApply(f1, args1), BuiltinApply(f2, args2)) => {
            match (&**f1, &**args1, &**f2, &**args2) {
                ("List", [ty_e1], "List", [ty_e2]) => is_type_check_same(ty_e1, ty_e2, env),
                ("Vec", [ty_e1, len1], "Vec", [ty_e2, len2]) => {
                    is_type_check_same(ty_e1, ty_e2, env)
                        && is_expr_check_same(len1, len2, &bty::nat(), env)
                }
                ("Either", [ty_l1, ty_r1], "Either", [ty_l2, ty_r2]) => {
                    is_type_check_same(ty_l1, ty_l2, env) && is_type_check_same(ty_r1, ty_r2, env)
                }
                ("=", [ty_x1, from1, to1], "=", [ty_x2, from2, to2]) => {
                    is_type_check_same(ty_x1, ty_x2, env)
                        && is_expr_check_same(from1, from2, ty_x1, env)
                        && is_expr_check_same(to1, to2, ty_x1, env)
                }
                ("U", [n1], "U", [n2]) => is_expr_check_same(n1, n2, &bty::nat(), env),
                _ => todo!(),
            }
        }
        _ => {
            todo!()
        }
    }
}

/// 检查是否相同表达式
/// 认为 `c1: ct` 与 `c2: ct` 已满足
/// 第八种 Judgement，见 Figure B.1。
#[throws]
pub fn expr_check_same(c1: &Expr<Never>, c2: &Expr<Never>, ct: &Type<Never>, env: &Env) {
    if !is_expr_check_same(c1, c2, ct, env) {
        throw!(ErrorKind::NotSame(
            dpp(c1, env).to_string(),
            dpp(c2, env).to_string(),
            dpp(ct, env).to_string(),
        ));
    }
}

#[tc_log(
    "check `{}` and `{}` are the same `{}`", dpp(c1, env), dpp(c2, env), dpp(ct, env);
    "=> {}", ret
)]
fn is_expr_check_same(c1: &Expr<Never>, c2: &Expr<Never>, ct: &Type<Never>, env: &Env) -> bool {
    use Expr::*;
    // TODO: 比较前充分计算 c1、c2、ct
    match (c1, c2) {
        // HypothesisSame
        (Identifier(id1), Identifier(id2)) => id1 == id2,
        (BuiltinApply("U", l1), BuiltinApply("U", l2)) => {
            is_expr_check_same(&l1[0], &l2[0], &bty::nat(), env)
        }
        // 比较自然数，考虑字面量和构造器表示
        (NatLiteral(m), NatLiteral(n)) => m == n,
        // NatSame-zero
        (BuiltinId("zero"), BuiltinId("zero")) => true,
        (BuiltinId("zero"), NatLiteral(0)) | (NatLiteral(0), BuiltinId("zero")) => true,
        (BuiltinId("zero"), BuiltinId(_) | BuiltinApply(_, _)) => false,
        (BuiltinId(_) | BuiltinApply(_, _), BuiltinId("zero")) => false,
        // NatSame-add1
        (BuiltinApply("add1", args), NatLiteral(n))
        | (NatLiteral(n), BuiltinApply("add1", args)) => {
            *n > 0 && is_expr_check_same(&args[0], &NatLiteral(n - 1), ct, env)
        }
        (BuiltinApply("add1", args), BuiltinApply("add1", args2)) => {
            is_expr_check_same(&args[0], &args2[0], ct, env)
        }
        // NatSame-Nat, AtomSame-Atom, ListSame-nil ...
        (BuiltinId(ty1), BuiltinId(ty2)) => ty1 == ty2,
        // AtomSame-tick
        (AtomLiteral(a1), AtomLiteral(a2)) => a1 == a2,
        // ΣSame-Σ
        (SigmaExpr(arg1, ty_a1, ty_d1), SigmaExpr(arg2, ty_a2, ty_d2)) => {
            is_type_check_same(ty_a1, ty_a2, env)
                && is_type_check_same(ty_d1, ty_d2, &env_ext(env, arg1.into(), ty_a1))
        }
        // ΣSame-cons
        (BuiltinApply("cons", args1), BuiltinApply("cons", args2)) => {
            let ([a1, d1], [a2, d2], SigmaExpr(arg, ty_a, ty_d)) = (&**args1, &**args2, ct) else {
                unreachable!()
            };
            is_expr_check_same(a1, a2, ty_a, env)
                && is_expr_check_same(
                    d1,
                    d2,
                    &substitute_arg(ty_d, arg, a1, env),
                    env,
                )
        }
        _ => {
            todo!()
        }
    }
}

pub fn default_environment() -> Env {
    Env::new()
}
