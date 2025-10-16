use crate::engine::utils::memory::MemoryManager;
use crate::engine::{EngineExtension, EngineReferences, EngineTypes};
use crate::tex::nodes::boxes::TeXBox;
use crate::tex::nodes::CustomNodeTrait;
use crate::tex::nodes::{display_do_indent, NodeTrait, NodeType};
use crate::tex::numerics::TeXDimen;
use crate::utils::errors::{TeXError, TeXResult};
use std::fmt::Formatter;
use std::path::{Path, PathBuf};

const PDFIUM_VERSION: &str = "7350";

#[derive(Clone, Debug)]
pub enum PDFNode<ET: EngineTypes> {
    Obj(PDFObj),
    XForm(PDFXForm<ET>),
    XImage(PDFXImage<ET>),
    PDFLiteral(PDFLiteral),
    PDFOutline(PDFOutline),
    PDFCatalog(PDFCatalog),
    PDFPageAttr(Box<[ET::Token]>),
    PDFPagesAttr(Box<[ET::Token]>),
    PDFDest(PDFDest<ET::Dim>),
    Color(ColorStackAction),
    PDFStartLink(PDFStartLink<ET>),
    PDFAnnot(PDFAnnot<ET>),
    PDFEndLink,
    PDFSave,
    PDFRestore,
    PDFMatrix {
        scale: f32,
        rotate: f32,
        skewx: f32,
        skewy: f32,
    },
}

#[derive(Debug, Clone)]
pub enum ActionSpec {
    User(String),
    Goto(GotoAction),
    Thread {
        file: Option<String>,
        target: NumOrName,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum PDFBoxSpec {
    MediaBox,
    CropBox,
    BleedBox,
    TrimBox,
    ArtBox,
}

#[derive(Debug, Clone)]
pub enum GotoAction {
    File {
        filename: String,
        struct_: Option<PDFStruct>,
        page: Option<i64>,
        target: String,
        newwindow: Option<bool>,
    },
    Current {
        struct_: Option<PDFStruct>,
        page: Option<i64>,
        target: NumOrName,
    },
}

#[derive(Debug, Clone)]
pub enum PDFDestType<D: TeXDimen> {
    XYZ {
        zoom: Option<i64>,
    },
    Fitr {
        width: Option<D>,
        height: Option<D>,
        depth: Option<D>,
    },
    Fitbh,
    Fitbv,
    Fitb,
    Fith,
    Fitv,
    Fit,
}

#[derive(Debug, Clone)]
pub enum NumOrName {
    Num(i64),
    Name(String),
}
impl NumOrName {
    pub fn as_name(self) -> String {
        match self {
            NumOrName::Name(s) => s,
            NumOrName::Num(i) => format!("NUM_{}", i),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PDFStruct {
    Num(i64),
    Name(String),
    Other(String),
}

pub fn num_or_name<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    token: &ET::Token,
) -> TeXResult<Option<NumOrName>, ET> {
    match engine.read_keywords(&[b"num", b"name"])? {
        Some(b"num") => Ok(Some(NumOrName::Num(engine.read_int(false, token)?.into()))),
        Some(b"name") => {
            let mut str = String::new();
            engine.read_maybe_braced_string(true, &mut str, token)?;
            Ok(Some(NumOrName::Name(str)))
        }
        _ => Ok(None),
    }
}

pub fn pdfdest_type<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: &ET::Token,
) -> TeXResult<PDFDestType<ET::Dim>, ET> {
    match engine.read_keywords(&[
        b"xyz", b"fitr", b"fitbh", b"fitbv", b"fitb", b"fith", b"fitv", b"fit",
    ])? {
        Some(b"xyz") => {
            let zoom = if engine.read_keyword(b"zoom")? {
                Some(engine.read_int(false, tk)?.into())
            } else {
                None
            };
            Ok(PDFDestType::XYZ { zoom })
        }
        Some(b"fitr") => {
            let mut width = None;
            let mut height = None;
            let mut depth = None;
            loop {
                match engine.read_keywords(&[b"width", b"height", b"depth"])? {
                    Some(b"width") => width = Some(engine.read_dim(false, tk)?),
                    Some(b"height") => height = Some(engine.read_dim(false, tk)?),
                    Some(b"depth") => depth = Some(engine.read_dim(false, tk)?),
                    _ => break,
                }
            }
            Ok(PDFDestType::Fitr {
                width,
                height,
                depth,
            })
        }
        Some(b"fitbh") => Ok(PDFDestType::Fitbh),
        Some(b"fitbv") => Ok(PDFDestType::Fitbv),
        Some(b"fitb") => Ok(PDFDestType::Fitb),
        Some(b"fith") => Ok(PDFDestType::Fith),
        Some(b"fitv") => Ok(PDFDestType::Fitv),
        Some(b"fit") => Ok(PDFDestType::Fit),
        _ => {
            TeXError::missing_keyword(
                engine.aux,
                engine.state,
                engine.mouth,
                &[
                    "xyz", "fitr", "fitbh", "fitbv", "fitb", "fith", "fitv", "fit",
                ],
            )?;
            Ok(PDFDestType::XYZ { zoom: None })
        }
    }
}

pub fn action_spec<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    token: &ET::Token,
) -> TeXResult<ActionSpec, ET> {
    match engine.read_keywords(&[b"user", b"goto", b"thread"])? {
        Some(b"user") => {
            let mut ret = String::new();
            engine.read_braced_string(true, true, token, &mut ret)?;
            Ok(ActionSpec::User(ret))
        }
        Some(b"goto") => {
            let file = if engine.read_keyword(b"file")? {
                let mut file = String::new();
                engine.read_braced_string(true, true, token, &mut file)?;
                Some(file)
            } else {
                None
            };
            let struct_ = if engine.read_keyword(b"struct")? {
                match num_or_name(engine, token)? {
                    None => {
                        let mut ret = String::new();
                        engine.read_braced_string(true, true, token, &mut ret)?;
                        Some(PDFStruct::Other(ret))
                    }
                    Some(NumOrName::Num(i)) => Some(PDFStruct::Num(i)),
                    Some(NumOrName::Name(s)) => Some(PDFStruct::Name(s)),
                }
            } else {
                None
            };
            match file {
                None => match num_or_name(engine, token)? {
                    Some(n) => Ok(ActionSpec::Goto(GotoAction::Current {
                        struct_,
                        page: None,
                        target: n,
                    })),
                    _ => {
                        let page = if engine.read_keyword(b"page")? {
                            Some(engine.read_int(false, token)?.into())
                        } else {
                            None
                        };
                        let mut str = String::new();
                        engine.read_braced_string(true, true, token, &mut str)?;
                        Ok(ActionSpec::Goto(GotoAction::Current {
                            struct_,
                            page,
                            target: NumOrName::Name(str),
                        }))
                    }
                },
                Some(filename) => {
                    let (page, target) = if engine.read_keyword(b"name")? {
                        let mut str = String::new();
                        engine.read_braced_string(true, true, token, &mut str)?;
                        (None, str)
                    } else {
                        let page = if engine.read_keyword(b"page")? {
                            Some(engine.read_int(false, token)?.into())
                        } else {
                            None
                        };
                        let mut str = String::new();
                        engine.read_braced_string(true, true, token, &mut str)?;
                        (page, str)
                    };
                    let newwindow = match engine.read_keywords(&[b"newwindow", b"nonewwindow"])? {
                        Some(b"newwindow") => Some(true),
                        Some(b"nonewwindow") => Some(false),
                        _ => None,
                    };
                    Ok(ActionSpec::Goto(GotoAction::File {
                        filename,
                        struct_,
                        page,
                        target,
                        newwindow,
                    }))
                }
            }
        }
        Some(b"thread") => {
            let file = if engine.read_keyword(b"file")? {
                let mut file = String::new();
                engine.read_braced_string(true, true, token, &mut file)?;
                Some(file)
            } else {
                None
            };

            match num_or_name(engine, token)? {
                Some(n) => Ok(ActionSpec::Thread { file, target: n }),
                None => {
                    TeXError::missing_keyword(
                        engine.aux,
                        engine.state,
                        engine.mouth,
                        &["num", "name"],
                    )?;
                    Ok(ActionSpec::Thread {
                        file,
                        target: NumOrName::Num(0),
                    })
                }
            }
        }
        None => {
            TeXError::missing_keyword(
                engine.aux,
                engine.state,
                engine.mouth,
                &["user", "goto", "thread"],
            )?;
            Ok(ActionSpec::User(String::new()))
        }
        _ => unreachable!(),
    }
}

// ----------------------------------------------------------------------------------------

#[derive(Copy, Clone, Debug)]
pub enum ColorStackAction {
    Current(usize),
    Push(usize, PDFColor),
    Pop(usize),
    Set(usize, PDFColor),
}

#[derive(Clone, Debug)]
pub struct PDFStartLink<ET: EngineTypes> {
    pub width: Option<ET::Dim>,
    pub height: Option<ET::Dim>,
    pub depth: Option<ET::Dim>,
    pub attr: Option<String>,
    pub action: ActionSpec,
}

#[derive(Clone, Debug)]
pub struct PDFDest<D: TeXDimen> {
    pub structnum: Option<i64>,
    pub id: NumOrName,
    pub dest: PDFDestType<D>,
}

#[derive(Clone, Debug)]
pub struct PDFLiteral {
    pub option: PDFLiteralOption,
    pub literal: String,
}

impl<ET: EngineTypes> CustomNodeTrait<ET> for PDFNode<ET> where ET::CustomNode: From<PDFNode<ET>> {}

impl<ET: EngineTypes> NodeTrait<ET> for PDFNode<ET>
where
    ET::CustomNode: From<PDFNode<ET>>,
{
    fn height(&self) -> ET::Dim {
        match self {
            PDFNode::XImage(img) => img.height(),
            _ => ET::Dim::default(),
        }
    }
    fn width(&self) -> ET::Dim {
        match self {
            PDFNode::XImage(img) => img.width(),
            _ => ET::Dim::default(),
        }
    }
    fn depth(&self) -> ET::Dim {
        match self {
            PDFNode::XImage(img) => img.depth(),
            _ => ET::Dim::default(),
        }
    }
    fn nodetype(&self) -> NodeType {
        NodeType::WhatsIt
    }
    fn opaque(&self) -> bool {
        matches!(self, PDFNode::Color(_))
    }
    fn display_fmt(&self, indent: usize, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_do_indent(indent, f)?;
        match self {
            PDFNode::PDFDest(PDFDest {
                structnum,
                id,
                dest,
            }) => write!(
                f,
                "<pdfdest structnum=\"{:?}\", id=\"{:?}\", dest=\"{:?}\">",
                structnum, id, dest
            ),
            PDFNode::PDFCatalog(c) => write!(
                f,
                "<pdfcatalog literal=\"{}\", action=\"{:?}\">",
                c.literal, c.action
            ),
            PDFNode::PDFOutline(o) => write!(
                f,
                "<pdfoutline attr=\"{}\", action=\"{:?}\", count=\"{:?}\", content=\"{}\">",
                o.attr, o.action, o.count, o.content
            ),
            PDFNode::Obj(o) => write!(f, "<pdfobj literal=\"{}\">", o.0),
            PDFNode::PDFAnnot(a) => write!(
                f,
                "<pdfannot width=\"{:?}\", height=\"{:?}\", depth=\"{:?}\", content=\"{}\">",
                a.width, a.height, a.depth, a.content
            ),
            PDFNode::PDFMatrix {
                scale,
                rotate,
                skewx,
                skewy,
            } => write!(
                f,
                "<pdfmatrix scale=\"{}\", rotate=\"{}\", skewx=\"{}\", skewy=\"{}\">",
                scale, rotate, skewx, skewy
            ),
            PDFNode::XForm(x) => {
                write!(
                    f,
                    "<pdfxform attr=\"{}\", resources=\"{}\">",
                    x.attr, x.resources
                )?;
                if let Some(bx) = &x.bx {
                    bx.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                write!(f, "</pdfxform>")
            }
            PDFNode::XImage(img) => write!(f, "<pdfximage {}>", img.filepath.display()),
            PDFNode::PDFPageAttr(_) => write!(f, "<pdfpageattr/>"),
            PDFNode::PDFPagesAttr(_) => write!(f, "<pdfpagesattr/>"),
            PDFNode::PDFSave => write!(f, "<pdfsave>"),
            PDFNode::PDFRestore => write!(f, "<pdfrestore>"),
            PDFNode::PDFLiteral(PDFLiteral { option, literal }) => write!(
                f,
                "<pdfliteral option=\"{:?}\", literal=\"{:?}\">",
                option, literal
            ),
            PDFNode::Color(c) => match c {
                ColorStackAction::Current(i) => write!(f, "<pdfcolorstack current=\"{}\">", i),
                ColorStackAction::Push(i, c) => {
                    write!(f, "<pdfcolorstack push=\"{}\", color=\"{:?}\">", i, c)
                }
                ColorStackAction::Pop(i) => write!(f, "<pdfcolorstack pop=\"{}\">", i),
                ColorStackAction::Set(i, c) => {
                    write!(f, "<pdfcolorstack set=\"{}\", color=\"{:?}\">", i, c)
                }
            },
            PDFNode::PDFStartLink(PDFStartLink {
                width,
                height,
                depth,
                attr,
                action,
            }) => {
                write!(f, "<pdfstartlink")?;
                if let Some(w) = width {
                    write!(f, " width=\"{:?}\"", w)?;
                }
                if let Some(h) = height {
                    write!(f, " height=\"{:?}\"", h)?;
                }
                if let Some(d) = depth {
                    write!(f, " depth=\"{:?}\"", d)?;
                }
                if let Some(a) = attr {
                    write!(f, " attr=\"{:?}\"", a)?;
                }
                write!(f, " action=\"{:?}\">", action)
            }
            PDFNode::PDFEndLink => write!(f, "<pdfendlink>"),
        }
    }
}

#[derive(Copy, Clone, Debug)]
pub enum PDFLiteralOption {
    None,
    Direct,
    Page,
}

#[cfg(all(target_os = "windows", feature = "pdfium"))]
const PDFIUM_NAME: &str = "pdfium.dll";
#[cfg(all(not(target_os = "windows"), feature = "pdfium"))]
const PDFIUM_NAME: &str = "libpdfium.so";

#[cfg(feature = "pdfium")]
static PDFIUM_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(feature = "pdfium")]
fn download_pdfium(lib_dir: &std::path::Path) {
    const BASE_URL: &str =
        "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium";

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    const PLATFORM: &str = "win-x64";
    #[cfg(all(target_os = "windows", target_arch = "x86"))]
    const PLATFORM: &str = "win-x86";
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const PLATFORM: &str = "linux-x64";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const PLATFORM: &str = "linux-arm64";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    const PLATFORM: &str = "mac-x64";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    const PLATFORM: &str = "mac-arm64";

    let _ = std::fs::create_dir_all(lib_dir);
    let download_url = format!("{BASE_URL}/{PDFIUM_VERSION}/pdfium-{PLATFORM}.tgz");
    let archive_path = lib_dir.join("pdfium.tgz");
    let Ok(mut response) = reqwest::blocking::get(download_url) else {
        log::warn!("Could not download pdfium");
        return;
    }; //.expect("Could not download pdfium");
    let Ok(mut dest) = std::fs::File::create(&archive_path) else {
        return;
    };
    let Ok(_) = std::io::copy(&mut response, &mut dest) else {
        return;
    };
    let Ok(tar_gz) = std::fs::File::open(&archive_path) else {
        return;
    };
    let tar = flate2::read::GzDecoder::new(tar_gz);
    let mut archive = tar::Archive::new(tar);
    let Ok(entries) = archive.entries() else {
        return;
    };

    for entry in entries {
        let Ok(mut entry) = entry else { continue };
        let Ok(entry_path) = entry.path() else {
            continue;
        };
        let Some(name) = entry_path.file_name() else {
            continue;
        };
        if name.to_str() == Some(PDFIUM_NAME) {
            let file_path = lib_dir.join(name);
            let Ok(_) = entry.unpack(file_path) else {
                continue;
            };
            break;
        }
    }
    let _ = std::fs::remove_file(archive_path);
}

pub trait PDFExtension<ET: EngineTypes>: EngineExtension<ET> {
    fn pdfmatches(&mut self) -> &mut Vec<String>;
    fn elapsed(&mut self) -> &mut std::time::Instant;
    fn colorstacks(&mut self) -> &mut Vec<Vec<PDFColor>>;
    fn current_colorstack(&mut self) -> &mut usize;
    fn pdfobjs(&mut self) -> &mut Vec<PDFObj>;
    fn pdfannots(&mut self) -> &mut Vec<PDFAnnot<ET>>;
    fn pdfxforms(&mut self) -> &mut Vec<PDFXForm<ET>>;
    fn pdfximages(&mut self) -> &mut Vec<PDFXImage<ET>>;
    #[cfg(feature = "pdfium")]
    fn pdfium_direct(&mut self) -> &mut Option<Option<pdfium_render::prelude::Pdfium>>;

    #[cfg(feature = "pdfium")]
    fn pdfium(&mut self) -> Option<&pdfium_render::prelude::Pdfium> {
        use pdfium_render::prelude::*;
        match self.pdfium_direct() {
            Some(p) => p.as_ref(),
            r => {
                let Ok(lock) = PDFIUM_LOCK.lock() else {
                    log::warn!("Could not lock PDFium lock");
                    return None;
                };
                let pdfium = Pdfium::bind_to_system_library().ok().or_else(|| {
                    std::env::current_exe().ok().and_then(|d| {
                        let lib_dir = d.parent()?.join("lib");
                        let lib_path = lib_dir.join(PDFIUM_NAME);
                        if !lib_path.exists() {
                            download_pdfium(&lib_dir);
                        }
                        Pdfium::bind_to_library(&lib_path)
                            .map_err(|e| {
                                log::warn!(
                                    "Could not bind to pdfium at {}: {e}",
                                    lib_path.display()
                                );
                            })
                            .ok()
                    })
                });
                *r = Some(pdfium.map(Pdfium::new));
                drop(lock);
                r.as_ref().unwrap_or_else(|| unreachable!()).as_ref()
            }
        }
    }
}

pub struct MinimalPDFExtension<ET: EngineTypes> {
    matches: Vec<String>,
    elapsed: std::time::Instant, //chrono::DateTime<chrono::Local>,
    colorstacks: Vec<Vec<PDFColor>>,
    current_colorstack: usize,
    pdfobjs: Vec<PDFObj>,
    pdfxforms: Vec<PDFXForm<ET>>,
    pdfximages: Vec<PDFXImage<ET>>,
    pdfannots: Vec<PDFAnnot<ET>>,
    #[cfg(feature = "pdfium")]
    pdfium: Option<Option<pdfium_render::prelude::Pdfium>>,
}
impl<ET: EngineTypes> EngineExtension<ET> for MinimalPDFExtension<ET> {
    fn new(_memory: &mut MemoryManager<ET::Token>) -> Self {
        Self {
            matches: Vec::new(),
            elapsed: std::time::Instant::now(),
            colorstacks: vec![vec![PDFColor::black()]],
            current_colorstack: 0,
            pdfobjs: Vec::new(),
            pdfannots: Vec::new(),
            pdfxforms: Vec::new(),
            pdfximages: Vec::new(),
            #[cfg(feature = "pdfium")]
            pdfium: None,
        }
    }
}
impl<ET: EngineTypes> PDFExtension<ET> for MinimalPDFExtension<ET> {
    fn pdfmatches(&mut self) -> &mut Vec<String> {
        &mut self.matches
    }

    fn elapsed(&mut self) -> &mut std::time::Instant {
        &mut self.elapsed
    }

    fn colorstacks(&mut self) -> &mut Vec<Vec<PDFColor>> {
        &mut self.colorstacks
    }

    fn current_colorstack(&mut self) -> &mut usize {
        &mut self.current_colorstack
    }

    fn pdfobjs(&mut self) -> &mut Vec<PDFObj> {
        &mut self.pdfobjs
    }

    fn pdfxforms(&mut self) -> &mut Vec<PDFXForm<ET>> {
        &mut self.pdfxforms
    }

    fn pdfannots(&mut self) -> &mut Vec<PDFAnnot<ET>> {
        &mut self.pdfannots
    }

    fn pdfximages(&mut self) -> &mut Vec<PDFXImage<ET>> {
        &mut self.pdfximages
    }

    #[cfg(feature = "pdfium")]
    fn pdfium_direct(&mut self) -> &mut Option<Option<pdfium_render::prelude::Pdfium>> {
        &mut self.pdfium
    }
}

#[derive(Debug, Clone)]
pub struct PDFObj(pub String);

#[derive(Debug, Clone)]
pub struct PDFXForm<ET: EngineTypes> {
    pub attr: String,
    pub resources: String,
    pub bx: Option<TeXBox<ET>>,
}
#[derive(Debug, Clone)]
pub struct PDFAnnot<ET: EngineTypes> {
    pub width: Option<ET::Dim>,
    pub height: Option<ET::Dim>,
    pub depth: Option<ET::Dim>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub enum PDFImage {
    None,
    Img(image::DynamicImage),
    #[cfg(feature = "pdfium")]
    PDF(image::DynamicImage),
}
impl PDFImage {
    pub fn width(&self) -> u32 {
        match self {
            PDFImage::None => 20,
            PDFImage::Img(img) => img.width(),
            #[cfg(feature = "pdfium")]
            PDFImage::PDF(img) => img.width() / 5,
        }
    }
    pub fn height(&self) -> u32 {
        match self {
            PDFImage::None => 20,
            PDFImage::Img(img) => img.height(),
            #[cfg(feature = "pdfium")]
            PDFImage::PDF(img) => img.height() / 5,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PDFXImage<ET: EngineTypes> {
    pub attr: String,
    pub width: Option<ET::Dim>,
    pub height: Option<ET::Dim>,
    pub depth: Option<ET::Dim>,
    pub colorspace: Option<i64>,
    pub page: Option<i64>,
    pub boxspec: Option<PDFBoxSpec>,
    pub filepath: PathBuf,
    pub img: PDFImage,
}
impl<ET: EngineTypes> PDFXImage<ET> {
    pub fn height(&self) -> ET::Dim {
        match (self.height, self.width) {
            (None, Some(w)) => {
                let scale = self.img.width() as f32 / (w.into() as f32);
                ET::Dim::from_sp((self.img.height() as f32 / scale).round() as i32)
            }
            (Some(h), _) => h,
            _ => ET::Dim::from_sp(65536 * (self.img.height() as i32)),
        }
    }

    pub fn width(&self) -> ET::Dim {
        match (self.height, self.width) {
            (Some(h), None) => {
                let scale = self.img.height() as f32 / (h.into() as f32);
                ET::Dim::from_sp((self.img.width() as f32 / scale).round() as i32)
            }
            (_, Some(w)) => w,
            _ => ET::Dim::from_sp(65536 * (self.img.width() as i32)),
        }
    }
    pub fn depth(&self) -> ET::Dim {
        self.depth.unwrap_or_default()
    }
}

#[cfg(feature = "pdfium")]
pub fn pdf_as_image<ET: EngineTypes, E: PDFExtension<ET>>(path: &Path, ext: &mut E) -> PDFImage {
    use pdfium_render::prelude::PdfRenderConfig;
    let Some(pdfium) = ext.pdfium() else {
        log::warn!("PDFium not loaded");
        return PDFImage::None;
    };
    let Ok(pdf) = pdfium.load_pdf_from_file(&path, None) else {
        log::warn!("Failed to load PDF file {}", path.display());
        return PDFImage::None;
    };
    let cfg = PdfRenderConfig::new().scale_page_by_factor(5.0);
    let pages = pdf.pages();
    let r = if let Ok(bmp) = pages.iter().next().unwrap().render_with_config(&cfg) {
        let img = bmp.as_image();
        PDFImage::PDF(img)
    } else {
        log::warn!("Failed to render PDF file {}", path.display());
        PDFImage::None
    };
    r
}

#[cfg(not(feature = "pdfium"))]
pub fn pdf_as_image<ET: EngineTypes, E: PDFExtension<ET>>(_path: &Path, _ext: &mut E) -> PDFImage {
    PDFImage::None
}

#[derive(Debug, Clone)]
pub struct PDFOutline {
    pub attr: String,
    pub action: ActionSpec,
    pub count: Option<i64>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct PDFCatalog {
    pub literal: String,
    pub action: Option<ActionSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(non_snake_case)]
pub struct PDFColor {
    R: u8,
    G: u8,
    B: u8,
}
impl PDFColor {
    pub fn black() -> Self {
        PDFColor { R: 0, G: 0, B: 0 }
    }
    pub fn parse<S: AsRef<str> + std::fmt::Display>(s: S) -> Self {
        macro_rules! parse {
            ($s:expr) => {
                match $s.parse::<f32>() {
                    Ok(f) => f,
                    _ => return Self::black(),
                }
            };
        }
        let ls: Vec<_> = s.as_ref().split(' ').collect();
        if matches!(ls.last(), Some(&"K")) && ls.len() > 4 {
            let third = 1.0 - parse!(ls[3]);
            let r = 255.0 * (1.0 - parse!(ls[0])) * third;
            let g = 255.0 * (1.0 - parse!(ls[1])) * third;
            let b = 255.0 * (1.0 - parse!(ls[2])) * third;
            if r > 255.0 || g > 255.0 || b > 255.0 || r < 0.0 || g < 0.0 || b < 0.0 {
                return Self::black();
            }
            PDFColor {
                R: (r.round() as u8),
                G: (g.round() as u8),
                B: (b.round() as u8),
            }
        } else if matches!(ls.last(), Some(&"RG")) && ls.len() > 3 {
            let r = 255.0 * parse!(ls[0]);
            let g = 255.0 * parse!(ls[1]);
            let b = 255.0 * parse!(ls[2]);
            if r > 255.0 || g > 255.0 || b > 255.0 || r < 0.0 || g < 0.0 || b < 0.0 {
                return Self::black();
            }
            PDFColor {
                R: (r.round() as u8),
                G: (g.round() as u8),
                B: (b.round() as u8),
            }
        } else if matches!(ls.last(), Some(&"G")) && ls.len() > 1 {
            let x = 255.0 * parse!(ls[0]);
            if !(0.0..=255.0).contains(&x) {
                return Self::black();
            }
            let x = (x.round()) as u8;
            PDFColor { R: x, G: x, B: x }
        } else {
            Self::black()
        }
    }
}
impl std::fmt::Display for PDFColor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.R, self.G, self.B)
    }
}
