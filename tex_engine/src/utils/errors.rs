/*! Recovery from errors.

  An instance of [`ErrorHandler`] provides methods that get called when errors occur during compilation.
  The signatures of these methods reflect where they are called and what needs to be returned in order
  to recover from the error.
*/
#![allow(clippy::result_unit_err)]

use crate::commands::primitives::PrimitiveIdentifier;
use crate::engine::state::State;
use crate::engine::utils::memory::MemoryManager;
use crate::engine::{EngineAux, EngineReferences, EngineTypes};
use crate::prelude::{Mouth, TeXMode};
use crate::tex::characters::{Character, StringLineSource};
use crate::tex::tokens::StandardToken;
use crate::tex::tokens::Token;
use crate::tex::tokens::control_sequences::CSHandler;
use std::fmt::Debug;
use std::marker::PhantomData;
use thiserror::Error;

#[derive(Debug, Error)]
#[error(transparent)]
pub struct TeXError<ET: EngineTypes>(Box<TeXErrorI<ET>>);
impl<ET: EngineTypes, E: Into<TeXErrorI<ET>>> From<E> for TeXError<ET> {
    #[inline]
    fn from(value: E) -> Self {
        Self(Box::new(value.into()))
    }
}
impl<ET: EngineTypes> TeXError<ET> {
    pub fn general(s: impl Into<String>) -> Self {
        Self(Box::new(TeXErrorI::General(s.into())))
    }
}

#[derive(Debug, Error)]
pub enum TeXErrorI<ET: EngineTypes> {
    #[error(transparent)]
    InvalidCharacter(#[from] InvalidCharacter<ET::Char>),
    #[error("Use of {0} doesn't match its definition")]
    WrongDefinition(String),
    #[error("! File ended while scanning use of `{0}`")]
    FileEndedWhileScanningUseOf(String),
    #[error("Undefined {0}")]
    Undefined(String),
    #[error("! Too many }}'s")]
    TooManyCloseBraces,
    #[error("Emergency stop.")]
    EmergencyStop,
    #[error("! Missing {{ inserted")]
    MissingBegingroup,
    #[error("! Missing }} inserted")]
    MissingEndgroup,
    #[error("! Missing $ inserted")]
    MissingDollar,
    #[error("Missing number, treated as zero.")]
    MissingNumber,
    #[error("Illegal unit of measure (pt inserted)")]
    MissingUnit,
    #[error("Runaway argument? Paragraph ended before {0} was complete.")]
    ParagraphEnded(String),
    #[error("! Missing {} inserted",.0[0])]
    MissingKeyword(&'static [&'static str]),
    #[error("Incomplete {}; all text was ignored after line {line_no}",.name.display::<u8>(Some(b'\\')))]
    IncompleteConditional {
        name: PrimitiveIdentifier,
        line_no: usize,
    },
    #[error("You can't use {} in {mode} mode",.name.display::<u8>(Some(b'\\')))]
    NotAllowedInMode {
        name: PrimitiveIdentifier,
        mode: TeXMode,
    },
    #[error("Errror: {0}")]
    General(String),
    #[error(transparent)]
    Fmt(#[from] std::fmt::Error),
    /*
    FileEndWhileScanningTextOf(ET::Token),
    MissingEndgroup,
    MissingArgument(ET::Token),
    MissingNumber,
    MissingKeyword(&'static[&'static str]),
    EndInsideGroup,
     */
}
impl<ET: EngineTypes> From<String> for TeXErrorI<ET> {
    #[inline]
    fn from(value: String) -> Self {
        Self::General(value)
    }
}

macro_rules! throw {
    ($aux:expr,$state:expr,$mouth:expr,$f:ident($($arg:expr),*) => $err:expr) => {{
        let eh = &$aux.error_handler;
        let ret = eh.$f(&$aux.outputs,&$aux.memory,$state$(,$arg)*);
        match ret {
            Ok(Some(src)) => {
                $mouth.push_string(src);
                Ok(())
            },
            Ok(None) => Ok(()),
            _ => Err($err.into())
        }
    }};
}
impl<ET: EngineTypes> TeXError<ET> {
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn incomplete_conditional<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        name: PrimitiveIdentifier,
    ) -> TeXResult<(), ET> {
        let line_no = mouth.line_number();
        throw!(aux,state,mouth,incomplete_conditional(name,line_no) => TeXErrorI::IncompleteConditional{name,line_no})
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn wrong_definition<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        found: &ET::Token,
        expected: &ET::Token,
        in_macro: &ET::Token,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,wrong_definition(found,expected,in_macro) =>
            TeXErrorI::WrongDefinition(Token::display(in_macro,aux.memory.cs_interner(),state.get_catcode_scheme(),state.get_escape_char()).to_string())
        )
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn file_end_while_use<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        in_macro: &ET::Token,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,file_end_while_use(in_macro) => TeXErrorI::FileEndedWhileScanningUseOf(Token::display(in_macro,aux.memory.cs_interner(),state.get_catcode_scheme(),state.get_escape_char()).to_string()))
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn undefined<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        token: &ET::Token,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,undefined(token) => TeXErrorI::Undefined(
            match token.to_enum() {
                StandardToken::ControlSequence(cs) =>
                    format!("control sequence {}{}",ET::Char::display_opt(state.get_escape_char()),aux.memory.cs_interner().resolve(&cs)),
                StandardToken::Character(c,_) =>
                    format!("active character {}",c.display()),
                StandardToken::Primitive(_) => unreachable!()
            }
        ))
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn not_allowed_in_mode<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        name: PrimitiveIdentifier,
        mode: TeXMode,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,not_allowed_in_mode(name,mode) => TeXErrorI::NotAllowedInMode{name,mode})
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn missing_begingroup<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,missing_begingroup() => TeXErrorI::MissingBegingroup)
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn missing_endgroup<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,missing_endgroup() => TeXErrorI::MissingEndgroup)
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn missing_number<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,missing_number() => TeXErrorI::MissingNumber)
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn missing_unit<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,missing_number() => TeXErrorI::MissingUnit)
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn missing_keyword<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        kws: &'static [&'static str],
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,missing_keyword(kws) => TeXErrorI::MissingKeyword(kws))
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn missing_dollar_inserted<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,missing_dollar() => TeXErrorI::MissingDollar)
    }
    /// #### Errors
    /// because that's what it's supposed to do
    pub fn paragraph_ended<M: Mouth<ET>>(
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
        t: &ET::Token,
    ) -> TeXResult<(), ET> {
        throw!(aux,state,mouth,paragraph_ended(t) => TeXErrorI::ParagraphEnded(
            match t.to_enum() {
                StandardToken::ControlSequence(cs) =>
                    format!("control sequence {}{}",ET::Char::display_opt(state.get_escape_char()),aux.memory.cs_interner().resolve(&cs)),
                StandardToken::Character(c,_) =>
                    format!("active character {}",c.display()),
                StandardToken::Primitive(_) => unreachable!()
            }
        ))
    }
}
pub type TeXResult<A, ET> = Result<A, TeXError<ET>>;

pub trait IntoErr<ET: EngineTypes, Err> {
    fn into_err(self, aux: &EngineAux<ET>, state: &ET::State) -> Err;
}
impl<ET: EngineTypes, Err, A> IntoErr<ET, Err> for A
where
    Err: From<A>,
{
    fn into_err(self, _aux: &EngineAux<ET>, _state: &ET::State) -> Err {
        self.into()
    }
}

pub trait RecoverableError<ET: EngineTypes>: Sized {
    /// #### Errors
    /// because that's what it's supposed to do
    fn recover<M: Mouth<ET>, Err>(
        self,
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> Result<(), Err>
    where
        Self: IntoErr<ET, Err>;

    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn throw<M: Mouth<ET>>(
        self,
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> Result<(), TeXError<ET>>
    where
        Self: IntoErr<ET, TeXError<ET>>,
    {
        self.recover(aux, state, mouth)
            .map_err(|e| e.into_err(aux, state))
    }
}
macro_rules! split {
    ($self:ident,$aux:expr,$state:ident,$mouth:ident,$f:ident($($arg:expr),*)) => {{
        throw!($aux,$state,$mouth,$f($($arg),*) => $self.into_err($aux,$state))
    }};
}

#[derive(Debug, Error)]
#[error("! Too many }}'s")]
pub enum GulletError<C: Character> {
    #[error("! Text line contains an invalid character.\n{}",.0.display())]
    InvalidChar(C),
    TooManyCloseBraces,
}
impl<C: Character> From<InvalidCharacter<C>> for GulletError<C> {
    fn from(InvalidCharacter(c): InvalidCharacter<C>) -> Self {
        Self::InvalidChar(c)
    }
}
impl<C: Character> From<TooManyCloseBraces> for GulletError<C> {
    fn from(_: TooManyCloseBraces) -> Self {
        Self::TooManyCloseBraces
    }
}
impl<ET: EngineTypes> From<GulletError<ET::Char>> for TeXErrorI<ET> {
    fn from(e: GulletError<ET::Char>) -> Self {
        match e {
            GulletError::InvalidChar(c) => Self::InvalidCharacter(InvalidCharacter(c)),
            GulletError::TooManyCloseBraces => Self::TooManyCloseBraces,
        }
    }
}

/// An error indicating that an invalid [`Character`] was encountered
#[derive(Debug, Error)]
#[error("! Text line contains an invalid character.\n{}",.0.display())]
pub struct InvalidCharacter<C: Character>(pub C);
impl<ET: EngineTypes> RecoverableError<ET> for InvalidCharacter<ET::Char> {
    fn recover<M: Mouth<ET>, Err>(
        self,
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> Result<(), Err>
    where
        Self: IntoErr<ET, Err>,
    {
        split!(self, aux, state, mouth, invalid_character(self.0))
    }
}

#[derive(Debug, Error)]
#[error("! Too many }}'s")]
pub struct TooManyCloseBraces;
impl<ET: EngineTypes> RecoverableError<ET> for TooManyCloseBraces {
    fn recover<M: Mouth<ET>, Err>(
        self,
        aux: &EngineAux<ET>,
        state: &ET::State,
        mouth: &mut M,
    ) -> Result<(), Err>
    where
        Self: IntoErr<ET, Err>,
    {
        split!(self, aux, state, mouth, too_many_closebraces())
    }
}
impl<ET: EngineTypes> From<TooManyCloseBraces> for TeXErrorI<ET> {
    fn from(_: TooManyCloseBraces) -> Self {
        Self::TooManyCloseBraces
    }
}

/// Trait for error recovery, to be implemented for an engine.
pub trait ErrorHandler<ET: EngineTypes> {
    /// "Text line contains an invalid character".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn invalid_character(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _c: ET::Char,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "Use of `\foo` doesn't match its definition".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn wrong_definition(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _found: &ET::Token,
        _expected: &ET::Token,
        _in_macro: &ET::Token,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "File ended while scanning use of `\foo`".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn file_end_while_use(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _in_macro: &ET::Token,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "Too many }'s".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn too_many_closebraces(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "Missing { inserted".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn missing_begingroup(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "Missing $ inserted".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn missing_dollar(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "Missing } inserted".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn missing_endgroup(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /// "Incomplete \if...; all text was ignored after line `n`".
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn incomplete_conditional(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _name: PrimitiveIdentifier,
        _line_no: usize,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /// "Undefined `[`control sequence`|`active character`]`"
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn undefined(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _token: &ET::Token,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /// "Runaway argument? Paragraph ended before `\foo` was complete."
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn paragraph_ended(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _token: &ET::Token,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /// "You can't use `\foo` in `M` mode."
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn not_allowed_in_mode(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _name: PrimitiveIdentifier,
        _mode: TeXMode,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }
    /// "Missing `x` inserted"
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn missing_keyword(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _kws: &'static [&'static str],
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /// "Missing number, treated as zero."
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn missing_number(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /// Any other possibly recoverable error.
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    fn other(
        &self,
        _out: &ET::Outputs,
        _memory: &MemoryManager<ET::Token>,
        _state: &ET::State,
        _msg: &str,
    ) -> Result<Option<StringLineSource<ET::Char>>, ()> {
        Err(())
    }

    /*
    /// "Runaway argument? Paragraph ended before `\foo` was complete."
    fn no_par<T:Token,St:AsRef<str>,S:TextLineSource<T::Char>>(&self, _tokenizer:&mut InputTokenizer<T::Char,S>, _name:St, _start:(usize, usize)) -> T {
        //let line = &tokenizer.string.line(start.0)[start.1..];
        //throw!("Runaway argument?\n{}\n! Paragraph ended before \\{} was complete.",InputLinePresenter(line),name.as_ref());
    }
    /// "Runaway argument? File ended while scanning use of `\foo`."
    fn file_end<T:Token,St:AsRef<str>,S:TextLineSource<T::Char>>(&self, _tokenizer:&mut InputTokenizer<T::Char,S>, _name:St, _start:(usize, usize)) -> T {
        //let line = &tokenizer.string.line(start.0)[start.1..];
        //throw!("Runaway argument?\n{}\n! File ended while scanning use of \\{}.",InputLinePresenter(line),name.as_ref());
    }

     */
    fn new() -> Self;
}

/// Default [`ErrorHandler`] that just throws a [`TeXError`].
pub struct ErrorThrower<ET: EngineTypes>(PhantomData<ET>);
impl<ET: EngineTypes> ErrorHandler<ET> for ErrorThrower<ET> {
    fn new() -> Self {
        Self(PhantomData)
    }
}

impl<ET: EngineTypes> EngineReferences<'_, ET> {
    /// #### Errors
    /// because that's what it's supposed to do
    #[inline]
    pub fn general_error(&mut self, msg: String) -> TeXResult<(), ET> {
        throw!(self.aux,self.state,self.mouth,other(&msg) => TeXErrorI::General(msg))
    }
}
