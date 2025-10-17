use crate::engine::state::RusTeXState;
use crate::engine::{Font, SRef, Types};
use crate::utils::{VecMap, VecSet};
use tex_engine::commands::primitives::PRIMITIVES;
use tex_engine::engine::state::State;
use tex_engine::engine::stomach::methods::ParLineSpec;
use tex_engine::pdflatex::nodes::PDFNode;
use tex_engine::tex::nodes::boxes::TeXBox;
use tex_engine::tex::nodes::{CustomNodeTrait, NodeTrait, NodeType};
use tex_engine::tex::numerics::{Dim32, Skip};

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineSkip {
    pub baselineskip: f32,
    pub lineskip: f32,
    pub lineskiplimit: f32,
}
impl LineSkip {
    pub fn get(state: &RusTeXState) -> Self {
        Self {
            baselineskip: state.get_primitive_skip(PRIMITIVES.baselineskip).base.0 as f32,
            lineskip: state.get_primitive_skip(PRIMITIVES.lineskip).base.0 as f32,
            lineskiplimit: state.get_primitive_dim(PRIMITIVES.lineskiplimit).0 as f32,
        }
    }
    pub fn factor(&self, font: &Font) -> f32 {
        use tex_engine::engine::fontsystem::Font;
        let at = font.get_at().0 as f32;
        (if self.baselineskip >= (self.lineskiplimit + at) {
            self.baselineskip
        } else {
            at + self.lineskip
        }) / 655360.0
    }
}

#[derive(Debug, Clone)]
pub enum RusTeXNode {
    PDFNode(PDFNode<Types>),
    FontChange(Font, bool),
    FontChangeEnd,
    ParagraphBegin {
        specs: Vec<ParLineSpec<Types>>,
        start: SRef,
        parskip: Skip<Dim32>,
        end: SRef,
        lineskip: LineSkip,
    },
    ParagraphEnd,
    HAlignBegin {
        lineskip: LineSkip,
    },
    HAlignEnd,
    Br,
    PGFGBegin {
        attrs: VecMap<&'static str, String>,
        tag: String,
    },
    PGFGEnd,
    PGFEscape(TeXBox<Types>),
    PGFSvg {
        bx: TeXBox<Types>,
        minx: Dim32,
        miny: Dim32,
        maxx: Dim32,
        maxy: Dim32,
    },
    PageBegin,
    PageEnd,
    AnnotBegin {
        tag: Option<String>,
        start: SRef,
        attrs: VecMap<String, String>,
        styles: VecMap<String, String>,
        classes: VecSet<String>,
    },
    InvisibleBegin,
    InvisibleEnd,
    AnnotEnd(SRef),
    Literal(String),
}
impl CustomNodeTrait<Types> for RusTeXNode {}
impl NodeTrait<Types> for RusTeXNode {
    fn height(&self) -> Dim32 {
        match self {
            Self::PDFNode(n) => n.height(),
            Self::PGFSvg { miny, maxy, .. } => *maxy + -*miny,
            _ => Dim32(0),
        }
    }
    fn depth(&self) -> Dim32 {
        match self {
            Self::PDFNode(n) => n.depth(),
            _ => Dim32(0),
        }
    }
    fn width(&self) -> Dim32 {
        match self {
            Self::PDFNode(n) => n.width(),
            Self::PGFSvg { minx, maxx, .. } => *maxx + -*minx,
            _ => Dim32(0),
        }
    }
    fn nodetype(&self) -> NodeType {
        NodeType::WhatsIt
    }
    fn display_fmt(&self, indent: usize, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PDFNode(n) => n.display_fmt(indent, f),
            Self::PGFSvg { bx, .. } => {
                write!(f, "<svg>")?;
                bx.display_fmt(indent + 2, f)?;
                write!(f, "</svg>")?;
                Ok(())
            }
            _ => write!(f, "<{:?}>", self),
        }
    }
    fn opaque(&self) -> bool {
        match self {
            Self::PDFNode(n) => n.opaque(),
            _ => true,
        }
    }
}
impl From<PDFNode<Types>> for RusTeXNode {
    fn from(value: PDFNode<Types>) -> Self {
        Self::PDFNode(value)
    }
}
