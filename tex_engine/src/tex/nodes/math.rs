/*! Nodes allowed in math mode */
use crate::commands::primitives::PRIMITIVES;
use crate::engine::EngineTypes;
use crate::engine::filesystem::SourceRef;
use crate::engine::fontsystem::Font;
use crate::engine::state::State;
use crate::tex::nodes::boxes::{TeXBox, ToOrSpread};
use crate::tex::nodes::vertical::VNode;
use crate::tex::nodes::{Leaders, ListTarget, NodeTrait, NodeType, WhatsitNode, display_do_indent};
use crate::tex::numerics::NumSet;
use crate::tex::numerics::{MuSkip, Skip, TeXDimen};
use crate::tex::tokens::token_lists::TokenList;
use either::Either;
use std::cell::OnceCell;
use std::fmt::{Debug, Formatter, Write};
use std::marker::PhantomData;

/// A math list node. Comes in two forms: an unresolved form, while the list is being
/// constructed and the various font styles are not fixed yet (e.g. because
/// an `\atop` comes later that shifts the font style), and a resolved form,
/// where the fonts have been determined and are fixed.
/// The parameter `S:`[`MathFontStyleT`] makes the distinction between the two forms.
/// For `S=`[`MathFontStyle`] (the resolved form), this implements [`NodeTrait`].
#[derive(Clone, Debug)]
pub enum MathNode<ET: EngineTypes, S: MathFontStyleT<ET>> {
    /// A math atom node (see [`MathAtom`])
    Atom(MathAtom<ET, S>),
    /// A penalty node, as produced by `\penalty`.
    Penalty(i32),
    /// A mark node, as produced by `\mark`.
    Mark(usize, TokenList<ET::Token>),
    /// A whatsit node, as produced by `\special`, `\write`, etc.
    Whatsit(WhatsitNode<ET>),
    /// A glue node, as produced by `\hskip`.
    HSkip(Skip<ET::Dim>),
    /// A glue node, as produced by `\mskip`. If resolved, `S` provides the adequate `em` value.
    MSkip { skip: MuSkip<ET::MuDim>, style: S },
    /// A glue node, as produced by `\hfil`.
    HFil,
    /// A glue node, as produced by `\hfill`.
    HFill,
    /// A glue node, as produced by `\hfilneg`.
    HFilneg,
    /// A glue node, as produced by `\hss`.
    Hss,
    /// A glue node, as produced by `\ `
    Space,
    /// A kern node, as produced by `\kern`.
    HKern(ET::Dim),
    /// A kern node, as produced by `\mkern`.
    MKern { kern: ET::MuDim, style: S },
    /// A leaders node, as produced by `\leaders` or `\cleaders` or `\xleaders`.
    Leaders(Leaders<ET>),
    /// A rule node, as produced by `\vrule`.
    VRule {
        /// The *provided* width of the rule.
        width: Option<ET::Dim>,
        /// The *provided* height of the rule.
        height: Option<ET::Dim>,
        /// The *provided* depth of the rule.
        depth: Option<ET::Dim>,
        /// The source reference for the start of the rule.
        start: SourceRef<ET>,
        /// The source reference for the end of the rule.
        end: SourceRef<ET>,
    },
    /// A "generalized fraction", as produced by `\over`, `\atop`, `\above`,
    /// `\abovewithdelims`, `\overwithdelims`, `\atopwithdelims`.
    Over {
        /// The source reference for the start of the node.
        start: SourceRef<ET>,
        /// The source reference for the end of the node.
        end: SourceRef<ET>,
        /// The numerator.
        top: Box<[MathNode<ET, S>]>,
        /// The optional separator height.
        sep: Option<ET::Dim>,
        /// The denominator.
        bottom: Box<[MathNode<ET, S>]>,
        /// The optional left delimiter.
        left: Option<(ET::Char, S)>,
        /// The optional right delimiter.
        right: Option<(ET::Char, S)>,
    },
    /// A `\mathchoice` node; if resolved, this is just a wrapper around more math nodes.
    Choice(S::Choice),
    /// Markers for the various font styles, e.g. `\displaystyle`, `\textstyle`, etc.
    Marker(S::Markers),
    /// A custom node.
    Custom(ET::CustomNode),
}
impl<ET: EngineTypes> MathNode<ET, UnresolvedMathFontStyle<ET>> {
    /// The node type of this node (as in `\lastnodetype`).
    pub fn nodetype(&self) -> NodeType {
        match self {
            MathNode::Penalty(_) => NodeType::Penalty,
            MathNode::VRule { .. } => NodeType::Rule,
            MathNode::HKern(_) | MathNode::MKern { .. } => NodeType::Kern,
            MathNode::Mark(_, _) => NodeType::Mark,
            MathNode::Whatsit(_) => NodeType::WhatsIt,
            MathNode::HSkip(_)
            | MathNode::MSkip { .. }
            | MathNode::Space
            | MathNode::HFil
            | MathNode::HFill
            | MathNode::HFilneg
            | MathNode::Hss => NodeType::Glue,
            MathNode::Leaders(_) => NodeType::Glue,
            MathNode::Custom(n) => n.nodetype(),
            _ => NodeType::Math,
        }
    }
    /// See [`NodeTrait::opaque`].
    pub fn opaque(&self) -> bool {
        match self {
            MathNode::Mark(_, _) => true,
            MathNode::Custom(n) => n.opaque(),
            _ => false,
        }
    }
}
impl<ET: EngineTypes> NodeTrait<ET> for MathNode<ET, MathFontStyle<ET>> {
    fn display_fmt(&self, indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MathNode::Penalty(p) => {
                display_do_indent(indent, f)?;
                write!(f, "<penalty:{}>", p)
            }
            MathNode::Mark(i, _) => {
                display_do_indent(indent, f)?;
                write!(f, "<mark:{}>", i)
            }
            MathNode::Over {
                top,
                sep,
                bottom,
                left,
                right,
                ..
            } => {
                display_do_indent(indent, f)?;
                write!(f, "<over")?;
                if let Some(s) = sep {
                    write!(f, " sep={}", s)?;
                }
                f.write_char('>')?;
                if let Some((l, _)) = left {
                    display_do_indent(indent + 2, f)?;
                    write!(f, "<left-delim = {}/>", l)?;
                }
                display_do_indent(indent + 2, f)?;
                f.write_str("<top>")?;
                for c in top.iter() {
                    c.display_fmt(indent + 4, f)?;
                }
                display_do_indent(indent + 2, f)?;
                f.write_str("</top>")?;
                display_do_indent(indent + 2, f)?;
                f.write_str("<bottom>")?;
                for c in bottom.iter() {
                    c.display_fmt(indent + 4, f)?;
                }
                display_do_indent(indent + 2, f)?;
                f.write_str("</bottom>")?;
                if let Some((r, _)) = right {
                    display_do_indent(indent + 2, f)?;
                    write!(f, "<right-delim = {}/>", r)?;
                }
                display_do_indent(indent, f)?;
                write!(f, "</over>")
            }
            MathNode::Marker(m) => m.display_fmt(indent, f),
            MathNode::Choice(c) => c.display_fmt(indent, f),
            MathNode::Leaders(l) => l.display_fmt(indent, f),
            MathNode::VRule {
                width,
                height,
                depth,
                ..
            } => {
                write!(f, "<vrule")?;
                if let Some(w) = width {
                    write!(f, " width={}", w)?;
                }
                if let Some(h) = height {
                    write!(f, " height={}", h)?;
                }
                if let Some(d) = depth {
                    write!(f, " depth={}", d)?;
                }
                write!(f, ">")
            }
            MathNode::Whatsit(w) => {
                display_do_indent(indent, f)?;
                write!(f, "{:?}", w)
            }
            MathNode::HSkip(s) => write!(f, "<hskip:{}>", s),
            MathNode::MSkip { skip, .. } => write!(f, "<mskip:{}>", skip),
            MathNode::MKern { kern, .. } => write!(f, "<mkern:{}>", kern),
            MathNode::HFil => write!(f, "<hfil>"),
            MathNode::HFill => write!(f, "<hfill>"),
            MathNode::HFilneg => write!(f, "<hfilneg>"),
            MathNode::Hss => write!(f, "<hss>"),
            MathNode::Space => write!(f, "<space>"),
            MathNode::HKern(d) => write!(f, "<hkern:{}>", d),
            MathNode::Custom(n) => n.display_fmt(indent, f),
            MathNode::Atom(a) => a.display_fmt(indent, f),
        }
    }
    fn height(&self) -> ET::Dim {
        match self {
            MathNode::VRule { height, .. } => height.unwrap_or_default(),
            MathNode::Custom(n) => n.height(),
            MathNode::Atom(a) => a.height(),
            MathNode::Leaders(l) => l.height(),
            MathNode::Choice(c) => c.height(),
            MathNode::Over { top, sep, .. } => {
                let mut inner = top
                    .iter()
                    .map(|c| c.height() + c.depth())
                    .max()
                    .unwrap_or_default();
                match sep {
                    None => (),
                    Some(s) => inner = inner + s.scale_float(0.5) + ET::Dim::from_sp(65536 * 3), // TODO heuristic
                }
                inner
            }
            _ => ET::Dim::default(),
        }
    }
    fn width(&self) -> ET::Dim {
        match self {
            MathNode::VRule { width, .. } => width.unwrap_or(ET::Dim::from_sp(26214)),
            MathNode::Custom(n) => n.width(),
            MathNode::HKern(d) => *d,
            MathNode::MKern { kern, style } => ET::Num::mudim_to_dim(*kern, style.get_em()),
            MathNode::MSkip { skip, style } => ET::Num::mudim_to_dim(skip.base, style.get_em()),
            MathNode::HSkip(s) => s.base,
            MathNode::Space => ET::Dim::from_sp(65536 * 5), // TODO heuristic; use spacefactor instead
            MathNode::Leaders(l) => l.width(),
            MathNode::Atom(a) => a.width(),
            MathNode::Choice(c) => c.width(),
            MathNode::Over { top, bottom, .. } => {
                let top: ET::Dim = top.iter().map(|c| c.width()).sum();
                let bot: ET::Dim = bottom.iter().map(|c| c.width()).sum();
                top.max(bot)
            }
            _ => ET::Dim::default(),
        }
    }
    fn depth(&self) -> ET::Dim {
        match self {
            MathNode::VRule { depth, .. } => depth.unwrap_or_default(),
            MathNode::Custom(n) => n.depth(),
            MathNode::Atom(a) => a.depth(),
            MathNode::Leaders(l) => l.depth(),
            MathNode::Choice(c) => c.depth(),
            MathNode::Over { bottom, sep, .. } => {
                let mut inner = bottom
                    .iter()
                    .map(|c| c.height() + c.depth())
                    .max()
                    .unwrap_or_default();
                match sep {
                    None => (),
                    Some(s) => inner = inner + s.scale_float(0.5) + ET::Dim::from_sp(65536 * 3), // TODO heuristic
                }
                inner
            }
            _ => ET::Dim::default(),
        }
    }
    fn nodetype(&self) -> NodeType {
        match self {
            MathNode::Penalty(_) => NodeType::Penalty,
            MathNode::VRule { .. } => NodeType::Rule,
            MathNode::HKern(_) | MathNode::MKern { .. } => NodeType::Kern,
            MathNode::Mark(_, _) => NodeType::Mark,
            MathNode::Whatsit(_) => NodeType::WhatsIt,
            MathNode::HSkip(_)
            | MathNode::MSkip { .. }
            | MathNode::Space
            | MathNode::HFil
            | MathNode::HFill
            | MathNode::HFilneg
            | MathNode::Hss => NodeType::Glue,
            MathNode::Leaders(_) => NodeType::Glue,
            MathNode::Custom(n) => n.nodetype(),
            _ => NodeType::Math,
        }
    }
    fn opaque(&self) -> bool {
        match self {
            MathNode::Mark(_, _) => true,
            MathNode::Custom(n) => n.opaque(),
            _ => false,
        }
    }
}

/// One of the 8 styles of math formatting; The TeXBook
/// calls these D,T,S,SS,D',T',S' and SS'.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct MathStyle {
    /// Whether the style is cramped or not (i.e. the `'`
    pub cramped: bool,
    /// The style itself (D,T,S,SS)
    pub style: MathStyleType,
}
impl MathStyle {
    /// The math style to use for a superscript in this style
    pub fn sup(self) -> Self {
        match self.style {
            MathStyleType::Text | MathStyleType::Display => MathStyle {
                cramped: self.cramped,
                style: MathStyleType::Script,
            },
            MathStyleType::Script | MathStyleType::ScriptScript => MathStyle {
                cramped: self.cramped,
                style: MathStyleType::ScriptScript,
            },
        }
    }
    /// The same math style, but cramped
    pub fn cramp(mut self) -> Self {
        self.cramped = true;
        self
    }
    /// The math style to use for a subscript in this style
    pub fn sub(self) -> Self {
        self.sup().cramp()
    }
    /// The math style to use for the numerator of a generalized fraction in this style
    pub fn numerator(self) -> Self {
        match self.style {
            MathStyleType::Display => MathStyle {
                cramped: self.cramped,
                style: MathStyleType::Text,
            },
            MathStyleType::Text => MathStyle {
                cramped: self.cramped,
                style: MathStyleType::Script,
            },
            MathStyleType::Script | MathStyleType::ScriptScript => MathStyle {
                cramped: self.cramped,
                style: MathStyleType::ScriptScript,
            },
        }
    }
    /// The math style to use for the denominator of a generalized fraction in this style
    pub fn denominator(self) -> Self {
        self.numerator().cramp()
    }
}

/// The four base math formatting styles
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum MathStyleType {
    Display,
    Text,
    Script,
    ScriptScript,
}

/// The 7 math classes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathClass {
    /// Ordinary
    Ord = 0,
    /// Large operator
    Op = 1,
    /// Binary operator
    Bin = 2,
    /// Relation
    Rel = 3,
    /// Opening delimiter
    Open = 4,
    /// Closing delimiter
    Close = 5,
    /// Punctuation
    Punct = 6,
}
impl From<u8> for MathClass {
    fn from(v: u8) -> Self {
        match v {
            0 => MathClass::Ord,
            1 => MathClass::Op,
            2 => MathClass::Bin,
            3 => MathClass::Rel,
            4 => MathClass::Open,
            5 => MathClass::Close,
            6 => MathClass::Punct,
            _ => panic!("Invalid math class {}", v),
        }
    }
}

/// This trait is implemented for exactly two types that indicate
/// whether we are in the unresolved [`UnresolvedMathFontStyle`] or resolved
/// ([`MathFontStyle`]) state.
pub trait MathFontStyleT<ET: EngineTypes>: Clone + Debug {
    /// The type of the choice node, which is either [`UnresolvedMathChoice`] or [`ResolvedChoice`].
    type Choice: MathChoiceT<ET>;
    /// The type of the markers, which is either [`UnresolvedMarkers`] or a dummy that never occurs.
    type Markers: Clone + Debug + NodeTrait<ET>;
}
/// Unresolved math font style. This is the state while the math list is being constructed.
/// Carries information about the family if relevant (or 0), to be picked
/// when the math list is resolved.
#[derive(Debug, Clone)]
pub struct UnresolvedMathFontStyle<ET: EngineTypes>(u8, PhantomData<ET>);
impl<ET: EngineTypes> MathFontStyleT<ET> for UnresolvedMathFontStyle<ET> {
    type Choice = UnresolvedMathChoice<ET>;
    type Markers = UnresolvedMarkers;
    //fn get_font(&self) -> &ET::Font { &self.text_font }
}
impl<ET: EngineTypes> UnresolvedMathFontStyle<ET> {
    /// Create a new unresolved math font style with the given family.
    pub fn of_fam(fam: u8) -> Self {
        Self(fam, PhantomData)
    }
    /// The family to use.
    pub fn fam(&self) -> u8 {
        self.0
    }
}

/// A resolved math font style. This is the state after the math list has been constructed.
/// Has a definite style and font.
#[derive(Debug, Clone)]
pub struct MathFontStyle<ET: EngineTypes> {
    pub style: MathStyleType,
    pub cramped: bool,
    pub font: ET::Font,
}
impl<ET: EngineTypes> MathFontStyleT<ET> for MathFontStyle<ET> {
    type Choice = ResolvedChoice<ET>;
    type Markers = PhantomData<ET>;
}
impl<ET: EngineTypes> MathFontStyle<ET> {
    /// The em value to use to compute widths (i.e. `\fontdimen6` of the font).
    pub fn get_em(&self) -> ET::Dim {
        self.font.get_dim(5)
    }
    /// The font to use for this style.
    pub fn get_font(&self) -> &ET::Font {
        &self.font
    }
}
impl<ET: EngineTypes> NodeTrait<ET> for PhantomData<ET> {
    fn display_fmt(&self, _indent: usize, _f: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
    fn height(&self) -> ET::Dim {
        ET::Dim::default()
    }
    fn width(&self) -> ET::Dim {
        ET::Dim::default()
    }
    fn depth(&self) -> ET::Dim {
        ET::Dim::default()
    }
    fn nodetype(&self) -> NodeType {
        NodeType::Math
    }
}

/// This trait is implemented for exactly two types that indicate
/// whether we are in the unresolved [`UnresolvedMathFontStyle`] or resolved
/// ([`MathFontStyle`]) state.
pub trait MathChoiceT<ET: EngineTypes>: Clone + Debug {}

/// A `\mathcoice` node, not yet resolved. When the current math list
/// is closed, one of the four choices is picked, depending on the
/// current style.
#[derive(Clone, Debug)]
pub struct UnresolvedMathChoice<ET: EngineTypes> {
    pub display: Box<[MathNode<ET, UnresolvedMathFontStyle<ET>>]>,
    pub text: Box<[MathNode<ET, UnresolvedMathFontStyle<ET>>]>,
    pub script: Box<[MathNode<ET, UnresolvedMathFontStyle<ET>>]>,
    pub scriptscript: Box<[MathNode<ET, UnresolvedMathFontStyle<ET>>]>,
}
impl<ET: EngineTypes> MathChoiceT<ET> for UnresolvedMathChoice<ET> {}

/// A resolved `\mathchoice` node. This is the state after the math list has been constructed,
/// at which point it is only a wrapper around a list of nodes.
pub struct ResolvedChoice<ET: EngineTypes>(pub Box<[MathNode<ET, MathFontStyle<ET>>]>);
impl<ET: EngineTypes> Debug for ResolvedChoice<ET> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("<resolved_choice>")?;
        for c in self.0.iter() {
            c.display_fmt(2, f)?;
        }
        f.write_str("</resolved_choice>")
    }
}
impl<ET: EngineTypes> Clone for ResolvedChoice<ET> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<ET: EngineTypes> MathChoiceT<ET> for ResolvedChoice<ET> {}
impl<ET: EngineTypes> ResolvedChoice<ET> {
    /// See [`NodeTrait::display_fmt`]
    pub fn display_fmt(&self, indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        for c in self.0.iter() {
            c.display_fmt(indent, f)?;
        }
        Ok(())
    }
    /// See [`NodeTrait::width`]
    pub fn width(&self) -> ET::Dim {
        self.0.iter().map(|c| c.width()).sum()
    }
    /// See [`NodeTrait::height`]
    pub fn height(&self) -> ET::Dim {
        self.0.iter().map(|c| c.height()).max().unwrap_or_default()
    }
    /// See [`NodeTrait::depth`]
    pub fn depth(&self) -> ET::Dim {
        self.0.iter().map(|c| c.depth()).max().unwrap_or_default()
    }
}

/// Markers inserted by `\displaystyle`, `\textstyle`, `\scriptstyle` and `\scriptscriptstyle`.
/// Only meaningful in unresolved mode, while the math list is open. When the list
/// is closed, these are removed and used to determine the font style at that point.
#[derive(Debug, Copy, Clone)]
pub enum UnresolvedMarkers {
    Display,
    Text,
    Script,
    ScriptScript,
}
impl<ET: EngineTypes> NodeTrait<ET> for UnresolvedMarkers {
    fn display_fmt(&self, _indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            UnresolvedMarkers::Display => f.write_str("<display>"),
            UnresolvedMarkers::Text => f.write_str("<text>"),
            UnresolvedMarkers::Script => f.write_str("<script>"),
            UnresolvedMarkers::ScriptScript => f.write_str("<scriptscript>"),
        }
    }
    fn height(&self) -> ET::Dim {
        ET::Dim::default()
    }
    fn width(&self) -> ET::Dim {
        ET::Dim::default()
    }
    fn depth(&self) -> ET::Dim {
        ET::Dim::default()
    }
    fn nodetype(&self) -> NodeType {
        NodeType::Math
    }
}

/// The most central kind of node in a math list. Consisting of a [nucleus](MathNucleus)
/// with optional superscript and subscript math lists.
#[derive(Clone, Debug)]
pub struct MathAtom<ET: EngineTypes, S: MathFontStyleT<ET>> {
    pub nucleus: MathNucleus<ET, S>,
    pub sup: Option<Box<[MathNode<ET, S>]>>,
    pub sub: Option<Box<[MathNode<ET, S>]>>,
}
impl<ET: EngineTypes, S: MathFontStyleT<ET>> MathAtom<ET, S> {
    /// Create a new empty math atom.
    pub fn empty() -> Self {
        Self {
            nucleus: MathNucleus::Simple {
                cls: MathClass::Ord,
                kernel: MathKernel::Empty,
                limits: None,
            },
            sup: None,
            sub: None,
        }
    }
}
impl<ET: EngineTypes> NodeTrait<ET> for MathAtom<ET, MathFontStyle<ET>> {
    fn display_fmt(&self, indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_do_indent(indent, f)?;
        if self.sub.is_none() || self.sup.is_none() {
            return self.nucleus.display_fmt(indent, f);
        }
        f.write_str("<atom>")?;
        self.nucleus.display_fmt(indent + 2, f)?;
        if let Some(sup) = &self.sup {
            display_do_indent(indent + 2, f)?;
            f.write_str("<sup>")?;
            for c in sup.iter() {
                c.display_fmt(indent + 4, f)?;
            }
            display_do_indent(indent + 2, f)?;
            f.write_str("</sup>")?;
        }
        if let Some(sub) = &self.sub {
            display_do_indent(indent + 2, f)?;
            f.write_str("<sub>")?;
            for c in sub.iter() {
                c.display_fmt(indent + 4, f)?;
            }
            display_do_indent(indent + 2, f)?;
            f.write_str("</sub>")?;
        }
        display_do_indent(indent, f)?;
        f.write_str("</atom>")
    }
    fn height(&self) -> ET::Dim {
        if self.sup.is_none() {
            return self.nucleus.height();
        }
        let h = self.nucleus.height();
        let limits = matches!(
            self.nucleus,
            MathNucleus::Simple {
                cls: MathClass::Op,
                limits: Some(true),
                ..
            }
        );
        let sup = self
            .sup
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.height() + c.depth())
            .max()
            .unwrap_or_default();
        if limits {
            h + sup + ET::Dim::from_sp(65536 * 3) // TODO heuristic
        } else {
            h + sup.scale_float(0.75) // TODO heuristic
        }
    }
    fn width(&self) -> ET::Dim {
        if self.sup.is_none() && self.sub.is_none() {
            return self.nucleus.width();
        }
        let w = self.nucleus.width();
        let sup = match self.sup {
            Some(ref ls) => ls.iter().map(|c| c.width()).sum(),
            _ => ET::Dim::default(),
        };
        let sub = match self.sub {
            Some(ref ls) => ls.iter().map(|c| c.width()).sum(),
            _ => ET::Dim::default(),
        };
        let limits = matches!(
            self.nucleus,
            MathNucleus::Simple {
                cls: MathClass::Op,
                limits: Some(true),
                ..
            }
        );
        if limits {
            w.max(sup).max(sub)
        } else {
            w + sup.max(sub) + ET::Dim::from_sp(65536 * 3) // TODO heuristic
        }
    }
    fn depth(&self) -> ET::Dim {
        if self.sub.is_none() {
            return self.nucleus.depth();
        }
        let h = self.nucleus.depth();
        let limits = matches!(
            self.nucleus,
            MathNucleus::Simple {
                cls: MathClass::Op,
                limits: Some(true),
                ..
            }
        );
        let sub = self
            .sub
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| c.depth() + c.height())
            .max()
            .unwrap_or_default();
        if limits {
            h + sub + ET::Dim::from_sp(65536 * 3) // TODO heuristic
        } else {
            //println!("HERE: {self:?}\n{h}+{sub}*0.75={}",sub.scale_float(0.75));
            h + sub.scale_float(0.75) // TODO heuristic
        }
    }

    fn nodetype(&self) -> NodeType {
        NodeType::Math
    }
}

/// The nucleus of a [`MathAtom`]; a cohesive "unit" with optional sub/superscript.
#[derive(Clone, Debug)]
pub enum MathNucleus<ET: EngineTypes, S: MathFontStyleT<ET>> {
    /// A simple nucleus, consisting of a [`MathKernel`] and a math class.
    /// `limits` is `None` during construction of the list, and `Some(true)` or `Some(false)`
    /// after the list has been closed, or if a large operator is followed by a `\limits` or `\nolimits`.
    Simple {
        cls: MathClass,
        kernel: MathKernel<ET, S>,
        limits: Option<bool>,
    },
    /// A `\mathinner` node, as produced by `{...}`
    Inner(MathKernel<ET, S>),
    /// A node produced by `\left...\right`.
    LeftRight {
        start: SourceRef<ET>,
        left: Option<(ET::Char, S)>,
        children: Box<[MathNode<ET, S>]>,
        right: Option<(ET::Char, S)>,
        end: SourceRef<ET>,
    },
    /// A node produced by `\middle`.
    Middle(ET::Char, S),
    /// A node produced by `\overline`.
    Overline(MathKernel<ET, S>),
    /// A node produced by `\underline`.
    Underline(MathKernel<ET, S>),
    /// A node produced by `\mathaccent`.
    Accent {
        accent: (ET::Char, S),
        inner: Box<[MathNode<ET, S>]>,
    },
    /// A node produced by `\radical`.
    Radical {
        rad: (ET::Char, S),
        inner: Box<[MathNode<ET, S>]>,
    },
    /// A node produced by `\vcenter`.
    VCenter {
        start: SourceRef<ET>,
        end: SourceRef<ET>,
        children: Box<[VNode<ET>]>,
        scaled: ToOrSpread<ET::Dim>,
    },
}
impl<ET: EngineTypes> NodeTrait<ET> for MathNucleus<ET, MathFontStyle<ET>> {
    fn display_fmt(&self, indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_do_indent(indent, f)?;
        match self {
            MathNucleus::Simple {
                cls: MathClass::Op,
                kernel,
                limits,
            } => {
                write!(f, "<mathop limits={:?}>", limits)?;
                kernel.display_fmt(indent + 2, f)?;
                display_do_indent(indent, f)?;
                f.write_str("</mathop>")
            }
            MathNucleus::Simple { cls, kernel, .. } => {
                write!(f, "<math{:?}>", cls)?;
                kernel.display_fmt(indent + 2, f)?;
                display_do_indent(indent, f)?;
                write!(f, "</math{:?}>", cls)
            }
            MathNucleus::Inner(k) => {
                write!(f, "<mathinner>")?;
                k.display_fmt(indent + 2, f)?;
                display_do_indent(indent, f)?;
                f.write_str("</mathinner>")
            }
            MathNucleus::LeftRight {
                left,
                right,
                children,
                ..
            } => {
                write!(f, "<leftright>")?;
                if let Some((l, _)) = left {
                    write!(f, "<left = {}/>", l)?;
                }
                for c in children.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                if let Some((r, _)) = right {
                    write!(f, "<right = {}/>", r)?;
                }
                display_do_indent(indent, f)?;
                f.write_str("</leftright>")
            }
            MathNucleus::Middle(c, _) => {
                write!(f, "<middle = {}/>", c)
            }
            MathNucleus::Overline(k) => {
                write!(f, "<overline>")?;
                k.display_fmt(indent + 2, f)?;
                display_do_indent(indent, f)?;
                f.write_str("</overline>")
            }
            MathNucleus::Underline(k) => {
                write!(f, "<underline>")?;
                k.display_fmt(indent + 2, f)?;
                display_do_indent(indent, f)?;
                f.write_str("</underline>")
            }
            MathNucleus::Accent { accent, inner } => {
                write!(f, "<accent char=\"{}\">", accent.0)?;
                for i in inner.iter() {
                    i.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                f.write_str("</accent>")
            }
            MathNucleus::Radical { rad, inner } => {
                write!(f, "<radical char=\"{}\">", rad.0)?;
                for i in inner.iter() {
                    i.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                f.write_str("</radical>")
            }
            MathNucleus::VCenter { children, .. } => {
                write!(f, "<vcenter>")?;
                for c in children.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                f.write_str("</vcenter>")
            }
        }
    }
    fn height(&self) -> ET::Dim {
        match self {
            MathNucleus::Simple {
                cls: MathClass::Op,
                kernel,
                ..
            } => (kernel.height() + kernel.depth()).scale_float(0.5),
            MathNucleus::Simple { kernel, .. } => kernel.height(),
            MathNucleus::Inner(k) => k.height(),
            MathNucleus::LeftRight { children, .. } => children
                .iter()
                .map(|c| c.height())
                .max()
                .unwrap_or_default(),
            MathNucleus::Overline(k) => k.height(),
            MathNucleus::Underline(k) => k.height(),
            MathNucleus::Middle(c, s) => s.get_font().get_ht(*c),
            MathNucleus::Accent {
                inner,
                accent: (c, f),
            } => {
                inner.iter().map(|c| c.height()).max().unwrap_or_default()
                    + f.get_font().get_ht(*c)
                    + f.get_font().get_dp(*c)
            }
            MathNucleus::Radical { inner, rad: (c, f) } => {
                inner.iter().map(|c| c.height()).max().unwrap_or_default() + f.get_font().get_ht(*c)
            } // + \epsilon?
            MathNucleus::VCenter { children, .. } => {
                children
                    .iter()
                    .map(|c| c.height() + c.depth())
                    .sum::<ET::Dim>()
                    + -children
                        .iter()
                        .last()
                        .map(|c| c.depth())
                        .unwrap_or_default()
            }
        }
    }
    fn width(&self) -> ET::Dim {
        match self {
            MathNucleus::Simple { kernel, .. } => kernel.width() + ET::Dim::from_sp(65536 * 2), // heuristic adjustment
            MathNucleus::Inner(k) => k.width(),
            MathNucleus::LeftRight { children, .. } => {
                children.iter().map(|c| c.width()).sum::<ET::Dim>() + ET::Dim::from_sp(65536 * 3)
            } // heuristic adjustment
            MathNucleus::Overline(k) => k.width(),
            MathNucleus::Underline(k) => k.width(),
            MathNucleus::Middle(c, s) => s.get_font().get_wd(*c),
            MathNucleus::Accent { inner, .. } => inner.iter().map(|c| c.width()).sum(),
            MathNucleus::Radical { inner, rad, .. } => {
                inner.iter().map(|c| c.width()).sum::<ET::Dim>() + rad.1.get_font().get_wd(rad.0)
            }
            MathNucleus::VCenter { children, .. } => {
                children.iter().map(|c| c.width()).max().unwrap_or_default()
            }
        }
    }
    fn depth(&self) -> ET::Dim {
        match self {
            MathNucleus::Simple {
                cls: MathClass::Op,
                kernel,
                ..
            } => (kernel.height() + kernel.depth()).scale_float(0.5),
            MathNucleus::Simple { kernel, .. } => kernel.depth(),
            MathNucleus::LeftRight { children, .. } => {
                children.iter().map(|c| c.depth()).max().unwrap_or_default()
            }
            MathNucleus::Inner(k) => k.depth(),
            MathNucleus::Overline(k) => k.depth(),
            MathNucleus::Underline(k) => k.depth(),
            MathNucleus::Middle(c, s) => s.get_font().get_dp(*c),
            MathNucleus::Accent { inner, .. } => {
                inner.iter().map(|c| c.depth()).max().unwrap_or_default()
            }
            MathNucleus::Radical { inner, .. } => {
                inner.iter().map(|c| c.depth()).max().unwrap_or_default()
            }
            MathNucleus::VCenter { children, .. } => children
                .iter()
                .last()
                .map(|c| c.depth())
                .unwrap_or_default(),
        }
    }

    fn nodetype(&self) -> NodeType {
        NodeType::Math
    }
}

/// The kernel of a [`MathNucleus`]; the actual content of the nucleus.
#[derive(Clone, Debug, Default)]
pub enum MathKernel<ET: EngineTypes, S: MathFontStyleT<ET>> {
    /// empty
    #[default]
    Empty,
    /// a single character
    Char { char: ET::Char, style: S },
    /// a box
    Box(TeXBox<ET>),
    /// a list of math nodes
    List {
        start: SourceRef<ET>,
        children: Box<[MathNode<ET, S>]>,
        end: SourceRef<ET>,
    },
}
impl<ET: EngineTypes> NodeTrait<ET> for MathKernel<ET, MathFontStyle<ET>> {
    fn display_fmt(&self, indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MathKernel::Empty => Ok(()),
            MathKernel::Char { char, .. } => {
                display_do_indent(indent, f)?;
                write!(f, "<char:{}/>", char)
            }
            MathKernel::Box(b) => b.display_fmt(indent, f),
            MathKernel::List { children, .. } => {
                for c in children.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                Ok(())
            }
        }
    }
    fn height(&self) -> ET::Dim {
        match self {
            MathKernel::Empty => ET::Dim::default(),
            MathKernel::Char { style, char } => style.get_font().get_ht(*char),
            MathKernel::Box(b) => b.height(),
            MathKernel::List { children, .. } => children
                .iter()
                .map(|c| c.height())
                .max()
                .unwrap_or_default(),
        }
    }
    fn width(&self) -> ET::Dim {
        match self {
            MathKernel::Empty => ET::Dim::default(),
            MathKernel::Char { style, char } => style.get_font().get_wd(*char),
            MathKernel::Box(b) => b.width(),
            MathKernel::List { children, .. } => children.iter().map(|c| c.width()).sum(),
        }
    }
    fn depth(&self) -> ET::Dim {
        match self {
            MathKernel::Empty => ET::Dim::default(),
            MathKernel::Char { style, char } => style.get_font().get_dp(*char),
            MathKernel::Box(b) => b.depth(),
            MathKernel::List { children, .. } => {
                children.iter().map(|c| c.depth()).max().unwrap_or_default()
            }
        }
    }
    fn nodetype(&self) -> NodeType {
        match self {
            MathKernel::Empty => NodeType::Math,
            MathKernel::Char { .. } => NodeType::MathChar,
            MathKernel::Box(b) => b.nodetype(),
            MathKernel::List { .. } => NodeType::Math,
        }
    }
}

/// A resolved math group; the result of a math list.
#[derive(Debug, Clone)]
pub struct MathGroup<ET: EngineTypes> {
    /// If this is a display math group, the `\abovedisplayskip` and `\belowdisplayskip`
    /// values.
    pub display: Option<(Skip<ET::Dim>, Skip<ET::Dim>)>,
    /// The nodes in this group
    pub children: Box<[MathNode<ET, MathFontStyle<ET>>]>,
    /// The source reference of the `$` or `$$` that started this group.
    pub start: SourceRef<ET>,
    /// The source reference of the `$` or `$$` that ended this group.
    pub end: SourceRef<ET>,
    /// If the mathgroup contained an `\eqno` or `\leqno`,
    /// the nodes *following* that command
    pub eqno: Option<(EqNoPosition, Box<[MathNode<ET, MathFontStyle<ET>>]>)>,
    /// The computed width of this group - i.e. the sum of the widths of the children.
    pub computed_width: OnceCell<ET::Dim>,
    /// The computed height of this group - i.e. the maximum of the heights of the children.
    pub computed_height: OnceCell<ET::Dim>,
    /// The computed depth of this group - i.e. the maximum of the depths of the children.
    pub computed_depth: OnceCell<ET::Dim>,
}
impl<ET: EngineTypes> NodeTrait<ET> for MathGroup<ET> {
    fn display_fmt(&self, indent: usize, f: &mut Formatter<'_>) -> std::fmt::Result {
        display_do_indent(indent, f)?;
        write!(
            f,
            "<{}math>",
            if self.display.is_some() {
                "display"
            } else {
                ""
            }
        )?;
        for c in self.children.iter() {
            c.display_fmt(indent + 2, f)?;
        }
        display_do_indent(indent, f)?;
        write!(
            f,
            "</{}math>",
            if self.display.is_some() {
                "display"
            } else {
                ""
            }
        )
    }
    fn height(&self) -> ET::Dim {
        *self.computed_height.get_or_init(|| {
            self.children
                .iter()
                .map(|c| c.height())
                .max()
                .unwrap_or_default()
        })
    }
    fn width(&self) -> ET::Dim {
        *self
            .computed_width
            .get_or_init(|| self.children.iter().map(|c| c.width()).sum())
    }
    fn depth(&self) -> ET::Dim {
        *self.computed_depth.get_or_init(|| {
            self.children
                .iter()
                .map(|c| c.depth())
                .max()
                .unwrap_or_default()
        })
    }
    fn nodetype(&self) -> NodeType {
        NodeType::Math
    }
    fn sourceref(&self) -> Option<(&SourceRef<ET>, &SourceRef<ET>)> {
        Some((&self.start, &self.end))
    }
}

impl<ET: EngineTypes> MathGroup<ET> {
    /// Create a new math group by closing a list of unresolved math nodes, iterating
    /// over it and resolving each node by determining the appropriate [`MathFontStyle`] for
    /// it.
    /// If `display` is `Some`, this is a display math group, and the two
    /// skips are the `\abovedisplayskip` and `\belowdisplayskip` values.
    pub fn close(
        state: &ET::State,
        display: Option<(Skip<ET::Dim>, Skip<ET::Dim>)>,
        start: SourceRef<ET>,
        end: SourceRef<ET>,
        children: Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
        eqno: Option<(EqNoPosition, Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>)>,
    ) -> Self {
        let style = if display.is_some() {
            MathStyle {
                style: MathStyleType::Display,
                cramped: false,
            }
        } else {
            MathStyle {
                style: MathStyleType::Text,
                cramped: false,
            }
        };
        let nch = Self::close_i(state, children, style);
        MathGroup {
            display,
            children: nch.into(),
            start,
            end,
            computed_width: OnceCell::new(),
            computed_height: OnceCell::new(),
            computed_depth: OnceCell::new(),
            eqno: eqno.map(|(pos, ch)| {
                (
                    pos,
                    Self::close_i(
                        state,
                        ch,
                        MathStyle {
                            style: MathStyleType::Text,
                            cramped: false,
                        },
                    )
                    .into(),
                )
            }),
        }
    }
    fn close_i(
        state: &ET::State,
        ls: Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
        mut style: MathStyle,
    ) -> Vec<MathNode<ET, MathFontStyle<ET>>> {
        ls.into_iter()
            .flat_map(|n| match n {
                MathNode::HSkip(s) => Some(MathNode::HSkip(s)),
                MathNode::HFil => Some(MathNode::HFil),
                MathNode::HFill => Some(MathNode::HFill),
                MathNode::HFilneg => Some(MathNode::HFilneg),
                MathNode::Hss => Some(MathNode::Hss),
                MathNode::Marker(UnresolvedMarkers::Display) => {
                    style.style = MathStyleType::Display;
                    None
                }
                MathNode::Marker(UnresolvedMarkers::Text) => {
                    style.style = MathStyleType::Text;
                    None
                }
                MathNode::Marker(UnresolvedMarkers::Script) => {
                    style.style = MathStyleType::Script;
                    None
                }
                MathNode::Marker(UnresolvedMarkers::ScriptScript) => {
                    style.style = MathStyleType::ScriptScript;
                    None
                }
                MathNode::Over {
                    start,
                    end,
                    top,
                    sep,
                    bottom,
                    left,
                    right,
                } => Some(MathNode::Over {
                    start,
                    end,
                    top: Self::close_i(state, top.into_vec(), style.numerator()).into(),
                    sep,
                    bottom: Self::close_i(state, bottom.into_vec(), style.denominator()).into(),
                    left: left.map(|(c, s)| (c, Self::resolve_style(state, style, s))),
                    right: right.map(|(c, s)| (c, Self::resolve_style(state, style, s))),
                }),
                MathNode::Choice(c) => Some(match style {
                    MathStyle {
                        style: MathStyleType::Display,
                        ..
                    } => MathNode::Choice(ResolvedChoice(
                        Self::close_i(state, c.display.into_vec(), style).into(),
                    )),
                    MathStyle {
                        style: MathStyleType::Text,
                        ..
                    } => MathNode::Choice(ResolvedChoice(
                        Self::close_i(state, c.text.into_vec(), style).into(),
                    )),
                    MathStyle {
                        style: MathStyleType::Script,
                        ..
                    } => MathNode::Choice(ResolvedChoice(
                        Self::close_i(state, c.script.into_vec(), style).into(),
                    )),
                    MathStyle {
                        style: MathStyleType::ScriptScript,
                        ..
                    } => MathNode::Choice(ResolvedChoice(
                        Self::close_i(state, c.scriptscript.into_vec(), style).into(),
                    )),
                }),
                MathNode::Space => Some(MathNode::Space),
                MathNode::Leaders(l) => Some(MathNode::Leaders(l)),
                MathNode::HKern(d) => Some(MathNode::HKern(d)),
                MathNode::Penalty(p) => Some(MathNode::Penalty(p)),
                MathNode::Mark(i, tl) => Some(MathNode::Mark(i, tl)),
                MathNode::VRule {
                    width,
                    height,
                    depth,
                    start,
                    end,
                } => Some(MathNode::VRule {
                    width,
                    height,
                    depth,
                    start,
                    end,
                }),
                MathNode::Whatsit(w) => Some(MathNode::Whatsit(w)),
                MathNode::Custom(n) => Some(MathNode::Custom(n)),
                MathNode::MSkip {
                    skip,
                    style: unresolved,
                } => Some(MathNode::MSkip {
                    skip,
                    style: Self::resolve_style(state, style, unresolved),
                }),
                MathNode::MKern {
                    kern,
                    style: unresolved,
                } => Some(MathNode::MKern {
                    kern,
                    style: Self::resolve_style(state, style, unresolved),
                }),
                MathNode::Atom(a) => Some(MathNode::Atom(MathAtom {
                    nucleus: Self::resolve_nucleus(state, a.nucleus, style),
                    sup: a
                        .sup
                        .map(|l| Self::close_i(state, l.into_vec(), style.sup()).into()),
                    sub: a
                        .sub
                        .map(|l| Self::close_i(state, l.into_vec(), style.sub()).into()),
                })),
            })
            .collect()
    }
    fn resolve_nucleus(
        state: &ET::State,
        n: MathNucleus<ET, UnresolvedMathFontStyle<ET>>,
        style: MathStyle,
    ) -> MathNucleus<ET, MathFontStyle<ET>> {
        match n {
            MathNucleus::Simple {
                cls: MathClass::Op,
                kernel,
                limits,
            } => MathNucleus::Simple {
                cls: MathClass::Op,
                kernel: Self::resolve_kernel(state, kernel, style),
                limits: match limits {
                    Some(l) => Some(l),
                    None => {
                        if style.style == MathStyleType::Display {
                            Some(true)
                        } else {
                            Some(false)
                        }
                    }
                },
            },
            MathNucleus::LeftRight {
                start,
                left,
                children,
                right,
                end,
            } => MathNucleus::LeftRight {
                start,
                left: left.map(|(c, s)| (c, Self::resolve_style(state, style, s))),
                children: Self::close_i(state, children.into_vec(), style).into(),
                right: right.map(|(c, s)| (c, Self::resolve_style(state, style, s))),
                end,
            },
            MathNucleus::Middle(c, f) => {
                MathNucleus::Middle(c, Self::resolve_style(state, style, f))
            }
            MathNucleus::Simple {
                cls,
                kernel,
                limits,
            } => MathNucleus::Simple {
                cls,
                kernel: Self::resolve_kernel(state, kernel, style),
                limits,
            },
            MathNucleus::Inner(k) => MathNucleus::Inner(Self::resolve_kernel(state, k, style)),
            MathNucleus::Overline(k) => {
                MathNucleus::Overline(Self::resolve_kernel(state, k, style))
            }
            MathNucleus::Underline(k) => {
                MathNucleus::Underline(Self::resolve_kernel(state, k, style))
            }
            MathNucleus::Accent {
                accent: (c, f),
                inner,
            } => MathNucleus::Accent {
                accent: (c, Self::resolve_style(state, style, f)),
                inner: Self::close_i(state, inner.into_vec(), style).into(),
            },
            MathNucleus::Radical { rad: (c, f), inner } => MathNucleus::Radical {
                rad: (c, Self::resolve_style(state, style, f)),
                inner: Self::close_i(state, inner.into_vec(), style).into(),
            },
            MathNucleus::VCenter {
                start,
                end,
                children,
                scaled,
            } => MathNucleus::VCenter {
                start,
                end,
                children,
                scaled,
            },
        }
    }
    fn resolve_kernel(
        state: &ET::State,
        n: MathKernel<ET, UnresolvedMathFontStyle<ET>>,
        style: MathStyle,
    ) -> MathKernel<ET, MathFontStyle<ET>> {
        match n {
            MathKernel::Empty => MathKernel::Empty,
            MathKernel::Char {
                char,
                style: unresolved,
            } => MathKernel::Char {
                char,
                style: Self::resolve_style(state, style, unresolved),
            },
            MathKernel::Box(b) => MathKernel::Box(b),
            MathKernel::List {
                start,
                children,
                end,
            } => MathKernel::List {
                start,
                children: Self::close_i(state, children.into_vec(), style).into(),
                end,
            },
        }
    }
    fn resolve_style(
        state: &ET::State,
        style: MathStyle,
        unresolved: UnresolvedMathFontStyle<ET>,
    ) -> MathFontStyle<ET> {
        let font = if unresolved.fam() > 15 {
            state.get_current_font().clone()
        } else {
            match style.style {
                MathStyleType::Script => state.get_scriptfont(unresolved.fam()).clone(),
                MathStyleType::ScriptScript => state.get_scriptscriptfont(unresolved.fam()).clone(),
                _ => state.get_textfont(unresolved.fam()).clone(),
            }
        };
        MathFontStyle {
            style: style.style,
            cramped: style.cramped,
            font,
        }
    }
}

/// The position of an eqno in a math list, i.e. `\eqno` (right) or `\leqno` (left).
#[derive(Debug, Copy, Clone)]
pub enum EqNoPosition {
    Left,
    Right,
}

/// Convenience struct for characters in math mode
#[derive(Debug, Clone)]
pub struct MathChar<ET: EngineTypes> {
    /// The character
    pub char: ET::Char,
    /// The math class determined from its mathcode
    pub cls: MathClass,
    /// The font style
    pub style: UnresolvedMathFontStyle<ET>,
}
impl<ET: EngineTypes> MathChar<ET> {
    /// Convert this into an unresolved [`MathAtom`].
    pub fn to_atom(self) -> MathAtom<ET, UnresolvedMathFontStyle<ET>> {
        MathAtom {
            nucleus: self.to_nucleus(),
            sup: None,
            sub: None,
        }
    }
    pub fn to_nucleus(self) -> MathNucleus<ET, UnresolvedMathFontStyle<ET>> {
        MathNucleus::Simple {
            cls: self.cls,
            kernel: MathKernel::Char {
                char: self.char,
                style: self.style,
            },
            limits: None,
        }
    }
    /// Create a new [`MathChar`] from a mathcode. If this was triggered by
    /// an actual character (rather than e.g. `\mathcar`), `source` is that
    /// character.
    pub fn from_u32(mathcode: u32, state: &ET::State, source: Option<ET::Char>) -> Self {
        let (mut cls, mut fam, pos) = {
            if mathcode == 0 {
                (
                    0,
                    0,
                    match source {
                        Some(c) => c.try_into().ok().unwrap(),
                        _ => 0,
                    },
                )
            } else {
                let char = (mathcode & 0xFF) as u8; // num % (16 * 16)
                let fam = ((mathcode >> 8) & 0xF) as u8; // (rest % 16)
                let rest_fam_shifted = (mathcode >> 12) & 0xF; // (((rest - fam) / 16) % 16)
                (rest_fam_shifted as u8, fam, char)
            }
        };
        if cls == 7 {
            let i = state.get_primitive_int(PRIMITIVES.fam).into();
            match i {
                i if !(0..=15).contains(&i) => {
                    cls = 0;
                }
                i => {
                    cls = 0;
                    fam = i as u8;
                }
            }
        }
        if cls > 7 {
            panic!("Invalid math class: {mathcode}({source:?}): {cls} {pos} {fam}");
        }
        let cls = MathClass::from(cls);
        let char = ET::Char::from(pos);
        MathChar {
            char,
            cls,
            style: UnresolvedMathFontStyle::of_fam(fam),
        }
    }
}

/// Convenience struct for math delimiters, as constructed from a delimiter code
#[derive(Clone, Debug)]
pub struct Delimiter<ET: EngineTypes> {
    /// The small variant of the delimiter
    pub small: MathChar<ET>,
    /// The large variant of the delimiter
    pub large: MathChar<ET>,
}
impl<ET: EngineTypes> Delimiter<ET> {
    fn default() -> Self {
        Delimiter {
            small: MathChar {
                char: ET::Char::from(0),
                cls: MathClass::Ord,
                style: UnresolvedMathFontStyle::of_fam(0),
            },
            large: MathChar {
                char: ET::Char::from(0),
                cls: MathClass::Ord,
                style: UnresolvedMathFontStyle::of_fam(0),
            },
        }
    }
    /// Create a new [`Delimiter`] from a delimiter code.
    pub fn from_int(num: ET::Int, state: &ET::State) -> Either<Self, (Self, i64)> {
        let num = num.into();
        if num < 0 || num > u32::MAX.into() {
            return either::Right((Self::default(), num));
        }
        let num = num as u32;
        let large = num & 0xFFF;
        let small = num >> 12;
        either::Left(Delimiter {
            small: MathChar::from_u32(small, state, None),
            large: MathChar::from_u32(large, state, None),
        })
    }
}

/// An open list of unresolved math nodes.
/// TODO: rethink this
#[derive(Clone, Debug)]
pub enum MathNodeList<ET: EngineTypes> {
    /// A simple list of nodes
    Simple(Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>),
    /// An open list after encountering an `\over` or `\above` or `\atop` or
    /// a related command. The current list up to that point is moved to the `top`,
    /// subsequent nodes are added to `bottom`. (see [`MathNode::Over`]
    Over {
        top: Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
        sep: Option<ET::Dim>,
        bottom: Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
        left: Option<(ET::Char, UnresolvedMathFontStyle<ET>)>,
        right: Option<(ET::Char, UnresolvedMathFontStyle<ET>)>,
    },
    /// An open list after encountering an `\eqno` or `\leqno`.
    /// The current list up to that point is moved to `main`,
    /// subsequent nodes are added to `eqno`. This can
    /// only happen, if this list's direct "parent" is a horizontal
    /// (i.e. non-math) list.
    EqNo {
        pos: EqNoPosition,
        main: Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
        eqno: Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
    },
}
impl<ET: EngineTypes> Default for MathNodeList<ET> {
    fn default() -> Self {
        MathNodeList::Simple(Vec::new())
    }
}
impl<ET: EngineTypes> MathNodeList<ET> {
    /// Push a node to the list.
    pub fn push(&mut self, n: MathNode<ET, UnresolvedMathFontStyle<ET>>) {
        match self {
            MathNodeList::Simple(v) => v.push(n),
            MathNodeList::Over { bottom, .. } => bottom.push(n),
            MathNodeList::EqNo { eqno, .. } => eqno.push(n),
        }
    }
    /// Close the list, returning the list of nodes and the optional eqno.
    pub fn close(
        self,
        start: SourceRef<ET>,
        end: SourceRef<ET>,
    ) -> (
        Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>,
        Option<(EqNoPosition, Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>>)>,
    ) {
        match self {
            MathNodeList::Simple(v) => (v, None),
            MathNodeList::Over {
                top,
                sep,
                bottom,
                left,
                right,
            } => (
                vec![MathNode::Over {
                    start,
                    end,
                    top: top.into(),
                    bottom: bottom.into(),
                    sep,
                    left,
                    right,
                }],
                None,
            ),
            MathNodeList::EqNo { main, eqno, pos } => (main, Some((pos, eqno))),
        }
    }
    /// Get the "open" list that nodes should be added to mutably
    pub fn list_mut(&mut self) -> &mut Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>> {
        match self {
            MathNodeList::Simple(v) => v,
            MathNodeList::Over { bottom, .. } => bottom,
            MathNodeList::EqNo { eqno, .. } => eqno,
        }
    }
    /// Get the "open" list that nodes should be added to immutably
    pub fn list(&self) -> &Vec<MathNode<ET, UnresolvedMathFontStyle<ET>>> {
        match self {
            MathNodeList::Simple(v) => v,
            MathNodeList::Over { bottom, .. } => bottom,
            MathNodeList::EqNo { eqno, .. } => eqno,
        }
    }
}

///Types of open math lists
#[derive(Clone, Debug)]
pub enum MathNodeListType<ET: EngineTypes> {
    /// The top-most math list
    Top {
        /// whether delimited by `$$` rather than `$`
        display: bool,
    },
    /// complex list target
    Target(ListTarget<ET, MathNode<ET, UnresolvedMathFontStyle<ET>>>),
    /// A list opened by `\left`, to be closed by `\right`
    LeftRight(Option<Delimiter<ET>>),
}
