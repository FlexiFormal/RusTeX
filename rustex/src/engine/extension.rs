use crate::engine::{CSName, Types};
use crate::shipout::state::ShipoutState;
use crate::utils::{VecMap, VecSet};
use pdfium_render::prelude::Pdfium;
use tex_engine::commands::Macro;
use tex_engine::engine::EngineExtension;
use tex_engine::engine::utils::memory::MemoryManager;
use tex_engine::pdflatex::nodes::{
    MinimalPDFExtension, PDFAnnot, PDFColor, PDFExtension, PDFObj, PDFXForm, PDFXImage,
};
use tex_engine::prelude::CSHandler;
use tex_engine::tex::catcodes::DEFAULT_SCHEME_U8;
use tex_engine::tex::tokens::CompactToken;

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CSS {
    File(String),
    Literal(String),
}

pub struct RusTeXExtension {
    pdf: MinimalPDFExtension<Types>,
    pub(crate) state: ShipoutState,
    pub(crate) change_markers: Vec<usize>,
    pub(crate) oddhead: CSName,
    pub(crate) oddfoot: CSName,
    pub(crate) evenhead: CSName,
    pub(crate) evenfoot: CSName,
    pub(crate) mkboth: CSName,
    pub(crate) specialpage: CSName,
    pub(crate) gobbletwo: Macro<CompactToken>,
    pub(crate) empty: Macro<CompactToken>,
    pub(crate) namespaces: VecMap<String, String>,
    pub(crate) metas: Vec<VecMap<String, String>>,
    pub(crate) top: VecMap<String, String>,
    pub(crate) css: VecSet<CSS>,
}
impl RusTeXExtension {
    pub(crate) fn push(&mut self) {
        self.change_markers.push(0)
    }
    pub(crate) fn pop(&mut self) -> usize {
        self.change_markers.pop().unwrap()
    }
}
impl EngineExtension<Types> for RusTeXExtension {
    fn new(memory: &mut MemoryManager<CompactToken>) -> Self {
        let mut namespaces = VecMap::default();
        namespaces.insert("dc".to_string(), "http://purl.org/dc/terms/".to_string());
        let mut ret = Self {
            pdf: MinimalPDFExtension::new(memory),
            state: ShipoutState::default(),
            change_markers: vec![],
            oddhead: memory.cs_interner_mut().cs_from_str("@oddhead"),
            oddfoot: memory.cs_interner_mut().cs_from_str("@oddfoot"),
            evenhead: memory.cs_interner_mut().cs_from_str("@evenhead"),
            evenfoot: memory.cs_interner_mut().cs_from_str("@evenfoot"),
            mkboth: memory.cs_interner_mut().cs_from_str("@mkboth"),
            specialpage: memory.cs_interner_mut().cs_from_str("if@specialpage"),
            gobbletwo: Macro::new::<_, _, Types>(
                memory.cs_interner_mut(),
                &DEFAULT_SCHEME_U8,
                "#1#2",
                "",
            )
            .unwrap(),
            empty: Macro::new::<_, _, Types>(memory.cs_interner_mut(), &DEFAULT_SCHEME_U8, "", "")
                .unwrap(),
            namespaces,
            metas: vec![],
            top: VecMap::default(),
            css: VecSet::default(),
        };
        ret.gobbletwo.long = true;
        ret
    }
}
impl PDFExtension<Types> for RusTeXExtension {
    fn pdfmatches(&mut self) -> &mut Vec<String> {
        self.pdf.pdfmatches()
    }

    fn elapsed(&mut self) -> &mut std::time::Instant {
        self.pdf.elapsed()
    }

    fn colorstacks(&mut self) -> &mut Vec<Vec<PDFColor>> {
        self.pdf.colorstacks()
    }

    fn current_colorstack(&mut self) -> &mut usize {
        self.pdf.current_colorstack()
    }

    fn pdfobjs(&mut self) -> &mut Vec<PDFObj> {
        self.pdf.pdfobjs()
    }

    fn pdfxforms(&mut self) -> &mut Vec<PDFXForm<Types>> {
        self.pdf.pdfxforms()
    }

    fn pdfximages(&mut self) -> &mut Vec<PDFXImage<Types>> {
        self.pdf.pdfximages()
    }

    fn pdfannots(&mut self) -> &mut Vec<PDFAnnot<Types>> {
        self.pdf.pdfannots()
    }
}
