use crate::*;
use ast::Literal;
use core_ast::*;

pub type Env = crate::utils::StackMap<Ref<str>, Type<()>>;
pub type Result<T> = core::result::Result<T, ()>;


macro_rules! assert_match {
    (let $p:tt($($i:ident),+) = $e:expr) => {
        let ($($i),+) = if let $p($($i),+) = $e {
            ($($i),+)
        } else {
            return Err(());
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
fn substitute_arg<M>(expr: &Expr<M>, var: &Argument, e: &Expr<M>, env: &Env) -> Expr<()> {
    match var {
        Argument::Symbol(sym) => substitute(expr, sym, e, env),
        Argument::Dummy => todo!(), // expr.clone()
    }
}

/// 产生随机符号
fn gensym() -> Ref<str> {
    todo!()
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
pub fn synthesize_with_type<M>(e: &Expr<M>, ty: &Type<()>, env: &Env) -> Result<Expr<()>> {
    use Expr::*;
    let ret = match e {
        Info(_, e) => return synthesize_with_type(e, ty, env),
        // FunI-1
        LambdaExpr(arg, r) => {
            assert_match!(let PiExpr(pi_arg, ty_arg, ty_ret) = ty);
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
        BuiltinApply(bf, args) => {
            todo!()
        }
        //Literal, Identifier, Apply, Pi, Sigma
        // Switch
        _ => {
            let (e_o, ty_o) = synthesize(e, env)?;
            type_check_same(&ty_o, ty, env)?;
            e_o
        }
    };
    Ok(ret)
}

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
pub fn synthesize<M>(e: &Expr<M>, env: &Env) -> Result<(Type<()>, Expr<()>)> {
    use Expr::*;
    let ret = match e {
        Info(_, e) => return synthesize(e, env),
        Literal(lit) => synthesize_literal(lit),
        // Hypothesis
        Identifier(ident) => match env.get(ident) {
            Some(ty) => (ty.clone(), Identifier(ident.clone())),
            None => return Err(()),
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
        BuiltinApply(bf, args) => {
            todo!()
        }
        LambdaExpr(_, _) => return Err(()),
    };
    Ok(ret)
}

/// 判断并计算表达式是一个类型（或 U）。
/// 第四种 Judgement，见 Figure B.1。
fn resolve_type<M>(e: &Expr<M>, env: &Env) -> Result<Type<()>> {
    todo!()
}

/// 检查是否相同类型
/// 第五种 Judgement，见 Figure B.1。
fn type_check_same(t1: &Type<()>, t2: &Type<()>, env: &Env) -> Result<()> {
    todo!()
}

/// 检查是否相同表达式
/// 第八种 Judgement，见 Figure B.1。
fn expr_check_same(c1: &Expr<()>, c2: &Expr<()>, ct: &Type<()>, env: &Env) -> Result<()> {
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
