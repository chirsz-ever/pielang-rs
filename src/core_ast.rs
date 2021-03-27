use crate::ast;
use crate::Ref;

macro_rules! claim_array {
    ($id:ident $name:ident: [$ty: ty; _] = $value:expr $(;)?) => {
        $id $name: [$ty; $value.len()] = $value;
    }
}

// TODO: 嵌入源代码位置信息，定义合适的错误类型

/// 表达式包含位置信息和元信息（类型等）
#[derive(Debug, Clone)]
pub enum Expr<MetaInfo> {
    /// 用于在抽象代码树中插入信息的中间层。
    Info(MetaInfo, Ref<Expr<MetaInfo>>),

    /// 字面量
    Literal(ast::Literal),

    /// 标识符，表示变量、函数、类型等
    Identifier(Identifier),

    /// `(λ (ident) expr)`，被转换为单层
    LambdaExpr(Identifier, Ref<Expr<MetaInfo>>),

    /// `(Π ((ident expr)) expr)`，被转换为单层
    /// 并将箭头表达式转换为 Π 表达式
    PiExpr(Identifier, Ref<Type<MetaInfo>>, Ref<Expr<MetaInfo>>),

    /// `(Σ ((ident expr)) expr)`，被转换为单层
    SigmaExpr(Identifier, Ref<Type<MetaInfo>>, Ref<Expr<MetaInfo>>),

    /// 函数调用或构造
    List(Vec<Expr<MetaInfo>>),
}

pub type Type<M> = Expr<M>;

/// 标识符，`Dummy` 用于将普通函数类型转换为 Pi 类型时，
/// 未来或可用于 `_` 语法
#[derive(Debug, Clone)]
pub enum Identifier {
    Dummy,
    Symbol(Ref<str>),
}

// TODO: 当未修改时直接引用整个子树
// TODO: 合并 unfold 和 check_builtin 的重复代码

/// 将 Pi 表达式、Sigma 表达式展开为单层，箭头表达式转换为 Pi 表达式
pub fn unfold(e: &ast::Expr) -> Expr<()> {
    use ast::Expr::*;
    match e {
        Literal(_, lit) => Expr::Literal(lit.clone()),
        Identifier(_, id) => Expr::Identifier(self::Identifier::Symbol(id.clone())),
        List(_, exprs) => Expr::List(exprs.iter().map(unfold).collect()),
        LambdaExpr(_, args, body) => {
            let mut e = unfold(body);
            for ast::Symbol(_, sym) in args {
                e = Expr::LambdaExpr(self::Identifier::Symbol(sym.clone()), Ref::new(e));
            }
            e
        }
        PiExpr(_, args, body) => {
            let mut e = unfold(body);
            for (ast::Symbol(_, sym), ty) in args {
                e = Expr::PiExpr(
                    self::Identifier::Symbol(sym.clone()),
                    Ref::new(unfold(ty)),
                    Ref::new(e),
                );
            }
            e
        }
        ArrowExpr(_, types) => {
            let mut tys = types.iter().map(unfold);
            // syntax.lalrpop 中的规则保证至少有两项，所以以下 `unwrap` 不会有问题
            let mut e = tys.next_back().unwrap();
            for ty in tys {
                e = Expr::PiExpr(self::Identifier::Dummy, Ref::new(ty), Ref::new(e));
            }
            e
        }
        SigmaExpr(_, args, body) => {
            let mut e = unfold(body);
            for (ast::Symbol(_, sym), ty) in args {
                e = Expr::SigmaExpr(
                    self::Identifier::Symbol(sym.clone()),
                    Ref::new(unfold(ty)),
                    Ref::new(e),
                );
            }
            e
        }
    }
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

/// 检测内置构造器和函数如 `which-Nat`，它们不可柯里化
pub fn check_builtin(e: &Expr<()>) -> Result<(), String> {
    use Expr::*;
    match e {
        Info(_, inner) => {
            check_builtin(inner)?;
        }
        Literal(_) => {}
        Identifier(_) => {}
        LambdaExpr(_arg, body) => check_builtin(body)?,
        PiExpr(_arg, ty, body) => {
            check_builtin(ty)?;
            check_builtin(body)?;
        }
        SigmaExpr(_arg, ty, body) => {
            check_builtin(ty)?;
            check_builtin(body)?;
        }
        List(exprs) => match exprs.as_slice() {
            [Identifier(self::Identifier::Symbol(f)), args @ ..] => {
                check_builtin_function(f, args.len())?;
            }
            [Info(_, inner), args @ ..] => {
                let mut f = inner.as_ref();
                while let Info(_, inner) = f {
                    f = inner;
                }
                if let Identifier(self::Identifier::Symbol(f)) = f {
                    check_builtin_function(f, args.len())?;
                }
            }
            _ => {
                for e in exprs {
                    check_builtin(e)?;
                }
            }
        },
    };
    Ok(())
}

fn check_builtin_function(f: &str, argc: usize) -> Result<(), String> {
    match PIE_BUILTIN_FUNCTIONS.iter().position(|(bf, _)| bf == &f) {
        None => {}
        Some(n) => {
            let valid_argc = PIE_BUILTIN_FUNCTIONS[n].1;
            if argc != valid_argc {
                return Err(format!(
                    "`{}` should take {} arguments, but here is {} arguments.",
                    f, valid_argc, argc
                ));
            }
        }
    }
    Ok(())
}
