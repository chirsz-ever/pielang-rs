use crate::ast;
use crate::core;
use crate::utils;
use ast::Id;
use ast::is_builtin_name;
use ast::to_builtin_name as bn;
use core::Argument;
use core::DBIPPrint as dpp;
use std::cell::Cell;
use std::cell::RefCell;
use std::fmt;
use utils::LocatedError;
use utils::Ref;
use utils::ToRef;

thread_local! {
    static INDENT: Cell<usize> = const { Cell::new(0) };
    /// 入口字符串栈，None 表示该帧已被 tc_log_end! 消费
    static TC_LOG_ENTRYS: RefCell<Vec<Option<String>>> = const { RefCell::new(Vec::new()) };
}

/// 仿函数宏：在函数体内展开入口日志并创建 IndentGuard。
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

/// 变量名 -> (类型, 表达式)
///
/// 必须是 Option, 因为在检查 lambda 表达式是 Pi 类型时，两边需要同步环境
pub type Env = crate::utils::StackMap<Option<Ref<str>>, (core::Expr, RefCell<Option<core::Expr>>)>;

type Error = LocatedError<ErrorKind>;

macro_rules! throw {
    ($e:expr) => {
        return Err(Error::from($e))
    };
    ($sp:expr, $e:expr) => {
        return Err(Error {
            loc: Some($sp),
            erk: $e,
        })
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

macro_rules! arrow {
    ($ty_a:expr, $ty_r:expr $(,)?) => {
        core::Expr::Pi(Argument::Dummy, ToRef::to_ref($ty_a), Ref::new(shift_dbi(&$ty_r, 1)))
    };
    ($ty_a:expr, $($e:tt)+) => {
        core::Expr::Pi(
            Argument::Dummy,
            ToRef::to_ref($ty_a),
            Ref::new(shift_dbi(&arrow!($($e)+), 1)))
    };
}

/// 仿函数宏：构造 `Π` 类型表达式，替代冗长且易错的
/// `Pi(Argument::Symbol(...), Ref::new(...), Ref::new(...))` 嵌套。
///
/// # 语法
/// 形如 `pi!{ k : T_k, _ : T_inner => body }`：
/// - `name : ty_a` 产生命名绑定（`Argument::Symbol(name)`）；
/// - `_ : ty_a` 产生匿名绑定（`Argument::Dummy`）；
/// - 多层绑定按从外到内先后列出，分隔用 `,`；
/// - `=> body` 是 codomain，按"所有绑定都已进入后的视角"书写，
///   与文件中其它手写 Pi 构造保持同样的语义；宏只在壳上工作，
///   不会对 codomain 做 `shift_dbi`，因此 codomain 中的 dbi 仍然手写。
///
/// # 例
/// ```notest
/// let ty_s = pi! {
///     k : I("Nat"),
///     _ : app!(shift_dbi(&m_o, 1), var!("k", 0))
///     => app!(shift_dbi(&m_o, 2), S("add1", vec![var!("k", 1)]))
/// };
/// ```
macro_rules! pi {
    // 单层命名
    ($n1:ident : $t1:expr => $t_r:expr) => {
        core::Expr::Pi(
            Argument::Symbol(stringify!($n1).into()),
            ToRef::to_ref($t1),
            ToRef::to_ref($t_r),
        )
    };
    // 单层匿名
    (_ : $t1:expr => $t_r:expr) => {
        core::Expr::Pi(
            Argument::Dummy,
            ToRef::to_ref($t1),
            ToRef::to_ref($t_r),
        )
    };
    // 多层：外层为命名
    ($n1:ident : $t1:expr, $($rest:tt)+) => {
        core::Expr::Pi(
            Argument::Symbol(stringify!($n1).into()),
            ToRef::to_ref($t1),
            ToRef::to_ref(pi! { $($rest)+ }),
        )
    };
    // 多层：外层为匿名
    (_ : $t1:expr, $($rest:tt)+) => {
        core::Expr::Pi(
            Argument::Dummy,
            ToRef::to_ref($t1),
            ToRef::to_ref(pi! { $($rest)+ }),
        )
    };
}

macro_rules! var {
    ($name:literal, $dbi:expr) => {
        core::Expr::Identifier($name.into(), $dbi)
    };
}

macro_rules! app {
    ($f:expr, $a:expr $(,)?) => {
        core::Expr::App(ToRef::to_ref($f), ToRef::to_ref($a))
    };
    ($f:expr, $a:expr, $($tt:tt)+) => {
        app!(core::Expr::App(ToRef::to_ref($f), ToRef::to_ref($a)), $($tt)+)
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
    ($e:literal) => {
        core::Expr::S("U", vec![Nat($e)])
    };
    ($e:expr) => {
        core::Expr::S("U", vec![$e])
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
                "{}{}{} --- error",
                "│".repeat(crate::type_check::INDENT.get()),
                "└",
                entry
            );
        }
    }
}

/// depth 表示当前作用域深度
/// TODO: 单元测试
fn shift_dbi_d(e: &core::Expr, inc: isize, depth: usize) -> core::Expr {
    use core::Expr::*;
    if inc == 0 {
        return e.clone();
    }
    match e {
        Identifier(name, idx) => {
            if *idx >= depth {
                Identifier(name.clone(), idx.strict_add_signed(inc))
            } else {
                e.clone()
            }
        }
        I(_) | Atom(_) | Nat(_) => e.clone(),
        S(bf, args) => S(
            bf,
            args.iter()
                .map(|arg| shift_dbi_d(arg, inc, depth))
                .collect(),
        ),
        App(f, arg) => App(
            Ref::new(shift_dbi_d(f, inc, depth)),
            Ref::new(shift_dbi_d(arg, inc, depth)),
        ),
        Lambda(arg, body) => Lambda(arg.clone(), Ref::new(shift_dbi_d(body, inc, depth + 1))),
        Pi(arg, ty_a, ty_r) => Pi(
            arg.clone(),
            Ref::new(shift_dbi_d(ty_a, inc, depth)),
            Ref::new(shift_dbi_d(ty_r, inc, depth + 1)),
        ),
        Sigma(arg, ty_a, ty_d) => Sigma(
            arg.clone(),
            Ref::new(shift_dbi_d(ty_a, inc, depth)),
            Ref::new(shift_dbi_d(ty_d, inc, depth + 1)),
        ),
    }
}

/// 所有自由变量的 dbi 值加上一个数
fn shift_dbi(e: &core::Expr, inc: usize) -> core::Expr {
    shift_dbi_d(e, inc as isize, 0)
}

/// 所有自由变量的 dbi 值加上一个数，可以是负数
fn shift_dbi_signed(e: &core::Expr, inc: isize) -> core::Expr {
    shift_dbi_d(e, inc, 0)
}

/// 执行 beta 变换 expr[e/var]，将 expr 中自由出现的 var 替换为 e，depth 表示当前作用域深度。
/// TODO: 也许可以统一 var 和 depth 参数?
/// TODO: 单元测试
fn substitute(expr: &core::Expr, var: usize, e: &core::Expr, depth: usize) -> core::Expr {
    use core::Expr::*;

    match expr {
        Nat(_) | Atom(_) | I(_) => expr.clone(),
        Identifier(i, idx) => {
            if *idx == var {
                shift_dbi(e, depth)
            } else if *idx > var {
                Identifier(i.clone(), idx - 1)
            } else {
                expr.clone()
            }
        }
        S(bid, args) => {
            let args_o = args
                .iter()
                .map(|arg| substitute(arg, var, e, depth))
                .collect();
            S(bid, args_o)
        }
        App(f, a) => {
            let f_o = substitute(f, var, e, depth);
            let a_o = substitute(a, var, e, depth);
            App(Ref::new(f_o), Ref::new(a_o))
        }
        Pi(a, ty_a, ty_r) => {
            let ty_a_o = substitute(ty_a, var, e, depth);
            let ty_r_o = substitute(ty_r, var + 1, e, depth + 1);
            Pi(a.clone(), Ref::new(ty_a_o), Ref::new(ty_r_o))
        }
        Sigma(a, ty_a, ty_d) => {
            let ty_a_o = substitute(ty_a, var, e, depth);
            let ty_d_o = substitute(ty_d, var + 1, e, depth + 1);
            Sigma(a.clone(), Ref::new(ty_a_o), Ref::new(ty_d_o))
        }
        Lambda(a, body) => {
            let body_o = substitute(body, var + 1, e, depth + 1);
            Lambda(a.clone(), Ref::new(body_o))
        }
    }
}

/// 对常用的 Argument 下 beta 变换简写
/// TODO: 单元测试
/// TODO: 无需替换时的优化
#[inline]
fn substitute_beta_arg(body: &core::Expr, arg: &Argument, e: &core::Expr, env: &Env) -> core::Expr {
    tc_log!(
        "substitute_beta_arg: substitute {} with {} in {}",
        arg,
        dpp(e, env),
        dpp(body, env)
    );

    // 即使 arg 在 body 中不出现，也要执行自由变量的 shift 操作
    let ret = substitute(body, 0, e, 0);

    tc_log_end!("=> {}", dpp(&ret, env));

    ret
}

#[inline]
fn env_ext(env: &Env, name: &Ref<str>, ty: &core::Expr) -> Env {
    env.insert(name.clone().into(), (ty.clone(), Default::default()))
}

fn env_ext_arg(env: &Env, arg: &Argument, ty: &core::Expr) -> Env {
    env.insert(arg.into(), (ty.clone(), Default::default()))
}

fn env_ext_dummy(env: &Env, ty: &core::Expr) -> Env {
    env.insert(None, (ty.clone(), Default::default()))
}

fn env_ext_arg_notype(env: &Env, arg: &Argument) -> Env {
    env_ext_arg(env, arg, &Default::default())
}

/// 先综合出 e 的类型，再检查其是否与 ty 相同
#[inline]
fn switch_rule(e: &ast::Expr, ty: &core::Expr, env: &Env) -> Result<core::Expr, Error> {
    let (ty_e_o, e_o) = synthesize(e, env)?;
    // attach location information and convert to ErrorKind::TypeNotMatch
    type_check_same(&ty_e_o, ty, env).map_err(|err| {
        if err.loc.is_none() {
            LocatedError {
                loc: Some(e.span()),
                erk: ErrorKind::TypeNotMatch {
                    expected: dpp(ty, env).to_string(),
                    given: dpp(&ty_e_o, env).to_string(),
                },
            }
        } else {
            err
        }
    })?;
    Ok(e_o)
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
/// 对于构造式，有唯一相关的类型与之匹配；
/// 其它表达式则应用 Switch 规则：试图综合得出其类型，再将结果与所给类型比较。
pub fn synthesize_with_type(
    e: &ast::Expr,
    ty: &core::Expr,
    env: &Env,
) -> Result<core::Expr, Error> {
    tc_log!("check `{}` is a `{}`", e, dpp(ty, env));

    use ast::Expr::*;
    use ast::Id;
    use core::Expr::*;

    // ch10: check vecnil is a (mot ..)
    let ty = &normalize(ty, env);

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
        (LambdaExpr(sp, args, r), Pi(_pi_arg, ty_arg, ty_ret))
            if let [Id(_, arg), rargs @ ..] = &args[..] =>
        {
            let arg = (*arg).into();
            if rargs.is_empty() {
                // FunI-1
                let r_o = synthesize_with_type(r, ty_ret, &env_ext(env, &arg, ty_arg))?;
                Lambda(arg.into(), r_o.into())
            } else {
                // FunI-2
                // FIXME: right span
                let r_o = synthesize_with_type(
                    &LambdaExpr(*sp, rargs.to_vec(), r.clone()),
                    ty_ret,
                    &env_ext(env, &arg, ty_arg),
                )?;
                Lambda(arg.into(), r_o.into())
            }
        }
        // ΣI
        (AppExpr(_, args), Sigma(arg, ty_a, ty_d)) if let [Ident(_, "cons"), a, d] = &args[..] => {
            let a_o = synthesize_with_type(a, ty_a, env)?;
            let d_o = synthesize_with_type(d, &substitute_beta_arg(ty_d, arg, &a_o, env), env)?;
            S("cons", vec![a_o, d_o])
        }
        // ListI-1
        (Ident(_, "nil"), S("List", _ty_args)) => I("nil"),
        // VecI-1
        (Ident(sp, "vecnil"), S("Vec", ty_args)) if let [_ty_e, l] = &ty_args[..] => {
            if let Nat(0) = l {
                I("vecnil")
            } else {
                throw!(
                    *sp,
                    ErrorKind::TypeNotMatch {
                        expected: format!("{}", dpp(ty, env)),
                        given: "vecnil".to_string(),
                    }
                )
            }
        }
        (AppExpr(_, args), S(ty_bf, ty_args)) => {
            match (args.as_slice(), &**ty_bf, &**ty_args) {
                // ListI-3，TLT 中不存在，我自己加的，使 (the (List (-> Nat Nat)) (:: (lambda (x) x) nil)) 这样的
                // 表达式能推导出类型。
                ([Ident(_, "::"), e, es], "List", [ty_1]) => {
                    let e_o = synthesize_with_type(e, ty_1, env)?;
                    let es_o = synthesize_with_type(es, ty, env)?;
                    S("::", vec![e_o, es_o])
                }
                // VecI-2
                ([Ident(_, "vec::"), e, es], "Vec", [ty_e, len]) if is_add1(len) => {
                    let e_o = synthesize_with_type(e, ty_e, env)?;
                    let sublen = sub1(len);
                    let ty_subvec = S(ty_bf, vec![ty_e.clone(), sublen]);
                    let es_o = synthesize_with_type(es, &ty_subvec, env)?;
                    S("vec::", vec![e_o, es_o])
                }
                // EitherI-1
                ([Ident(_, "left"), lt], "Either", [ty_l, _ty_r]) => {
                    let lt_o = synthesize_with_type(lt, ty_l, env)?;
                    S("left", vec![lt_o])
                }
                // EitherI-2
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

fn ident_occur_in(n: usize, e: &core::Expr) -> bool {
    use core::Expr::*;
    match e {
        Identifier(_, m) => *m == n,
        I(_) | Atom(_) | Nat(_) => false,
        S(_, args) => args.iter().any(|arg| ident_occur_in(n, arg)),
        App(f, arg) => ident_occur_in(n, f) || ident_occur_in(n, arg),
        Lambda(_, body) => ident_occur_in(n + 1, body),
        Pi(_, ty_a, ty_r) => ident_occur_in(n, ty_a) || ident_occur_in(n + 1, ty_r),
        Sigma(_, ty_a, ty_d) => ident_occur_in(n, ty_a) || ident_occur_in(n + 1, ty_d),
    }
}

fn is_add1(e: &core::Expr) -> bool {
    use core::Expr::*;
    match e {
        Nat(0) => false,
        Nat(_) | S("add1", _) => true,
        _ => false,
    }
}

fn sub1(e: &core::Expr) -> core::Expr {
    use core::Expr::*;
    match e {
        Nat(n) => {
            debug_assert_ne!(*n, 0);
            Nat(n - 1)
        }
        S("add1", args) if let [n] = &**args => n.clone(),
        _ => unreachable!(),
    }
}

fn add1(e: &core::Expr) -> core::Expr {
    use core::Expr::*;
    match e {
        Nat(n) => Nat(n + 1),
        _ => S("add1", vec![e.clone()]),
    }
}

// fn ppenv(env: &Env) -> String {
//     let mut s = String::new();
//     s.push_str("[");
//     for (i, (name, (_ty, _))) in env.iter().enumerate() {
//         let name_str = name.as_deref().unwrap_or("_");
//         s.push_str(&format!(
//             "{}",
//             name_str,
//         ));
//         if i + 1 < env.iter().count() {
//             s.push_str(" ");
//         }
//     }
//     s.push_str("]");
//     s
// }

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
pub fn synthesize(e: &ast::Expr, env: &Env) -> Result<(core::Expr, core::Expr), Error> {
    tc_log!("synthesize `{}`", e);

    use ast::Expr::*;
    use core::Expr::*;

    let mut ret = match e {
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
        // UF
        Ident(_, "U") => (U!(1), U!(0)),
        // nil 和 vecnil 必须附加类型
        Ident(sp, "nil" | "vecnil") => throw!(
            *sp,
            ErrorKind::CannotInferType {
                expr: format!("{}", e)
            }
        ),
        // Hypothesis
        Ident(_, id) => 'x: {
            for (i, (name, (ty, _))) in env.iter().enumerate() {
                if name.as_deref().is_some_and(|n| *n == **id) {
                    // convert to de Bruijn index
                    break 'x (shift_dbi(ty, i + 1), Identifier((*id).into(), i));
                }
            }
            unreachable!("Identifier {} not found in env", id)
        }
        // lambda 表达式无法直接综合出类型，必须通过 Pi 类型检查
        LambdaExpr(sp, _args, _body) => {
            throw!(
                *sp,
                ErrorKind::CannotInferType {
                    expr: e.to_string()
                }
            )
        }
        // FunF-1, FunF-2
        PiExpr(sp, args, body) => {
            match args.as_slice() {
                // FunF-1
                [(Id(_, id), ty_a)] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let id = (*id).into();
                    let (l_r, ty_r_o) = resolve_type(body, &env_ext(env, &id, &ty_a_o))?;
                    let arg_o = if ident_occur_in(0, &ty_r_o) {
                        Argument::Symbol(id)
                    } else {
                        Argument::Dummy
                    };
                    (
                        U!(Nat(std::cmp::max(l_a, l_r))),
                        Pi(arg_o, ty_a_o.into(), ty_r_o.into()),
                    )
                }
                // FunF-2
                [(Id(_, id), ty_a), rargs @ ..] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let id = (*id).into();
                    let (l_r, ty_r_o) = resolve_type(
                        &PiExpr(*sp, rargs.to_vec(), body.clone()),
                        &env_ext(env, &id, &ty_a_o),
                    )?;
                    let arg_o = if ident_occur_in(0, &ty_r_o) {
                        Argument::Symbol(id)
                    } else {
                        Argument::Dummy
                    };
                    (
                        U!(Nat(std::cmp::max(l_a, l_r))),
                        Pi(arg_o, ty_a_o.into(), ty_r_o.into()),
                    )
                }
                _ => unreachable!(),
            }
        }
        // FunF->1, FunF->2
        ArrowExpr(sp, args) => {
            match args.as_slice() {
                // FunF->1, (→ A R) -> (Π ((_ A)) R)
                [ty_a, ty_r] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let (l_r, ty_r_o) = resolve_type(ty_r, &env_ext_dummy(env, &ty_a_o))?;
                    (
                        U!(Nat(std::cmp::max(l_a, l_r))),
                        Pi(Argument::Dummy, Ref::new(ty_a_o), Ref::new(ty_r_o)),
                    )
                }
                // FunF->2, (→ A B ... R) -> (Π ((_ A)) (→ B ... R))
                [ty_a, rargs @ ..] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    // FIXME: right span
                    let (l_r, ty_r_o) = resolve_type(
                        &ArrowExpr(*sp, rargs.to_vec()),
                        &env_ext_dummy(env, &ty_a_o),
                    )?;
                    (
                        U!(Nat(std::cmp::max(l_a, l_r))),
                        Pi(Argument::Dummy, Ref::new(ty_a_o), Ref::new(ty_r_o)),
                    )
                }
                _ => unreachable!(),
            }
        }
        // ΣF-1, ΣF-2
        SigmaExpr(sp, args, body) => {
            match args.as_slice() {
                // ΣF-1
                [(Id(_, id), ty_a)] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let id = (*id).into();
                    let (l_d, ty_d_o) = resolve_type(body, &env_ext(env, &id, &ty_a_o))?;
                    let arg_o = if ident_occur_in(0, &ty_d_o) {
                        Argument::Symbol(id)
                    } else {
                        Argument::Dummy
                    };
                    (
                        U!(Nat(std::cmp::max(l_a, l_d))),
                        Sigma(arg_o, ty_a_o.into(), ty_d_o.into()),
                    )
                }
                // ΣF-2, (Σ ((a A)(b B)...) D) -> (Σ ((a A)) (Σ ((b B)...) D))
                [(Id(_, id), ty_a), rargs @ ..] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    let id = (*id).into();
                    let (l_d, ty_d_o) = resolve_type(
                        &SigmaExpr(*sp, rargs.to_vec(), body.clone()),
                        &env_ext(env, &id, &ty_a_o),
                    )?;
                    let arg_o = if ident_occur_in(0, &ty_d_o) {
                        Argument::Symbol(id)
                    } else {
                        Argument::Dummy
                    };
                    (
                        U!(Nat(std::cmp::max(l_a, l_d))),
                        Sigma(arg_o, ty_a_o.into(), ty_d_o.into()),
                    )
                }
                _ => unreachable!(),
            }
        }
        AppExpr(_, exprs) => {
            match exprs.as_slice() {
                // (U n): (U (add1 n))
                [Ident(_, "U"), NatLit(_, n)] => (U!(Nat(*n + 1)), U!(Nat(*n))),
                [Ident(_, "U"), n] => {
                    let n_o = synthesize_with_type(n, &I("Nat"), env)?;
                    (U!(add1(&n_o)), U!(n_o))
                }
                // nil 和 vecnil 必须附加类型
                [Ident(sp, s)] => throw!(
                    *sp,
                    ErrorKind::CannotInferType {
                        expr: s.to_string()
                    }
                ),
                // "The" 规则
                [Ident(_, "the"), ty, expr] => {
                    let (_, ty_o) = resolve_type(ty, env)?;
                    let expr_o = synthesize_with_type(expr, &ty_o, env)?;
                    (ty_o, expr_o)
                }
                // ListF
                [Ident(_, "List"), ty_e] => {
                    let (l, ty_e_o) = resolve_type(ty_e, env)?;
                    (U!(Nat(l)), bapp!("List", ty_e_o))
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
                    let n_o = synthesize_with_type(n, &I("Nat"), env)?;
                    (I("Nat"), S("add1", vec![n_o]))
                }
                // VecE-1
                [Ident(sp, "head"), v] => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let S("Vec", [ty_e, len]) = &ty_v; env };
                    if is_add1(len) {
                        (ty_e.clone(), S("head", vec![v_o]))
                    } else {
                        throw!(
                            *sp,
                            ErrorKind::TypeNotMatch {
                                expected: "Vec longer than 1".to_owned(),
                                given: format!("{}", v),
                            }
                        )
                    }
                }
                // VecF
                [Ident(_, "Vec"), ty, len] => {
                    let (l, ty_o) = resolve_type(ty, env)?;
                    let len_o = synthesize_with_type(len, &I("Nat"), env)?;
                    (U!(Nat(l)), bapp!("Vec", ty_o, len_o))
                }
                // VecE-2
                [Ident(sp, "tail"), v] => {
                    let (ty_v, v_o) = synthesize(v, env)?;
                    try_match! { let S("Vec", [ty_e, len]) = &ty_v; env };
                    if is_add1(len) {
                        let ty_subv = bapp!("Vec", ty_e.clone(), sub1(len));
                        (ty_subv, S("tail", vec![v_o]))
                    } else {
                        throw!(
                            *sp,
                            ErrorKind::TypeNotMatch {
                                expected: "Vec longer than 1".to_owned(),
                                given: format!("{}", v),
                            }
                        )
                    }
                }
                // ΣF-Pair
                [Ident(_, "Pair"), ty_a, ty_d] => {
                    let (l_a, ty_a_o) = resolve_type(ty_a, env)?;
                    // (Pair A D) -> (Σ ((_ : A)) D), introduced a dummy argument
                    let (l_d, ty_d_o) = resolve_type(ty_d, &env_ext_dummy(env, &ty_a_o))?;
                    (
                        U!(Nat(std::cmp::max(l_a, l_d))),
                        Sigma(Argument::Dummy, Ref::new(ty_a_o), Ref::new(ty_d_o)),
                    )
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
                    try_match! { let Sigma(x, _ty_a, ty_d) = &ty_pr; env };
                    let car_pr = bapp!("car", pr_o.clone());
                    let ty_d_o = substitute_beta_arg(ty_d, x, &car_pr, env);
                    (ty_d_o, S("cdr", vec![pr_o]))
                }
                // NatE-1
                [Ident(_, "which-Nat"), t, b, s] => {
                    let t_o = synthesize_with_type(t, &I("Nat"), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_s = arrow!(I("Nat"), ty_b.clone());
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b, S("which-Nat", vec![t_o, b_o, s_o]))
                }
                // NatE-2
                [Ident(_, "iter-Nat"), t, b, s] => {
                    let t_o = synthesize_with_type(t, &I("Nat"), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = arrow!(&ty_b, &ty_b);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), S("iter-Nat", vec![t_o, b_o, s_o]))
                }
                // NatE-3
                [Ident(_, "rec-Nat"), t, b, s] => {
                    let t_o = synthesize_with_type(t, &I("Nat"), env)?;
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = arrow!(I("Nat"), &ty_b, &ty_b);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    // FIXME: TLT 中需要多一层 the 表达式
                    (ty_b.as_ref().clone(), S("rec-Nat", vec![t_o, b_o, s_o]))
                }
                // NatE-4
                [Ident(_, "ind-Nat"), t, m, b, s] => {
                    let t_o = synthesize_with_type(t, &I("Nat"), env)?;
                    // m : Nat -> U
                    let ty_m = arrow!(I("Nat"), U!());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    let ty_b = &app!(&m_o, Nat(0));
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    // s : (k : Nat) -> (m k) -> (m (add1 k))
                    let ty_s = pi! {
                        k : I("Nat"),
                        _ : app!(shift_dbi(&m_o, 1), var!("k", 0))
                        => app!(shift_dbi(&m_o, 2), bapp!("add1", var!("k", 1)))
                    };
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    let ty_o = app!(&m_o, t_o.clone());
                    (ty_o, bapp!("ind-Nat", t_o, m_o.as_ref().clone(), b_o, s_o))
                }
                // ListE-1
                // (rec-List (the (List E) t) b s)
                // TODO: 搞清该怎么改
                [Ident(_, "rec-List"), t, b, s] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("List", [ty_e]) = &ty_t; env }
                    let (ty_b, b_o) = synthesize(b, env)?;
                    let ty_b = Ref::new(ty_b);
                    let ty_s = arrow!(ty_e, &ty_t, &ty_b, &ty_b,);
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    let t_o = S("the", vec![ty_t, t_o]);
                    (ty_b.as_ref().clone(), S("rec-List", vec![t_o, b_o, s_o]))
                }
                // ListE-2
                [Ident(_, "ind-List"), t, m, b, s] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("List", [ty_e]) = &ty_t; env }
                    // m : (List E) -> U
                    let ty_m = arrow!(ty_t.clone(), U!());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    let ty_b = app!(&m_o, I("nil"));
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    // s : (x : E) -> (xs : List E) -> (m xs) -> (m (:: x xs))
                    let ty_s = pi! {
                        x : ty_e,
                        xs : shift_dbi(&ty_t, 1),
                        _ : app!(shift_dbi(&m_o, 2), var!("xs", 0))
                        => app!(
                            shift_dbi(&m_o, 3),
                            bapp!("::", var!("x", 2), var!("xs", 1))
                        )
                    };
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    (
                        app!(&m_o, t_o.clone()),
                        S("ind-List", vec![t_o, m_o.as_ref().clone(), b_o, s_o]),
                    )
                }
                // VecE-3
                [Ident(_, "ind-Vec"), l, t, m, b, s] => {
                    let l_o = synthesize_with_type(l, &I("Nat"), env)?;
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("Vec", [ty_e, n]) = &ty_t; env }
                    expr_check_same(&l_o, n, &I("Nat"), env)?;
                    // m : (k : Nat) -> (Vec E k) -> U
                    let ty_m = pi! {
                        k : I("Nat"),
                        _ : bapp!("Vec", shift_dbi(ty_e, 1), var!("k", 0))
                        => U!()
                    };
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    let ty_b = app!(&m_o, Nat(0), I("vecnil"));
                    let b_o = synthesize_with_type(b, &ty_b, env)?;
                    // s : (k : Nat) -> (e : E) -> (es : (Vec E k)) -> (m k es) -> (m (add1 k) (vec:: e es))
                    let ty_s = pi! {
                        k : I("Nat"),
                        e : shift_dbi(ty_e, 1),
                        es : bapp!("Vec", shift_dbi(ty_e, 2), var!("k", 1)),
                        _ : app!(shift_dbi(&m_o, 3),var!("k", 2),var!("es", 0))
                        => app!(shift_dbi(&m_o, 4), S("add1", vec![var!("k", 3)]), bapp!("vec::",var!("e", 2),var!("es", 1)))
                    };
                    let s_o = synthesize_with_type(s, &ty_s, env)?;
                    (
                        app!(&m_o, l_o.clone(), t_o.clone()),
                        S("ind-Vec", vec![l_o, t_o, m_o.as_ref().clone(), b_o, s_o]),
                    )
                }
                // EitherF
                [Ident(_, "Either"), ty_l, ty_r] => {
                    let (l_l, ty_l_o) = resolve_type(ty_l, env)?;
                    let (l_r, ty_r_o) = resolve_type(ty_r, env)?;
                    (
                        U!(Nat(std::cmp::max(l_l, l_r))),
                        bapp!("Either", ty_l_o, ty_r_o),
                    )
                }
                // EitherE
                [Ident(_, "ind-Either"), t, m, bl, br] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    // m : (Either P S) -> U
                    try_match! { let S("Either", [ty_p, ty_s]) = &ty_t; env }
                    let ty_m = arrow!(ty_t.clone(), U!());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    // b_l : (x : P) -> (m (left x))
                    let ty_bl = pi! {
                        x : ty_p.clone()
                        => app!(shift_dbi(&m_o, 1), bapp!("left", var!("x", 0)))
                    };
                    let bl_o = synthesize_with_type(bl, &ty_bl, env)?;
                    // b_r : (x : S) -> (m (right x))
                    let ty_br = pi! {
                        x : ty_s.clone()
                        => app!(
                            shift_dbi(&m_o, 1),
                            bapp!("right", var!("x", 0))
                        )
                    };
                    let br_o = synthesize_with_type(br, &ty_br, env)?;
                    (
                        app!(m_o.clone(), t_o.clone()),
                        S("ind-Either", vec![t_o, m_o, bl_o, br_o]),
                    )
                }
                // AbsE
                [Ident(_, "ind-Absurd"), t, m] => {
                    let t_o = synthesize_with_type(t, &I("Absurd"), env)?;
                    let (_lm, m_o) = resolve_type(m, env)?;
                    (m_o.clone(), S("ind-Absurd", vec![t_o, m_o]))
                }
                // EqF
                [Ident(_, "="), ty, from, to] => {
                    let (l, ty_o) = resolve_type(ty, env)?;
                    let from_o = synthesize_with_type(from, &ty_o, env)?;
                    let to_o = synthesize_with_type(to, &ty_o, env)?;
                    (U!(Nat(l)), bapp!("=", ty_o, from_o, to_o))
                }
                // EqE-1
                [Ident(_, "replace"), t, m, b] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    // t : (= X from to)
                    try_match! { let S("=", [ty_x, from, to]) = &ty_t; env }
                    // m : X -> U
                    let ty_m = arrow!(ty_x.clone(), U!());
                    let m_o = synthesize_with_type(m, &ty_m, env)?;
                    let m_o = Ref::new(m_o);
                    // b : m from
                    let b_o = synthesize_with_type(b, &app!(&m_o, from.clone()), env)?;
                    (
                        app!(&m_o, to.clone()),
                        bapp!("replace", t_o, m_o.as_ref().clone(), b_o),
                    )
                }
                // EqE-2
                [Ident(_, "cong"), t, f] => {
                    let (ty_t, t_o) = synthesize(t, env)?;
                    try_match! { let S("=", [ty_x1, from, to]) = &ty_t; env }
                    let (ty_f, f_o) = synthesize(f, env)?;
                    try_match! { let Pi(_arg, ty_x2, ty_y) = &ty_f; env }
                    if ident_occur_in(0, ty_y) {
                        throw!(
                            f.span(),
                            ErrorKind::TypeNotMatch {
                                expected: "non-dependent-type function".into(),
                                given: f.to_string()
                            }
                        )
                    }
                    type_check_same(ty_x1, ty_x2, env)?;
                    let f_o = Ref::new(f_o);
                    let ty = bapp!(
                        "=",
                        shift_dbi_signed(ty_y, -1),
                        app!(&f_o, from.clone()),
                        app!(&f_o, to.clone())
                    );
                    // CHECK: TLT 中需要多一个参数?
                    (ty, bapp!("cong", ty_x1.clone(), t_o, f_o.as_ref().clone()))
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
                //     let ty_b = app!(m_o, from.clone(), bapp!("same", from.clone()));
                //     let b_o = synthesize_with_type(b, &ty_b, env)?;
                //     (
                //         app!(m_o, to.clone(), t_o.clone()),
                //         bapp!("ind-=", t_o, m_o.as_ref().clone(), b_o),
                //     )
                // }
                // FunE-1, FunE-2
                [f, args @ ..] => {
                    assert!(!args.is_empty());
                    assert!(
                        !matches!(f, Ident(_, id) if is_builtin_name(id)),
                        "function application should not be builtin name {f}"
                    );
                    let (ty_f, f_o, arg);
                    if let [arg_0] = args {
                        // FunE-1
                        arg = arg_0;
                        (ty_f, f_o) = synthesize(f, env)?;
                    } else {
                        // FunE-2
                        no_else!( let [ args_n_1 @ .., arg_n ] = args );
                        let mut sub_exprs = vec![f.clone()];
                        sub_exprs.extend_from_slice(args_n_1);
                        // (f a b c) -> ((f a b) c)
                        let sub_app = AppExpr(e.span(), sub_exprs);
                        arg = arg_n;
                        (ty_f, f_o) = synthesize(&sub_app, env)?;
                    }
                    try_match! { let Pi(arg_f, ty_a, ty_r) = &ty_f; env };
                    let arg_o = synthesize_with_type(arg, ty_a, env)?;
                    let ty_r_o = substitute_beta_arg(ty_r, arg_f, &arg_o, env);
                    (ty_r_o, app!(f_o, arg_o))
                }
                _ => unreachable!("synthesize: unexpected application: {}", e),
            }
        }
    };

    ret.0 = normalize(&ret.0, env);

    if matches!(ret.0, I("Trivial")) {
        ret.1 = I("sole");
    }

    tc_log_end!("=> (the {} {})", dpp(&ret.0, env), dpp(&ret.1, env));
    Ok(ret)
}

/// 表达式正规化，基本就是不断计算，直到不能再计算为止。
/// TODO: 优化性能
pub fn normalize(e: &core::Expr, env: &Env) -> core::Expr {
    let mut e_o = e.clone();
    while let Some(next) = normalize_once(&e_o, env) {
        e_o = next;
    }
    e_o
}

/// 正规化子表达式，返回 (是否变化, 正规化结果)。
/// 利用 Deref coercion，$e 可以是 &Expr 或 &Rc<Expr>。
macro_rules! norm {
    ($e:expr, $env:expr) => {{
        let __e: &core::Expr = $e;
        match normalize_once(__e, $env) {
            Some(v) => (true, v),
            None => (false, __e.clone()),
        }
    }};
}

/// 如果条件为 true，返回 Some($e)，否则 None。
macro_rules! some_if {
    ($cond:expr => $e:expr) => {
        if $cond { Some($e) } else { None }
    };
}

/// 计算一次；如果无法进一步计算则返回 None。
fn normalize_once(e: &core::Expr, env: &Env) -> Option<core::Expr> {
    use core::Expr::*;
    match e {
        I(_) | Atom(_) | Nat(_) => None,
        // Hypothesis
        Identifier(_, idx) => {
            let (_, (_, def)) = env.get_index(*idx).expect("Identifier index out of bounds");
            let d = def.borrow();
            d.as_ref().map(|d| shift_dbi(d, *idx + 1))
        }
        // FunSame-β, ((λ (x) body) arg) -> body[x := arg]
        App(f, arg) => {
            let (c1, f_o) = norm!(f, env);
            let (c2, arg_o) = norm!(arg, env);
            if let Lambda(arg_f, body) = &f_o {
                Some(substitute_beta_arg(body, arg_f, &arg_o, env))
            } else {
                some_if!(c1 || c2 => App(Ref::new(f_o), Ref::new(arg_o)))
            }
        }
        // FunSame-η, (λ (x) (f x)) -> f
        Lambda(arg, body) => {
            let (c, body_o) = norm!(body, &env_ext_arg_notype(env, arg));
            if let Argument::Symbol(a) = arg {
                if let App(f, arg_f) = &body_o
                    && let Identifier(_, 0) = &**arg_f
                    && ident_occur_in(0, f)
                {
                    Some((**f).clone())
                } else {
                    some_if!(c => Lambda(Argument::Symbol(a.clone()), Ref::new(body_o)))
                }
            } else {
                some_if!(c => Lambda(arg.clone(), Ref::new(body_o)))
            }
        }
        Pi(arg, ty_a, ty_r) => {
            let (c1, ty_a_o) = norm!(ty_a, env);
            let (c2, ty_r_o) = norm!(ty_r, &env_ext_arg_notype(env, arg));
            some_if!(c1 || c2 => Pi(arg.clone(), ty_a_o.into(), ty_r_o.into()))
        }
        Sigma(arg, ty_a, ty_d) => {
            let (c1, ty_a_o) = norm!(ty_a, env);
            let (c2, ty_d_o) = norm!(ty_d, &env_ext_arg_notype(env, arg));
            some_if!(c1 || c2 => Sigma(arg.clone(), ty_a_o.into(), ty_d_o.into()))
        }
        // NatI-4?
        S(bf, args) => match (*bf, args.as_slice()) {
            ("add1", [n]) => {
                let (c, n_o) = norm!(n, env);
                match n_o {
                    Nat(v) => Some(Nat(v + 1)),
                    n_o => some_if!(c => S("add1", vec![n_o])),
                }
            }
            // ΣSame-ι1, (car (cons a d)) -> a
            ("car", [p]) => {
                let (c, p_o) = norm!(p, env);
                if let S("cons", cons_args) = &p_o
                    && let [a, _d] = &cons_args[..]
                {
                    Some(a.clone())
                } else {
                    some_if!(c => S("car", vec![p_o]))
                }
            }
            // ΣSame-ι2, (cdr (cons a d)) -> d
            ("cdr", [p]) => {
                let (c, p_o) = norm!(p, env);
                if let S("cons", cons_args) = &p_o
                    && let [_a, d] = &cons_args[..]
                {
                    Some(d.clone())
                } else {
                    some_if!(c => S("cdr", vec![p_o]))
                }
            }
            // ΣSame-η, (cons (car p) (cdr p)) -> p
            // FIXME: expr_check_same 不需要 ct 参数?
            ("cons", [a, d]) => {
                let (c1, a_o) = norm!(a, env);
                let (c2, d_o) = norm!(d, env);
                if let S("car", p1) = &a_o
                    && let S("cdr", p2) = &d_o
                    && expr_check_same(&p1[0], &p2[0], &I("ignore"), env).is_ok()
                {
                    Some(p1[0].clone())
                } else {
                    some_if!(c1 || c2 => S("cons", vec![a_o, d_o]))
                }
            }
            // NatSame-w-Nι1, NatSame-w-Nι2
            ("which-Nat", [t, b, s]) => {
                let (c1, t_o) = norm!(t, env);
                let (c2, b_o) = norm!(b, env);
                let (c3, s_o) = norm!(s, env);
                if matches!(&t_o, Nat(0)) {
                    Some(b_o)
                } else if is_add1(&t_o) {
                    let n = sub1(&t_o);
                    Some(App(s_o.into(), n.into()))
                } else {
                    some_if!(c1 || c2 || c3 => S("which-Nat", vec![t_o, b_o, s_o]))
                }
            }
            // NatSame-it-Nι1, NatSame-it-Nι2
            ("iter-Nat", [t, b, s]) => {
                let (c1, t_o) = norm!(t, env);
                let (c2, b_o) = norm!(b, env);
                let (c3, s_o) = norm!(s, env);
                if matches!(&t_o, Nat(0)) {
                    Some(b_o)
                } else if is_add1(&t_o) {
                    let n_sub1 = sub1(&t_o);
                    let iter_sub1 = S("iter-Nat", vec![n_sub1, b_o.clone(), s_o.clone()]);
                    Some(App(s_o.into(), iter_sub1.into()))
                } else {
                    some_if!(c1 || c2 || c3 => S("iter-Nat", vec![t_o, b_o, s_o]))
                }
            }
            // NatSame-r-Nι1, NatSame-r-Nι2
            ("rec-Nat", [t, b, s]) => {
                let (c1, t_o) = norm!(t, env);
                let (c2, b_o) = norm!(b, env);
                let (c3, s_o) = norm!(s, env);
                if matches!(&t_o, Nat(0)) {
                    Some(b_o)
                } else if is_add1(&t_o) {
                    let n_sub1 = sub1(&t_o);
                    let rec_sub1 = S("rec-Nat", vec![n_sub1.clone(), b_o.clone(), s_o.clone()]);
                    Some(app!(s_o, n_sub1, rec_sub1))
                } else {
                    some_if!(c1 || c2 || c3 => S("rec-Nat", vec![t_o, b_o, s_o]))
                }
            }
            // ListSame-r-Lι1, ListSame-r-Lι2
            ("rec-List", [t, b, s]) => {
                no_else!( let (c1, S("the", t_args)) = norm!(t, env) );
                no_else!( let [ty_t, t_o] = &t_args[..] );
                let (c2, b_o) = norm!(b, env);
                let (c3, s_o) = norm!(s, env);
                if let I("nil") = &t_o {
                    Some(b_o)
                } else if let S("::", args) = &t_o
                    && let [e, es] = &args[..]
                {
                    let rec_es = S(
                        "rec-List",
                        vec![S("the", vec![ty_t.clone(), es.clone()]), b_o, s_o.clone()],
                    );
                    Some(app!(s_o, e.clone(), es.clone(), rec_es))
                } else {
                    some_if!(c1 || c2 || c3 => S("rec-List", vec![S("the", t_args), b_o, s_o]))
                }
            }
            // ListSame-i-Lι1, ListSame-i-Lι2
            ("ind-List", [t, m, b, s]) => {
                let (c1, t) = norm!(t, env);
                let (c2, m) = norm!(m, env);
                let (c3, b) = norm!(b, env);
                let (c4, s) = norm!(s, env);
                if let I("nil") = t {
                    Some(b)
                } else if let S("::", ref t_args) = t
                    && let [e, es] = &t_args[..]
                {
                    Some(app!(
                        s.clone(),
                        e.clone(),
                        es.clone(),
                        S("ind-List", vec![es.clone(), m, b, s])
                    ))
                } else {
                    some_if!( c1 || c2 || c3 || c4 => S("ind-List", vec![t, m, b, s]))
                }
            }
            // NatSame-in-Nι1, NatSame-in-Nι2
            ("ind-Nat", [t, m, b, s]) => {
                let (c1, t_o) = norm!(t, env);
                let (c2, m_o) = norm!(m, env);
                let (c3, b_o) = norm!(b, env);
                let (c4, s_o) = norm!(s, env);
                match &t_o {
                    Nat(0) => Some(b_o),
                    Nat(_) | S("add1", _) => {
                        let n_sub1 = sub1(&t_o);
                        let ind_sub1 = S(
                            "ind-Nat",
                            vec![n_sub1.clone(), m_o.clone(), b_o.clone(), s_o.clone()],
                        );
                        Some(app!(s_o, n_sub1, ind_sub1))
                    }
                    _ => some_if!(c1 || c2 || c3 || c4 => S("ind-Nat", vec![t_o, m_o, b_o, s_o])),
                }
            }
            // VecSame-hι
            ("head", [v]) => {
                let (c, v_o) = norm!(v, env);
                if let S("vec::", vec_args) = &v_o
                    && let [e, _es] = &vec_args[..]
                {
                    Some(e.clone())
                } else {
                    some_if!(c => S("head", vec![v_o]))
                }
            }
            // VecSame-tι
            ("tail", [v]) => {
                let (c, v_o) = norm!(v, env);
                if let S("vec::", vec_args) = &v_o
                    && let [_e, es] = &vec_args[..]
                {
                    Some(es.clone())
                } else {
                    some_if!(c => S("tail", vec![v_o]))
                }
            }
            // VecSame-i-Vι1, VecSame-i-Vι2
            // TODO: test
            ("ind-Vec", [l, t, m, b, s]) => {
                let (c1, l) = norm!(l, env);
                let (c2, t) = norm!(t, env);
                let (c3, m) = norm!(m, env);
                let (c4, b) = norm!(b, env);
                let (c5, s) = norm!(s, env);
                if matches!(l, Nat(0)) || matches!(t, I("vecnil")) {
                    Some(b)
                } else if is_add1(&l)
                    && let S("vec::", ref t_args) = t
                    && let [e, es] = &t_args[..]
                {
                    let l1 = sub1(&l);
                    Some(app!(
                        s.clone(),
                        l1.clone(),
                        e.clone(),
                        es.clone(),
                        S("ind-Vec", vec![l1, es.clone(), m, b, s])
                    ))
                } else {
                    some_if!( c1 || c2 || c3 || c4 || c5 => S("ind-Vec", vec![l, t, m, b, s]))
                }
            }
            // EqSame-rι, (replace (same e) m b) -> b
            ("replace", [t, m, b]) => {
                let (c1, t_o) = norm!(t, env);
                let (c2, m_o) = norm!(m, env);
                let (c3, b_o) = norm!(b, env);
                if let S("same", _same_args) = &t_o {
                    // no_else!( let [e] = &same_args[..] );
                    Some(b_o)
                } else {
                    some_if!(c1 || c2 || c3 => S("replace", vec![t_o, m_o, b_o]))
                }
            }
            // EqSame-cι, (cong (same e) f) -> (same (f e))
            ("cong", [ty_x, t, f]) => {
                let (c1, ty_x_o) = norm!(ty_x, env);
                let (c2, t_o) = norm!(t, env);
                let (c3, f_o) = norm!(f, env);
                if let S("same", same_args) = &t_o
                    && let [e] = &same_args[..]
                {
                    Some(S("same", vec![app!(f_o, e.clone())]))
                } else {
                    some_if!(c1 || c2 || c3 => S("cong", vec![ty_x_o, t_o, f_o]))
                }
            }
            // EqSame-sι, (symm (same e)) -> (same e)
            ("symm", [t]) => {
                let (c, t_o) = norm!(t, env);
                if let S("same", same_args) = &t_o
                    && let [e] = &same_args[..]
                {
                    Some(S("same", vec![e.clone()]))
                } else {
                    some_if!(c => S("symm", vec![t_o]))
                }
            }
            // EitherSame-i-Eι1, EitherSame-i-Eι2
            ("ind-Either", [t, m, bl, br]) => {
                let (c1, t) = norm!(t, env);
                let (c2, m) = norm!(m, env);
                let (c3, bl) = norm!(bl, env);
                let (c4, br) = norm!(br, env);
                if let S("left", left_args) = &t
                    && let [x] = &left_args[..]
                {
                    Some(app!(bl.clone(), x.clone()))
                } else if let S("right", right_args) = &t
                    && let [x] = &right_args[..]
                {
                    Some(app!(br.clone(), x.clone()))
                } else {
                    some_if!( c1 || c2 || c3 || c4 => S("ind-Either", vec![t, m, bl, br]))
                }
            }
            (bf, args) => {
                let results: Vec<_> = args.iter().map(|a| norm!(a, env)).collect();
                if results.iter().any(|(c, _)| *c) {
                    Some(S(bf, results.into_iter().map(|(_, v)| v).collect()))
                } else {
                    None
                }
            }
        },
    }
}

/// 判断并计算表达式是一个类型或 U(n)，返回其类型层级
/// 改进的第四种 Judgement，见 Figure B.1。
pub fn resolve_type(e: &ast::Expr, env: &Env) -> Result<(u64, core::Expr), Error> {
    tc_log!("resolve `{}` is a type", e);

    use ast::Expr::*;
    use core::Expr::*;

    // 先排除不是类型的项
    match e {
        NatLit(sp, _) | AtomLit(sp, _) => {
            throw!(*sp, ErrorKind::NotType(format!("{}", e)));
        }
        AppExpr(_, args) => {
            match args.as_slice() {
                // 构造子不是类型
                [Ident(sp, ctr), ..] if PIE_CONSTRUCTORS.contains(ctr) => {
                    throw!(*sp, ErrorKind::NotType(format!("{}", e)))
                }
                // ignore other cases
                _ => {}
            }
        }
        // 非类型单例对象
        Ident(sp, id @ ("zero" | "sole" | "nil" | "vecnil")) => {
            throw!(*sp, ErrorKind::NotType(id.to_string()));
        }
        // lambda 表达式不是类型
        LambdaExpr(sp, _, _) => {
            throw!(*sp, ErrorKind::NotType(e.to_string()));
        }
        // ignore other cases
        _ => {}
    }

    // El
    let (ty_o, e_o) = synthesize(e, env)?;
    let l = match &ty_o {
        S("U", arg) if let [Nat(n)] = &arg[..] => *n,
        _ => throw!(e.span(), ErrorKind::NotType(format!("{}", e))),
    };

    tc_log_end!("=> (the (U {}) {})", l, dpp(&e_o, env));
    Ok((l, e_o))
}

const PIE_CONSTRUCTORS: &[&str] = &["add1", "::", "vec::", "same", "left", "right"];

const PIE_TYPE_CONSTRUCTORS: &[&str] = &["U", "List", "Vec", "Either", "="];

// const PIE_ELIMINATORS: &[&str] = &[
//     "car",
//     "cdr",
//     "which-Nat",
//     "iter-Nat",
//     "rec-Nat",
//     "ind-Nat",
//     "rec-List",
//     "ind-List",
//     "head",
//     "tail",
//     "ind-Vec",
//     "symm",
//     "cong",
//     "replace",
//     "trans",
//     "ind-=",
//     "ind-Either",
//     "ind-Absurd",
// ];

// fn print_env(env: &Env) {
//     eprintln!("Current environment:");
//     for (i, (id, (_, def))) in env.iter().enumerate() {
//         let def_str = match def.borrow().as_ref() {
//             Some(d) => format!("{}", dpp(d, env)),
//             None => "_".to_string(),
//         };
//         eprintln!("  {}: {:?} = {}", i, id, def_str);
//     }
// }

/// 检查是否相同类型
/// 第五种 Judgement，见 Figure B.1。
fn type_check_same(ty1: &core::Expr, ty2: &core::Expr, env: &Env) -> Result<(), Error> {
    let (ty1_i, ty2_i) = (ty1, ty2);
    tc_log!(
        "check `{}` and `{}` are the same type",
        dpp(ty1_i, env),
        dpp(ty2_i, env)
    );

    macro_rules! throw_ne {
        () => {
            throw!(ErrorKind::NotSame(
                dpp(ty1_i, env).to_string(),
                dpp(ty2_i, env).to_string(),
                "(U _)".to_owned(),
            ))
        };
    }

    let ty1 = &normalize(ty1_i, env);
    let ty2 = &normalize(ty2_i, env);

    // eprintln!("is_type_check_same: normalized ty1 = {}", dpp(ty1, env));
    // eprintln!("is_type_check_same: normalized ty2 = {}", dpp(ty2, env));
    // print_env(env);

    use core::Expr::*;
    match (ty1, ty2) {
        (Identifier(_id1, idx1), Identifier(_id2, idx2)) => {
            if idx1 != idx2 {
                throw_ne!()
            }
        }
        (I(ty1), I(ty2)) => {
            if ty1 != ty2 {
                throw_ne!()
            }
        }
        // ΣSame-Σ
        (Sigma(a1, ty_a1, ty_r1), Sigma(_a2, ty_a2, ty_r2)) => {
            type_check_same(ty_a1, ty_a2, env)?;
            type_check_same(ty_r1, ty_r2, &env_ext_arg(env, a1, ty_a1))?;
        }
        // FunSame-Π
        (Pi(a1, ty_a1, ty_r1), Pi(_a2, ty_a2, ty_r2)) => {
            type_check_same(ty_a1, ty_a2, env)?;
            type_check_same(ty_r1, ty_r2, &env_ext_arg(env, a1, ty_a1))?;
        }
        (S(f1, args1), S(f2, args2)) => match (&**f1, &**args1, &**f2, &**args2) {
            // ListSame-List
            ("List", [ty_e1], "List", [ty_e2]) => {
                type_check_same(ty_e1, ty_e2, env)?;
            }
            // VecSame-Vec
            ("Vec", [ty_e1, len1], "Vec", [ty_e2, len2]) => {
                type_check_same(ty_e1, ty_e2, env)?;
                expr_check_same(len1, len2, &I("Nat"), env)?;
            }
            // EitherSame-Either
            ("Either", [ty_l1, ty_r1], "Either", [ty_l2, ty_r2]) => {
                type_check_same(ty_l1, ty_l2, env)?;
                type_check_same(ty_r1, ty_r2, env)?;
            }
            // EqSame-=
            ("=", [ty_x1, from1, to1], "=", [ty_x2, from2, to2]) => {
                type_check_same(ty_x1, ty_x2, env)?;
                expr_check_same(from1, from2, ty_x1, env)?;
                expr_check_same(to1, to2, ty_x1, env)?;
            }
            ("U", [n1], "U", [n2]) => {
                expr_check_same(n1, n2, &I("Nat"), env)?;
            }
            ("U", _, "List" | "Vec" | "=" | "Either", _) => throw_ne!(),
            ("List" | "Vec" | "=" | "Either", _, "U", _) => throw_ne!(),
            // beta-equal
            (f1, args1, f2, args2) if f1 == f2 => {
                if args1.len() != args2.len() {
                    throw_ne!()
                }
                for (arg1, arg2) in args1.iter().zip(args2.iter()) {
                    expr_check_same(arg1, arg2, &I("ignore"), env)?;
                }
            }
            _ => throw_ne!(),
        },
        (S("U", _), I("Atom" | "Nat" | "Trivial" | "Absurd")) => throw_ne!(),
        (I("Atom" | "Nat" | "Trivial" | "Absurd"), S("U", _)) => throw_ne!(),
        _ => throw_ne!(),
    }
    tc_log_end!("=> OK");
    Ok(())
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
    tc_log!(
        "check `{}` and `{}` are the same `{}`",
        dpp(c1, env),
        dpp(c2, env),
        dpp(ct, env)
    );

    macro_rules! throw_ne {
        () => {
            throw!(ErrorKind::NotSame(
                dpp(c1, env).to_string(),
                dpp(c2, env).to_string(),
                dpp(ct, env).to_string(),
            ))
        };
    }

    macro_rules! throw_if_not {
        ($e:expr) => {
            if !$e {
                throw_ne!()
            }
        };
    }

    let ct = &normalize(ct, env);

    if let I("Trivial" | "Absurd") = ct {
        tc_log_end!("=> OK");
        return Ok(());
    }

    let c1 = &normalize(c1, env);
    let c2 = &normalize(c2, env);

    use core::Expr::*;
    match (c1, c2) {
        // HypothesisSame
        (Identifier(_, idx1), Identifier(_, idx2)) => throw_if_not!(idx1 == idx2),
        // FIXME: should be (U n) <: (U m) if n <= m
        (S("U", l1), S("U", l2)) => expr_check_same(&l1[0], &l2[0], &I("Nat"), env)?,
        // 比较自然数，考虑字面量和构造器表示
        // NatSame-zero, NatSame-literal
        (Nat(m), Nat(n)) => throw_if_not!(m == n),
        // NatSame-add1
        (S("add1", args), Nat(n)) | (Nat(n), S("add1", args)) => {
            throw_if_not!(*n > 0);
            expr_check_same(&args[0], &Nat(n - 1), ct, env)?;
        }
        (S("add1", args), S("add1", args2)) => expr_check_same(&args[0], &args2[0], ct, env)?,
        // NatSame-Nat, AtomSame-Atom, ListSame-nil ...
        (I(ty1), I(ty2)) => throw_if_not!(ty1 == ty2),
        // AtomSame-tick
        (Atom(a1), Atom(a2)) => throw_if_not!(a1 == a2),
        // ΣSame-Σ
        (Sigma(arg1, ty_a1, ty_d1), Sigma(_arg2, ty_a2, ty_d2)) => {
            type_check_same(ty_a1, ty_a2, env)?;
            type_check_same(ty_d1, ty_d2, &env_ext_arg(env, arg1, ty_a1))?;
        }
        // FunSame-Π
        (Pi(a1, ty_a1, ty_r1), Pi(_a2, ty_a2, ty_r2)) => {
            type_check_same(ty_a1, ty_a2, env)?;
            type_check_same(ty_r1, ty_r2, &env_ext_arg(env, a1, ty_a1))?;
        }
        // FunSame-λ
        (Lambda(_, r1), Lambda(_, r2)) => {
            if let Pi(a, ty_a, ty_r) = ct {
                expr_check_same(r1, r2, ty_r, &env_ext_arg(env, a, ty_a))?;
            } else {
                expr_check_same(
                    r1,
                    r2,
                    &I("ignore"),
                    &env_ext_arg(env, &Argument::Dummy, &I("ignore")),
                )?;
            }
        }
        (S(f1, args1), S(f2, args2)) if f1 == f2 && PIE_TYPE_CONSTRUCTORS.contains(f1) => {
            type_check_same(c1, c2, env)?;
        }
        (S(f1, args1), S(f2, args2)) if f1 == f2 => {
            throw_if_not!(args1.len() == args2.len());
            for (arg1, arg2) in args1.iter().zip(args2.iter()) {
                expr_check_same(arg1, arg2, ct, env)?;
            }
        }
        _ => throw_ne!(),
    };
    tc_log_end!("=> OK");
    Ok(())
}

pub fn default_environment() -> Env {
    Env::new()
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    thread_local! {
        static EXPR_PARSER: crate::syntax::ExprParser = crate::syntax::ExprParser::new();
        static STATEMENT_PARSER: crate::syntax::GlobalStatemantParser = crate::syntax::GlobalStatemantParser::new();
    }

    fn check_syntax(e: &ast::Expr, env: &Env) -> Result<(), String> {
        let env_1 = env.iter().map(|(k, _)| (k.as_deref(), ())).collect();
        ast::check_syntax(e, &env_1).map_err(|e| format!("{}", e))?;
        Ok(())
    }

    fn do_synthesize(s: &str) -> String {
        let e = EXPR_PARSER.with(|p| p.parse(s)).expect("parse error");
        match do_expression(&e) {
            Ok(v) => v,
            Err(e) => format!("Error: {}", e),
        }
    }

    fn do_expression(e: &ast::Expr) -> Result<String, String> {
        let env = default_environment();
        check_syntax(&e, &env)?;
        let (ty_o, e_o) = synthesize(e, &env).map_err(|e| format!("{}", e))?;
        let ty_o = normalize(&ty_o, &env);
        let e_o = normalize(&e_o, &env);
        Ok(format!("(the {} {})", dpp(&ty_o, &env), dpp(&e_o, &env)))
    }

    fn do_statement_0(s: &str) -> Result<String, String> {
        let stat = STATEMENT_PARSER.with(|p| p.parse(s)).expect("parse error");
        let env = default_environment();
        let out = match stat {
            ast::GlobalStatemant::Expression(e) => do_expression(&e)?,
            ast::GlobalStatemant::CheckSame(_, ty, e1, e2) => {
                check_syntax(&e1, &env)?;
                check_syntax(&e2, &env)?;
                check_syntax(&ty, &env)?;
                let (_, ty_o) = resolve_type(&ty, &env).map_err(|e| format!("{}", e))?;
                let ty_n = normalize(&ty_o, &env);
                let e1_o = synthesize_with_type(&e1, &ty_n, &env).map_err(|e| format!("{}", e))?;
                let e2_o = synthesize_with_type(&e2, &ty_n, &env).map_err(|e| format!("{}", e))?;
                let e1_n = normalize(&e1_o, &env);
                let e2_n = normalize(&e2_o, &env);
                match expr_check_same(&e1_n, &e2_n, &ty_n, &env) {
                    Ok(_) => String::new(),
                    Err(_) => return Err("not the same type".to_string()),
                }
            }
            _ => unimplemented!("only support expression and check-same statements"),
        };
        Ok(out)
    }

    fn do_statement(s: &str) -> String {
        eprintln!("do_statement: {}", s);
        match do_statement_0(s) {
            Ok(v) => v,
            Err(e) => format!("Error: {}", e),
        }
    }

    #[test]
    fn test_expression() {
        // Atom
        insta::assert_snapshot!(do_synthesize("(the U Atom)"), @"(the U Atom)");
        insta::assert_snapshot!(do_synthesize("'a"), @"(the Atom 'a)");
        insta::assert_snapshot!(do_synthesize("(quote atom)"), @"(the Atom 'atom)");
        insta::assert_snapshot!(do_synthesize("(the Atom 'a)"), @"(the Atom 'a)");
        // Nat
        insta::assert_snapshot!(do_synthesize("(the U Nat)"), @"(the U Nat)");
        insta::assert_snapshot!(do_synthesize("zero"), @"(the Nat 0)");
        insta::assert_snapshot!(do_synthesize("(add1 zero)"), @"(the Nat 1)");
        insta::assert_snapshot!(do_synthesize("114"), @"(the Nat 114)");
        insta::assert_snapshot!(do_synthesize("(the Nat 0)"), @"(the Nat 0)");
        insta::assert_snapshot!(do_synthesize("(the Nat zero)"), @"(the Nat 0)");
        insta::assert_snapshot!(do_synthesize("(the Nat (add1 zero))"), @"(the Nat 1)");
        insta::assert_snapshot!(do_synthesize("(the Nat 114)"), @"(the Nat 114)");
        insta::assert_snapshot!(do_synthesize("(which-Nat 0 'a (lambda (_) 'b))"), @"(the Atom 'a)");
        insta::assert_snapshot!(do_synthesize("(which-Nat 1 'a (lambda (_) 'b))"), @"(the Atom 'b)");
        insta::assert_snapshot!(do_synthesize("(iter-Nat 0 'a (lambda (_) 'b))"), @"(the Atom 'a)");
        insta::assert_snapshot!(do_synthesize("(iter-Nat 1 'a (lambda (_) 'b))"), @"(the Atom 'b)");
        insta::assert_snapshot!(do_synthesize("(iter-Nat 5 3 (lambda (s) (add1 s)))"), @"(the Nat 8)");
        insta::assert_snapshot!(do_synthesize("(rec-Nat 0 'a (lambda (_ _) 'b))"), @"(the Atom 'a)");
        insta::assert_snapshot!(do_synthesize("(rec-Nat 1 'a (lambda (_ _) 'b))"), @"(the Atom 'b)");
        // Trivial
        insta::assert_snapshot!(do_synthesize("(the U Trivial)"), @"(the U Trivial)");
        insta::assert_snapshot!(do_synthesize("sole"), @"(the Trivial sole)");
        insta::assert_snapshot!(do_synthesize("(the Trivial sole)"), @"(the Trivial sole)");
        // Absurd
        insta::assert_snapshot!(do_synthesize("(the U Absurd)"), @"(the U Absurd)");
        insta::assert_snapshot!(do_synthesize("(the (→ Absurd Nat) (λ (nope) (ind-Absurd nope Nat)))"), @"(the (→ Absurd Nat) (λ (nope) (ind-Absurd nope Nat)))");
        insta::assert_snapshot!(do_synthesize("(the (→ Absurd Nat) (λ (nope) (ind-Absurd (the Absurd nope) Nat)))"), @"(the (→ Absurd Nat) (λ (nope) (ind-Absurd nope Nat)))");
        // lambda
        insta::assert_snapshot!(do_synthesize("(the (→ Nat Nat) (λ (x) x))"), @"(the (→ Nat Nat) (λ (x) x))");
        insta::assert_snapshot!(do_synthesize("(the (→ Nat Nat Nat) (λ (x y) x))"), @"(the (→ Nat Nat Nat) (λ (x y) x))");
        insta::assert_snapshot!(do_synthesize("(the (→ Nat Nat) (λ (x) (add1 x)))"), @"(the (→ Nat Nat) (λ (x) (add1 x)))");
        insta::assert_snapshot!(do_synthesize("(the (-> Nat Nat) (lambda (x) ((the (-> Atom Nat) (lambda (y) 0)) 'a)))"), @"(the (→ Nat Nat) (λ (x) 0))");
        insta::assert_snapshot!(do_synthesize("(the (-> Nat Nat) (lambda (x) ((the (-> Atom Nat) (lambda (y) (add1 x))) 'a)))"), @"(the (→ Nat Nat) (λ (x) (add1 x)))");
        insta::assert_snapshot!(do_synthesize("(the (-> (-> (-> Nat Nat) Nat Nat) Nat Nat) (lambda (f x) (f (lambda (y) y) x)))"), @"(the (→ (→ (→ Nat Nat) Nat Nat) Nat Nat) (λ (f x) (f (λ (y) y) x)))");
        insta::assert_snapshot!(do_synthesize("((the (→ Nat Nat Nat) (λ (x y) x)) 0 1)"), @"(the Nat 0)");
        insta::assert_snapshot!(do_synthesize("((the (→ Nat Atom Nat) (λ (x y) x)) 1 'a)"), @"(the Nat 1)");
        // Pair
        insta::assert_snapshot!(do_synthesize("(the (Pair Nat Atom) (cons 0 'a))"), @"(the (Pair Nat Atom) (cons 0 'a))");
        insta::assert_snapshot!(do_synthesize("(car (the (Pair Atom Nat) (cons 'a 0)))"), @"(the Atom 'a)");
        insta::assert_snapshot!(do_synthesize("(cdr (the (Pair Atom Nat) (cons 'a 0)))"), @"(the Nat 0)");
        insta::assert_snapshot!(do_synthesize("(the (-> (Pair Atom Nat) (Pair Atom Nat)) (λ (p) (cons (car p) (cdr p))))"), @"(the (→ (Pair Atom Nat) (Pair Atom Nat)) (λ (p) p))");
        // Either
        insta::assert_snapshot!(do_synthesize("(Either Nat Atom)"), @"(the U (Either Nat Atom))");
        insta::assert_snapshot!(do_synthesize("(the (Either Nat Atom) (left 0))"), @"(the (Either Nat Atom) (left 0))");
        insta::assert_snapshot!(do_synthesize("(the (Either Nat Atom) (right 'a))"), @"(the (Either Nat Atom) (right 'a))");
        insta::assert_snapshot!(do_synthesize("(ind-Either (the (Either Nat Atom) (left 0)) (λ (_) Nat) (λ (x) x) (λ (y) 1))"), @"(the Nat 0)");
        insta::assert_snapshot!(do_synthesize("(ind-Either (the (Either Nat Atom) (right 'a)) (λ (_) Nat) (λ (x) x) (λ (y) 1))"), @"(the Nat 1)");
        insta::assert_snapshot!(do_synthesize("(ind-Either (the (Either Nat Atom) (left 0)) (λ (_) Atom) (λ (x) 'b) (λ (y) y))"), @"(the Atom 'b)");
        insta::assert_snapshot!(do_synthesize("(ind-Either (the (Either Nat Atom) (right 'a)) (λ (_) Atom) (λ (x) 'b) (λ (y) y))"), @"(the Atom 'a)");
        // Error cases
        insta::assert_snapshot!(do_synthesize("(the Nat 'a)"), @"Error: 9:11: Expected Nat but given Atom");
        insta::assert_snapshot!(do_synthesize("(the Atom zero)"), @"Error: 10:14: Expected Atom but given Nat");
        insta::assert_snapshot!(do_synthesize("(the Trivial 0)"), @"Error: 13:14: Expected Trivial but given Nat");
        insta::assert_snapshot!(do_synthesize("(the Trivial 'a)"), @"Error: 13:15: Expected Trivial but given Atom");
        insta::assert_snapshot!(do_synthesize("(the Absurd 0)"), @"Error: 12:13: Expected Absurd but given Nat");
        insta::assert_snapshot!(do_synthesize("(the 0 'a)"), @"Error: 5:6: 0 is not a type");
        insta::assert_snapshot!(do_synthesize("(the sole 'a)"), @"Error: 5:9: sole is not a type");
        insta::assert_snapshot!(do_synthesize("(the Nat U)"), @"Error: 9:10: Expected Nat but given (U 1)");
        insta::assert_snapshot!(do_synthesize("(the U 'a)"), @"Error: 7:9: Expected U but given Atom");
        insta::assert_snapshot!(do_synthesize("(ind-Either (the (Either Nat Atom) (left 0)) (λ (x) x) (λ (y) y))"), @"Error: 0:67: ind-Either need 4 arguments, got 3");
        insta::assert_snapshot!(do_synthesize("(ind-Either (the (Either Nat Atom) (right 'a)) (λ (x) x) (λ (y) y))"), @"Error: 0:69: ind-Either need 4 arguments, got 3");
    }

    #[test]
    fn pi_sigma_scope() {
        insta::assert_snapshot!(do_synthesize("(Pi ((A U)(D U)) (→ A D))"), @"(the (U 1) (Π ((A U)(D U)) (→ A D)))");
        insta::assert_snapshot!(do_synthesize("(Pi ((A U)(D U)) (Pair A D))"), @"(the (U 1) (Π ((A U)(D U)) (Pair A D)))");
        insta::assert_snapshot!(do_synthesize("(Pi ((A U)(D U)) (Pi ((a A)(d D)) (→ A D)))"), @"(the (U 1) (Π ((A U)(D U)) (→ A D A D)))");
    }

    #[test]
    fn tlt_tests() {
        insta::assert_snapshot!(do_statement("(the U (Pair Atom Atom))"), @"(the U (Pair Atom Atom))");
        insta::assert_snapshot!(do_statement("(the (Pair Atom Atom) (cons 'ratatouille 'baguette))"), @"(the (Pair Atom Atom) (cons 'ratatouille 'baguette))");
        insta::assert_snapshot!(do_statement("(the (Pair Atom Nat) (cons 'ratatouille 0))"), @"(the (Pair Atom Nat) (cons 'ratatouille 0))");
        insta::assert_snapshot!(do_statement("(the (Pair Atom Atom) (cons 'ratatouille 0))"), @"Error: 41:42: Expected Atom but given Nat");
        insta::assert_snapshot!(do_statement("(check-same (Pair Atom Atom) (cons 'aubergine 'courgette) (cons 'aubergine 'courgette))"), @"");
        insta::assert_snapshot!(do_statement("(check-same (Pair Atom Atom) (cons 'aubergine 'courgette) (cons 'aubergine 'bbb))"), @"Error: not the same type");
        insta::assert_snapshot!(do_statement("(check-same U Atom Atom)"), @"");
        insta::assert_snapshot!(do_statement("(check-same U Atom Nat)"), @"Error: not the same type");
        insta::assert_snapshot!(do_statement("(check-same U (Pair Atom Nat) (Pair Atom Nat))"), @"");
        insta::assert_snapshot!(do_statement("(check-same U (Pair Nat Atom) (Pair Atom Nat))"), @"Error: not the same type");
        insta::assert_snapshot!(do_statement("(check-same Nat 0 0)"), @"");
        insta::assert_snapshot!(do_statement("(check-same Nat 0 1)"), @"Error: not the same type");
        insta::assert_snapshot!(do_statement("(check-same Nat zero 0)"), @"");
        insta::assert_snapshot!(do_statement("(check-same Nat zero (add1 zero))"), @"Error: not the same type");
        insta::assert_snapshot!(do_statement("(check-same Nat 1 (add1 zero))"), @"");
        insta::assert_snapshot!(do_statement("(check-same Nat (add1 zero) (add1 zero))"), @"");
        insta::assert_snapshot!(do_statement("(check-same (→ Nat Nat) (λ (x) x) (λ (x) x))"), @"");
        insta::assert_snapshot!(do_statement("(check-same (→ Nat Nat) (λ (x) x) (λ (y) y))"), @"");
        insta::assert_snapshot!(do_statement("(check-same (→ Nat Nat) (λ (x) x) (λ (y) 0))"), @"Error: not the same type");
        insta::assert_snapshot!(do_statement("(check-same (→ Nat (Pair Nat Nat)) (λ (a) (cons a a)) (λ (d) (cons d d)))"), @"");
        insta::assert_snapshot!(do_statement("(check-same (→ Atom Nat Atom) (λ (x y) x) (λ (a b) a))"), @"");
        insta::assert_snapshot!(do_statement("(which-Nat zero 'naught (λ (n) 'more))"), @"(the Atom 'naught)");
        insta::assert_snapshot!(do_statement("(which-Nat 4 'naught (λ (n) 'more))"), @"(the Atom 'more)");
        insta::assert_snapshot!(do_statement("(the (Pair U U) (cons Atom Nat))"), @"(the (Pair U U) (cons Atom Nat))");
        insta::assert_snapshot!(do_statement("(Pair U U)"), @"(the (U 1) (Pair U U))");
        insta::assert_snapshot!(do_statement("(Pair Atom U)"), @"(the (U 1) (Pair Atom U))");
        insta::assert_snapshot!(do_statement("(-> U U)"), @"(the (U 1) (→ U U))");
        insta::assert_snapshot!(do_statement("(iter-Nat 5 3 (lambda (smaller) (add1 smaller)))"), @"(the Nat 8)");
        insta::assert_snapshot!(do_statement("(iter-Nat 0 3 (lambda (smaller) (add1 smaller)))"), @"(the Nat 3)");
        insta::assert_snapshot!(do_statement("(rec-Nat (add1 zero) 0 (λ (n-1 almost) (add1 (add1 almost))))"), @"(the Nat 2)");
        insta::assert_snapshot!(do_statement("(rec-Nat zero 0 (λ (n-1 almost) (add1 (add1 almost))))"), @"(the Nat 0)");
        insta::assert_snapshot!(do_statement("(List Atom)"), @"(the U (List Atom))");
        insta::assert_snapshot!(do_statement("(the (List Atom) nil)"), @"(the (List Atom) nil)");
        insta::assert_snapshot!(do_statement("(the (List Atom) nil)"), @"(the (List Atom) nil)");
        insta::assert_snapshot!(do_statement("(the (List (List Atom)) nil)"), @"(the (List (List Atom)) nil)");
        insta::assert_snapshot!(do_statement("(the (List 'potato) nil)"), @"Error: 11:18: 'potato is not a type");
        insta::assert_snapshot!(do_statement("(Vec Atom 3)"), @"(the U (Vec Atom 3))");
        insta::assert_snapshot!(do_statement("(the (Vec Atom 0) vecnil)"), @"(the (Vec Atom 0) vecnil)");
        insta::assert_snapshot!(do_statement("(the (Vec Atom 1) (vec:: 'oyster vecnil))"), @"(the (Vec Atom 1) (vec:: 'oyster vecnil))");
        insta::assert_snapshot!(do_statement("(the (Vec Atom 2) (vec:: 'oyster vecnil))"), @"Error: 33:39: Expected (Vec Atom 1) but given vecnil");
        insta::assert_snapshot!(do_statement("(the (Vec Atom 3) (vec:: 'crimini (vec:: 'shiitake vecnil)))"), @"Error: 51:57: Expected (Vec Atom 1) but given vecnil");
        insta::assert_snapshot!(do_statement("(head (the (Vec Atom 2) (vec:: 'a (vec:: 'b vecnil))))"), @"(the Atom 'a)");
        insta::assert_snapshot!(do_statement("(head (the (Vec Atom 0) vecnil))"), @"Error: 1:5: Expected Vec longer than 1 but given (the (Vec Atom 0) vecnil)");
        insta::assert_snapshot!(do_statement("(tail (the (Vec Atom 2) (vec:: 'a (vec:: 'b vecnil))))"), @"(the (Vec Atom 1) (vec:: 'b vecnil))");
        insta::assert_snapshot!(do_statement("(tail (the (Vec Atom 1) (vec:: 'a vecnil)))"), @"(the (Vec Atom 0) vecnil)");
        insta::assert_snapshot!(do_statement("(tail (the (Vec Atom 0) vecnil))"), @"Error: 1:5: Expected Vec longer than 1 but given (the (Vec Atom 0) vecnil)");
        insta::assert_snapshot!(do_statement("(= Atom 'kale 'blackberries)"), @"(the U (= Atom 'kale 'blackberries))");
        insta::assert_snapshot!(do_statement("(= Nat 1 (add1 zero))"), @"(the U (= Nat 1 1))");
        insta::assert_snapshot!(do_statement("(= U Nat Nat)"), @"(the (U 1) (= U Nat Nat))");
        insta::assert_snapshot!(do_statement("(the U (Σ ((bread Atom)) (= Atom bread 'bagel)))"), @"(the U (Σ ((bread Atom)) (= Atom bread 'bagel)))");
        insta::assert_snapshot!(do_statement("(the (Σ ((bread Atom)) (= Atom bread 'bagel)) (cons 'bagel (same 'bagel)))"), @"(the (Σ ((bread Atom)) (= Atom bread 'bagel)) (cons 'bagel (same 'bagel)))");
        insta::assert_snapshot!(do_statement("(Σ ((A U)) A)"), @"(the (U 1) (Σ ((A U)) A))");
        insta::assert_snapshot!(do_statement("(the (Σ ((A U)) A) (cons Nat 4))"), @"(the (Σ ((A U)) A) (cons Nat 4))");
    }

    #[test]
    fn test_normalize() {
        insta::assert_snapshot!(do_statement("(the Nat zero)"), @"(the Nat 0)");
        insta::assert_snapshot!(do_statement("(the Nat (add1 zero))"), @"(the Nat 1)");
        insta::assert_snapshot!(do_statement("((the (-> Nat Nat) (lambda (x) (add1 x))) 1)"), @"(the Nat 2)");
    }

    #[test]
    fn nil_vecnil() {
        insta::assert_snapshot!(do_statement("nil"), @"Error: 0:3: Can't determine the type of nil");
        insta::assert_snapshot!(do_statement("vecnil"), @"Error: 0:6: Can't determine the type of vecnil");
    }

    #[test]
    fn normalize_by_type() {
        insta::assert_snapshot!(do_statement("(the (Pi ((x Trivial)) (= Trivial x sole)) (lambda (x) (same sole)))"), @"(the (→ Trivial (= Trivial sole sole)) (λ (x) (same sole)))");
        insta::assert_snapshot!(do_statement("(the (Pi ((x Absurd)(y Absurd)) (= Absurd x y)) (lambda (x y) (same x)))"), @"(the (Π ((x Absurd)(y Absurd)) (= Absurd x y)) (λ (x y) (same x)))");

        insta::assert_snapshot!(do_statement("(the (Pi ((p (Pair Atom Atom))) (= (Pair Atom Atom) p (cons (car p) (cdr p)))) (lambda (p) (same p)))"), @"(the (Π ((p (Pair Atom Atom))) (= (Pair Atom Atom) p p)) (λ (p) (same p)))");

        insta::assert_snapshot!(do_statement("(check-same
          (Pi ((x Trivial)) (= Trivial x sole))
          (lambda (x) (same x))
          (lambda (x) (same sole)))"), @"");
        insta::assert_snapshot!(do_statement("(the (Pi ((x Trivial)) (= Trivial x sole)) (lambda (x) (same x)))"), @"(the (→ Trivial (= Trivial sole sole)) (λ (x) (same sole)))");
        insta::assert_snapshot!(do_statement("(the (Pi ((x Trivial)) (= Trivial x sole)) (lambda (x) (same sole)))"), @"(the (→ Trivial (= Trivial sole sole)) (λ (x) (same sole)))");
    }
}
