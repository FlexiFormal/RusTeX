use crate::prelude::{
    CSHandler, CSName, CategoryCode, CategoryCodeScheme, Character, CommandCode, Token,
};
use crate::tex::characters::CharacterMap;
use crate::tex::tokens::control_sequences::ResolvedCSName;
use crate::tex::tokens::StandardToken;
use std::fmt::{Arguments, Display, Write};
use std::marker::PhantomData;

/// A list of [`Token`]s; conceptually, a wrapper around `Rc<[T]>`
#[cfg(not(feature = "multithreaded"))]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenList<T: Token>(pub shared_vector::SharedVector<T>);

#[cfg(feature = "multithreaded")]
#[derive(Clone, Debug, PartialEq)]
pub struct TokenList<T: Token>(pub shared_vector::AtomicSharedVector<T>);
impl<T: Token> TokenList<T> {
    /// Whether the list is empty

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// return the `i`th token in the list. Will panic if out of bounds.

    pub fn get(&self, i: usize) -> &T {
        &(*self.0)[i]
    }

    /// wraps this list in a [`TokenListDisplay`], which implements [`Display`].
    /// If `double_par` is true, parameter tokens will be doubled.
    pub fn display<'a>(
        &'a self,
        int: &'a <T::CS as CSName<T::Char>>::Handler,
        cc: &'a CategoryCodeScheme<T::Char>,
        escapechar: Option<T::Char>,
        double_par: bool,
    ) -> TokenListDisplay<'a, T> {
        TokenListDisplay {
            ls: self.0.as_slice(),
            int,
            cc,
            escapechar,
            double_par,
        }
    }
}
impl<T: Token> FromIterator<T> for TokenList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = shared_vector::Vector::new();
        for t in iter {
            v.push(t);
        }
        Self(v.into())
    }
}

/// A helper struct that implements [`Display`] for [`TokenList`]. Needs a [`CSHandler`] and a [`CategoryCodeScheme`]
/// to resolve control sequences and insert spaces between them properly.
pub struct TokenListDisplay<'a, T: Token> {
    ls: &'a [T],
    int: &'a <T::CS as CSName<T::Char>>::Handler,
    cc: &'a CategoryCodeScheme<T::Char>,
    escapechar: Option<T::Char>,
    double_par: bool,
}
impl<'a, T: Token> TokenListDisplay<'a, T> {
    /// Creates a new [`TokenListDisplay`] from a [`Vec`] of [`Token`]s.
    /// If `double_par` is true, parameter tokens will be doubled.
    pub fn from_vec(
        v: &'a Vec<T>,
        int: &'a <T::CS as CSName<T::Char>>::Handler,
        cc: &'a CategoryCodeScheme<T::Char>,
        escapechar: Option<T::Char>,
        double_par: bool,
    ) -> Self {
        Self {
            ls: v.as_slice(),
            int,
            cc,
            escapechar,
            double_par,
        }
    }
    /// allows for writing the tokens directly to a [`CharWrite`]; potentially circumventing the need to
    /// convert it to a string only to convert it back to tokents
    pub fn fmt_cw<W: CharWrite<T::Char, T::CS>>(&self, f: &mut W) -> std::fmt::Result {
        for t in self.ls.iter() {
            match t.is_argument_marker() {
                Some(i) => write!(f, "#{}", (i + 1))?,
                _ => match t.to_enum() {
                    StandardToken::Character(c, CommandCode::Parameter) if self.double_par => {
                        f.push_char(c);
                        f.push_char(c)
                    }
                    _ => f.push_tk(t, self.int, self.cc, self.escapechar), //o.display_fmt(self.int,self.cc,self.escapechar,f)?
                },
            }
        }
        Ok(())
    }
}

impl<'a, T: Token> Display for TokenListDisplay<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_cw(&mut StringCharWrite::new(f))
    }
}

/// An extension of [`Write`] that can handle [`Character`]s and [`CSName`]s
/// (and hence [`Token`]s) directly. Useful, since it allows for
/// directly turning strings into [`TokenList`]s using `write!`, `format!` etc.
pub trait CharWrite<C: Character, CS: CSName<C>>: std::fmt::Write {
    /// Pushes a [`Character`] to the underlying writer.
    fn push_char(&mut self, c: C);
    /// Pushes a [`CSName`] to the underlying writer.
    fn push_cs<I: CSHandler<C, CS>>(
        &mut self,
        cs: CS,
        int: &I,
        cc: &CategoryCodeScheme<C>,
        esc: Option<C>,
    );
    /// Pushes a [`Token`] to the underlying writer.
    fn push_tk<T: Token<Char = C, CS = CS>>(
        &mut self,
        t: &T,
        int: &<T::CS as CSName<T::Char>>::Handler,
        cc: &CategoryCodeScheme<T::Char>,
        escapechar: Option<T::Char>,
    ) {
        match t.to_enum() {
            StandardToken::Character(c, _) => self.push_char(c),
            StandardToken::ControlSequence(cs) => self.push_cs(cs, int, cc, escapechar),
            StandardToken::Primitive(id) => self.push_cs(
                int.get(&id.display::<C>(None).to_string())
                    .expect("Something went wrong"),
                int,
                cc,
                escapechar,
            ),
        }
    }
}
impl<'a, C: Character, CS: CSName<C>, A: CharWrite<C, CS>> CharWrite<C, CS> for &'a mut A {
    fn push_char(&mut self, c: C) {
        (*self).push_char(c)
    }
    fn push_cs<I: CSHandler<C, CS>>(
        &mut self,
        cs: CS,
        int: &I,
        cc: &CategoryCodeScheme<C>,
        esc: Option<C>,
    ) {
        (*self).push_cs(cs, int, cc, esc)
    }
}

/// Wrapper struct that adds [`CharWrite`] to any [`Write`]
pub struct StringCharWrite<'a, W: Write, C: Character, CS: CSName<C>>(
    &'a mut W,
    PhantomData<C>,
    PhantomData<CS>,
);
impl<'a, W: Write, C: Character, CS: CSName<C>> StringCharWrite<'a, W, C, CS> {
    pub fn new(f: &'a mut W) -> Self {
        Self(f, PhantomData, PhantomData)
    }
}
impl<'a, W: Write, C: Character, CS: CSName<C>> std::fmt::Write for StringCharWrite<'a, W, C, CS> {
    fn write_char(&mut self, c: char) -> std::fmt::Result {
        self.0.write_char(c)
    }

    fn write_fmt(&mut self, args: Arguments<'_>) -> std::fmt::Result {
        self.0.write_fmt(args)
    }

    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.write_str(s)
    }
}
impl<'a, W: Write, C: Character, CS: CSName<C>> CharWrite<C, CS> for StringCharWrite<'a, W, C, CS> {
    fn push_char(&mut self, c: C) {
        c.display_fmt(self.0)
    }
    fn push_cs<I: CSHandler<C, CS>>(
        &mut self,
        cs: CS,
        int: &I,
        cc: &CategoryCodeScheme<C>,
        esc: Option<C>,
    ) {
        let res = int.resolve(&cs);
        write!(self, "{}{}", C::display_opt(esc), res).unwrap();
        if res.len() == 1 {
            let c = res.iter().next().unwrap();
            if cc.get(c) == &CategoryCode::Letter {
                self.write_char(' ').unwrap()
            }
        } else {
            self.write_char(' ').unwrap()
        }
    }
}

/// Struct that allows to `write!` and `format!` by converting the string to
/// [`Token`]s and passes them to a closure. All tokens have [`CommandCode::Other`]
/// except for space characters.
/// For example, `write!(Tokenizer::new(|t| vec.push(t), "ab c")` will
/// push four tokens to `vec`, where the first, second and fourth have
/// [`CommandCode::Other`] and the third has [`CommandCode::Space`].
pub struct Otherize<'a, T: Token, F: FnMut(T)>(&'a mut F, PhantomData<T>);
impl<'a, T: Token, F: FnMut(T)> Otherize<'a, T, F> {
    /// Creates a new [`Otherize`] from a closure.

    pub fn new(f: &'a mut F) -> Self {
        Self(f, PhantomData)
    }
}

impl<'a, T: Token, F: FnMut(T)> CharWrite<T::Char, T::CS> for Otherize<'a, T, F> {
    fn push_char(&mut self, c: T::Char) {
        if matches!(c.try_into(), Ok(b' ')) {
            (self.0)(T::space())
        } else {
            (self.0)(T::from_char_cat(c, CommandCode::Other))
        }
    }
    fn push_cs<I: CSHandler<T::Char, T::CS>>(
        &mut self,
        cs: T::CS,
        int: &I,
        cc: &CategoryCodeScheme<T::Char>,
        esc: Option<T::Char>,
    ) {
        if let Some(e) = esc {
            (self.0)(T::from_char_cat(e, CommandCode::Other));
        }
        let res = int.resolve(&cs);
        for c in res.iter() {
            if matches!(c.try_into(), Ok(b' ')) {
                (self.0)(T::space());
            } else {
                (self.0)(T::from_char_cat(c, CommandCode::Other));
            }
        }
        if res.len() == 1 {
            let c = res.iter().next().unwrap();
            if cc.get(c) == &CategoryCode::Letter {
                (self.0)(T::space())
            }
        } else {
            (self.0)(T::space())
        }
    }
}
impl<'a, T: Token, F: FnMut(T)> std::fmt::Write for Otherize<'a, T, F> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for u in T::Char::string_to_iter(s) {
            if matches!(u.try_into(), Ok(b' ')) {
                (self.0)(T::space())
            } else {
                (self.0)(T::from_char_cat(u, CommandCode::Other))
            }
        }
        Ok(())
    }
}

impl<T: Token> From<shared_vector::Vector<T>> for TokenList<T> {
    fn from(value: shared_vector::Vector<T>) -> Self {
        Self(value.into())
    }
}

/// A [`MacroExpansion`] bundles the [`TokenList`] of a macro with its arguments.
pub struct MacroExpansion<T: Token> {
    pub ls: TokenList<T>,
    index: usize,
    currarg: Option<(usize, usize)>,
    pub args: [Vec<T>; 9],
}
impl<T: Token> MacroExpansion<T> {
    /// Creates a new [`MacroExpansion`] from a [`TokenList`] and a list of arguments.
    pub fn new(ls: TokenList<T>, args: [Vec<T>; 9]) -> Self {
        Self {
            ls,
            index: 0,
            currarg: None,
            args,
        }
    }

    /// useful for debugging: prints the expansion from the current index to a [`Write`]
    pub fn preview<W: Write>(
        &self,
        int: &<T::CS as CSName<T::Char>>::Handler,
        cc: &CategoryCodeScheme<T::Char>,
        escapechar: Option<T::Char>,
        mut w: W,
    ) {
        let mut currarg = self.currarg;
        let mut index = self.index;
        loop {
            match currarg {
                Some((i, j)) if j < self.args[i].len() => {
                    self.args[i][j]
                        .display_fmt(int, cc, escapechar, &mut w)
                        .unwrap();
                    currarg = Some((i, j + 1));
                }
                Some(_) => currarg = None,
                None if index < self.ls.0.len() => {
                    let t = self.ls.get(index);
                    index += 1;
                    match t.is_argument_marker() {
                        Some(i) => currarg = Some((i.into(), 0)),
                        _ => t.display_fmt(int, cc, escapechar, &mut w).unwrap(),
                    }
                }
                _ => return,
            }
        }
    }
}
impl<T: Token> Iterator for MacroExpansion<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((i, j)) = self.currarg {
                if let Some(t) = self.args[i].get(j) {
                    self.currarg = Some((i, j + 1));
                    return Some(t.clone());
                } else {
                    self.currarg = None;
                }
            }
            match self.ls.0.get(self.index) {
                None => return None,
                Some(t) => {
                    self.index += 1;
                    if let Some(i) = t.is_argument_marker() {
                        self.currarg = Some((i.into(), 0));
                    } else {
                        return Some(t.clone());
                    }
                }
            }
        }
    }
}
