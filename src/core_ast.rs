use crate::ast;
use crate::utils::map_result;
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
    Identifier(Ref<str>),

    /// `(λ (ident) expr)`，被转换为单层
    LambdaExpr(Argument, Ref<Expr<MetaInfo>>),

    /// `(Π ((ident expr)) expr)`，被转换为单层
    /// 并将箭头表达式转换为 Π 表达式
    PiExpr(Argument, Ref<Type<MetaInfo>>, Ref<Expr<MetaInfo>>),

    /// `(Σ ((ident expr)) expr)`，被转换为单层
    SigmaExpr(Argument, Ref<Type<MetaInfo>>, Ref<Expr<MetaInfo>>),

    /// 函数调用，经过柯里化转换为只有一个参数
    Apply(Ref<Expr<MetaInfo>>, Ref<Expr<MetaInfo>>),

    /// 内建调用
    // 这里或许可以用两层 `Info` 给第一参数加上元信息
    BuiltinApply(Ref<str>, Vec<Expr<MetaInfo>>),
}

pub type Type<M> = Expr<M>;

/// 标识符，`Dummy` 用于将普通函数类型转换为 Pi 类型时，
/// 未来或可用于 `_` 语法
#[derive(Debug, Clone)]
pub enum Argument {
    Dummy,
    Symbol(Ref<str>),
}

/// 将 Pi 表达式、Sigma 表达式展开为单层，箭头表达式转换为 Pi 表达式，
/// 调用分别转化为函数调用和内建调用，并检查内建调用的合法性
pub fn unfold(e: &ast::Expr) -> Result<Expr<()>, String> {
    use ast::Expr::*;
    let ret = match e {
        Literal(_, lit) => Expr::Literal(lit.clone()),
        Identifier(_, id) => Expr::Identifier(id.clone()),
        List(_, exprs) => match &**exprs {
            [Identifier(_, f), args @ ..] if get_builtin_argument_number(f).is_some() => {
                let valid_argc = get_builtin_argument_number(f).unwrap();
                if args.len() == valid_argc {
                    Expr::BuiltinApply(f.clone(), map_result(args, unfold)?)
                } else {
                    return Err(format!(
                        "`{}` should take {} arguments, but here is {}.",
                        f,
                        valid_argc,
                        args.len()
                    ));
                }
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
            let mut e = unfold(body)?;
            // 注意从后向前的顺序
            for (ast::Symbol(_, sym), ty) in args.iter().rev() {
                e = Expr::PiExpr(
                    self::Argument::Symbol(sym.clone()),
                    Ref::new(unfold(ty)?),
                    Ref::new(e),
                );
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
            let mut e = unfold(body)?;
            // 注意从后向前的顺序
            for (ast::Symbol(_, sym), ty) in args.iter().rev() {
                e = Expr::SigmaExpr(
                    self::Argument::Symbol(sym.clone()),
                    Ref::new(unfold(ty)?),
                    Ref::new(e),
                );
            }
            e
        }
    };
    Ok(ret)
}

/// 将列表经过柯里化转换为函数调用
fn unfold_list(exprs: &[ast::Expr]) -> Result<Expr<()>, String> {
    let mut es = exprs.iter();
    let mut f = unfold(es.next().unwrap())?;
    for e in es {
        f = Expr::Apply(Ref::new(f), Ref::new(unfold(e)?));
    }
    Ok(f)
}

/// 通过内建函数名获取其应有的参数数量，如果传入的不是内建函数名，返回 `None`。
fn get_builtin_argument_number(fname: &str) -> Option<usize> {
    for (bf, n) in std::array::IntoIter::new(PIE_BUILTIN_FUNCTIONS) {
        if bf == fname {
            return Some(n);
        }
    }
    None
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
