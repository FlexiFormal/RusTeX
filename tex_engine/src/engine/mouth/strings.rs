use crate::prelude::*;
use crate::tex::characters::{TextLine, TextLineSource};
use crate::utils::errors::InvalidCharacter;

/// An [`InputTokenizer`] is in one of three states
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum MouthState {
    /// Beginning of line
    NewLine,
    /// After a space (or control sequence)
    SkipBlank,
    /// In the middle of a line
    MidLine,
}

/** Takes a [`TextLineSource`] and lazily turns it into [`Token`]s, given a [`CategoryCodeScheme`] and an optional
    end-of-line [`Character`]. The primary use case is to process an input `.tex` file.

  *Example:*
```rust
# use tex_engine::utils::errors::ErrorThrower;
# use tex_engine::engine::mouth::strings::InputTokenizer;
# use tex_engine::tex::tokens::{Token,StandardToken};
# use tex_engine::tex::catcodes::DEFAULT_SCHEME_U8;
# use tex_engine::utils::Ptr;
# use tex_engine::tex::catcodes::CommandCode;
# use tex_engine::tex::characters::StringLineSource;
# use tex_engine::prelude::CSHandler;
#
type T = StandardToken<u8,Ptr<str>>;
let mut cs_handler = ();
let cc = &DEFAULT_SCHEME_U8;

let string = "\\foo   \n  \n   {a}{!}";
let input: StringLineSource<u8> = string.into();
let mut tokenizer = InputTokenizer::new(input);
let eol = Some(b'\r');
let par = CSHandler::<u8,Ptr<str>>::par(&cs_handler);
let next = tokenizer.get_next(&mut cs_handler,cc,None,&par); // \foo
assert!(matches!(next,Ok(Some(T::ControlSequence(s))) if &*s == "foo"));
let next = tokenizer.get_next(&mut cs_handler,cc,eol,&par); // \par
assert!(matches!(next,Ok(Some(T::ControlSequence(s))) if &*s == "par"));
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // {
assert_eq!(next.command_code(), CommandCode::BeginGroup);
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // a
assert_eq!(next.command_code(), CommandCode::Letter);
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // }
assert_eq!(next.command_code(), CommandCode::EndGroup);
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // {
assert_eq!(next.command_code(), CommandCode::BeginGroup);
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // !
assert_eq!(next.command_code(), CommandCode::Other);
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // }
assert_eq!(next.command_code(), CommandCode::EndGroup);
let next : T = tokenizer.get_next(&mut cs_handler,cc,eol,&par).unwrap().unwrap(); // end of line => space
assert_eq!(next.command_code(), CommandCode::Space);
assert!(tokenizer.get_next::<T>(&mut cs_handler,cc,eol,&par).unwrap().is_none()); // EOF
```
*/
#[derive(Clone, Debug)]
pub struct InputTokenizer<C: Character, S: TextLineSource<C>> {
    pub state: MouthState,
    line: usize,
    col: usize,
    current_line: TextLine<C>,
    pub source: S,
    pub(crate) eof: bool,
    tempstr: Vec<C>,
}

type Csh<T> = <<T as Token>::CS as CSName<<T as Token>::Char>>::Handler;

impl<C: Character, S: TextLineSource<C>> InputTokenizer<C, S> {
    /// Create a new [`InputTokenizer`] from a [`TextLineSource`]
    pub fn new(mut source: S) -> Self {
        Self {
            state: MouthState::NewLine,
            line: 1,
            col: 0,
            current_line: source.get_line().unwrap_or(TextLine::default()),
            source,
            eof: false,
            tempstr: Vec::new(),
        }
    }
    /// The current line

    pub fn line(&self) -> usize {
        self.line
    }
    /// The current column

    pub fn column(&self) -> usize {
        self.col + 1
    }
    /// whether the file end has been reached

    pub fn eof(&self) -> bool {
        self.eof
    }

    fn get_char(&mut self) -> Option<C> {
        if self.col >= self.current_line.len() {
            None
        } else {
            let next = self.current_line[self.col];
            self.col += 1;
            Some(next)
        }
    }

    /// `\readline` - read a line of input as [`Character`]s of [`CategoryCode::Other`] (except for ` `, which has
    /// [`Space`](CategoryCode::Space)) and passing each token to the given function.
    pub fn readline<T: Token<Char = C>, F: FnMut(T)>(&mut self, mut f: F) {
        while self.col < self.current_line.len() {
            let next = self.current_line[self.col];
            self.col += 1;
            match next.try_into() {
                Ok(b' ') => f(T::space()),
                _ => f(T::from_char_cat(next, CommandCode::Other)),
            }
        }
        self.next_line();
    }

    /// `\read` - read a line of input as [`Character`]s in the currenct [`CategoryCodeScheme`], respecting
    /// braces ([`CategoryCode::BeginGroup`] and [`EndGroup`](CategoryCode::EndGroup)) and passing each token to the
    /// given function.
    pub fn read<T: Token<Char = C>, F: FnMut(T)>(
        &mut self,
        handler: &mut Csh<T>,
        cc: &CategoryCodeScheme<C>,
        endline: Option<C>,
        par_token: &T::CS,
        mut f: F,
    ) -> Result<(), InvalidCharacter<C>> {
        let mut ingroups = 0;
        let mut ret: Result<(), InvalidCharacter<C>> = Ok(());
        let line = self.line;
        while self.line == line || ingroups > 0 {
            match self.get_char() {
                None => {
                    if self.eof {
                        return ret;
                    }
                    if let Some(n) = self.return_endline::<T>(cc, endline, par_token) {
                        f(n)
                    }
                    return ret;
                }
                Some(c) => match self.check_char::<T>(handler, cc, endline, c, par_token) {
                    Ok(None) if self.line == line || ingroups > 0 => (),
                    Ok(None) => return ret,
                    Ok(Some(tk)) => {
                        if tk.command_code() == CommandCode::BeginGroup {
                            ingroups += 1
                        } else if tk.command_code() == CommandCode::EndGroup {
                            ingroups -= 1
                        }
                        f(tk)
                    }
                    Err(i) => {
                        f(T::from_char_cat(i.0, CommandCode::Other));
                        ret = Err(i)
                    }
                },
            }
        }
        ret
    }

    /// Get the next [`Token`] from the [`InputTokenizer`] (if not empty). Throws [`InvalidCharacter`]
    /// on encountering a character of code [`CategoryCode::Invalid`].
    pub fn get_next<T: Token<Char = C>>(
        &mut self,
        handler: &mut Csh<T>,
        cc: &CategoryCodeScheme<C>,
        endline: Option<C>,
        par_token: &T::CS,
    ) -> Result<Option<T>, InvalidCharacter<C>> {
        loop {
            match self.get_char() {
                None if self.eof => return Ok(None),
                None => {
                    if let Some(e) = self.return_endline::<T>(cc, endline, par_token) {
                        //debug_log!(trace=>"Returning endline {}",e.printable(&interner));
                        return Ok(Some(e));
                    }
                }
                Some(c) => {
                    if let Some(t) = self.check_char::<T>(handler, cc, endline, c, par_token)? {
                        return Ok(Some(t));
                    }
                }
            };
        }
    }

    fn check_char<T: Token<Char = C>>(
        &mut self,
        handler: &mut Csh<T>,
        cc: &CategoryCodeScheme<C>,
        endline: Option<C>,
        c: C,
        par_token: &T::CS,
    ) -> Result<Option<T>, InvalidCharacter<C>> {
        use CategoryCode::*;
        match cc.get(c) {
            EOL if self.state == MouthState::NewLine => {
                self.next_line();
                Ok(Some(self.do_par(par_token.clone())))
            }
            EOL => Ok(self.return_endline::<T>(cc, endline, par_token)),
            Space if self.state == MouthState::SkipBlank => Ok(None),
            Space if self.state == MouthState::NewLine => Ok(None),
            Space => {
                self.state = MouthState::SkipBlank;
                Ok(Some(T::space()))
            }
            Ignored => Ok(None),
            Comment => {
                self.next_line();
                self.state = MouthState::NewLine;
                Ok(None)
            }
            Invalid => Err(InvalidCharacter(c)),
            Escape => Ok(Some(self.get_escape::<T>(handler, cc, endline))),
            Superscript => match self.maybe_superscript(c) {
                Some(c) => self.check_char::<T>(handler, cc, endline, c, par_token),
                None => {
                    self.state = MouthState::MidLine;
                    Ok(Some(T::from_char_cat(c, CommandCode::Superscript)))
                }
            },
            cc => {
                self.state = MouthState::MidLine;
                Ok(Some(T::from_char_cat(c, (*cc).into())))
            }
        }
    }

    fn next_line(&mut self) {
        if let Some(next) = self.source.get_line() {
            self.current_line = next;
            self.line += 1;
            self.col = 0;
        } else {
            self.eof = true;
            self.col = self.current_line.len();
            self.state = MouthState::MidLine;
        }
    }

    fn do_par<T: Token<Char = C>>(&mut self, par: T::CS) -> T {
        if self.current_line.is_empty() {
            while let Some(line) = self.source.get_line() {
                self.line += 1;
                if !line.is_empty() {
                    self.current_line = line;
                    break;
                }
            }
        }
        T::from_cs(par)
    }

    fn return_endline<T: Token<Char = C>>(
        &mut self,
        cc: &CategoryCodeScheme<C>,
        endline: Option<C>,
        par: &T::CS,
    ) -> Option<T> {
        use CategoryCode::*;
        self.next_line();
        let ret = match endline {
            None => None,
            Some(c) => match cc.get(c) {
                Space | EOL if self.state == MouthState::SkipBlank => None,
                Space if self.state == MouthState::NewLine => None,
                EOL if self.state == MouthState::NewLine => Some(self.do_par(par.clone())),
                EOL => Some(T::space()),
                Ignored | Invalid | Comment => None,
                o => Some(T::from_char_cat(c, (*o).into())),
            },
        };
        self.state = MouthState::NewLine;
        ret
    }

    fn get_escape<T: Token<Char = C>>(
        &mut self,
        handler: &mut Csh<T>,
        cc: &CategoryCodeScheme<C>,
        endline: Option<C>,
    ) -> T {
        let name = match self.get_char() {
            None => {
                self.next_line();
                match endline {
                    None => handler.empty_str(),
                    Some(c) => {
                        self.tempstr.clear();
                        self.tempstr.push(c);
                        handler.cs_from_chars(&self.tempstr)
                    }
                }
            }
            Some(next) => self.check_escape::<T>(handler, cc, next),
        };
        T::from_cs(name)
    }

    fn check_escape<T: Token<Char = C>>(
        &mut self,
        handler: &mut Csh<T>,
        cc: &CategoryCodeScheme<C>,
        next: C,
    ) -> T::CS {
        use CategoryCode::*;
        match cc.get(next) {
            Superscript => match self.maybe_superscript(next) {
                Some(c) => self.check_escape::<T>(handler, cc, c),
                None => {
                    self.state = MouthState::MidLine;
                    self.tempstr.clear();
                    self.tempstr.push(next);
                    handler.cs_from_chars(&self.tempstr)
                }
            },
            Letter => self.get_cs_name::<T>(handler, cc, next),
            _ => {
                self.state = MouthState::MidLine;
                self.tempstr.clear();
                self.tempstr.push(next);
                handler.cs_from_chars(&self.tempstr)
            }
        }
    }

    fn get_cs_name<T: Token<Char = C>>(
        &mut self,
        handler: &mut Csh<T>,
        cc: &CategoryCodeScheme<C>,
        first: C,
    ) -> T::CS {
        self.tempstr.clear();
        self.tempstr.push(first);
        self.state = MouthState::SkipBlank;
        loop {
            match self.get_char() {
                None => break,
                Some(next) => match cc.get(next) {
                    CategoryCode::Letter => self.tempstr.push(next),
                    CategoryCode::Superscript => {
                        let curr = self.col;
                        match self.maybe_superscript(next) {
                            Some(c) if *cc.get(c) == CategoryCode::Letter => self.tempstr.push(c),
                            _ => {
                                self.col = curr;
                                self.col -= 1;
                                break;
                            }
                        }
                    }
                    _ => {
                        self.col -= 1;
                        break;
                    }
                },
            }
        }
        handler.cs_from_chars(&self.tempstr)
    }

    fn cond(i: C) -> bool {
        (Into::<C>::into(48u8) <= i && i <= Into::<C>::into(57u8))
            || (Into::<C>::into(97u8) <= i && i <= Into::<C>::into(102u8))
    }

    fn maybe_superscript(&mut self, firstsup: C) -> Option<C> {
        match self.get_char() {
            None => None,
            Some(c) if c != firstsup => {
                self.col -= 1;
                None
            }
            Some(_) => match self.get_char() {
                None => {
                    self.col -= 1;
                    None
                }
                Some(first) => match self.get_char() {
                    None => {
                        if first < (128).into() {
                            let u: u8 = match first.try_into() {
                                Ok(u) => u,
                                Err(_) => {
                                    self.col -= 1;
                                    return None;
                                }
                            };
                            let ch: C = (if u < 64 { u + 64 } else { u - 64 }).into();
                            Some(ch)
                        } else {
                            self.col -= 2;
                            None
                        }
                    }
                    Some(second) => {
                        if Self::cond(first) && Self::cond(second) {
                            let ufirst: u8 = match first.try_into() {
                                Ok(u) => u,
                                Err(_) => {
                                    self.col -= 2;
                                    return None;
                                }
                            };
                            let usecond: u8 = match second.try_into() {
                                Ok(u) => u,
                                Err(_) => {
                                    self.col -= 2;
                                    return None;
                                }
                            };
                            let char = u8::from_str_radix(
                                std::str::from_utf8(&[ufirst, usecond]).unwrap(),
                                16,
                            )
                            .unwrap();
                            Some(char.into())
                        } else {
                            self.col -= 1;
                            if first < (128).into() {
                                let u: u8 = match first.try_into() {
                                    Ok(u) => u,
                                    Err(_) => {
                                        self.col -= 2;
                                        return None;
                                    }
                                };
                                let ch: C = (if u < 64 { u + 64 } else { u - 64 }).into();
                                Some(ch)
                            } else {
                                self.col -= 2;
                                None
                            }
                        }
                    }
                },
            },
        }
    }

    /// Only useful for debugging purposes: Print the next `len` [`Character`]s to the given [`Write`](std::fmt::Write)r.
    pub fn preview<W: std::fmt::Write>(&self, len: &mut usize, mut f: W) -> std::fmt::Result {
        if self.current_line.is_empty() {
            return Ok(());
        }
        if self.current_line.len() > self.col {
            for c in &self.current_line[self.col..] {
                *len -= 1;
                c.display_fmt(&mut f);
                if *len == 0 {
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}
