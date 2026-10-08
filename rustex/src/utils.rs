use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::ops::{Add, AddAssign};
use tex_engine::tex::numerics::{Dim32, Skip, StretchShrink};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FlexMagnitude {
    Fixed = 0,
    Fil = 1,
    Fill = 2,
    Filll = 3,
}
impl PartialOrd for FlexMagnitude {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        let s = *self as u8;
        let o = *other as u8;
        s.partial_cmp(&o)
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub(crate) enum Flex {
    Fixed(i32),
    Fil(i32),
    Fill(i32),
    Filll(i32),
}
impl Default for Flex {
    fn default() -> Self {
        Self::Fixed(0)
    }
}
impl Flex {
    pub const fn is_zero(self) -> bool {
        matches!(self, Self::Fixed(0))
    }
    pub const fn magnitude(self) -> FlexMagnitude {
        match self {
            Self::Fixed(_) => FlexMagnitude::Fixed,
            Self::Fil(_) => FlexMagnitude::Fil,
            Self::Fill(_) => FlexMagnitude::Fill,
            Self::Filll(_) => FlexMagnitude::Filll,
        }
    }
}
impl PartialOrd for Flex {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering as O;
        match (self, other) {
            (Self::Fixed(a), Self::Fixed(b)) => a.partial_cmp(b),
            (Self::Fixed(_), _) => Some(O::Less),
            (_, Self::Fixed(_)) => Some(O::Greater),
            (Self::Fil(a), Self::Fil(b)) => a.partial_cmp(b),
            (Self::Fil(_), _) => Some(O::Less),
            (_, Self::Fil(_)) => Some(O::Greater),
            (Self::Fill(a), Self::Fill(b)) => a.partial_cmp(b),
            (Self::Fill(_), _) => Some(O::Less),
            (_, Self::Fill(_)) => Some(O::Greater),
            (Self::Filll(a), Self::Filll(b)) => a.partial_cmp(b),
        }
    }
}
impl Add for Flex {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        match (self, other) {
            (Self::Fixed(a), Self::Fixed(b)) => Self::Fixed(a + b),
            (Self::Fil(a), Self::Fil(b)) => Self::Fil(a + b),
            (Self::Fill(a), Self::Fill(b)) => Self::Fill(a + b),
            (Self::Filll(a), Self::Filll(b)) => Self::Filll(a + b),
            (Self::Fixed(_), Self::Fil(b)) => Self::Fil(b),
            (Self::Fil(a), Self::Fixed(_)) => Self::Fil(a),
            (Self::Fixed(_) | Self::Fil(_), Self::Fill(b)) => Self::Fill(b),
            (Self::Fill(a), Self::Fixed(_) | Self::Fil(_)) => Self::Fill(a),
            (_, Self::Filll(b)) => Self::Filll(b),
            (Self::Filll(a), _) => Self::Filll(a),
        }
    }
}
impl From<StretchShrink<Dim32>> for Flex {
    fn from(ss: StretchShrink<Dim32>) -> Self {
        match ss {
            StretchShrink::Dim(d) => Self::Fixed(d.0),
            StretchShrink::Fil(d) if d != 0 => Self::Fil(d),
            StretchShrink::Fill(d) if d != 0 => Self::Fill(d),
            StretchShrink::Filll(d) if d != 0 => Self::Filll(d),
            _ => Self::Fixed(0),
        }
    }
}
#[derive(Copy, Clone)]
pub(crate) struct Margin {
    pub(crate) base: i32,
    pub(crate) stretch: Flex,
    pub(crate) shrink: Flex,
}
impl Debug for Margin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "<skip {}", self.base)?;
        if !self.stretch.is_zero() {
            write!(f, " plus {:?}", self.stretch)?;
        }
        if !self.shrink.is_zero() {
            write!(f, " minus {:?}", self.shrink)?;
        }
        f.write_str(">")
    }
}
impl Margin {
    pub(crate) const fn fil() -> Self {
        Self {
            base: 0,
            stretch: Flex::Fil(65536),
            shrink: Flex::Fixed(0),
        }
    }
    pub(crate) const fn fill() -> Self {
        Self {
            base: 0,
            stretch: Flex::Fill(65536),
            shrink: Flex::Fixed(0),
        }
    }
    /*pub(crate) const fn filll() -> Self {
        Self {
            base: 0,
            stretch: Flex::Filll(1),
            shrink: Flex::Fixed(0),
        }
    }*/
    pub(crate) const fn ss() -> Self {
        Self {
            base: 0,
            stretch: Flex::Fil(65536),
            shrink: Flex::Fil(65536),
        }
    }
    pub(crate) const fn is_zero(&self) -> bool {
        self.base == 0 && self.stretch.is_zero() && self.shrink.is_zero()
    }
}
impl Add for Margin {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            base: self.base + other.base,
            stretch: self.stretch + other.stretch,
            shrink: self.shrink + other.shrink,
        }
    }
}
impl From<Skip<Dim32>> for Margin {
    fn from(skip: Skip<Dim32>) -> Self {
        let base = skip.base.0;
        let stretch = skip.stretch.map(|s| s.into()).unwrap_or_default();
        let shrink = skip.shrink.map(|s| s.into()).unwrap_or_default();
        Self {
            base,
            stretch,
            shrink,
        }
    }
}
impl From<Dim32> for Margin {
    fn from(dim: Dim32) -> Self {
        Self {
            base: dim.0,
            stretch: Flex::Fixed(0),
            shrink: Flex::Fixed(0),
        }
    }
}
impl AddAssign for Margin {
    fn add_assign(&mut self, other: Self) {
        self.base += other.base;
        self.stretch = self.stretch + other.stretch;
        self.shrink = self.shrink + other.shrink;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VecSet<K: PartialEq> {
    pub inner: Vec<K>,
}
impl<K: PartialEq> Default for VecSet<K> {
    #[inline]
    fn default() -> Self {
        Self { inner: Vec::new() }
    }
}
impl<K: PartialEq> VecSet<K> {
    #[inline]
    pub fn contains(&self, key: &K) -> bool {
        self.inner.contains(key)
    }
    #[inline]
    pub fn insert(&mut self, key: K) {
        if !self.inner.contains(&key) {
            self.inner.push(key);
        }
    }
}
impl<K: PartialEq> IntoIterator for VecSet<K> {
    type Item = K;
    type IntoIter = std::vec::IntoIter<K>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}
impl<K: PartialEq> FromIterator<K> for VecSet<K> {
    fn from_iter<T: IntoIterator<Item = K>>(iter: T) -> Self {
        let mut set = VecSet::default();
        for item in iter {
            set.insert(item);
        }
        set
    }
}

#[derive(Clone)]
pub struct VecMap<K, V> {
    pub inner: Vec<(K, V)>,
}
impl<K: Debug, V: Debug> Debug for VecMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.inner.iter().map(|(k, v)| (k, v)))
            .finish()
    }
}
impl<K, V> Default for VecMap<K, V> {
    fn default() -> Self {
        Self { inner: Vec::new() }
    }
}
impl<K: PartialEq, V> VecMap<K, V> {
    pub fn get<E: ?Sized>(&self, key: &E) -> Option<&V>
    where
        for<'a> &'a E: PartialEq<&'a K>,
    {
        self.inner.iter().find(|(k, _)| key == k).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.inner
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }
    pub fn insert(&mut self, key: K, value: V) {
        match self.inner.iter_mut().find(|(k, _)| k == &key) {
            Some((_, v)) => *v = value,
            None => self.inner.push((key, value)),
        };
    }
    pub fn get_or_insert_mut(&mut self, key: K, value: impl FnOnce() -> V) -> &mut V {
        if let Some((i, (_, v))) = self.inner.iter().enumerate().find(|(_, (k, _))| k == &key) {
            return &mut self.inner.get_mut(i).unwrap().1;
        };
        let value = value();
        self.inner.push((key, value));
        &mut self.inner.last_mut().unwrap().1
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.inner.iter().map(|(k, v)| (k, v))
    }
    pub fn contains_key<E: ?Sized>(&self, key: &E) -> bool
    where
        for<'a> &'a E: PartialEq<&'a K>,
    {
        self.inner.iter().any(|(k, _)| key == k)
    }
}
impl<K, V> IntoIterator for VecMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}
impl<'a, 'b> Into<VecMap<Cow<'a, str>, Cow<'b, str>>> for VecMap<String, String> {
    fn into(self) -> VecMap<Cow<'a, str>, Cow<'b, str>> {
        VecMap {
            inner: self
                .inner
                .into_iter()
                .map(|(k, v)| (Cow::Owned(k), Cow::Owned(v)))
                .collect(),
        }
    }
}
impl<S, T> From<Vec<(S, T)>> for VecMap<S, T> {
    #[inline]
    fn from(value: Vec<(S, T)>) -> Self {
        Self { inner: value }
    }
}
