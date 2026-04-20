/*! Data structures for reading input text. */
use crate::tex::catcodes::{CategoryCode, CategoryCodeScheme};
use std::fmt::{Debug, Display};

/** A single character in a `.tex` file; in plain TeX, this is a `u8`,
but in e.g. XeTeX, it is a UTF-8 character. */
pub trait Character:
    Sized
    + Eq
    + Copy
    + Display
    + Debug
    + From<u8>
    + TryInto<u8>
    + TryFrom<u64>
    + Into<u64>
    + Ord
    + std::hash::Hash
    + Default
    + 'static
{
    /// Type that maps characters to other data.
    type CharMap<A: Clone + Default>: CharacterMap<Self, A>;
    /// Iterator over characters in a string.
    type Iter<'a>: ExactSizeIterator<Item = Self>;
    /// minimal value of this type in numeric form (e.g. `0` for `u8`)
    const MIN: Self;
    /// maximal value of this type in numeric form (e.g. `255` for `u8`)
    const MAX: Self;
    /// Convert a line in a file/string (as a vector of bytes) into a [`Vec`] of [`Character`]s.
    fn convert(input: Vec<u8>) -> TextLine<Self>;

    fn slice_from_str<R>(s: &str, then: impl FnOnce(&[Self]) -> R) -> R;

    /// Display this character to a [`Write`](std::fmt::Write) (e.g. a `&mut String`). Relevant for e.g.
    /// TeX's convention to display control characters using `^^` encoding.
    fn display_fmt<W: std::fmt::Write>(&self, target: &mut W);

    /// Convert this character to a [`DisplayableCharacter`] that calls [`display`](Self::display_fmt); useful in
    /// `format!` and `write!` macros.

    fn display(&self) -> DisplayableCharacter<Self> {
        DisplayableCharacter(*self)
    }

    /// Convert this character to a `char`.
    fn to_char(&self) -> char;

    /// Like [`display`](Self::display), but for an [`Option`]`<`[`Character`]`>`. Useful for
    /// `format!` and `write!` macros specifically for the current `\ecapechar` (which may or may not be defined).

    fn display_opt(c: Option<Self>) -> DisplayableCharacterOpt<Self> {
        DisplayableCharacterOpt(c)
    }
    /// The starting [`CategoryCodeScheme`] for this character type.
    fn starting_catcode_scheme() -> CategoryCodeScheme<Self>;

    /// Convert a string to an iterator over characters.
    fn string_to_iter(string: &str) -> Self::Iter<'_>;
}

/// Helper structure to display a [`Character`] in a `format!` or `write!` macro.
pub struct DisplayableCharacter<C: Character>(C);
impl<C: Character> Display for DisplayableCharacter<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.display_fmt(f);
        Ok(())
    }
}

/// Helper structure to display an [`Option`]`<`[`Character`]`>` in a `format!` or `write!` macro (primarily
/// for the current `\escapechar`).
pub struct DisplayableCharacterOpt<C: Character>(Option<C>);
impl<C: Character> Display for DisplayableCharacterOpt<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(c) = self.0 {
            c.display_fmt(f)
        }
        Ok(())
    }
}

/** A map from characters `C:`[Character] to some other type `A`. For `u8`, we can simply use `[A;256]`.
   For example, a [`CategoryCodeScheme`] is a [`CharacterMap`]`<`[`CategoryCode`]`>`.
*/
pub trait CharacterMap<C: Character, A: Default>: Clone {
    fn get(&self, c: C) -> &A;
    fn get_mut(&mut self, c: C) -> &mut A;
    fn default() -> Self;
}

impl Character for u8 {
    type CharMap<A: Clone + Default> = [A; 256];
    const MIN: Self = 0;
    const MAX: Self = 255;

    fn to_char(&self) -> char {
        *self as char
    }

    type Iter<'a> = ByteIterator<'a>;

    fn slice_from_str<R>(s: &str, then: impl FnOnce(&[Self]) -> R) -> R {
        if s.contains("^^") {
            then(&Self::string_to_iter(s).collect::<Vec<_>>())
        } else {
            then(s.as_bytes())
        }
    }

    fn string_to_iter(string: &str) -> Self::Iter<'_> {
        ByteIterator(string.as_bytes())
    }

    fn convert(input: Vec<u8>) -> TextLine<Self> {
        input.into()
    }

    #[allow(unused_must_use)]
    fn display_fmt<W: std::fmt::Write>(&self, target: &mut W) {
        if self.is_ascii() {
            target.write_char(*self as char);
        } else if *self > 128 && (*self - 64).is_ascii() {
            target.write_str("^^");
            target.write_char((*self - 64) as char);
        } else {
            target.write_str(format!("^^{:x}", *self).as_str());
        }
    }

    fn starting_catcode_scheme() -> [CategoryCode; 256] {
        super::catcodes::STARTING_SCHEME_U8
    }
}

/// Iterator over bytes in a string, converting `^^` encoding to individual bytes (otherwise, we could
/// simply use `string.as_bytes().iter()`).
pub struct ByteIterator<'a>(&'a [u8]);
impl<'a> Iterator for ByteIterator<'a> {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            None
        } else if self.0.starts_with(b"^^") {
            let b = self.0[2];
            if b <= 60 || self.0.len() == 3 {
                self.0 = &self.0[3..];
                Some(b + 64)
            } else {
                let r =
                    u8::from_str_radix(std::str::from_utf8(&self.0[2..4]).unwrap(), 16).unwrap();
                self.0 = &self.0[4..];
                Some(r)
            }
        } else {
            let b = self.0[0];
            self.0 = &self.0[1..];
            Some(b)
        }
    }
}

impl ExactSizeIterator for ByteIterator<'_> {
    fn len(&self) -> usize {
        let mut num = 0usize;
        let mut iter = self.0.iter();
        while let Some(b) = iter.next() {
            if *b == b'^' {
                if let Some(b'^') = iter.next() {
                    if let Some(b) = iter.next() {
                        if *b <= 60 {
                            num += 1;
                        } else {
                            iter.next();
                            num += 1;
                        }
                    } else {
                        num += 1;
                    }
                } else {
                    num += 1;
                }
            } else {
                num += 1;
            }
        }
        num
    }
}

impl<A: Clone + Default> CharacterMap<u8, A> for [A; 256] {
    fn get(&self, c: u8) -> &A {
        &self[c as usize]
    }

    fn get_mut(&mut self, c: u8) -> &mut A {
        &mut self[c as usize]
    }
    fn default() -> Self {
        array_init::array_init(|_| A::default())
    }
}

/// A single line of characters.
pub type TextLine<C> = Box<[C]>;

/// A source of lines of characters, e.g. a file or a string.
pub trait TextLineSource<C: Character> {
    /// returns the next line of characters, or `None` if there are no more lines.
    fn get_line(&mut self) -> Option<TextLine<C>>;
}
/// A source of lines of characters generated from a string.
pub struct StringLineSource<C: Character> {
    pub lines: std::vec::IntoIter<TextLine<C>>,
}
impl<C: Character> StringLineSource<C> {
    /// Obtain the characters in a string: Takes a byte iterator as input and returns a vector of [`TextLine`]s;
    /// (split at `\n` or `\r\n`, removing trailing spaces).
    pub fn make_lines<I: Iterator<Item = u8>>(iter: I) -> Vec<TextLine<C>> {
        let mut lines = Vec::new();
        let mut curr = Vec::new();
        for b in iter {
            if b == b'\n' {
                if let Some(b'\r') = curr.last() {
                    curr.pop();
                }
                while let Some(b' ') = curr.last() {
                    curr.pop();
                }
                lines.push(C::convert(std::mem::take(&mut curr)));
            } else {
                curr.push(b);
            }
        }
        if !curr.is_empty() {
            lines.push(C::convert(curr));
        }
        lines
    }
}
impl<C: Character> From<Vec<TextLine<C>>> for StringLineSource<C> {
    fn from(lines: Vec<TextLine<C>>) -> Self {
        Self {
            lines: lines.into_iter(),
        }
    }
}
impl<C: Character> TextLineSource<C> for StringLineSource<C> {
    fn get_line(&mut self) -> Option<TextLine<C>> {
        self.lines.next()
    }
}
impl<C: Character> From<&str> for StringLineSource<C> {
    fn from(s: &str) -> Self {
        Self::make_lines(s.as_bytes().iter().copied()).into()
    }
}
impl<C: Character> From<String> for StringLineSource<C> {
    fn from(s: String) -> Self {
        Self::make_lines(s.into_bytes().into_iter()).into()
    }
}
