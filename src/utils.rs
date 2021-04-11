use std::borrow::Borrow;
use std::cmp::Eq;
use std::fmt;

/// 引用类型，当前仅为 `std::rc::Rc`，未来或可使用 GC。
pub type Ref<T> = std::rc::Rc<T>;

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
