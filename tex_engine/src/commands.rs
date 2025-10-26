/*! [Commands](TeXCommand) - [primitives](PrimitiveCommand), [macros](Macro), etc. The B-book largely calls these
"equivalents", but we use the more standard term "command" instead.
*/

use crate::commands::methods::MacroParser;
use crate::commands::primitives::{PrimitiveIdentifier, PRIMITIVES};
use crate::engine::fontsystem::Font;
use crate::engine::fontsystem::FontSystem;
use crate::engine::mouth::strings::InputTokenizer;
use crate::engine::state::State;
use crate::engine::{EngineAux, EngineReferences, EngineTypes};
use crate::tex::catcodes::{CategoryCodeScheme, CommandCode};
use crate::tex::characters::StringLineSource;
use crate::tex::nodes::boxes::{BoxInfo, TeXBox};
use crate::tex::nodes::WhatsitFunction;
use crate::tex::numerics::{MuSkip, Skip, TeXInt};
use crate::tex::tokens::control_sequences::CSName;
use crate::tex::tokens::token_lists::{CharWrite, StringCharWrite, TokenList};
use crate::tex::tokens::Token;
use crate::utils::errors::TeXResult;
use either::Either;
use std::fmt::Display;

pub mod etex;
pub mod methods;
pub mod primitives;
pub mod tex;

/// A [`Token`] that has been resolved to a [`TeXCommand`] or a character (if not a control sequence / active character).
#[derive(Debug)]
pub enum ResolvedToken<'a, ET: EngineTypes> {
    /// The token is a simple character with the given [`CommandCode`].
    Tk { char: ET::Char, code: CommandCode },
    /// The token is a control sequence or active character, which
    ///is currently defined as the give [`TeXCommand`] (or undefined).
    Cmd(Option<&'a TeXCommand<ET>>),
}

/// See [`Gullet::char_or_primitive`](crate::engine::gullet::Gullet::char_or_primitive).
#[derive(Debug)]
pub enum CharOrPrimitive<ET: EngineTypes> {
    Char(ET::Char, Option<CommandCode>),
    Primitive(PrimitiveIdentifier),
}

/// A currently active conditional, e.g. `\ifnum`, `\ifx`, etc.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ActiveConditional<I: TeXInt> {
    /// An unfinished conditional, e.g. `\ifnum` before both numbers have been read.
    Unfinished(PrimitiveIdentifier),
    /// `\ifcase` of the provided number
    Case(I),
    /// A conditional that has evaluated to true
    True(PrimitiveIdentifier),
    /// A conditional that has evaluated to false after the matching `\else` branch
    Else(PrimitiveIdentifier),
}
impl<I: TeXInt> ActiveConditional<I> {
    /// The (original, primitive) name of the conditional.
    pub fn name(&self) -> PrimitiveIdentifier {
        match self {
            Self::Case(_) => PRIMITIVES.ifcase,
            Self::True(n) | Self::Unfinished(n) | Self::Else(n) => *n,
        }
    }
}

/// A command.
#[derive(Clone, Debug)]
pub enum TeXCommand<ET: EngineTypes> {
    /// A user defined [`Macro`], to be expanded (unless [protected](Macro::protected))
    Macro(Macro<ET::Token>),
    /// A character with the given [`CommandCode`]; e.g. the result of `\let\foo={`.
    Char { char: ET::Char, code: CommandCode },
    /// A character defined via `\chardef\foo...`.
    CharDef(ET::Char),
    /// A math character defined via `\mathchardef\foo...`.
    MathChar(u32),
    /// A font defined via `\font\foo...`.
    Font(<ET::FontSystem as FontSystem>::Font),
    /// An integer register defined via `\countdef\foo...`.
    IntRegister(usize),
    /// A dimension register defined via `\dimendef\foo...`.
    DimRegister(usize),
    /// A skip register defined via `\skipdef\foo...`.
    SkipRegister(usize),
    /// A muskip register defined via `\muskipdef\foo...`.
    MuSkipRegister(usize),
    /// A token register defined via `\toksdef\foo...`.
    ToksRegister(usize),
    /// A [primitive command](PrimitiveCommand), e.g. `\relax`, `\endgroup`, `\count` etc.
    Primitive {
        name: PrimitiveIdentifier,
        cmd: PrimitiveCommand<ET>,
    },
}
impl<ET: EngineTypes> TeXCommand<ET> {
    /// returns a helper struct for displaying the `\meaning` of this command; implements [`Display`].
    pub const fn meaning<'a>(
        &'a self,
        int: &'a <<ET::Token as Token>::CS as CSName<ET::Char>>::Handler,
        cc: &'a CategoryCodeScheme<ET::Char>,
        escapechar: Option<ET::Char>,
    ) -> Meaning<'a, ET> {
        Meaning {
            cmd: self,
            int,
            cc,
            escapechar,
        }
    }

    /// implements `\the` for this command, e.g. `\the\count0` or `\the\font`.
    /// #### Errors
    /// If self is not allowed after `\the`
    pub fn the<F: FnMut(&mut EngineAux<ET>, &ET::State, &mut ET::Gullet, ET::Token)>(
        &self,
        engine: &mut EngineReferences<ET>,
        token: ET::Token,
        mut cont: F,
    ) -> TeXResult<(), ET> {
        use crate::tex::tokens::token_lists::Otherize;
        use std::fmt::Write;
        match self {
            Self::IntRegister(u) => {
                let val = engine.state.get_int_register(*u);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::DimRegister(u) => {
                let val = engine.state.get_dim_register(*u);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::SkipRegister(u) => {
                let val = engine.state.get_skip_register(*u);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::MuSkipRegister(u) => {
                let val = engine.state.get_muskip_register(*u);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::CharDef(c) => {
                let val: u64 = (*c).into();
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::MathChar(u) => {
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{u}"
                )?;
            }
            Self::ToksRegister(u) => {
                for t in &engine.state.get_toks_register(*u).0 {
                    cont(engine.aux, engine.state, engine.gullet, t.clone());
                }
            }
            Self::Font(fnt) => {
                let t = fnt.name();
                cont(
                    engine.aux,
                    engine.state,
                    engine.gullet,
                    ET::Token::from_cs(t.clone()),
                );
            }
            Self::Primitive { name, cmd } => return cmd.the(engine, token, *name, cont),
            o => engine.general_error(format!(
                "You can't use {} after \\the",
                o.meaning(
                    engine.aux.memory.cs_interner(),
                    engine.state.get_catcode_scheme(),
                    engine.state.get_escape_char()
                )
            ))?,
        }
        Ok(())
    }
}

/// A *primitive* command defined from the outset. All of the `fn` methods
/// are called with (at least) the current [`EngineReferences`] and the [`Token`] that
/// triggered the command.
#[derive(Copy, Clone, Debug)]
pub enum PrimitiveCommand<ET: EngineTypes> {
    /// A conditional, e.g. `\ifnum`, `\ifx`, etc.
    Conditional(fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<bool, ET>),
    /// An expandable primitive, e.g. `\the`, `\number`, etc. - should push its expansion to the `Vec` argument.
    Expandable(fn(&mut EngineReferences<ET>, &mut Vec<ET::Token>, ET::Token) -> TeXResult<(), ET>),
    /// An expandable primitive that does not actually produce any tokens, or does via more complicated means
    /// than simply returning a `Vec<ET::Token>` - e.g. `\csname`, `\input`, `\else`, etc.
    SimpleExpandable(fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<(), ET>),
    /// A primitive that cannot be expanded, e.g. `\relax`, `\end`, etc. See [`CommandScope`].
    Unexpandable {
        scope: CommandScope,
        apply: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<(), ET>,
    },
    /// An assignment primitive, e.g. `\def`, `\advance` - basically, an unexpandable primitive that
    /// causes `\afterassignment` to be inserted.
    Assignment(fn(&mut EngineReferences<ET>, ET::Token, bool) -> TeXResult<(), ET>),
    /// A primitive that yields an integer value if one is expected, or optionally can assign one if not;
    /// e.g. `\count`.
    Int {
        read: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<ET::Int, ET>,
        assign: Option<
            for<'a, 'b> fn(&'a mut EngineReferences<'b, ET>, ET::Token, bool) -> TeXResult<(), ET>,
        >,
    },
    /// A primitive that yields a dimension value if one is expected, or optionally can assign one if not;
    /// e.g. `\dimen`.
    Dim {
        read: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<ET::Dim, ET>,
        assign: Option<
            for<'a, 'b> fn(&'a mut EngineReferences<'b, ET>, ET::Token, bool) -> TeXResult<(), ET>,
        >,
    },
    /// A primitive that yields a skip value if one is expected, or optionally can assign one if not;
    /// e.g. `\skip`.
    Skip {
        read: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<Skip<ET::Dim>, ET>,
        assign: Option<
            for<'a, 'b> fn(&'a mut EngineReferences<'b, ET>, ET::Token, bool) -> TeXResult<(), ET>,
        >,
    },
    /// A primitive that yields a muskip value if one is expected, or optionally can assign one if not;
    /// e.g. `\muskip`.
    MuSkip {
        read: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<MuSkip<ET::MuDim>, ET>,
        assign: Option<
            for<'a, 'b> fn(&'a mut EngineReferences<'b, ET>, ET::Token, bool) -> TeXResult<(), ET>,
        >,
    },
    /// A primitive that yields a [`Font`] if one is expected, or optionally can assign one if not;
    /// e.g. `\font`.
    FontCmd {
        read: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<ET::Font, ET>,
        assign: Option<
            for<'a, 'b> fn(&'a mut EngineReferences<'b, ET>, ET::Token, bool) -> TeXResult<(), ET>,
        >,
    },
    /// A primitive that yields either a finished [`TeXBox`], or opens a new one, depending on
    /// the case of the return value. Used for e.g. `\setbox` or `\raise`, which may be followed by
    /// a finished box (e.g. `\box0`) or a new box (e.g. `\hbox{...}`).
    Box(
        fn(
            &mut EngineReferences<ET>,
            ET::Token,
        ) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET>,
    ),
    /// A primitive assignable integer value, e.g. `\hangindent` or `\tolerance`.
    PrimitiveInt,
    /// A primitive assignable dimension value, e.g. `\parindent` or `\hsize`.
    PrimitiveDim,
    /// A primitive assignable skip value, e.g. `\parskip` or `\lineskip`.
    PrimitiveSkip,
    /// A primitive assignable muskip value, e.g. `\thinmuskip` or `\medmuskip`.
    PrimitiveMuSkip,
    /// A primitive assignable token list, e.g. `\everypar` or `\output`.
    PrimitiveToks,
    /// A Whatsit, e.g. `\write`, `\special`, etc. - if following an `\immediate`, the `immediate` function
    /// is called, otherwise, `get` may return a (boxed) continuation to be called at shipout.
    Whatsit {
        get: fn(
            &mut EngineReferences<ET>,
            ET::Token,
        ) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET>,
        immediate: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<(), ET>,
        the: Option<fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<Vec<ET::Token>, ET>>,
    },
    /// `\relax` - does nothing.
    Relax,
}
impl<ET: EngineTypes> PrimitiveCommand<ET> {
    /// implements `\the` for this command, e.g. `\the\count0` or `\the\font`.
    /// #### Errors
    /// If self is not allowed after `\the`
    pub fn the<F: FnMut(&mut EngineAux<ET>, &ET::State, &mut ET::Gullet, ET::Token)>(
        &self,
        engine: &mut EngineReferences<ET>,
        token: ET::Token,
        name: PrimitiveIdentifier,
        mut cont: F,
    ) -> TeXResult<(), ET> {
        use crate::tex::tokens::token_lists::Otherize;
        use std::fmt::Write;
        match self {
            Self::Int { read, .. } => {
                let val = read(engine, token)?;
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::Dim { read, .. } => {
                let val = read(engine, token)?;
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::Skip { read, .. } => {
                let val = read(engine, token)?;
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::MuSkip { read, .. } => {
                let val = read(engine, token)?;
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::PrimitiveInt => {
                let val = engine.state.get_primitive_int(name);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::PrimitiveDim => {
                let val = engine.state.get_primitive_dim(name);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::PrimitiveSkip => {
                let val = engine.state.get_primitive_skip(name);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::PrimitiveMuSkip => {
                let val = engine.state.get_primitive_muskip(name);
                write!(
                    Otherize::new(&mut |t| cont(engine.aux, engine.state, engine.gullet, t)),
                    "{val}"
                )?;
            }
            Self::PrimitiveToks => {
                for t in &engine.state.get_primitive_tokens(name).0 {
                    cont(engine.aux, engine.state, engine.gullet, t.clone());
                }
            }
            Self::FontCmd { read, .. } => {
                let fnt = read(engine, token)?;
                let t = fnt.name();
                cont(
                    engine.aux,
                    engine.state,
                    engine.gullet,
                    ET::Token::from_cs(t.clone()),
                );
            }
            Self::Whatsit { the: Some(the), .. } => {
                for t in the(engine, token)? {
                    cont(engine.aux, engine.state, engine.gullet, t);
                }
            }
            _ if name == PRIMITIVES.toks => {
                let u = engine.read_register_index(false, &token)?;
                for t in &engine.state.get_toks_register(u).0 {
                    cont(engine.aux, engine.state, engine.gullet, t.clone());
                }
            }
            _ => engine.general_error(format!(
                "You can't use {} after \\the",
                name.display(engine.state.get_escape_char())
            ))?,
        }
        Ok(())
    }
}

/// A helper struct for displaying the `\meaning` of a [`TeXCommand`]; implements [`Display`].
pub struct Meaning<'a, ET: EngineTypes> {
    cmd: &'a TeXCommand<ET>,
    int: &'a <<ET::Token as Token>::CS as CSName<ET::Char>>::Handler,
    cc: &'a CategoryCodeScheme<ET::Char>,
    escapechar: Option<ET::Char>,
}
impl<ET: EngineTypes> Meaning<'_, ET> {
    /// Write the meaning directly to a [`CharWrite`].
    /// #### Errors
    /// Formatting error (should never happen)
    pub fn write_chars<W: CharWrite<ET::Char, ET::CSName>>(&self, f: &mut W) -> std::fmt::Result {
        match self.cmd {
            TeXCommand::Macro(m) => m.meaning_char(self.int, self.cc, self.escapechar, f),
            TeXCommand::Char { char, code } => code.meaning(*char, f),
            TeXCommand::CharDef(c) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "char\"{:X}", Into::<u64>::into(*c))
            }
            TeXCommand::MathChar(u) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "mathchar\"{u:X}")
            }
            TeXCommand::Font(i) => {
                write!(f, "select font ")?;
                i.display(self.int, f)
            }
            TeXCommand::IntRegister(i) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "count{i}")
            }
            TeXCommand::DimRegister(i) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "dimen{i}")
            }
            TeXCommand::SkipRegister(i) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "skip{i}")
            }
            TeXCommand::MuSkipRegister(i) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "muskip{i}")
            }
            TeXCommand::ToksRegister(i) => {
                if let Some(e) = self.escapechar {
                    f.push_char(e);
                }
                write!(f, "toks{i}")
            }
            TeXCommand::Primitive { name, .. } => {
                write!(f, "{}", name.display(self.escapechar))
            }
        }
    }
}
impl<ET: EngineTypes> Display for Meaning<'_, ET> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write_chars(&mut StringCharWrite::new(f))
    }
}

/// A macro signature, e.g. `#1#2#3` or `#1t#2\foo{`.
#[derive(Clone, Debug)]
pub struct MacroSignature<T: Token> {
    /// The number of parameters, e.g. `3` in `#1#2#3`.
    pub arity: u8,
    /// The token list specifying the parameters, e.g. `[#1,#2,#3]` in `#1#2#3`.
    pub params: TokenList<T>,
}

/// A macro, e.g. the result of `\def\foo#1#2{...}`.
#[derive(Clone, Debug)]
pub struct Macro<T: Token> {
    /// Whether the macro is protected, e.g. the result of `\protected\def`.
    pub protected: bool,
    /// Whether the macro is long, e.g. the result of `\long\def`.
    pub long: bool,
    /// Whether the macro is outer, e.g. the result of `\outer\def`.
    pub outer: bool,
    /// The expansion of the macro, e.g. `...` in `\def\foo#1#2{...}`.
    pub expansion: TokenList<T>,
    /// The signatureof the macro, e.g. `#1#2` in `\def\foo#1#2{...}`.
    pub signature: MacroSignature<T>,
}
impl<T: Token> Macro<T> {
    /// Convenience method for creating a new macro from a signature and expansion as strings; given the provided [`CategoryCodeScheme`].
    /// Allows for e.g. `as_point = Macro::new(int,`[`&DEFAULT_SCHEME_U8`](crate::tex::catcodes::DEFAULT_SCHEME_U8)`,"#1#2","(#1,#2)")`.
    /// #### Errors
    /// on invalid tex strings
    pub fn new<Sig: AsRef<str>, Exp: AsRef<str>, ET: EngineTypes<Token = T, Char = T::Char>>(
        int: &mut <T::CS as CSName<T::Char>>::Handler,
        cc: &CategoryCodeScheme<T::Char>,
        sig: Sig,
        exp: Exp,
    ) -> TeXResult<Self, ET> {
        let mut parser = MacroParser::new();
        let sig = sig.as_ref();
        if !sig.is_empty() {
            let sigsrc: StringLineSource<T::Char> = sig.into();
            let mut sigsrc = InputTokenizer::new(sigsrc);
            while let Some(t) = sigsrc.get_next(int, cc, None)? {
                parser.do_signature_token::<ET>(t)?;
            }
        }
        let exp = exp.as_ref();
        let expsrc: StringLineSource<T::Char> = exp.into();
        let mut expsrc = InputTokenizer::new(expsrc);
        while let Some(t) = expsrc.get_next(int, cc, None)? {
            parser.do_expansion_token::<ET>(t)?;
        }
        Ok(parser.close(false, false, false))
    }

    /// returns a helper struct for displaying the `\meaning` of this command; implements [`Display`].
    pub fn meaning<'a>(
        &'a self,
        int: &'a <T::CS as CSName<T::Char>>::Handler,
        cc: &'a CategoryCodeScheme<T::Char>,
        escapechar: Option<T::Char>,
    ) -> impl Display + 'a {
        MacroMeaning {
            cmd: self,
            int,
            cc,
            escapechar,
        }
    }
    /// Write the meaning directly to a [`CharWrite`].
    /// #### Errors
    /// Formatting error (should never happen)
    pub fn meaning_char<F: CharWrite<T::Char, T::CS>>(
        &self,
        int: &<T::CS as CSName<T::Char>>::Handler,
        cc: &CategoryCodeScheme<T::Char>,
        escapechar: Option<T::Char>,
        f: &mut F,
    ) -> std::fmt::Result {
        if self.protected {
            if let Some(e) = escapechar {
                f.push_char(e);
            }
            write!(f, "protected ")?;
        }
        if self.long {
            if let Some(e) = escapechar {
                f.push_char(e);
            }
            write!(f, "long ")?;
        }
        if self.outer {
            if let Some(e) = escapechar {
                f.push_char(e);
            }
            write!(f, "outer ")?;
        }
        write!(f, "macro:")?;
        self.signature
            .params
            .display(int, cc, escapechar, false)
            .fmt_cw(f)?;
        write!(f, "->")?;
        self.expansion.display(int, cc, escapechar, true).fmt_cw(f)
    }
}

struct MacroMeaning<'a, T: Token> {
    cmd: &'a Macro<T>,
    int: &'a <T::CS as CSName<T::Char>>::Handler,
    cc: &'a CategoryCodeScheme<T::Char>,
    escapechar: Option<T::Char>,
}
impl<T: Token> Display for MacroMeaning<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cmd.meaning_char(
            self.int,
            self.cc,
            self.escapechar,
            &mut StringCharWrite::new(f),
        )
    }
}

/// The scope of a [`PrimitiveCommand::Unexpandable`].
#[derive(Clone, Debug, Copy)]
pub enum CommandScope {
    /// The command is only valid in vertical mode. If occuring in horizontal mode, it will
    /// close the current paragraph, i.e. switch to vertical mode.
    /// In restricted horizontal or math mode, it will throw an error.
    SwitchesToVertical,
    /// The command is only valid in horizontal mode. If occuring in (internal) vertical mode, it will
    /// open a new paragraph, i.e. switch to horizontal mode. In math mode, it will throw an error.
    SwitchesToHorizontal,
    /// The command is only valid in math mode. If occuring in non-math mode, it will
    /// throw an error
    MathOnly,
    /// The command is only valid in horizontal or math mode. If occuring in vertical mode, it will
    /// open a new paragraph, i.e. switch to horizontal mode.
    SwitchesToHorizontalOrMath,
    /// The command is valid anywhere and will not switch modes.
    Any,
}
