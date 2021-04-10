use crate::*;
use ast::Literal;
use core_ast::*;
use fehler::{throw, throws};

pub type Env = crate::utils::StackMap<Ref<str>, Type<()>>;
pub type Error = ();

macro_rules! assert_match {
    (let $p:tt($($i:ident),+) = $e:expr) => {
        let ($($i),+) = if let $p($($i),+) = $e {
            ($($i),+)
        } else {
            throw!(());
        };
    };
}

// TODO: 使用 De Bruijn 方法解决变量名、作用域的各种问题

/// 执行 expr[var/e]，将 expr 中自由出现的 var 替换为 e，e 应当是没有自由变量的。
fn substitute<M>(expr: &Expr<M>, var: &str, e: &Expr<M>, env: &Env) -> Expr<()> {
    todo!()
}

/// 对常用的 Argument 模式的简写
#[inline]
fn substitute_arg(expr: &Expr<()>, var: &Argument, e: &Expr<()>, env: &Env) -> Expr<()> {
    match var {
        Argument::Symbol(sym) => substitute(expr, sym, e, env),
        Argument::Dummy => expr.clone(),
    }
}

fn env_ext_arg(env: &Env, arg: &Argument, ty: &Type<()>) -> Env {
    match arg {
        Argument::Symbol(sym) => env.insert(sym.clone(), ty.clone()),
        Argument::Dummy => env.clone(),
    }
}

/// 产生随机符号
fn gensym() -> Ref<str> {
    todo!()
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
#[throws]
pub fn synthesize_with_type<M>(e: &Expr<M>, ty: &Type<()>, env: &Env) -> Expr<()> {
    use Expr::*;
    match &ty {
        // FunI-1
        PiExpr(pi_arg, ty_arg, ty_ret) => {
            // 除去外层 Info
            let mut e = e;
            while let Info(_, inner) = e {
                e = inner;
            }
            assert_match!(let LambdaExpr(arg, r) = e);
            // TODO: 优化此处
            match (arg, pi_arg) {
                (Argument::Dummy, Argument::Dummy) => {
                    let r_o = synthesize_with_type(r, ty_ret, env)?;
                    LambdaExpr(Argument::Dummy, Ref::new(r_o))
                }
                (Argument::Dummy, Argument::Symbol(sym)) => {
                    let r_o = synthesize_with_type(
                        r,
                        ty_ret,
                        &env.insert(sym.clone(), Clone::clone(&ty_arg)),
                    )?;
                    LambdaExpr(Argument::Dummy, Ref::new(r_o))
                }
                (Argument::Symbol(sym), Argument::Dummy) => {
                    let r_o = synthesize_with_type(
                        r,
                        ty_ret,
                        &env.insert(sym.clone(), Clone::clone(&ty_arg)),
                    )?;
                    LambdaExpr(Argument::Symbol(sym.clone()), Ref::new(r_o))
                }
                (Argument::Symbol(sym), Argument::Symbol(pi_sym)) if **sym == **pi_sym => {
                    let r_o = synthesize_with_type(
                        r,
                        ty_ret,
                        &env.insert(sym.clone(), Clone::clone(&ty_arg)),
                    )?;
                    LambdaExpr(Argument::Symbol(sym.clone()), Ref::new(r_o))
                }
                (Argument::Symbol(sym), Argument::Symbol(pi_sym)) => {
                    let new_sym = gensym();
                    let r_new = substitute(r, sym, &Identifier(new_sym.clone()), env);
                    let ty_ret_new = substitute(ty_ret, pi_sym, &Identifier(new_sym.clone()), env);
                    let r_o = synthesize_with_type(
                        &r_new,
                        &ty_ret_new,
                        &env.insert(new_sym.clone(), Clone::clone(&ty_arg)),
                    )?;
                    LambdaExpr(Argument::Symbol(new_sym), Ref::new(r_o))
                }
            }
        }
        SigmaExpr(sigma_arg, ty_arg, ty_ret) => {
            todo!()
        }
        BuiltinApply(bf, args) => {
            todo!()
        }
        // Switch
        _ => {
            let (ty_e_o, e_o) = synthesize(e, env)?;
            type_check_same(&ty_e_o, &ty, env)?;
            e_o
        }
    }
}

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
#[throws]
pub fn synthesize<M>(e: &Expr<M>, env: &Env) -> (Type<()>, Expr<()>) {
    use Expr::*;
    match e {
        Info(_, e) => synthesize(e, env)?,
        Literal(lit) => synthesize_literal(lit),
        // Hypothesis
        Identifier(ident) => match env.get(ident) {
            Some(ty) => (ty.clone(), Identifier(ident.clone())),
            None => throw!(()),
        },
        PiExpr(arg, ty, body) => {
            todo!()
        }
        SigmaExpr(arg, ty, body) => {
            todo!()
        }
        // FunE-1
        Apply(f, arg) => {
            let (ty_f, f_o) = synthesize(f, env)?;
            assert_match!(let PiExpr(var, ty_arg, ty_ret) = ty_f);
            let arg_o = synthesize_with_type(arg, &ty_arg, env)?;
            let ty = substitute_arg(&ty_ret, &var, &arg_o, env);
            (ty, Apply(Ref::new(f_o), Ref::new(arg_o)))
        }
        // 目前还未引入 Universe Hierarchy，但这条规则似乎没有问题
        U(n) => (U(n + 1), U(*n)),
        BuiltinApply(bf, args) => {
            match (&**bf, &**args) {
                // "The" 规则
                ("the", [ty, expr]) => {
                    let ty_o = resolve_type(ty, env)?;
                    let expr_o = synthesize_with_type(expr, &ty_o, env)?;
                    (ty_o, expr_o)
                }
                _ => unreachable!(),
            }
        }
        LambdaExpr(_, _) => throw!(()),
    }
}

/// 判断并计算表达式是一个类型（或 U）。
/// 第四种 Judgement，见 Figure B.1。
#[throws]
fn resolve_type<M>(e: &Expr<M>, env: &Env) -> Type<()> {
    use Expr::*;
    match e {
        Info(_, e) => resolve_type(e, env)?,
        // FunF-1
        PiExpr(arg, ty, ty_ret) => {
            let ty_o = resolve_type(ty, env)?;
            let ty_ret_o = resolve_type(ty_ret, &env_ext_arg(&env, &arg, &ty_o))?;
            PiExpr(arg.clone(), Ref::new(ty_o), Ref::new(ty_ret_o))
        }
        // SigmaF-1
        SigmaExpr(arg, ty, ty_d) => {
            let ty_o = resolve_type(ty, env)?;
            let ty_d_o = resolve_type(ty_d, &env_ext_arg(&env, &arg, &ty_o))?;
            SigmaExpr(arg.clone(), Ref::new(ty_o), Ref::new(ty_d_o))
        }
        BuiltinApply(bf, args) => {
            todo!()
        }
        // UF
        U(n) => U(*n),
        //Literal, Lambda, Identifier, Apply
        // El
        _ => synthesize_with_type(e, &U(0), env)?,
    }
}

/// 检查是否相同类型
/// 第五种 Judgement，见 Figure B.1。
#[throws]
fn type_check_same(ty1: &Type<()>, ty2: &Type<()>, env: &Env) {
    use Expr::*;
    dbg!(ty1, ty2);
    // TODO: 比较前充分计算 ty1 和 ty2
    match (ty1, ty2) {
        (Identifier(id1), Identifier(id2)) => {
            if id1 != id2 {
                throw!(());
            }
        }
        (U(m), U(n)) => {
            if m != n {
                throw!(());
            }
        }
        _ => {
            todo!()
        }
    }
}

/// 检查是否相同表达式
/// 第八种 Judgement，见 Figure B.1。
#[throws]
fn expr_check_same(c1: &Expr<()>, c2: &Expr<()>, ct: &Type<()>, env: &Env) {
    todo!()
}

/// 直接从字面量推导类型
fn synthesize_literal(lit: &Literal) -> (Type<()>, Expr<()>) {
    let ty = match lit {
        Literal::Nat(_) => Type::Identifier("Nat".into()),
        Literal::Atom(_) => Type::Identifier("Atom".into()),
    };
    (ty, Expr::Literal(lit.clone()))
}

pub fn default_environment() -> Env {
    Env::new()
        .insert("Nat".into(), Expr::U(0))
        .insert("Atom".into(), Expr::U(0))
}
