#![allow(clippy::module_name_repetitions)]
/*! A TeX engine combines all the necessary components into a struct capable of compiling a TeX file into
    some output format.
*/
use crate::commands::primitives::PRIMITIVES;
use crate::commands::{ResolvedToken, TeXCommand};
use crate::engine::filesystem::{File, FileSystem, VirtualFile};
use crate::engine::fontsystem::{Font, FontSystem, TfmFont, TfmFontSystem};
use crate::engine::gullet::{DefaultGullet, Gullet};
use crate::engine::mouth::{DefaultMouth, Mouth};
use crate::engine::state::State;
use crate::engine::stomach::{DefaultStomach, Stomach};
use crate::engine::utils::memory::MemoryManager;
use crate::engine::utils::outputs::{LogOutputs, Outputs};
use crate::tex;
use crate::tex::catcodes::CommandCode;
use crate::tex::characters::Character;
use crate::tex::nodes::vertical::VNode;
use crate::tex::nodes::CustomNodeTrait;
use crate::tex::numerics::{Dim32, Mu, MuDim, NumSet, Numeric, TeXDimen, TeXInt};
use crate::tex::tokens::control_sequences::{CSName, InternedCSName};
use crate::tex::tokens::Token;
use crate::utils::errors::{ErrorHandler, ErrorThrower, TeXError, TeXResult};
use chrono::{Datelike, Timelike};
use std::convert::Infallible;
use std::fmt::Debug;

pub mod filesystem;
pub mod fontsystem;
pub mod gullet;
pub mod mouth;
pub mod state;
pub mod stomach;
pub mod utils;

/**
    The types that an engine needs to implement. To reduce overhad in function signature,
    we bundle all of them into a single trait and pass that around.
*/
pub trait EngineTypes: Sized + Copy + Clone + Debug + 'static {
    type Char: Character;
    type CSName: CSName<Self::Char>;
    #[cfg(feature = "multithreaded")]
    type Token: Token<Char = Self::Char, CS = Self::CSName> + Send + Sync;
    #[cfg(not(feature = "multithreaded"))]
    type Token: Token<Char = Self::Char, CS = Self::CSName>;
    type Extension: EngineExtension<Self>;
    #[cfg(feature = "multithreaded")]
    type File: File<Char = Self::Char> + Send + Sync;
    #[cfg(not(feature = "multithreaded"))]
    type File: File<Char = Self::Char>;
    type FileSystem: FileSystem<File = Self::File>;
    type Int: TeXInt;
    type Dim: TeXDimen + Numeric<Self::Int>;
    type MuDim: MuDim + Numeric<Self::Int>;
    type Num: crate::tex::numerics::NumSet<Int = Self::Int, Dim = Self::Dim, MuDim = Self::MuDim>;
    type State: State<Self>;
    type Outputs: Outputs;
    type Mouth: Mouth<Self>;
    type Gullet: Gullet<Self>;
    type Stomach: Stomach<Self>;
    type ErrorHandler: ErrorHandler<Self>;
    type CustomNode: CustomNodeTrait<Self>;
    type Font: Font<Char = Self::Char, Int = Self::Int, Dim = Self::Dim, CS = Self::CSName>;
    type FontSystem: FontSystem<
        Font = Self::Font,
        Char = Self::Char,
        Int = Self::Int,
        Dim = Self::Dim,
        CS = Self::CSName,
    >;
}
/// Auxiliary components passed around to [`PrimitiveCommand`](crate::commands::PrimitiveCommand)s.
pub struct EngineAux<ET: EngineTypes> {
    /// memory management and interning of control sequence names
    pub memory: MemoryManager<ET::Token>,
    /// error handling
    pub error_handler: ET::ErrorHandler,
    /// printing to logs or the terminal
    pub outputs: ET::Outputs,
    /// start time of the current job
    pub start_time: chrono::DateTime<chrono::Local>,
    /// `\jobname`
    pub jobname: String,
    /// extension components
    pub extension: ET::Extension,
}

struct Colon<'c, ET: EngineTypes> {
    out: Box<dyn FnMut(&mut EngineReferences<ET>, VNode<ET>) -> TeXResult<(), ET> + 'c>,
}
impl<'c, ET: EngineTypes> Colon<'c, ET> {
    fn new<F: FnMut(&mut EngineReferences<ET>, VNode<ET>) -> TeXResult<(), ET> + 'c>(f: F) -> Self {
        Colon { out: Box::new(f) }
    }
    fn out(&mut self, engine: &mut EngineReferences<ET>, n: VNode<ET>) -> TeXResult<(), ET> {
        (self.out)(engine, n)
    }
}
impl<ET: EngineTypes> Default for Colon<'_, ET> {
    fn default() -> Self {
        Colon {
            out: Box::new(|_, _| Ok(())),
        }
    }
}

impl<ET: EngineTypes> EngineReferences<'_, ET> {
    /// ships out the [`VNode`], passing it on to the provided continuation.
    /// #### Errors
    /// On LaTeX errors in Whatsits (e.g. non-immediate `\write`s)
    pub fn shipout(&mut self, n: VNode<ET>) -> TeXResult<(), ET> {
        let mut colon = std::mem::take(&mut self.colon);
        let r = colon.out(self, n);
        self.colon = colon;
        r
    }
}

/**
    This struct combines all the necessary components for use in [`PrimitiveCommand`](crate::commands::PrimitiveCommand)s.
    We use public fields instead of accessor methods to convince the borrow checker
    that all the components are *independent*, and avoid "Cannot borrow as mutable because already borrowed as
    immutable" errors.
*/
pub struct EngineReferences<'et, ET: EngineTypes> {
    pub state: &'et mut ET::State,
    pub mouth: &'et mut ET::Mouth,
    pub gullet: &'et mut ET::Gullet,
    pub stomach: &'et mut ET::Stomach,
    pub filesystem: &'et mut ET::FileSystem,
    pub fontsystem: &'et mut ET::FontSystem,
    colon: Colon<'et, ET>,
    pub aux: &'et mut EngineAux<ET>,
}

/// Example implementation of [`EngineTypes`] for a plain TeX engine.
#[derive(Copy, Clone, Debug)]
pub struct DefaultPlainTeXEngineTypes;
impl EngineTypes for DefaultPlainTeXEngineTypes {
    type Char = u8;
    type CSName = InternedCSName<u8>;
    type Token = super::tex::tokens::CompactToken;
    type Extension = ();
    type Int = i32;
    type Dim = Dim32;
    type MuDim = Mu;
    type Num = tex::numerics::DefaultNumSet;
    type State = state::tex_state::DefaultState<Self>;
    type File = VirtualFile<u8>;
    type FileSystem = filesystem::NoOutputFileSystem<u8>;
    type Outputs = LogOutputs;
    type Mouth = DefaultMouth<Self>;
    type Gullet = DefaultGullet<Self>;
    type CustomNode = Infallible;
    type ErrorHandler = ErrorThrower<Self>;
    type Stomach = DefaultStomach<Self>;
    type Font = TfmFont<i32, Dim32, InternedCSName<u8>>;
    type FontSystem = TfmFontSystem<i32, Dim32, InternedCSName<u8>>;
}

/// A [`TeXEngine`] combines all necessary components (see [`EngineTypes`]) to compile a TeX file into some output format.
pub trait TeXEngine: Sized {
    type Types: EngineTypes;
    /// Returns mutable references to the components of the engine.
    fn get_engine_refs(&mut self) -> EngineReferences<Self::Types>;
    /// Initializes the engine with a file, e.g. `latex.ltx` or `pdftex.cfg`.
    /// #### Errors
    /// On TeX errors in init files
    #[allow(clippy::cast_possible_wrap)]
    fn init_file(&mut self, s: &str) -> TeXResult<(), Self::Types> {
        log::debug!("Initializing with file {}", s);
        let mut comps = self.get_engine_refs();
        comps.aux.start_time = chrono::Local::now();
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.year,
            comps.aux.start_time.year().into(),
            true,
        );
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.month,
            (comps.aux.start_time.month() as i32).into(),
            true,
        );
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.day,
            (comps.aux.start_time.day() as i32).into(),
            true,
        );
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.time,
            (((comps.aux.start_time.hour() * 60) + comps.aux.start_time.minute()) as i32).into(),
            true,
        );
        let file = comps.filesystem.get(s);
        let Some(filename) = file
            .path()
            .file_stem()
            .and_then(|s| s.to_str().map(ToString::to_string))
        else {
            return Err(TeXError::General(format!("Invalid init file {s}")));
        };
        comps.aux.jobname = filename;
        comps.push_file(file);
        comps.top_loop()
    }

    /// #### Errors
    /// On TeX errors in init files
    #[allow(clippy::cast_possible_wrap)]
    fn run<
        F: FnMut(&mut EngineReferences<Self::Types>, VNode<Self::Types>) -> TeXResult<(), Self::Types>,
    >(
        &mut self,
        f: F,
    ) -> TeXResult<(), Self::Types> {
        let mut comps = self.get_engine_refs();
        comps.aux.start_time = chrono::Local::now();
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.year,
            comps.aux.start_time.year().into(),
            true,
        );
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.month,
            (comps.aux.start_time.month() as i32).into(),
            true,
        );
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.day,
            (comps.aux.start_time.day() as i32).into(),
            true,
        );
        comps.state.set_primitive_int(
            comps.aux,
            PRIMITIVES.time,
            (((comps.aux.start_time.hour() * 60) + comps.aux.start_time.minute()) as i32).into(),
            true,
        );
        comps.push_every(PRIMITIVES.everyjob);
        comps.colon = Colon::new(f);
        comps.top_loop()
    }

    /// Compile a `.tex` file. All finished pages are passed to the provided continuation.
    ///
    /// #### Errors
    /// On TeX errors
    fn do_file_default<
        F: FnMut(&mut EngineReferences<Self::Types>, VNode<Self::Types>) -> TeXResult<(), Self::Types>,
    >(
        &mut self,
        s: &str,
        f: F,
    ) -> TeXResult<(), Self::Types> {
        log::debug!("Running file {}", s);
        {
            let mut comps = self.get_engine_refs();
            let file = comps.filesystem.get(s);
            let Some(parent) = file.path().parent() else {
                return Err(TeXError::General(format!("Invalid file {s}")));
            };
            comps.filesystem.set_pwd(parent.to_path_buf());
            let Some(filename) = file
                .path()
                .file_stem()
                .and_then(|s| s.to_str().map(ToString::to_string))
            else {
                return Err(TeXError::General(format!("Invalid init file {s}")));
            };
            comps.aux.jobname = filename;
            comps.push_file(file);
        }
        self.run(f)
    }
    /// Registers all primitives of plain TeX and sets the default variables.
    fn initialize_tex_primitives(&mut self) {
        super::commands::tex::register_tex_primitives(self);
        let mag = PRIMITIVES.mag;
        let fam = PRIMITIVES.fam;
        let refs = self.get_engine_refs();
        refs.state
            .set_primitive_int(refs.aux, mag, (1000).into(), true);
        refs.state
            .set_primitive_int(refs.aux, fam, (-1).into(), true);
    }

    /// Initialize the engine by processing `plain.tex`.
    /// #### Errors
    /// On TeX errors
    fn initialize_plain_tex(&mut self) -> TeXResult<(), Self::Types> {
        self.initialize_tex_primitives();
        self.init_file("plain.tex")
    }

    /// Registers all primitives of plain TeX, e-TeX and sets the default variables.
    fn initialize_etex_primitives(&mut self) {
        self.initialize_tex_primitives();
        super::commands::etex::register_etex_primitives(self);
    }

    /// Initialize the engine by processing `eplain.tex`.
    /// #### Errors
    /// On TeX errors
    fn initialize_eplain_tex(&mut self) -> TeXResult<(), Self::Types> {
        self.initialize_etex_primitives();
        self.init_file("eplain.tex")
    }

    /// Initialized the engine by processing `latex.ltx`. Only call this (for modern LaTeX setups)
    /// after calling [`initialize_etex_primitives`](TeXEngine::initialize_etex_primitives) first.
    /// #### Errors
    /// On TeX errors
    fn load_latex(&mut self) -> TeXResult<(), Self::Types> {
        self.init_file("latex.ltx")
    }
}

/// Default implementation of a [`TeXEngine`] for the provided [`EngineTypes`].
pub struct DefaultEngine<ET: EngineTypes> {
    pub aux: EngineAux<ET>,
    pub state: ET::State,
    pub filesystem: ET::FileSystem,
    pub fontsystem: ET::FontSystem,
    pub mouth: ET::Mouth,
    pub gullet: ET::Gullet,
    pub stomach: ET::Stomach,
}
impl<ET: EngineTypes> Default for DefaultEngine<ET> {
    fn default() -> Self {
        let mut memory = MemoryManager::default();
        let mut aux = EngineAux {
            outputs: ET::Outputs::new(),
            error_handler: ET::ErrorHandler::new(),
            start_time: chrono::Local::now(),
            extension: ET::Extension::new(&mut memory),
            memory,
            jobname: String::new(),
        };
        let fontsystem = ET::FontSystem::new(&mut aux);
        let mut state = ET::State::new(fontsystem.null(), &mut aux);
        let mut mouth = ET::Mouth::new(&mut aux, &mut state);
        let gullet = ET::Gullet::new(&mut aux, &mut state, &mut mouth);
        let stomach = ET::Stomach::new(&mut aux, &mut state);
        Self {
            state,
            aux,
            fontsystem,
            filesystem: ET::FileSystem::new(crate::utils::PWD.to_path_buf()),
            mouth,
            gullet,
            stomach,
        }
    }
}
impl<ET: EngineTypes> TeXEngine for DefaultEngine<ET> {
    type Types = ET;
    fn get_engine_refs(&mut self) -> EngineReferences<ET> {
        EngineReferences {
            aux: &mut self.aux,
            state: &mut self.state,
            filesystem: &mut self.filesystem,
            fontsystem: &mut self.fontsystem,
            mouth: &mut self.mouth,
            gullet: &mut self.gullet,
            stomach: &mut self.stomach,
            colon: Colon::default(),
        }
    }
}
/// A plain TeX engine with default components.
pub type PlainTeXEngine = DefaultEngine<DefaultPlainTeXEngineTypes>;

/** Additional components we want to add to a [`EngineReferences`] can be implemented here.
   Notably, `()` extends this trait if we don't need any additional components.
*/
pub trait EngineExtension<ET: EngineTypes> {
    fn new(memory: &mut MemoryManager<ET::Token>) -> Self;
}
impl<ET: EngineTypes<Extension = ()>> EngineExtension<ET> for () {
    fn new(_memory: &mut MemoryManager<ET::Token>) -> Self {}
}

impl<ET: EngineTypes> EngineReferences<'_, ET> {
    /// Runs the provided closure and prints the result to `\write-1` iff `\tracingcommands > 0`.
    pub fn trace_command<D: std::fmt::Display, F: FnOnce(&mut Self) -> D>(&mut self, f: F) {
        let trace = self.state.get_primitive_int(PRIMITIVES.tracingcommands)
            > <ET::Num as NumSet>::Int::default();
        if trace {
            let d = f(self);
            self.aux.outputs.write_neg1(format_args!("{{{d}}}"));
        }
    }
    /// Entry point for compilation. This function is called by [`TeXEngine::do_file_default`].
    /// #### Errors
    /// On TeX errors
    pub fn top_loop(&mut self) -> TeXResult<(), ET> {
        crate::expand_loop!(ET::Stomach::every_top(self) => token => {
            if token.is_primitive() == Some(PRIMITIVES.noexpand) {
                let _ = self.get_next(false);
                continue
            }
        }; self,
            ResolvedToken::Tk { char, code } => ET::Stomach::do_char(self, token, char, code)?,
            ResolvedToken::Cmd(Some(TeXCommand::Char {char, code})) => ET::Stomach::do_char(self, token, *char, *code)?,
            ResolvedToken::Cmd(None) => TeXError::undefined(self.aux,self.state,self.mouth,&token)?,
            ResolvedToken::Cmd(Some(cmd)) => crate::do_cmd!(self,token,cmd)
        );
        Ok(())
    }
}

/// Expands tokens until a non-expandable Token is found, the [`ResolvedToken`] of which is then matched by the
/// provided `$case` patterns.
#[macro_export]
macro_rules! expand_loop {
    ($engine:ident,$tk:ident,$($case:tt)*) => {{
        $crate::expand_loop!(ET;$engine,$tk,$($case)*)
    }};
    ($then:expr => $engine:ident,$tk:ident,$($case:tt)*) => {{
        $then;
        while let Some($tk) = $engine.get_next(false)? {
            $crate::expand!($engine,$tk;$($case)*);
            $then;
        }
    }};
    ($then:expr => $tk:ident => $first:expr; $engine:ident,$($case:tt)*) => {{
        $then;
        while let Some($tk) = $engine.get_next(false)? {
            $first;
            $crate::expand!($engine,$tk;$($case)*);
            $then;
        }
    }};
    ($tk:ident => $first:expr; $engine:ident,$($case:tt)*) => {{
        while let Some($tk) = $engine.get_next(false)? {
            $first;
            $crate::expand!($engine,$tk;$($case)*);
        }
    }};
    ($ET:ty; $engine:ident,$tk:ident,$($case:tt)*) => {{
        while let Some($tk) = $engine.get_next(false)? {
            $crate::expand!($ET;$engine,$tk;$($case)*);
        }
    }};
    ($ET:ty; $tk:ident => $first:expr; $engine:ident,$($case:tt)*) => {{
        while let Some($tk) = $engine.get_next(false)? {
            $first;
            $crate::expand!($ET;$engine,$tk;$($case)*);
        }
    }}
}

/// Expands the provided token or, if not expandable, matches the [`ResolvedToken`] against the provided `$case` patterns.
#[macro_export]
macro_rules! expand {
    ($engine:ident,$tk:expr;$($case:tt)*) => {
        $crate::expand!(ET;$engine,$tk;$($case)*)
    };
    ($ET:ty; $engine:ident,$token:expr;$($case:tt)*) => {
        let cmd = <<$ET as EngineTypes>::Gullet as $crate::engine::gullet::Gullet<$ET>>::resolve($engine.state,&$token);
        match cmd {
            $crate::commands::ResolvedToken::Cmd(Some($crate::commands::TeXCommand::Macro(m))) =>
                <<$ET as EngineTypes>::Gullet as $crate::engine::gullet::Gullet<$ET>>::do_macro($engine,m.clone(),$token)?,
            $crate::commands::ResolvedToken::Cmd(Some($crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Conditional(cond)})) =>
                <<$ET as EngineTypes>::Gullet as $crate::engine::gullet::Gullet<$ET>>::do_conditional($engine,*name,$token,*cond,false)?,
            $crate::commands::ResolvedToken::Cmd(Some($crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Expandable(e)})) =>
                <<$ET as EngineTypes>::Gullet as $crate::engine::gullet::Gullet<$ET>>::do_expandable($engine,*name,$token,*e)?,
            $crate::commands::ResolvedToken::Cmd(Some($crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::SimpleExpandable(e)})) =>
                <<$ET as EngineTypes>::Gullet as $crate::engine::gullet::Gullet<$ET>>::do_simple_expandable($engine,*name,$token,*e)?,
            $($case)*
        }
    }
}

/// Default treatment of unexpandable tokens, i.e. passed to the relevant [`Stomach`] method or throws
/// [`ErrorHandler::not_allowed_in_mode`] errors.
#[macro_export]
macro_rules! do_cmd {
    ($engine:ident,$token:expr,$cmd:ident) => {
        $crate::do_cmd!(ET;$engine,$token,$cmd)
    };
    ($ET:ty;$engine:ident,$token:expr,$cmd:ident) => {
        match $cmd {
            /*$crate::commands::TeXCommand::CharDef(char) if <$ET as EngineTypes>::Stomach::data_mut($engine.stomach).mode().is_math() =>
                <$ET as EngineTypes>::Stomach::do_char_in_math($engine, *char)?,*/
            $crate::commands::TeXCommand::CharDef(char)  => <$ET as EngineTypes>::Stomach::do_defed_char($engine, $token, *char)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Unexpandable { scope, apply }} =>
                <$ET as EngineTypes>::Stomach::do_unexpandable($engine, *name, *scope,$token, *apply)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Assignment(assign)} =>
                <$ET as EngineTypes>::Stomach::do_assignment($engine, *name, $token, *assign,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Int {  assign: Some(assign), .. }} =>
                <$ET as EngineTypes>::Stomach::do_assignment($engine, *name, $token, *assign,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Dim { assign: Some(assign), .. }} =>
                <$ET as EngineTypes>::Stomach::do_assignment($engine, *name, $token, *assign,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Skip { assign: Some(assign), .. }} =>
                <$ET as EngineTypes>::Stomach::do_assignment($engine, *name, $token, *assign,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::MuSkip { assign: Some(assign), .. }} =>
                <$ET as EngineTypes>::Stomach::do_assignment($engine, *name, $token, *assign,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::FontCmd { assign: Some(assign), .. }} =>
                <$ET as EngineTypes>::Stomach::do_assignment($engine, *name, $token, *assign,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Box(read)} =>
                <$ET as EngineTypes>::Stomach::do_box($engine, *name, $token, *read)?,
            $crate::commands::TeXCommand::Font(f) =>
                <$ET as EngineTypes>::Stomach::assign_font($engine, $token, f.clone(),false)?,
            $crate::commands::TeXCommand::IntRegister(u) =>
                <$ET as EngineTypes>::Stomach::assign_int_register($engine, *u,false,$token)?,
            $crate::commands::TeXCommand::DimRegister(u) =>
                <$ET as EngineTypes>::Stomach::assign_dim_register($engine, *u,false,$token)?,
            $crate::commands::TeXCommand::SkipRegister(u) =>
                <$ET as EngineTypes>::Stomach::assign_skip_register($engine, *u,false,$token)?,
            $crate::commands::TeXCommand::MuSkipRegister(u) =>
               <$ET as EngineTypes>::Stomach::assign_muskip_register($engine, *u,false,$token)?,
            $crate::commands::TeXCommand::ToksRegister(u) =>
                <$ET as EngineTypes>::Stomach::assign_toks_register($engine,$token, *u,false)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::Whatsit { get, .. }} =>
                <$ET as EngineTypes>::Stomach::do_whatsit($engine, *name,$token, *get)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::PrimitiveInt} =>
                <$ET as EngineTypes>::Stomach::assign_primitive_int($engine,*name,false,$token)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::PrimitiveDim} =>
                <$ET as EngineTypes>::Stomach::assign_primitive_dim($engine,*name,false,$token)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::PrimitiveSkip} =>
                <$ET as EngineTypes>::Stomach::assign_primitive_skip($engine,*name,false,$token)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::PrimitiveMuSkip} =>
                <$ET as EngineTypes>::Stomach::assign_primitive_muskip($engine,*name,false,$token)?,
            $crate::commands::TeXCommand::Primitive{name,cmd:$crate::commands::PrimitiveCommand::PrimitiveToks} =>
                <$ET as EngineTypes>::Stomach::assign_primitive_toks($engine,$token,*name,false)?,
            $crate::commands::TeXCommand::MathChar(u) if <$ET as EngineTypes>::Stomach::data_mut($engine.stomach).mode().is_math() =>
                <$ET as EngineTypes>::Stomach::do_mathchar($engine,*u,Some($token)),
            $crate::commands::TeXCommand::Primitive{cmd:$crate::commands::PrimitiveCommand::Relax,..} => (),
            $crate::commands::TeXCommand::Primitive{
                cmd:$crate::commands::PrimitiveCommand::Int { .. } |
                    $crate::commands::PrimitiveCommand::Dim { .. } |
                    $crate::commands::PrimitiveCommand::Skip { .. } |
                    $crate::commands::PrimitiveCommand::MuSkip { .. } |
                    $crate::commands::PrimitiveCommand::FontCmd { .. }
                ,name
            } =>
                TeXError::not_allowed_in_mode($engine.aux,$engine.state,$engine.mouth,*name,
                    <$ET as EngineTypes>::Stomach::data_mut($engine.stomach).mode()
                )?,
            $crate::commands::TeXCommand::MathChar(_) =>
                TeXError::not_allowed_in_mode($engine.aux,$engine.state,$engine.mouth,
                    $crate::commands::primitives::PRIMITIVES.mathchar,
                    <$ET as EngineTypes>::Stomach::data_mut($engine.stomach).mode()
                )?,
            $crate::commands::TeXCommand::Macro(_) |
            $crate::commands::TeXCommand::Primitive{ cmd:$crate::commands::PrimitiveCommand::Conditional { .. } |
                $crate::commands::PrimitiveCommand::Expandable { .. } |
                $crate::commands::PrimitiveCommand::SimpleExpandable { .. },..
            } | TeXCommand::Char {..} => unreachable!(),
        }
    }
}
