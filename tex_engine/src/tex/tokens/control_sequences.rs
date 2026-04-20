/*! A control sequence is a [`Token`](super::Token) of the (usually) form `\foo`.
    We can just use strings to represent them, but there is room for optimization by e.g. interning
    them, which requires some more infrastructure to intern and resolve them.

    We implement the trait [`CSHandler`] for the Unit type `()`, in case
    we don't want to do any interning and just use [`Ptr`]`<str>`s.
*/

use crate::prelude::{CategoryCode, CategoryCodeScheme};
use crate::tex::characters::Character;
use crate::tex::characters::CharacterMap;
use crate::utils::{HMap, Ptr};
use std::fmt::{Debug, Display, Write};
use std::hash::Hash;
use std::marker::PhantomData;
use std::num::NonZeroU32;

/** The name of a control sequence. */
pub trait CSName<C: Character>: Clone + Eq + 'static + std::hash::Hash + Debug {
    /// The type of the handler for this control sequence name.
    type Handler: CSHandler<C, Self>;
    /// The type used for mapping control sequence names to e.g. [`Command`](crate::commands::TeXCommand)s.
    type Map<A>: CSNameMap<C, Self, A>
    where
        A: Clone;
    fn display_fmt<W: Write>(
        &self,
        int: &Self::Handler,
        cc: &CategoryCodeScheme<C>,
        escapechar: Option<C>,
        f: &mut W,
    ) -> std::fmt::Result {
        let res = int.resolve(self);
        write!(f, "{}{}", C::display_opt(escapechar), res)?;
        if res.len() == 1 {
            let c = res.iter().next().unwrap();
            match cc.get(c) {
                CategoryCode::Letter => write!(f, " "),
                _ => Ok(()),
            }
        } else {
            write!(f, " ")
        }
    }
    fn id(&self) -> usize;
}

/// How to map control sequence names to e.g. [`Command`](crate::commands::TeXCommand)s.
pub trait CSNameMap<C: Character, CS: CSName<C>, A: Clone>: Clone + Default {
    /// Returns the value associated with the given control sequence name, if any.
    fn get(&self, cs: &CS) -> Option<&A>;
    /// Inserts a new value for the given control sequence name, returning the old value if any.
    fn insert(&mut self, cs: CS, a: A) -> Option<A>;
    /// Removes the value associated with the given control sequence name, returning it if any.
    fn remove(&mut self, cs: &CS) -> Option<A>;
    fn into_iter(self) -> impl Iterator<Item = (CS, A)>;
}

impl<C: Character, CS: CSName<C>, A: Clone> CSNameMap<C, CS, A> for HMap<CS, A> {
    fn get(&self, cs: &CS) -> Option<&A> {
        self.get(cs)
    }

    fn insert(&mut self, cs: CS, a: A) -> Option<A> {
        self.insert(cs, a)
    }

    fn remove(&mut self, cs: &CS) -> Option<A> {
        self.remove(cs)
    }
    fn into_iter(self) -> impl Iterator<Item = (CS, A)> {
        <HMap<CS, A> as IntoIterator>::into_iter(self)
    }
}

impl<C: Character> CSName<C> for Ptr<str> {
    type Handler = ();
    type Map<A>
        = HMap<Self, A>
    where
        A: Clone;
    fn id(&self) -> usize {
        use std::hash::Hasher;
        let mut hash = std::hash::DefaultHasher::new();
        self.hash(&mut hash);
        hash.finish() as usize
    }
}

/// A control sequence name that has been interned.
/// Uses *consecutive* `u32` values
pub type InternedCSName<C> = (NonZeroU32, PhantomData<C>);
impl<C: Character> CSName<C> for InternedCSName<C> {
    type Handler = CSInterner<C>;
    type Map<A>
        = CSNameVec<C, A>
    where
        A: Clone;
    fn id(&self) -> usize {
        self.0.get() as usize - 1
    }
}

/// A [`CSNameMap`] that uses a [`Vec`] to store the values for [`InternedCSName`].
/// Since the [`InternedCSName`]s are consecutive, we can use a [`Vec`] instead of a [`HMap`].
#[derive(Clone)]
pub struct CSNameVec<C: Character, A: Clone>(Vec<Option<A>>, PhantomData<C>);
impl<C: Character, A: Clone> Default for CSNameVec<C, A> {
    fn default() -> Self {
        Self(Vec::new(), PhantomData)
    }
}

impl<C: Character, A: Clone> CSNameMap<C, InternedCSName<C>, A> for CSNameVec<C, A> {
    fn get(&self, cs: &InternedCSName<C>) -> Option<&A> {
        self.0.get(cs.0.get() as usize - 1).and_then(|x| x.as_ref())
    }
    fn insert(&mut self, cs: InternedCSName<C>, a: A) -> Option<A> {
        let idx = cs.0.get() as usize - 1;
        if self.0.len() <= idx {
            self.0.resize(idx + 1, None);
        }
        std::mem::replace(&mut self.0[idx], Some(a))
    }
    fn remove(&mut self, cs: &InternedCSName<C>) -> Option<A> {
        let idx = cs.0.get() as usize - 1;
        if self.0.len() <= idx {
            return None;
        }
        self.0[idx].take()
    }
    fn into_iter(self) -> impl Iterator<Item = (InternedCSName<C>, A)> {
        self.0.into_iter().enumerate().filter_map(|(i, x)| {
            x.map(|x| ((NonZeroU32::new((i + 1) as u32).unwrap(), PhantomData), x))
        })
    }
}

/// A control sequence name that has been interned needs to be resolved again to
/// get the actual name / display it etc.
pub trait ResolvedCSName<'a, C: Character>: Display {
    /// The type of the iterator over the characters of the control sequence name.
    type Iter: Iterator<Item = C>;
    /// Returns an iterator over the characters of the control sequence name.
    fn iter(&self) -> Self::Iter;
    /// Returns the length of the control sequence name in terms of the underlying [`Character`] type.
    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/** Handles control sequence names - conversion from/to strings, displaying etc. */
pub trait CSHandler<C: Character, CS: CSName<C>>: Default + Clone {
    /// The type of the resolved control sequence name (for displaying / iterating over the underlying
    /// [`Character`]s.
    type Resolved<'a>: ResolvedCSName<'a, C>
    where
        Self: 'a;
    /// Creates a new control sequence name from a string.
    fn cs_from_str(&mut self, s: &str) -> CS;
    /// Creates a new control sequence name from a vector of characters.
    fn cs_from_chars(&mut self, v: &[C]) -> CS;
    /// Resolves a control sequence name.
    fn resolve<'a>(&'a self, cs: &'a CS) -> Self::Resolved<'a>;
    /// Returns the name of the `\par` control sequence.
    fn par(&self) -> CS;
    /// Returns the name of the empty control sequence.
    fn empty_str(&self) -> CS;
    /// Creates a control sequence name from a string slice iff it is a previously interned control sequence name.
    fn get(&self, s: &str) -> Option<CS>;
}

impl<'a, C: Character> ResolvedCSName<'a, C> for &'a str {
    type Iter = C::Iter<'a>;
    fn iter(&self) -> Self::Iter {
        C::string_to_iter(self)
    }
    fn len(&self) -> usize {
        C::string_to_iter(self).len()
    }
}

impl<C: Character> CSHandler<C, Ptr<str>> for () {
    type Resolved<'a> = &'a str;

    fn cs_from_str(&mut self, s: &str) -> Ptr<str> {
        s.into()
    }
    fn get(&self, s: &str) -> Option<Ptr<str>> {
        Some(s.into())
    }
    fn resolve<'a>(&'a self, cs: &'a Ptr<str>) -> Self::Resolved<'a> {
        cs
    }
    fn par(&self) -> Ptr<str> {
        "par".to_string().into()
    }
    fn empty_str(&self) -> Ptr<str> {
        "".to_string().into()
    }
    fn cs_from_chars(&mut self, v: &[C]) -> Ptr<str> {
        let mut s = String::new();
        for c in v {
            c.display_fmt(&mut s);
        }
        s.into()
    }
}

/// A [`CSHandler`] that interns control sequence names as `u32`.
#[derive(Clone)]
pub struct CSInterner<C: Character> {
    map: HMap<Box<[C]>, NonZeroU32>,
    ls: Vec<C>,
    idx: Vec<usize>,
}
impl<C: Character> CSInterner<C> {
    #[allow(dead_code)]
    fn cap(&self) -> usize {
        self.idx.len()
    }
    fn new() -> Self {
        let mut map: HMap<Box<[C]>, NonZeroU32> = HMap::default();
        map.insert(Box::new([]), NonZeroU32::new(1).unwrap());
        let mut r = CSInterner {
            map,
            ls: Vec::new(),
            idx: vec![0],
        };
        r.from_static("par");
        r
    }
    /// Interns a `&'static str` as a control sequence name
    pub fn from_static(&mut self, s: &'static str) -> InternedCSName<C> {
        C::slice_from_str(s, |s| self.intern(s))
    }
    /// Interns a `String` as a control sequence name
    pub fn from_string<S: AsRef<str>>(&mut self, s: S) -> InternedCSName<C> {
        C::slice_from_str(s.as_ref(), |s| self.intern(s))
    }
    /// Resolves a control sequence name to a sequence of [`Character`]s
    pub fn resolve(&self, i: InternedCSName<C>) -> &[C] {
        self.get(i.0)
    }

    pub fn intern(&mut self, v: &[C]) -> InternedCSName<C> {
        if let Some(x) = self.map.get(v) {
            return (*x, PhantomData);
        }
        self.ls.extend(v);
        let len = self.ls.len();
        self.idx.push(len);
        let len = self.idx.len() - 1;
        // SAFETY: len + 1 > 0
        let r = unsafe { NonZeroU32::new_unchecked(len as u32 + 1) };
        self.map.insert(v.into(), r);
        (r, PhantomData)
    }

    fn get(&self, i: NonZeroU32) -> &[C] {
        let i = i.get() as usize - 1;
        if i == 0 {
            return &[];
        }
        let s = self.idx[i - 1];
        let e = self.idx[i];
        &self.ls[s..e]
    }
}
impl<C: Character> CSHandler<C, InternedCSName<C>> for CSInterner<C> {
    type Resolved<'a> = DisplayCSName<'a, C>;
    fn cs_from_str(&mut self, s: &str) -> InternedCSName<C> {
        C::slice_from_str(s, |s| self.intern(s))
        //self.intern(C::string_to_iter(s).collect::<Vec<_>>().as_slice())
    }
    fn get(&self, s: &str) -> Option<InternedCSName<C>> {
        C::slice_from_str(s, |s| self.map.get(s).map(|i| (*i, PhantomData)))
    }
    fn cs_from_chars(&mut self, v: &[C]) -> InternedCSName<C> {
        self.intern(v)
    }
    fn par(&self) -> InternedCSName<C> {
        // SAFETY: 2 > 0
        const { (NonZeroU32::new(2).unwrap(), PhantomData) }
    }
    fn empty_str(&self) -> InternedCSName<C> {
        // SAFETY: 1 > 0
        const { (NonZeroU32::new(1).unwrap(), PhantomData) }
    }
    fn resolve<'a>(&'a self, cs: &InternedCSName<C>) -> DisplayCSName<'a, C> {
        DisplayCSName(self.get(cs.0))
    }
}
/// Utility struct for displaying a control sequence name.
pub struct DisplayCSName<'a, C: Character>(&'a [C]);
impl<C: Character> Display for DisplayCSName<'_, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.0 {
            c.display_fmt(f);
        }
        Ok(())
    }
}

impl<'a, C: Character> ResolvedCSName<'a, C> for DisplayCSName<'a, C> {
    type Iter = std::iter::Copied<std::slice::Iter<'a, C>>;

    fn iter(&self) -> Self::Iter {
        self.0.iter().copied()
    }

    fn len(&self) -> usize {
        self.0.len()
    }
}
impl<C: Character> Default for CSInterner<C> {
    fn default() -> Self {
        Self::new()
    }
}
