use crate::engine::extension::CSS;
use crate::shipout;
use crate::shipout::html::{CompilationDisplay, ImageOptions};
use crate::shipout::state::{FontData, Shipout, ShipoutNodeV, ShipoutWrapper, Top};
use crate::utils::{VecMap, VecSet};
use extension::RusTeXExtension;
use fonts::Fontsystem;
use nodes::RusTeXNode;
use output::RusTeXOutput;
use state::RusTeXState;
use std::fmt::Display;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use stomach::RusTeXStomach;
use tex_engine::commands::{Macro, TeXCommand};
use tex_engine::engine::filesystem::FileSystem;
use tex_engine::engine::filesystem::{File, SourceReference, VirtualFile};
use tex_engine::engine::fontsystem::FontSystem;
use tex_engine::engine::gullet::DefaultGullet;
use tex_engine::engine::gullet::Gullet;
use tex_engine::engine::mouth::DefaultMouth;
use tex_engine::engine::mouth::Mouth;
use tex_engine::engine::state::State as OrigState;
use tex_engine::engine::stomach::Stomach as StomachT;
use tex_engine::engine::utils::memory::MemoryManager;
use tex_engine::engine::EngineExtension;
use tex_engine::engine::TeXEngine;
use tex_engine::engine::{DefaultEngine, EngineAux, EngineReferences, EngineTypes};
use tex_engine::pdflatex::nodes::PDFColor;
use tex_engine::pdflatex::PDFTeXEngine;
use tex_engine::prelude::*;
use tex_engine::tex;
use tex_engine::tex::catcodes::AT_LETTER_SCHEME;
use tex_engine::tex::numerics::{Dim32, Mu};
use tex_engine::tex::tokens::CompactToken;
use tex_engine::utils::errors::{ErrorThrower, TeXError, TeXResult};
use tex_engine::utils::HMap;

pub mod commands;
pub(crate) mod extension;
pub mod files;
pub mod fonts;
pub(crate) mod nodes;
pub mod output;
pub(crate) mod pgf;
pub mod state;
pub mod stomach;

pub type Extension = RusTeXExtension;
pub(crate) type Font = tex_engine::engine::fontsystem::TfmFont<i32, Dim32, InternedCSName<u8>>;
pub(crate) type SRef = SourceReference<<<Types as EngineTypes>::File as File>::SourceRefID>;
pub(crate) type Refs<'a, 'b> = &'a mut EngineReferences<'b, Types>;
pub(crate) type CSName = InternedCSName<u8>;

#[derive(Clone, Debug, Copy)]
pub struct Types;

pub type Res<R> = TeXResult<R, Types>;

impl EngineTypes for Types {
    type Char = u8;
    type CSName = CSName;
    type Token = CompactToken;
    type Extension = Extension;
    type File = VirtualFile<u8>;
    type FileSystem = files::RusTeXFileSystem;
    type Int = i32;
    type Dim = Dim32;
    type MuDim = Mu;
    type Num = tex::numerics::DefaultNumSet;
    type State = RusTeXState;
    type Outputs = RusTeXOutput;
    type Mouth = DefaultMouth<Self>;
    type Gullet = DefaultGullet<Self>;
    type Stomach = RusTeXStomach;
    type CustomNode = RusTeXNode;
    type Font = tex_engine::engine::fontsystem::TfmFont<i32, Dim32, InternedCSName<u8>>;
    type FontSystem = Fontsystem;
    type ErrorHandler = ErrorThrower<Self>;
}

thread_local! {
    static MAIN_STATE : Mutex<Option<(RusTeXState,MemoryManager<CompactToken>)>> = const { Mutex::new(None) };
    static FONT_SYSTEM : Mutex<Option<Fontsystem>> = const { Mutex::new(None) };
}

fn get_state(log: bool) -> (RusTeXState, MemoryManager<CompactToken>) {
    MAIN_STATE.with(|state| {
        let mut guard = state.lock().unwrap();
        match &mut *guard {
            Some((s, m)) => (s.clone(), m.clone()),
            n => {
                //let start = std::time::Instant::now();
                let mut engine = DefaultEngine::<Types>::default();
                if log {
                    engine.aux.outputs = RusTeXOutput::Print(true);
                }
                commands::register_primitives_preinit(&mut engine);
                engine.initialize_pdflatex().unwrap();
                commands::register_primitives_postinit(&mut engine);
                engine.init_file("rustex_defs.def").unwrap();
                *n = Some((engine.state.clone(), engine.aux.memory.clone()));
                FONT_SYSTEM.with(|f| f.lock().unwrap().replace(engine.fontsystem.clone()));
                //println!("Initialized in {:?}", start.elapsed());
                (engine.state, engine.aux.memory)
            }
        }
    })
}

fn get_engine(log: bool) -> DefaultEngine<Types> {
    let (mut state, mut memory) = get_state(log);
    let fontsystem = FONT_SYSTEM.with(|f| f.lock().unwrap().clone()).unwrap();
    let mut aux = EngineAux {
        outputs: RusTeXOutput::None,
        error_handler: ErrorThrower::new(),
        start_time: chrono::Local::now(),
        extension: Extension::new(&mut memory),
        memory,
        jobname: String::new(),
    };
    let mut mouth = DefaultMouth::new(&mut aux, &mut state);
    let gullet = DefaultGullet::new(&mut aux, &mut state, &mut mouth);
    let stomach = RusTeXStomach::new(&mut aux, &mut state);
    DefaultEngine {
        state,
        aux,
        fontsystem,
        filesystem: files::RusTeXFileSystem::new(tex_engine::utils::PWD.to_path_buf()),
        mouth,
        gullet,
        stomach,
    }
}

pub struct CompilationResult {
    out: Vec<ShipoutNodeV>,
    pub error: Option<(TeXError<Types>, Vec<FileTrace>)>,
    pub font_data: HMap<Box<str>, FontData>,
    top_font: Font,
    top_width: i32,
    page_width: i32,
    sourcerefs: bool,
    metas: Vec<VecMap<String, String>>,
    top: VecMap<String, String>,
    img: ImageOptions,
    css: VecSet<CSS>,
    pub font_info: bool,
}
impl CompilationResult {
    pub fn write_out(&self, path: &Path) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        let mut f = BufWriter::new(&mut f);
        write!(f, "{self}")?;
        f.flush()
    }
}
impl Display for CompilationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut dsp = CompilationDisplay {
            color: PDFColor::default(),
            font: self.top_font.clone(),
            width: self.top_width,
            indent: 0,
            in_link: false,
            attrs: VecMap::default(),
            styles: VecMap::default(),
            sourcerefs: self.sourcerefs,
            font_data: &self.font_data,
            image: &self.img,
            font_info: self.font_info,
            f,
        };
        dsp.display(
            &self.metas,
            &self.top,
            &self.css.inner,
            self.page_width,
            &self.out,
        )
    }
}

pub trait RusTeXEngineT {
    fn initialize(log: bool);
    fn get() -> Self;
    fn run<S: AsRef<str>>(&mut self, file: S, settings: Settings) -> CompilationResult;
    fn do_file<S: AsRef<str>>(file: S, settings: Settings) -> CompilationResult;
}

pub(crate) fn register_command(
    e: &mut DefaultEngine<Types>,
    globally: bool,
    name: &'static str,
    sig: &'static str,
    exp: &'static str,
    protect: bool,
    long: bool,
) {
    let e = e.get_engine_refs();
    let name = e.aux.memory.cs_interner_mut().cs_from_str(name);
    let mut cmd =
        Macro::new::<_, _, Types>(e.aux.memory.cs_interner_mut(), &AT_LETTER_SCHEME, sig, exp)
            .unwrap();
    if protect {
        cmd.protected = true
    }
    if long {
        cmd.long = true
    }
    e.state
        .set_command(e.aux, name, Some(TeXCommand::Macro(cmd)), globally)
}

#[derive(Default)]
pub struct Settings {
    pub sourcerefs: bool,
    pub verbose: bool,
    pub log: bool,
    pub image_options: ImageOptions,
    pub insert_font_info: bool,
}

/*pub struct RusTeXEngine {
    engine: DefaultEngine<Types>
}*/
pub type RusTeXEngine = DefaultEngine<Types>;

mod sealed {
    pub trait Sealed {}
}
impl sealed::Sealed for RusTeXEngine {}

pub trait RusTeXEngineExt: sealed::Sealed {
    fn run_string(&mut self, file: PathBuf, content: &str) -> Option<TeXError<Types>>;
    fn do_result(
        &mut self,
        result: Option<TeXError<Types>>,
        settings: Settings,
    ) -> CompilationResult;
}

impl RusTeXEngineExt for RusTeXEngine {
    fn run_string(&mut self, file: PathBuf, content: &str) -> Option<TeXError<Types>> {
        let s = file.display().to_string();
        self.filesystem
            .set_pwd(file.parent().unwrap().to_path_buf());
        self.filesystem.add_file(file, content);
        let r = match self.do_file_pdf(&s, shipout::shipout) {
            Ok(_) => None,
            Err(e) => {
                self.aux.outputs.errmessage(format!(
                    "{}\n\nat {}",
                    e,
                    self.mouth.current_sourceref().display(&self.filesystem)
                ));
                Some(e)
            }
        };
        {
            let mut s = std::mem::take(&mut self.aux.extension.state);
            let mut refs = self.get_engine_refs();
            let mut istate = Shipout {
                nodes: std::mem::take(&mut s.output),
                state: Top,
                wrapper: std::mem::take(&mut s.wrapper),
                previous: std::mem::take(&mut s.previous),
                top_state: &mut s,
                engine: &mut refs,
            };
            let _ = ShipoutWrapper::close_all(&mut istate);
            let nodes = std::mem::take(&mut istate.nodes);
            drop(istate);
            drop(refs);
            s.output = nodes;
            self.aux.extension.state = s;
        }
        r
    }
    fn do_result(
        &mut self,
        result: Option<TeXError<Types>>,
        settings: Settings,
    ) -> CompilationResult {
        let result = result.map(|e| {
            let filetrace = self
                .mouth
                .file_trace()
                .filter_map(|sr| {
                    let id = sr.file?;
                    let file = self.filesystem.ref_str(Some(id));
                    let file = self
                        .filesystem
                        .inner
                        .kpse
                        .pwd
                        .join(file)
                        .canonicalize()
                        .ok()?;
                    Some(FileTrace {
                        file,
                        line: sr.line as u32,
                        col: sr.column as u32,
                    })
                })
                .collect::<Vec<_>>();
            (e, filetrace)
        });
        {
            let mut s = std::mem::take(&mut self.aux.extension.state);
            let mut refs = self.get_engine_refs();
            let mut istate = Shipout {
                nodes: std::mem::take(&mut s.output),
                state: Top,
                wrapper: std::mem::take(&mut s.wrapper),
                previous: std::mem::take(&mut s.previous),
                top_state: &mut s,
                engine: &mut refs,
            };
            let _ = ShipoutWrapper::close_all(&mut istate);
            let nodes = std::mem::take(&mut istate.nodes);
            drop(istate);
            drop(refs);
            s.output = nodes;
            self.aux.extension.state = s;
        }
        let out = std::mem::take(&mut self.aux.extension.state.output);
        let css = std::mem::take(&mut self.aux.extension.css);
        let font_data = std::mem::take(&mut self.aux.extension.state.font_data);
        let top_font = self
            .aux
            .extension
            .state
            .top_font
            .clone()
            .unwrap_or_else(|| self.fontsystem.null());
        let top_width = self
            .aux
            .extension
            .state
            .top_width
            .clone()
            .unwrap_or_default();
        let page_width = self
            .aux
            .extension
            .state
            .page_width
            .clone()
            .unwrap_or_default();
        let top = std::mem::take(&mut self.aux.extension.top);
        let metas = std::mem::take(&mut self.aux.extension.metas);
        CompilationResult {
            out,
            error: result,
            css,
            font_data,
            top_font,
            top_width,
            top,
            metas,
            page_width,
            sourcerefs: settings.sourcerefs,
            font_info: settings.insert_font_info,
            img: settings.image_options,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileTrace {
    pub file: PathBuf,
    pub line: u32,
    pub col: u32,
}

impl RusTeXEngineT for RusTeXEngine {
    fn initialize(log: bool) {
        let _ = get_engine(log);
    }

    fn get() -> Self {
        get_engine(false)
    }

    fn run<S: AsRef<str>>(&mut self, file: S, settings: Settings) -> CompilationResult {
        let res = match self.do_file_pdf(file.as_ref(), shipout::shipout) {
            Ok(_) => None,
            Err(e) => {
                self.aux.outputs.errmessage(format!(
                    "{}\n\nat {}",
                    e,
                    self.mouth.current_sourceref().display(&self.filesystem)
                ));
                Some(e)
            }
        };
        self.do_result(res, settings)
    }

    fn do_file<S: AsRef<str>>(file: S, settings: Settings) -> CompilationResult {
        let mut engine = Self::get();
        engine.stomach.continuous = true;
        if settings.log {
            engine.aux.outputs = RusTeXOutput::Print(settings.verbose);
        }
        let ret = <Self as RusTeXEngineT>::run(&mut engine, file, settings);
        FONT_SYSTEM.with(|f| f.lock().unwrap().replace(engine.fontsystem));
        ret
    }
}
