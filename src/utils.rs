use std::borrow::Borrow;
use std::cmp::Eq;

/// 引用类型，当前仅为 `std::rc::Rc`，未来或可使用 GC。
pub type Ref<T> = std::rc::Rc<T>;

/// 使用链表实现的层次索引数据结构，适合于表示嵌套作用域
#[derive(Debug, Clone, Default)]
pub struct StackMap<K, V>(Option<Ref<StackMapNode<K, V>>>);

#[derive(Debug, Clone)]
struct StackMapNode<K, V> {
    kv: (K, V),
    next: Option<Ref<StackMapNode<K, V>>>,
}

impl<K, V> StackMap<K, V> {
    pub fn new() -> StackMap<K, V> {
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
