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

/// 执行 expr[var/e]，将 expr 中自由出现的 var 替换为 e
fn substitute<M>(expr: &Expr<M>, var: &str, e: &Expr<M>, env: &Env) -> Expr<()> {
    todo!()
}

/// 检查表达式 `e` 属于（已检查的）类型 `ty`，返回检查结果。
/// 第六种 Judgement，见 Figure B.1。
pub fn synthesize_with_type<M>(e: &Expr<M>, ty: &Type<()>, env: &Env) -> Result<Expr<()>> {
    todo!()
}

/// 对表达式 `e` 进行类型检查，返回检查结果。
/// 第七种 Judgement，见 Figure B.1。
pub fn synthesize<M>(e: &Expr<M>, env: &Env) -> Result<(Type<()>, Expr<()>)> {
    use Expr::*;
    let ret = match e {
        Info(_, e) => return synthesize(e, env),
        Literal(lit) => synthesize_literal(lit),
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
        Apply(f, arg) => {
            let (ty_f, f_o) = synthesize(f, env)?;
            assert_match!(let PiExpr(x, ty_arg, ty_ret) = ty_f);
            let arg_o = synthesize_with_type(arg, &ty_arg, env)?;
            let ty;
            match x {
                Argument::Symbol(var) => {
                    ty = substitute(&ty_ret, &var, &arg_o, env);
                }
                Argument::Dummy => {
                    ty = Clone::clone(&ty_ret);
                }
            }
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
fn type_check_same(t1: &Type<()>, t2: &Type<()>, env: &Env) -> bool {
    todo!()
}

/// 检查是否相同表达式
/// 第八种 Judgement，见 Figure B.1。
fn expr_check_same(c1: &Expr<()>, c2: &Expr<()>, ct: &Type<()>, env: &Env) -> bool {
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
