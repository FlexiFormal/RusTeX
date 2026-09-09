use crate::engine::Types;
use std::vec::IntoIter;
use tex_engine::prelude::{HNode, MathNode, VNode};
use tex_engine::tex::nodes::math::MathFontStyle;

#[derive(Debug, Clone)]
pub(crate) struct ExtensibleIter<T> {
    curr: IntoIter<T>,
    next: Vec<IntoIter<T>>,
}
impl<T> ExtensibleIter<T> {
    pub fn is_empty(&self) -> bool {
        self.curr.len() == 0 && self.next.iter().all(|i| i.len() == 0)
    }
    pub fn prefix(&mut self, vec: Vec<T>) {
        let old = std::mem::replace(&mut self.curr, vec.into_iter());
        self.next.push(old);
    }
    pub fn extend(&mut self, v: impl Into<Vec<T>>) {
        self.next.insert(0, v.into().into_iter());
    }
}
impl<T> From<Vec<T>> for ExtensibleIter<T> {
    fn from(v: Vec<T>) -> Self {
        Self {
            curr: v.into_iter(),
            next: Vec::new(),
        }
    }
}
impl<T> From<Box<[T]>> for ExtensibleIter<T> {
    fn from(v: Box<[T]>) -> Self {
        Self {
            curr: v.into_vec().into_iter(),
            next: Vec::new(),
        }
    }
}
impl<T> DoubleEndedIterator for ExtensibleIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if let Some(it) = self.next.first_mut() {
            if let Some(n) = it.next_back() {
                return Some(n);
            }
            self.next.remove(0);
            return self.next_back();
        }
        self.curr.next_back()
    }
}
impl<T> Iterator for ExtensibleIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(n) = self.curr.next() {
            return Some(n);
        }
        while let Some(mut it) = self.next.pop() {
            if let Some(n) = it.next() {
                self.curr = it;
                return Some(n);
            }
        }
        None
    }
}

pub(crate) type VNodes = ExtensibleIter<VNode<Types>>;
pub(crate) type HNodes = ExtensibleIter<HNode<Types>>;
pub(crate) type MNodes = ExtensibleIter<MNode>;
pub(crate) type MNode = MathNode<Types, MathFontStyle<Types>>;
