use std::borrow::Borrow;
use std::cmp::Eq;
use std::fmt;
use thiserror::Error;

/// 引用类型，当前仅为 `std::rc::Rc`，未来或可使用 GC。
pub type Ref<T> = std::rc::Rc<T>;

/// 存储 De Bruijn index 的类型
pub type DBI = usize;

/// 在源代码中起始和结束位置，前闭后开
#[derive(Debug, Clone, Copy)]
pub struct Span(pub usize, pub usize);

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let Span(l, r) = self;
        write!(f, "{}:{}", l, r)
    }
}

/// 使用链表实现的层次索引数据结构，适合于表示嵌套作用域
#[derive(Debug, Clone, Default)]
pub struct StackMap<K, V>(Option<Ref<StackMapNode<K, V>>>);

#[derive(Debug, Clone)]
struct StackMapNode<K, V> {
    kv: (K, V),
    next: Option<Ref<StackMapNode<K, V>>>,
}

impl<K, V> StackMap<K, V> {
    pub const fn new() -> StackMap<K, V> {
        StackMap(None)
    }

    pub fn get<Q: ?Sized>(&self, x: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Eq,
    {
        let mut next = &self.0;
        while let Some(node) = next {
            if node.kv.0.borrow() == x {
                return Some(&node.kv.1);
            }
            next = &node.next;
        }
        None
    }

    pub fn insert(&self, k: K, v: V) -> StackMap<K, V> {
        StackMap(Some(Ref::new(StackMapNode {
            kv: (k, v),
            next: self.0.clone(),
        })))
    }

    pub fn iter(&self) -> StakMapIter<'_, K, V> {
        StakMapIter {
            curr: self.0.as_deref(),
        }
    }
}

impl<K, V> FromIterator<(K, V)> for StackMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut ret = StackMap::new();
        for (k, v) in iter {
            ret = ret.insert(k, v);
        }
        ret
    }
}

pub struct StakMapIter<'a, K, V> {
    curr: Option<&'a StackMapNode<K, V>>,
}

impl<'a, K, V> Iterator for StakMapIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        match self.curr {
            None => None,
            Some(StackMapNode { kv, next }) => {
                let (k, v) = kv;
                self.curr = next.as_ref().map(|n| &**n);
                Some((k, v))
            }
        }
    }
}

impl<K, V> fmt::Display for StackMap<K, V>
where
    K: fmt::Display,
    V: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(f, "{{")?;
        let mut node = &self.0;
        match node {
            Some(head) => {
                write!(f, "{}: {}", head.kv.0, head.kv.1)?;
                node = &head.next;
            }
            None => {}
        }
        while let Some(cur) = node {
            write!(f, " ,{}: {}", cur.kv.0, cur.kv.1)?;
            node = &cur.next;
        }
        write!(f, "}}")
    }
}

pub fn map_result<T, U, E>(
    it: impl IntoIterator<Item = T>,
    mut f: impl FnMut(T) -> Result<U, E>,
) -> Result<Vec<U>, E> {
    let mut v = Vec::new();
    for x in it.into_iter() {
        v.push(f(x)?);
    }
    Ok(v)
}

/// 带有位置信息的错误类型
#[derive(Debug, Clone, Error)]
pub struct LocatedError<ErrorKind>
where
    ErrorKind: fmt::Debug + fmt::Display,
{
    pub loc: Option<Span>,
    pub erk: ErrorKind,
}

impl<ErrorKind> fmt::Display for LocatedError<ErrorKind>
where
    ErrorKind: fmt::Debug + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self { loc: None, erk } => write!(f, "{}", erk),
            Self {
                loc: Some(span),
                erk,
            } => write!(f, "{}: {}", span, erk),
        }
    }
}

impl<ErrorKind> From<ErrorKind> for LocatedError<ErrorKind>
where
    ErrorKind: fmt::Debug + fmt::Display,
{
    fn from(erk: ErrorKind) -> Self {
        Self { loc: None, erk }
    }
}
