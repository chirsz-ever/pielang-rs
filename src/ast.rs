pub type Ref<T> = std::rc::Rc<T>;

/// 在源代码中起始和结束位置，前闭后开
#[derive(Debug, Clone, Copy)]
pub struct Span(pub usize, pub usize);

/// 顶层语句允许 define 语句、claim 语句和表达式。
#[derive(Debug, Clone)]
pub enum GlobalStatemant<MetaInfo> {
    /// `(claim varname type)`
    Claim(Span, Identifier<MetaInfo>, SExpr<MetaInfo>),

    /// `(define varname expression)`
    Define(Span, Identifier<MetaInfo>, SExpr<MetaInfo>),
    Expression(SExpr<MetaInfo>),
}

/// 表达式包含位置信息和元信息（类型等）
#[derive(Debug, Clone)]
pub struct SExpr<MetaInfo> {
    pub span: Span,
    pub meta_info: MetaInfo,
    pub inner: SExprInner<MetaInfo>,
}

#[derive(Debug, Clone)]
pub enum SExprInner<MetaInfo> {
    /// 字面量，表示一个值
    Literal(Literal),
    /// 标识符，表示变量、函数、类型等
    Identifier(Identifier<MetaInfo>),
    /// 以下 4 项为无法通过自定义或特殊变量实现的语法项
    /// `(λ (ident+) expr)`，解析时转换为单层
    LambdaExpr {
        arg: Identifier<MetaInfo>,
        body: Ref<SExpr<MetaInfo>>,
    },
    /// `(Π ((ident expr)+) expr)`，解析时转换为单层
    PiExpr {
        arg: Identifier<MetaInfo>,
        arg_type: Ref<SExpr<MetaInfo>>,
        body: Ref<SExpr<MetaInfo>>,
    },
    /// `(Π ((ident expr)+) expr)`，解析时转换为单层
    /// 并把 `(→ expr+ expr)` 转换为 Π 表达式
    SigmaExpr {
        arg: Identifier<MetaInfo>,
        arg_type: Ref<SExpr<MetaInfo>>,
        body: Ref<SExpr<MetaInfo>>,
    },
    /// `(Σ ((ident expr)+) expr)`，解析时转换为单层
    TheExpr {
        ty: Ref<SExpr<MetaInfo>>,
        expr: Ref<SExpr<MetaInfo>>,
    },
    /// 函数调用或构造
    SList(Vec<SExpr<MetaInfo>>),
}

/// 字面量，目前有整数和原子
#[derive(Debug, Clone)]
pub enum Literal {
    Nat(u64),
    Atom(Ref<str>),
}

/// 标识符，`Dummy` 用于将普通函数类型转换为 Pi 类型时，
/// 未来或可用于 `_` 语法
#[derive(Debug, Clone)]
pub enum Identifier<MetaInfo> {
    Dummy,
    Identifier(Span, Ref<ScopeNode<MetaInfo>>),
}

/// 基于作用域的数据结构
#[derive(Debug, Clone)]
pub struct ScopeNode<MetaInfo> {
    pub varname: Ref<str>,
    pub meta_info: MetaInfo,
    pub outter: Option<Ref<ScopeNode<MetaInfo>>>,
}
