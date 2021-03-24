use crate::Ref;

/// 在源代码中起始和结束位置，前闭后开
#[derive(Debug, Clone, Copy)]
pub struct Span(pub usize, pub usize);

/// 顶层语句允许 define 语句、claim 语句和表达式。
#[derive(Debug, Clone)]
pub enum GlobalStatemant<MetaInfo> {
    /// `(claim varname type)`
    Claim(Span, Identifier, SExpr<MetaInfo>),

    /// `(define varname expression)`
    Define(Span, Identifier, SExpr<MetaInfo>),
    Expression(SExpr<MetaInfo>),
}

/// 表达式包含位置信息和元信息（类型等）
#[derive(Debug, Clone)]
pub enum SExpr<MetaInfo> {
    /// 用于在抽象代码树中插入信息的中间层，最多允许有一层。
    Info {
        info: MetaInfo,
        inner: Ref<SExpr<MetaInfo>>,
    },
    /// 字面量，表示一个值
    Literal(Literal),
    /// 标识符，表示变量、函数、类型等
    Identifier(Identifier),
    /// 以下 4 项为无法通过自定义或特殊变量实现的语法项
    /// `(λ (ident+) expr)`，解析时转换为单层
    LambdaExpr {
        arg: Identifier,
        body: Ref<SExpr<MetaInfo>>,
    },
    /// `(Π ((ident expr)+) expr)`，解析时转换为单层
    PiExpr {
        arg: Identifier,
        arg_type: Ref<Type<MetaInfo>>,
        body: Ref<SExpr<MetaInfo>>,
    },
    /// `(Π ((ident expr)+) expr)`，解析时转换为单层
    /// 并把 `(→ expr+ expr)` 转换为 Π 表达式
    SigmaExpr {
        arg: Identifier,
        arg_type: Ref<Type<MetaInfo>>,
        body: Ref<SExpr<MetaInfo>>,
    },
    /// `(Σ ((ident expr)+) expr)`，解析时转换为单层
    TheExpr {
        ty: Ref<Type<MetaInfo>>,
        expr: Ref<SExpr<MetaInfo>>,
    },
    /// 函数调用或构造
    SList(Vec<SExpr<MetaInfo>>),
}

pub type Type<MetaInfo> = SExpr<MetaInfo>;

/// 字面量，目前有整数和原子
#[derive(Debug, Clone)]
pub enum Literal {
    Nat(u64),
    Atom(Ref<str>),
}

/// 标识符，`Dummy` 用于将普通函数类型转换为 Pi 类型时，
/// 未来或可用于 `_` 语法
#[derive(Debug, Clone)]
pub enum Identifier {
    Dummy,
    Identifier(Ref<str>),
}
