use crate::engine::fonts::FontStore;
use crate::engine::{Font, Refs, Res, SRef, Types};
use std::any::Any;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::fmt::{Display, Formatter, Write};
use std::ops::{Add, AddAssign};
use tex_engine::commands::primitives::PRIMITIVES;
use tex_engine::engine::filesystem::FileSystem;
use tex_engine::engine::fontsystem::{Font as FontTrait, FontSystem};
use tex_engine::engine::state::State;
use tex_engine::engine::stomach::methods::ParLineSpec;
use tex_engine::pdflatex::nodes::{
    ActionSpec, ColorStackAction, GotoAction, NumOrName, PDFColor, PDFExtension, PDFStartLink,
    PDFXImage,
};
use tex_engine::prelude::{HNode, MathNode, VNode};
use tex_engine::tex::nodes::boxes::{HBoxInfo, ToOrSpread, VBoxInfo};
use tex_engine::tex::nodes::math::{MathClass, MathFontStyle, MathStyleType};
use tex_engine::tex::numerics::{Dim32, Skip, StretchShrink};
use tex_engine::utils::{HMap, HSet};
use tex_glyphs::fontstyles::ModifierSeq;
use tex_glyphs::glyphs::{Glyph, GlyphName};
//use crate::shipout::html::{HTMLChild, HTMLNode};
use crate::engine::nodes::LineSkip;
use crate::shipout::annotations;
use crate::shipout::utils::VNodes;
use crate::utils::{Margin, VecMap, VecSet};

mod sealed {
    pub(crate) trait Sealed {}
}
#[derive(Copy, Clone, PartialEq, Eq)]
pub(crate) enum ModeKind {
    Top,
    V,
    H,
    Par,
    Table,
    Row,
    Math,
    SVG,
}
pub(crate) trait ShipoutModeT: sealed::Sealed + Any {
    type NodeType: ShipoutNodeT;
    fn kind() -> ModeKind;
}
pub(crate) trait VLike: ShipoutModeT<NodeType = ShipoutNodeV> {}
pub(crate) trait HLike: ShipoutModeT<NodeType = ShipoutNodeH> {}
#[derive(Debug)]
pub(crate) struct Top;
impl sealed::Sealed for Top {}
impl ShipoutModeT for Top {
    type NodeType = ShipoutNodeV;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::Top
    }
}
impl VLike for Top {}

#[derive(Debug)]
pub(crate) struct V;
impl sealed::Sealed for V {}
impl ShipoutModeT for V {
    type NodeType = ShipoutNodeV;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::V
    }
}
impl VLike for V {}
pub(crate) struct H;
impl sealed::Sealed for H {}
impl ShipoutModeT for H {
    type NodeType = ShipoutNodeH;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::H
    }
}
impl HLike for H {}
pub(crate) struct Par;
impl sealed::Sealed for Par {}
impl ShipoutModeT for Par {
    type NodeType = ShipoutNodeH;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::Par
    }
}
impl HLike for Par {}

pub(crate) struct Table {
    num_cols: u8,
}
impl sealed::Sealed for Table {}
impl ShipoutModeT for Table {
    type NodeType = ShipoutNodeTable;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::Table
    }
}
pub(crate) struct Row {
    num_cols: u8,
}
impl sealed::Sealed for Row {}
impl ShipoutModeT for Row {
    type NodeType = ShipoutNodeHRow;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::Row
    }
}
pub(crate) struct Math;
impl sealed::Sealed for Math {}
impl ShipoutModeT for Math {
    type NodeType = ShipoutNodeM;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::Math
    }
}

#[derive(Default)]
pub(crate) struct SVG;

impl sealed::Sealed for SVG {}
impl ShipoutModeT for SVG {
    type NodeType = ShipoutNodeSVG;
    #[inline(always)]
    fn kind() -> ModeKind {
        ModeKind::SVG
    }
}

#[derive(Debug)]
pub(crate) struct ShipoutState {
    pub(crate) output: Vec<ShipoutNodeV>,
    pub(crate) top_font: Option<Font>,
    pub(crate) top_width: Option<i32>,
    pub(crate) page_width: Option<i32>,
    nullfont: Option<Font>,
    pub(crate) previous: Vec<(ShipoutNodes, ShipoutWrapper)>,
    pub(crate) wrapper: ShipoutWrapper,
    //wrappers: Vec<ShipoutWrapper>,
    pub(crate) font_data: HMap<Box<str>, FontData>, /*
                                                    pub(crate) output:Vec<HTMLChild>,
                                                    pub(crate) nodes:Vec<HTMLNode>,
                                                    pub(crate) fonts: Vec<Font>,
                                                    pub(crate) colors:Vec<PDFColor>,
                                                    pub(crate) widths: Vec<Dim32>,
                                                    pub(crate) lineskip:Vec<LineSkip>,
                                                    pub(crate) modes:Vec<ShipoutMode>,
                                                    pub(crate) fontlinks:HSet<String>,
                                                    pub(crate) in_content:bool,
                                                    pub(crate) missing_glyphs: HSet<(String,u8,String)>,
                                                    pub(crate) missing_fonts: HSet<String> */
}

#[derive(Default, Clone, Debug)]
pub(crate) enum ShipoutWrapper {
    Color(PDFColor),
    Font(Font),
    Link(String),
    Annotation {
        start: SRef,
        attrs: VecMap<Cow<'static, str>, Cow<'static, str>>,
        styles: VecMap<Cow<'static, str>, Cow<'static, str>>,
        classes: VecSet<Cow<'static, str>>,
        tag: Option<String>,
    },
    SVG {
        tag: String,
        attrs: VecMap<&'static str, String>,
    },
    Matrix {
        scale: f32,
        rotate: f32,
        skewx: f32,
        skewy: f32,
    },
    #[default]
    None,
}
impl ShipoutWrapper {
    fn kind(&self) -> Option<WrapperKind> {
        match self {
            ShipoutWrapper::Color(..) => Some(WrapperKind::Color),
            ShipoutWrapper::Font(..) => Some(WrapperKind::Font),
            ShipoutWrapper::Link(..) => Some(WrapperKind::Link),
            ShipoutWrapper::Annotation { .. } => Some(WrapperKind::Annotation),
            ShipoutWrapper::SVG { .. } => Some(WrapperKind::SVG),
            ShipoutWrapper::Matrix { .. } => Some(WrapperKind::Matrix),
            ShipoutWrapper::None => None,
        }
    }
    #[inline(always)]
    fn prec(&self) -> u8 {
        match self.kind() {
            Some(k) => k.prec(),
            None => 255,
        }
    }
    fn close_ii<Node: ShipoutNodeT>(
        self,
        engine: Refs,
        fonts: &mut HMap<Box<str>, FontData>,
        nodes: Vec<Node>,
    ) -> Result<Common<Node>, Vec<Node>> {
        match self {
            ShipoutWrapper::Color(c) => Common::with_color(c, nodes),
            ShipoutWrapper::Font(f) => Common::with_font(engine, fonts, f, nodes),
            ShipoutWrapper::Link(href) => Ok(Common::with_link(href, nodes)),
            ShipoutWrapper::Annotation {
                attrs,
                styles,
                classes,
                tag,
                ..
            } => Ok(Common::with_annotation(attrs, styles, classes, tag, nodes)),
            ShipoutWrapper::Matrix {
                scale,
                rotate,
                skewx,
                skewy,
            } => Ok(Common::with_matrix(scale, rotate, skewx, skewy, nodes)),
            _ => unreachable!(),
        }
    }
    #[inline(always)]
    fn close_svg(
        self,
        engine: Refs,
        fonts: &mut HMap<Box<str>, FontData>,
        nodes: Vec<ShipoutNodeSVG>,
    ) -> Result<ShipoutNodeSVG, Vec<ShipoutNodeSVG>> {
        match self {
            ShipoutWrapper::SVG { tag, attrs } => Ok(ShipoutNodeSVG::svg_node(tag, attrs, nodes)),
            _ => self.close_ii(engine, fonts, nodes).map(|e| e.into()),
        }
    }
    fn close_i(
        self,
        engine: Refs,
        fonts: &mut HMap<Box<str>, FontData>,
        nodes: ShipoutNodes,
        target: &mut ShipoutNodes,
    ) {
        if nodes.is_empty()
            || (match &nodes {
                ShipoutNodes::V(v) => v.iter().all(|n| matches!(n, ShipoutNodeV::KernSkip(_))),
                _ => false,
            })
        {
            match self.kind() {
                Some(
                    WrapperKind::Color
                    | WrapperKind::Annotation
                    | WrapperKind::Font
                    | WrapperKind::Link,
                ) => return,
                _ => (),
            }
        }
        match (nodes, target) {
            (ShipoutNodes::V(nodes), ShipoutNodes::V(target)) => {
                match self.close_ii(engine, fonts, nodes) {
                    Ok(r) => target.push(r.into()),
                    Err(ls) => target.extend(ls.into_iter()),
                }
            }
            (ShipoutNodes::H(nodes), ShipoutNodes::H(target)) => {
                match self.close_ii(engine, fonts, nodes) {
                    Ok(r) => target.push(r.into()),
                    Err(ls) => target.extend(ls.into_iter()),
                }
            }
            (ShipoutNodes::HRow(nodes), ShipoutNodes::HRow(target)) => {
                match self.close_ii(engine, fonts, nodes) {
                    Ok(r) => target.push(r.into()),
                    Err(ls) => target.extend(ls.into_iter()),
                }
            }
            (ShipoutNodes::Math(nodes), ShipoutNodes::Math(target)) => {
                match self.close_ii(engine, fonts, nodes) {
                    Ok(r) => target.push(r.into()),
                    Err(ls) => target.extend(ls.into_iter()),
                }
            }
            (ShipoutNodes::Table(nodes), ShipoutNodes::Table(target)) => {
                match self.close_ii(engine, fonts, nodes) {
                    Ok(r) => target.push(r.into()),
                    Err(ls) => target.extend(ls.into_iter()),
                }
            }
            (ShipoutNodes::SVG(nodes), ShipoutNodes::SVG(target)) => {
                match self.close_svg(engine, fonts, nodes) {
                    Ok(r) => target.push(r.into()),
                    Err(ls) => target.extend(ls.into_iter()),
                }
            }
            _ => unreachable!(),
        }
    }
}
#[derive(Copy, Clone, PartialEq, Eq)]
enum WrapperKind {
    Color,
    Font,
    Link,
    Annotation,
    Matrix,
    SVG,
}
impl WrapperKind {
    fn prec(self) -> u8 {
        match self {
            WrapperKind::Color | WrapperKind::Font => 1,
            WrapperKind::Link => 5,
            WrapperKind::Annotation => 10,
            WrapperKind::SVG => 15,
            WrapperKind::Matrix => 20,
        }
    }
}

impl PartialEq<ShipoutWrapper> for WrapperKind {
    fn eq(&self, other: &ShipoutWrapper) -> bool {
        match self {
            WrapperKind::Color => matches!(other, ShipoutWrapper::Color(..)),
            WrapperKind::Font => matches!(other, ShipoutWrapper::Font(..)),
            WrapperKind::Link => matches!(other, ShipoutWrapper::Link(..)),
            WrapperKind::Annotation => matches!(other, ShipoutWrapper::Annotation { .. }),
            WrapperKind::SVG => matches!(other, ShipoutWrapper::SVG { .. }),
            WrapperKind::Matrix => matches!(other, ShipoutWrapper::Matrix { .. }),
        }
    }
}
impl PartialEq<Option<ShipoutWrapper>> for WrapperKind {
    fn eq(&self, other: &Option<ShipoutWrapper>) -> bool {
        match other {
            Some(other) => self == other,
            None => false,
        }
    }
}

impl ShipoutWrapper {
    fn is_none(&self) -> bool {
        matches!(self, ShipoutWrapper::None)
    }

    fn close_all_svg(state: &mut Shipout<SVG>) {
        let mut reopen = Vec::new();
        loop {
            match state.wrapper.kind() {
                Some(WrapperKind::SVG) => state.close_node(),
                Some(_) => {
                    let (nodes, mut wrapper) = state.previous.pop().unwrap();
                    let nodes = std::mem::replace(
                        &mut state.nodes,
                        ShipoutNodeSVG::get(nodes).unwrap_or_else(|| todo!()),
                    );
                    std::mem::swap(&mut state.wrapper, &mut wrapper);
                    reopen.push(wrapper.clone());
                    match wrapper.close_ii(state.engine, &mut state.top_state.font_data, nodes) {
                        Ok(r) => state.nodes.push(r.into()),
                        Err(ls) => state.nodes.extend(ls.into_iter()),
                    }
                }
                None => break,
            }
        }
        for r in reopen.into_iter().rev() {
            let nodes = std::mem::take(&mut state.nodes);
            let wrapper = std::mem::replace(&mut state.wrapper, r);
            state
                .previous
                .push((ShipoutNodeSVG::into_nodes(nodes), wrapper));
        }
    }
    pub(crate) fn close_all<Mode: ShipoutModeT>(state: &mut Shipout<Mode>) -> Vec<ShipoutWrapper> {
        let mut reopen = Vec::new();
        loop {
            match state.wrapper.kind() {
                Some(_) => {
                    let (nodes, mut wrapper) = state.previous.pop().unwrap();
                    let nodes = std::mem::replace(
                        &mut state.nodes,
                        Mode::NodeType::get(nodes).unwrap_or_else(|| todo!()),
                    );
                    std::mem::swap(&mut state.wrapper, &mut wrapper);
                    reopen.push(wrapper.clone());
                    match wrapper.close_ii(state.engine, &mut state.top_state.font_data, nodes) {
                        Ok(r) => state.nodes.push(r.into()),
                        Err(ls) => state.nodes.extend(ls.into_iter()),
                    }
                }
                None => {
                    reopen.reverse();
                    return reopen;
                }
            }
        }
    }
    fn close<Mode: ShipoutModeT>(state: &mut Shipout<Mode>, kind: WrapperKind) {
        let mut reopen = Vec::new();
        let mut curr = (
            Mode::NodeType::into_nodes(std::mem::take(&mut state.nodes)),
            std::mem::take(&mut state.wrapper),
        );
        while kind != curr.1 {
            if let Some(next) = state.previous.pop() {
                reopen.push(std::mem::replace(&mut curr, next));
            } else {
                // TODO this shouldn't happen
                state.previous.push(curr);
                for r in reopen.into_iter().rev() {
                    state.previous.push(r);
                }
                let (nodes, wrapper) = state.previous.pop().unwrap();
                state.nodes = Mode::NodeType::get(nodes).unwrap();
                state.wrapper = wrapper;
                return;
            }
        }
        if state.previous.is_empty() {
            todo!()
        }
        let mut last = 1usize;
        for mut r in reopen.into_iter().rev() {
            if r.1.prec() <= kind.prec() {
                let w = r.1.clone();
                let ns = r.0.clone_new();
                r.1.close_i(
                    state.engine,
                    &mut state.top_state.font_data,
                    r.0,
                    &mut curr.0,
                );
                state.previous.push((ns, w));
                last += 1;
            } else {
                let rclone = r.0.clone_new();
                let r0 = std::mem::replace(&mut r.0, rclone);
                let curr0 = std::mem::replace(&mut curr.0, r0);
                let idx = state.previous.len() - last;
                curr.1.clone().close_i(
                    state.engine,
                    &mut state.top_state.font_data,
                    curr0,
                    &mut state.previous.get_mut(idx).unwrap().0,
                );
                last = 1;
                state.previous.push(r);
            }
        }
        let idx = state.previous.len() - last;
        curr.1.close_i(
            state.engine,
            &mut state.top_state.font_data,
            curr.0,
            &mut state.previous.get_mut(idx).unwrap().0,
        );
        let (nodes, wrapper) = state.previous.pop().unwrap();
        state.nodes = Mode::NodeType::get(nodes).unwrap_or_else(|| todo!());
        state.wrapper = wrapper;
    }
}

pub(crate) struct Shipout<'a, 'b, Mode: ShipoutModeT> {
    pub(crate) engine: Refs<'a, 'b>,
    pub(crate) top_state: &'a mut ShipoutState,
    pub(crate) wrapper: ShipoutWrapper,
    pub(crate) nodes: Vec<Mode::NodeType>,
    pub(crate) state: Mode,
    pub(crate) previous: Vec<(ShipoutNodes, ShipoutWrapper)>,
}
impl<'a, 'b, Mode: ShipoutModeT> Shipout<'a, 'b, Mode> {
    #[inline(always)]
    fn do_in<R, M: ShipoutModeT>(
        &mut self,
        new: impl FnOnce() -> M,
        f: impl FnOnce(&mut Shipout<M>) -> R,
    ) -> (R, Vec<M::NodeType>, bool, bool) {
        self.previous.push((
            Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
            std::mem::take(&mut self.wrapper),
        ));
        let mut s = Shipout {
            engine: self.engine,
            nodes: Vec::new(),
            state: new(),
            wrapper: ShipoutWrapper::None,
            top_state: self.top_state,
            previous: std::mem::take(&mut self.previous),
        };
        let r = f(&mut s);
        let reopen = ShipoutWrapper::close_all(&mut s);
        assert!(s.wrapper.is_none());
        let (nodes, wrapper) = s.previous.pop().unwrap();
        self.previous = s.previous;
        let nodes = Mode::NodeType::get(nodes).unwrap_or_else(|| todo!());
        self.nodes = nodes;
        self.wrapper = wrapper;
        let uses_color = s.nodes.iter().any(|n| n.uses_previous_color());
        let uses_font = s.nodes.iter().any(|n| n.uses_previous_font());
        for r in reopen.into_iter() {
            let nodes = std::mem::take(&mut self.nodes);
            let wrapper = std::mem::replace(&mut self.wrapper, r);
            self.previous
                .push((Mode::NodeType::into_nodes(nodes), wrapper));
        }
        (r, s.nodes, uses_color, uses_font)
    }

    #[inline(always)]
    pub(crate) fn in_v<R>(
        &mut self,
        start: SRef,
        end: SRef,
        info: VBoxInfo<Types>,
        f: impl FnOnce(&mut Shipout<V>) -> Result<R, Option<VNode<Types>>>,
    ) -> Result<R, Option<VNode<Types>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(|| V, f);
        self.nodes.push(
            Common::VBox {
                sref: SourceRef::new(start, end, self.engine),
                info,
                children: nodes,
                uses_color,
                uses_font,
            }
            .into(),
        );
        r
    }
    #[inline(always)]
    pub(crate) fn in_h<R>(
        &mut self,
        start: SRef,
        end: SRef,
        info: HBoxInfo<Types>,
        preskip: Option<Skip<Dim32>>,
        f: impl FnOnce(&mut Shipout<H>) -> Result<R, Option<HNode<Types>>>,
    ) -> Result<R, Option<HNode<Types>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(|| H, f);
        self.nodes.push(
            Common::HBox {
                sref: SourceRef::new(start, end, self.engine),
                info,
                children: nodes,
                preskip: preskip.map(|r| r.into()),
                uses_color,
                uses_font,
            }
            .into(),
        );
        r
    }
    #[inline(always)]
    pub(crate) fn in_svg<R>(
        &mut self,
        start: SRef,
        end: SRef,
        minx: i32,
        miny: i32,
        maxx: i32,
        maxy: i32,
        f: impl FnOnce(&mut Shipout<SVG>) -> Result<R, Option<VNode<Types>>>,
    ) -> Result<R, Option<VNode<Types>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(
            || SVG,
            |state| {
                let r = f(state);
                ShipoutWrapper::close_all_svg(state);
                r
            },
        );
        self.nodes.push(
            Common::SVG {
                children: nodes,
                minx,
                miny,
                maxx,
                maxy,
                sref: SourceRef::new(start, end, self.engine),
                uses_color,
                uses_font,
            }
            .into(),
        );
        r
    }

    #[inline(always)]
    pub(crate) fn push(&mut self, node: Mode::NodeType) {
        self.nodes.push(node)
    }

    #[inline(always)]
    pub(crate) fn open_link(&mut self, link: PDFStartLink<Types>) {
        let s = match link.action {
            ActionSpec::Goto(GotoAction::Current { target, .. }) => {
                format!("#{}", target.as_name())
            }
            ActionSpec::User(str) => {
                let url = if str.contains("/URI(") {
                    str.split("/URI(")
                        .last()
                        .unwrap()
                        .split(')')
                        .next()
                        .unwrap()
                } else if str.contains("/F(") {
                    str.split("/F(").last().unwrap().split(')').next().unwrap()
                } else {
                    ""
                };
                url.to_string()
            }
            _ => todo!(),
        };
        let oldwrap = std::mem::replace(&mut self.wrapper, ShipoutWrapper::Link(s));
        self.previous.push((
            Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
            oldwrap,
        ))
    }
    #[inline(always)]
    pub(crate) fn close_link(&mut self) {
        ShipoutWrapper::close(self, WrapperKind::Link)
    }
    #[inline(always)]
    pub(crate) fn open_font(&mut self, font: Font, global: bool) {
        if global {
            self.top_state.top_font = Some(font)
        } else {
            let oldwrap = std::mem::replace(&mut self.wrapper, ShipoutWrapper::Font(font));
            self.previous.push((
                Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
                oldwrap,
            ))
        }
    }
    #[inline(always)]
    pub(crate) fn close_font(&mut self) {
        ShipoutWrapper::close(self, WrapperKind::Font)
    }
    #[inline(always)]
    pub(crate) fn open_annot(
        &mut self,
        start: SRef,
        attrs: VecMap<Cow<'static, str>, Cow<'static, str>>,
        styles: VecMap<Cow<'static, str>, Cow<'static, str>>,
        classes: VecSet<Cow<'static, str>>,
        tag: Option<String>,
    ) {
        let oldwrap = std::mem::replace(
            &mut self.wrapper,
            ShipoutWrapper::Annotation {
                start,
                attrs,
                styles,
                classes,
                tag,
            },
        );
        self.previous.push((
            Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
            oldwrap,
        ))
    }
    #[inline(always)]
    pub(crate) fn close_annot(&mut self, end: SRef) {
        // TODO: end
        ShipoutWrapper::close(self, WrapperKind::Annotation)
    }
    #[inline(always)]
    pub(crate) fn open_matrix(&mut self, scale: f32, rotate: f32, skewx: f32, skewy: f32) {
        let oldwrap = std::mem::replace(
            &mut self.wrapper,
            ShipoutWrapper::Matrix {
                scale,
                rotate,
                skewx,
                skewy,
            },
        );
        self.previous.push((
            Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
            oldwrap,
        ))
    }
    #[inline(always)]
    pub(crate) fn close_matrix(&mut self) {
        ShipoutWrapper::close(self, WrapperKind::Matrix)
    }
    #[inline(always)]
    pub(crate) fn do_color(&mut self, act: ColorStackAction) {
        let stack = self.engine.aux.extension.colorstacks();
        match act {
            ColorStackAction::Set(idx, c) => {
                *stack[idx].last_mut().unwrap() = c;
                if *self.engine.aux.extension.current_colorstack() == idx {
                    let oldwrap = std::mem::replace(&mut self.wrapper, ShipoutWrapper::Color(c));
                    self.previous.push((
                        Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
                        oldwrap,
                    ))
                }
            }
            ColorStackAction::Push(idx, c) => {
                stack[idx].push(c);
                if *self.engine.aux.extension.current_colorstack() == idx {
                    let oldwrap = std::mem::replace(&mut self.wrapper, ShipoutWrapper::Color(c));
                    self.previous.push((
                        Mode::NodeType::into_nodes(std::mem::take(&mut self.nodes)),
                        oldwrap,
                    ))
                }
            }
            ColorStackAction::Pop(idx) => {
                stack[idx].pop();
                if *self.engine.aux.extension.current_colorstack() == idx {
                    ShipoutWrapper::close(self, WrapperKind::Color)
                }
            }
            ColorStackAction::Current(idx) => {
                if *self.engine.aux.extension.current_colorstack() != idx {
                    todo!()
                }
            }
        }
    }
}
impl<'a, 'b, Mode: VLike> Shipout<'a, 'b, Mode> {
    pub(crate) fn skipv(&mut self, skip: Margin) {
        if skip.is_zero() {
            return;
        }
        if let Some(ShipoutNodeV::KernSkip(old)) = self.nodes.last_mut() {
            *old += skip;
            if old.is_zero() {
                self.nodes.pop();
            }
        } else {
            self.push(ShipoutNodeV::KernSkip(skip))
        }
    }
    #[inline(always)]
    pub(crate) fn reopen_halign(
        &mut self,
        start: SRef,
        end: SRef,
        children: &mut Vec<ShipoutNodeTable>,
        num_cols: &mut u8,
        uses_color: &mut bool,
        uses_font: &mut bool,
        f: impl FnOnce(&mut Shipout<Row>) -> Result<(), Option<HNode<Types>>>,
    ) -> Result<(), Option<HNode<Types>>> {
        let ((r, nc), nodes, uc2, uf2) = self.do_in(
            || Row { num_cols: 0 },
            |state| {
                let r = f(state);
                (r, state.state.num_cols)
            },
        );
        *num_cols = (*num_cols).max(nc);
        let sref = SourceRef::new(start, end, self.engine);
        children.push(ShipoutNodeTable::Row {
            sref,
            num_cols: nc,
            children: nodes,
            uses_color: uc2,
            uses_font: uf2,
        });
        *uses_font |= uf2;
        *uses_color |= uc2;
        r
    }
    #[inline(always)]
    pub(crate) fn in_halign<R>(
        &mut self,
        line_skip: LineSkip,
        f: impl FnOnce(&mut Shipout<Table>) -> Result<R, Option<VNode<Types>>>,
    ) -> Result<R, Option<VNode<Types>>> {
        let ((r, num_cols), nodes, uses_color, uses_font) = self.do_in(
            || Table { num_cols: 0 },
            |state| {
                let r = f(state);
                (r, state.state.num_cols)
            },
        );
        self.nodes.push(ShipoutNodeV::HAlign {
            children: nodes,
            uses_color,
            uses_font,
            num_cols,
            line_skip,
        });
        r
    }
    #[inline(always)]
    pub(crate) fn in_par<R>(
        &mut self,
        mut specs: Vec<ParLineSpec<Types>>,
        start: SRef,
        end: SRef,
        lineskip: LineSkip,
        parskip: Skip<Dim32>,
        f: impl FnOnce(&mut Shipout<Par>) -> Result<R, Option<VNode<Types>>>,
    ) -> Result<R, Option<VNode<Types>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(|| Par, f);
        let spec = specs.pop().unwrap();
        let align: Alignment = Alignment::from(spec.leftskip, spec.rightskip);

        let p = ShipoutNodeV::Paragraph {
            sref: SourceRef::new(start, end, self.engine),
            width: spec.target.0,
            left_skip: spec.leftskip.into(),
            right_skip: spec.rightskip.into(),
            parskip: parskip.into(),
            line_skip: lineskip,
            alignment: align,
            children: nodes,
            uses_color,
            uses_font,
        };
        self.nodes.push(p);
        r
    }
}

impl<'a, 'b, Mode: HLike> Shipout<'a, 'b, Mode> {
    pub(crate) fn indent(&mut self, indent: i32) {
        if indent == 0 {
            return;
        }
        if let Some(ShipoutNodeH::Indent(old)) = self.nodes.last_mut() {
            *old += indent;
            if *old == 0 {
                self.nodes.pop();
            }
        } else {
            self.push(ShipoutNodeH::Indent(indent))
        }
    }
    pub(crate) fn skiph(&mut self, skip: Margin) {
        if skip.is_zero() {
            return;
        }
        if let Some(ShipoutNodeH::KernSkip(old)) = self.nodes.last_mut() {
            *old += skip;
            if old.is_zero() {
                self.nodes.pop();
            }
        } else {
            self.push(ShipoutNodeH::KernSkip(skip))
        }
    }

    #[inline(always)]
    pub(crate) fn in_math<R>(
        &mut self,
        start: SRef,
        end: SRef,
        display: Option<(Margin, Margin)>,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(|| Math, f);
        self.nodes.push(
            ShipoutNodeH::Math {
                sref: SourceRef::new(start, end, self.engine),
                display,
                children: nodes,
                uses_color,
                uses_font,
            }
            .into(),
        );
        r
    }
}

impl<'a, 'b> Shipout<'a, 'b, Table> {
    #[inline(always)]
    pub(crate) fn in_noalign(
        &mut self,
        f: impl FnOnce(&mut Shipout<V>) -> Result<(), Option<VNode<Types>>>,
    ) -> Result<(), Option<VNode<Types>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(|| V, f);
        self.nodes.push(ShipoutNodeTable::NoAlign {
            children: nodes,
            uses_color,
            uses_font,
        });
        r
    }
    #[inline(always)]
    pub(crate) fn in_row(
        &mut self,
        start: SRef,
        end: SRef,
        f: impl FnOnce(&mut Shipout<Row>) -> Result<(), Option<HNode<Types>>>,
    ) -> Result<(), Option<HNode<Types>>> {
        let ((r, num_cols), nodes, uses_color, uses_font) = self.do_in(
            || Row { num_cols: 0 },
            |state| {
                let r = f(state);
                (r, state.state.num_cols)
            },
        );
        self.state.num_cols = self.state.num_cols.max(num_cols);
        self.nodes.push(ShipoutNodeTable::Row {
            sref: SourceRef::new(start, end, self.engine),
            num_cols,
            children: nodes,
            uses_color,
            uses_font,
        });
        r
    }
}

impl<'a, 'b> Shipout<'a, 'b, Row> {
    #[inline(always)]
    pub(crate) fn in_cell(
        &mut self,
        start: SRef,
        end: SRef,
        height: Dim32,
        spans: u8,
        f: impl FnOnce(&mut Shipout<H>) -> Result<(), Option<HNode<Types>>>,
    ) -> Result<(), Option<HNode<Types>>> {
        let (r, nodes, uses_color, uses_font) = self.do_in(|| H, f);
        self.state.num_cols += spans + 1;
        self.nodes.push(ShipoutNodeHRow::Cell {
            sref: SourceRef::new(start, end, self.engine),
            spans: spans + 1,
            children: nodes,
            height,
            uses_color,
            uses_font,
        });
        r
    }
}
impl<'a, 'b> Shipout<'a, 'b, SVG> {
    pub(crate) fn open_node(&mut self, mut attrs: VecMap<&'static str, String>, tag: String) {
        for (k, v) in attrs.inner.iter_mut() {
            if *k == "stroke-width" {
                *v = Self::strtonum(v)
            } else if *k == "d" {
                *v = Self::parse_path(v)
            } else if *k == "transform" {
                *v = Self::parse_transform(v)
            }
        }
        let oldwrap = std::mem::replace(&mut self.wrapper, ShipoutWrapper::SVG { tag, attrs });
        self.previous.push((
            ShipoutNodeSVG::into_nodes(std::mem::take(&mut self.nodes)),
            oldwrap,
        ))
    }

    fn strtonum(ts: &str) -> String {
        (ts.parse::<f32>().unwrap() * 1.5).to_string()
    }
    fn parse_path(ts: &str) -> String {
        let mut ret = String::new();
        let mut s = ts.trim();
        while !s.is_empty() {
            if s.starts_with("M") {
                s = s[1..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                let n2 = Self::parse_get_num(&mut s);
                write!(ret, "M {} {} ", Self::scale(n1), Self::scale(-n2)).unwrap();
            } else if s.starts_with("L") {
                s = s[1..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                let n2 = Self::parse_get_num(&mut s);
                write!(ret, "L {} {} ", Self::scale(n1), Self::scale(-n2)).unwrap();
            } else if s.starts_with("C") {
                s = s[1..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                let n2 = Self::parse_get_num(&mut s);
                let n3 = Self::parse_get_num(&mut s);
                let n4 = Self::parse_get_num(&mut s);
                let n5 = Self::parse_get_num(&mut s);
                let n6 = Self::parse_get_num(&mut s);
                write!(
                    ret,
                    "C {} {} {} {} {} {} ",
                    Self::scale(n1),
                    Self::scale(-n2),
                    Self::scale(n3),
                    Self::scale(-n4),
                    Self::scale(n5),
                    Self::scale(-n6)
                )
                .unwrap();
            } else if s.starts_with("Z") {
                write!(ret, "Z ").unwrap();
                s = s[1..].trim_start();
            } else if s.starts_with("h") {
                s = s[1..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                write!(ret, "h {} ", Self::scale(n1)).unwrap();
            } else if s.starts_with("v") {
                s = s[1..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                write!(ret, "v {} ", Self::scale(-n1)).unwrap();
            } else {
                todo!("parse_path: {}", s);
            }
        }
        ret
    }

    pub fn parse_transform(ts: &str) -> String {
        let mut ret = String::new();
        let mut s = ts.trim();
        while !s.is_empty() {
            if s.starts_with("translate(") {
                s = &s[10..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                let n2 = Self::parse_get_num(&mut s);
                write!(ret, "translate({},{})", Self::scale(n1), Self::scale(-n2)).unwrap();
            } else if s.starts_with("matrix(") {
                s = s[7..].trim_start();
                let n1 = Self::parse_get_num(&mut s);
                let n2 = Self::parse_get_num(&mut s);
                let n3 = Self::parse_get_num(&mut s);
                let n4 = Self::parse_get_num(&mut s);
                let n5 = Self::parse_get_num(&mut s);
                let n6 = Self::parse_get_num(&mut s);
                write!(
                    ret,
                    "matrix({},{},{},{},{},{})",
                    n1,
                    n2,
                    n3,
                    n4,
                    Self::scale(n5),
                    Self::scale(-n6)
                )
                .unwrap();
            } else {
                todo!("parse_transform: {}", s);
            }
        }
        ret
    }
    fn parse_get_num(s: &mut &str) -> f32 {
        match s.find(|x| x == ' ' || x == ')' || x == ',') {
            Some(i) => {
                let f = s[..i].parse::<f32>().unwrap();
                *s = &s[i + 1..].trim_start();
                f
            }
            None => {
                let f = s.parse::<f32>().unwrap();
                *s = "";
                f
            }
        }
    }
    fn scale(f: f32) -> f32 {
        1.5 * f
    }
    #[inline(always)]
    pub(crate) fn close_node(&mut self) {
        ShipoutWrapper::close(self, WrapperKind::SVG)
    }
}

impl<'a, 'b> Shipout<'a, 'b, Math> {
    fn in_atom<R>(
        &mut self,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<
        (
            R,
            Vec<ShipoutNodeM>,
            Option<Vec<ShipoutNodeM>>,
            Option<Vec<ShipoutNodeM>>,
            bool,
            bool,
        ),
        Option<MathNode<Types, MathFontStyle<Types>>>,
    > {
        let (r, nodes, mut uses_color, mut uses_font) = self.do_in(|| Math, f);
        let r = r?;
        let sub = if let Some(f) = sub {
            let (r, nodes, uses_color2, uses_font2) = self.do_in(|| Math, f);
            r?;
            uses_color = uses_color || uses_color2;
            uses_font = uses_font || uses_font2;
            Some(nodes)
        } else {
            None
        };
        let sup = if let Some(f) = sup {
            let (r, nodes, uses_color2, uses_font2) = self.do_in(|| Math, f);
            r?;
            uses_color = uses_color || uses_color2;
            uses_font = uses_font || uses_font2;
            Some(nodes)
        } else {
            None
        };
        Ok((r, nodes, sub, sup, uses_color, uses_font))
    }
    fn do_sub_sup(
        &mut self,
        node: ShipoutNodeM,
        sub: Option<Vec<ShipoutNodeM>>,
        sup: Option<Vec<ShipoutNodeM>>,
        limits: bool,
    ) {
        match (sub, sup) {
            (None, None) => self.push(node),
            (Some(sub), None) => self.push(ShipoutNodeM::Sub {
                base: node.into(),
                sub,
                limits,
            }),
            (None, Some(sup)) => self.push(ShipoutNodeM::Sup {
                base: node.into(),
                sup,
                limits,
            }),
            (Some(sub), Some(sup)) => self.push(ShipoutNodeM::SubSup {
                base: node.into(),
                sub,
                sup,
                limits,
            }),
        }
    }
    #[inline(always)]
    pub(crate) fn accent<R>(
        &mut self,
        char: u8,
        style: MathFontStyle<Types>,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, children, sub, sup, uses_color, uses_font) = self.in_atom(f, sub, sup)?;
        let accent = match self.do_mathchar_i(char, &style.font) {
            Ok(cos) => Ok(cos),
            Err(glyph) => Err((glyph.to_string().into(), char, style.font.filename().into())),
        };
        self.do_sub_sup(
            ShipoutNodeM::Accent {
                children,
                accent,
                uses_color,
                uses_font,
            },
            sub,
            sup,
            false,
        );
        Ok(r)
    }
    #[inline(always)]
    pub(crate) fn inner<R>(
        &mut self,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        self.with_class(MathClass::Ord, false, f, sub, sup)
    }
    #[inline(always)]
    pub(crate) fn middle(
        &mut self,
        char: u8,
        style: MathFontStyle<Types>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>> {
        let middle = match self.do_mathchar_i(char, &style.font) {
            Ok(cos) => Ok(cos),
            Err(glyph) => Err((glyph.to_string().into(), char, style.font.filename().into())),
        };
        let sub = if let Some(f) = sub {
            let (r, nodes, uses_color2, uses_font2) = self.do_in(|| Math, f);
            r?;
            Some(nodes)
        } else {
            None
        };
        let sup = if let Some(f) = sup {
            let (r, nodes, uses_color2, uses_font2) = self.do_in(|| Math, f);
            r?;
            Some(nodes)
        } else {
            None
        };
        self.do_sub_sup(
            ShipoutNodeM::Middle(middle),
            sub,
            sup,
            style.style == MathStyleType::Display,
        );
        Ok(())
    }
    #[inline(always)]
    pub(crate) fn overline<R>(
        &mut self,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, children, sub, sup, uses_color, uses_font) = self.in_atom(f, sub, sup)?;
        self.do_sub_sup(
            ShipoutNodeM::Overline {
                children,
                uses_color,
                uses_font,
            },
            sub,
            sup,
            false,
        );
        Ok(r)
    }

    #[inline(always)]
    pub(crate) fn underline<R>(
        &mut self,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, children, sub, sup, uses_color, uses_font) = self.in_atom(f, sub, sup)?;
        self.do_sub_sup(
            ShipoutNodeM::Underline {
                children,
                uses_color,
                uses_font,
            },
            sub,
            sup,
            false,
        );
        Ok(r)
    }
    #[inline(always)]
    pub(crate) fn over(
        &mut self,
        start: SRef,
        end: SRef,
        left: Option<(u8, MathFontStyle<Types>)>,
        right: Option<(u8, MathFontStyle<Types>)>,
        sep: Option<Dim32>,
        topf: impl FnOnce(&mut Shipout<Math>) -> Result<(), ()>,
        botf: impl FnOnce(&mut Shipout<Math>) -> Result<(), ()>,
    ) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, top, mut uses_color, mut uses_font) = self.do_in(|| Math, topf);
        r.map_err(|_| None)?;
        let (r, bottom, uses_color2, uses_font2) = self.do_in(|| Math, botf);
        r.map_err(|_| None)?;
        uses_color = uses_color || uses_color2;
        uses_font = uses_font || uses_font2;
        let left = left.map(|(char, fs)| match self.do_mathchar_i(char, &fs.font) {
            Ok(cos) => Ok(cos),
            Err(glyph) => Err((glyph.to_string().into(), char, fs.font.filename().into())),
        });
        let right = right.map(|(char, fs)| match self.do_mathchar_i(char, &fs.font) {
            Ok(cos) => Ok(cos),
            Err(glyph) => Err((glyph.to_string().into(), char, fs.font.filename().into())),
        });
        let sref = SourceRef::new(start, end, self.engine);
        self.push(ShipoutNodeM::Over {
            top,
            bottom,
            sref,
            left,
            right,
            sep: sep.map(|d| d.0),
            uses_color,
            uses_font,
        });
        Ok(())
    }
    #[inline(always)]
    pub(crate) fn left_right<R>(
        &mut self,
        start: SRef,
        end: SRef,
        left: Option<(u8, MathFontStyle<Types>)>,
        right: Option<(u8, MathFontStyle<Types>)>,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, children, sub, sup, uses_color, uses_font) = self.in_atom(f, sub, sup)?;
        let left = left.map(|(char, fs)| match self.do_mathchar_i(char, &fs.font) {
            Ok(cos) => Ok(cos),
            Err(glyph) => Err((glyph.to_string().into(), char, fs.font.filename().into())),
        });
        let right = right.map(|(char, fs)| match self.do_mathchar_i(char, &fs.font) {
            Ok(cos) => Ok(cos),
            Err(glyph) => Err((glyph.to_string().into(), char, fs.font.filename().into())),
        });
        let sref = SourceRef::new(start, end, self.engine);
        self.do_sub_sup(
            ShipoutNodeM::LeftRight {
                children,
                sref,
                left,
                right,
                uses_color,
                uses_font,
            },
            sub,
            sup,
            false,
        );
        Ok(r)
    }
    #[inline(always)]
    pub(crate) fn radical<R>(
        &mut self,
        char: u8,
        style: MathFontStyle<Types>,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, children, sub, sup, uses_color, uses_font) = self.in_atom(f, sub, sup)?;
        // TODO char?
        self.do_sub_sup(
            ShipoutNodeM::Radical {
                children,
                uses_color,
                uses_font,
            },
            sub,
            sup,
            false,
        );
        Ok(r)
    }

    pub(crate) fn with_class<R>(
        &mut self,
        cls: MathClass,
        limits: bool,
        f: impl FnOnce(&mut Shipout<Math>) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, nodes, sub, sup, uses_color, uses_font) = self.in_atom(f, sub, sup)?;
        match (sub, sup) {
            (None, None) => match self.nodes.last_mut() {
                Some(ShipoutNodeM::WithClass {
                    class,
                    children,
                    uses_font: fnt,
                    uses_color: clr,
                }) if *class == cls => {
                    *fnt = *fnt || uses_font;
                    *clr = *clr || uses_color;
                    children.extend(nodes.into_iter())
                }
                _ => self.push(ShipoutNodeM::WithClass {
                    class: cls,
                    children: nodes,
                    uses_color,
                    uses_font,
                }),
            },
            (sub, sup) => self.do_sub_sup(
                ShipoutNodeM::WithClass {
                    class: cls,
                    children: nodes,
                    uses_color,
                    uses_font,
                },
                sub,
                sup,
                limits,
            ),
        }
        Ok(r)
    }

    pub(crate) fn vcenter<R>(
        &mut self,
        start: SRef,
        end: SRef,
        width: i32,
        f: impl FnOnce(&mut Shipout<V>) -> Result<R, Option<VNode<Types>>>,
        sub: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
        sup: Option<
            impl FnOnce(&mut Shipout<Math>) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>>,
        >,
    ) -> Result<R, Option<MathNode<Types, MathFontStyle<Types>>>> {
        let (r, nodes, mut uses_color, mut uses_font) = self.do_in(|| V, f);
        let r = r.map_err(|_| None)?;
        let sub = if let Some(f) = sub {
            let (r, nodes, uses_color2, uses_font2) = self.do_in(|| Math, f);
            r?;
            uses_color = uses_color || uses_color2;
            uses_font = uses_font || uses_font2;
            Some(nodes)
        } else {
            None
        };
        let sup = if let Some(f) = sup {
            let (r, nodes, uses_color2, uses_font2) = self.do_in(|| Math, f);
            r?;
            uses_color = uses_color || uses_color2;
            uses_font = uses_font || uses_font2;
            Some(nodes)
        } else {
            None
        };
        let sref = SourceRef::new(start, end, self.engine);
        self.do_sub_sup(
            ShipoutNodeM::VCenter {
                sref,
                width,
                children: nodes,
                uses_color,
                uses_font,
            },
            sub,
            sup,
            false,
        );
        Ok(r)
    }
    pub(crate) fn do_mathchar_i(&mut self, char: u8, font: &Font) -> Result<ModifiedGlyph, String> {
        let fs = &mut self.engine.fontsystem.glyphmaps;
        let fontname = font.filename();
        let data = match self.top_state.font_data.entry(fontname.into()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(FontData::new(fontname, fs)),
        };
        let glyphtable = fs.get_glyphlist(fontname);
        let glyph = glyphtable.get(char);
        if !glyph.is_defined() {
            data.missing_glyph(glyph.name(), char);
            return Err(glyph.name().to_string());
        }
        let modifiers = data.modifiers;
        Ok(ModifiedGlyph { modifiers, glyph })
        /*let cos: CharOrStr = if let Some(m) = modifiers {
            glyph.to_string().apply(m).to_string().into()
        } else {
            glyph.to_string().into()
        };
        Ok(cos)*/
    }
    pub(crate) fn do_mathchar(&mut self, char: u8, font: Font, cramped: bool, display: bool) {
        match self.do_mathchar_i(char, &font) {
            Ok(cos) => self.push(ShipoutNodeM::Glyph {
                char: cos,
                idx: char,
                cramped,
                font,
                display,
            }),
            Err(name) => self.push(ShipoutNodeM::MissingGlyph {
                font_name: font.filename().into(),
                char,
                name: name.into(),
            }),
        }
    }
}

impl Default for ShipoutState {
    fn default() -> Self {
        Self {
            output: Vec::new(),
            nullfont: None,
            top_font: None,
            top_width: None,
            page_width: None,
            //wrappers: Vec::new(),
            wrapper: ShipoutWrapper::None,
            previous: Vec::new(),
            font_data: HMap::default(),
            /*
            output:Vec::new(),
            nodes:Vec::new(),
            fonts:Vec::new(),
            widths:Vec::new(),
            colors:vec!(PDFColor::black()),
            fontlinks:HSet::default(),
            modes:vec!(ShipoutMode::Top),
            lineskip:Vec::new(),
            in_content:false,
            nullfont:None,
            missing_fonts:HSet::default(),
            missing_glyphs:HSet::default() */
        }
    }
}
impl ShipoutState {
    /*
    #[inline]
    pub fn mode(&self) -> ShipoutMode {
        *self.modes.last().unwrap()
    }
    pub fn do_in_and<F1:FnOnce(&mut Self) -> Res<()>>(&mut self, node:HTMLNode,mode:Option<ShipoutMode>, cont:F1) -> Res<HTMLNode> {
        let disc = node.tag.clone();
        self.nodes.push(node);
        if let Some(mode) = mode {
            self.modes.push(mode);
        }
        cont(self)?;
        let node = annotations::close_all(self.mode(),&mut self.nodes,|t| t == &disc);
        if let Some(_) = mode {
            self.modes.pop();
        }
        if node.tag == disc {
            Ok(node)
        } else {
            todo!()
        }
    }

    pub fn do_in<F1:FnOnce(&mut Self) -> Res<()>>(&mut self, node:HTMLNode,mode:Option<ShipoutMode>, cont:F1) -> Res<()> {
        let r = self.do_in_and(node, mode,cont)?;
        self.push(r);Ok(())
    }

    pub fn push_child(&mut self, child:HTMLChild) {
        let mode = self.mode();
        match self.nodes.last_mut() {
            Some(parent) => parent.push_child(mode,child),
            None => self.output.push(child),
        }
    }
    pub fn push_space(&mut self) {
        let mode = self.mode();
        match self.nodes.last_mut() {
            Some(parent) => parent.push_space(mode),
            None => unreachable!()
        }
    }

    pub fn push_comment<D:std::fmt::Display>(&mut self, d:D) {
        match self.nodes.last_mut() {
            Some(parent) => parent.push_comment(d),
            None => self.output.push(HTMLChild::Text(d.to_string()))
        }
    }

    pub fn push(&mut self, node:HTMLNode) {
        let mode = self.mode();
        match self.nodes.last_mut() {
            Some(parent) => parent.push_open_node(mode,node),
            None => node.close(mode,&mut self.output),
        }
    }

    pub fn push_glyph(&mut self,glyph:Glyph) {
        let mode = self.mode();
        self.nodes.last_mut().unwrap().push_glyph(mode,glyph)
    }

     */
    pub(crate) fn split_state<R, F: FnOnce(&mut Shipout<Top>) -> R>(engine: Refs, f: F) -> R {
        let mut state = std::mem::take(&mut engine.aux.extension.state);
        let output = std::mem::take(&mut state.output);
        //let wrappers = std::mem::take(&mut state.wrappers);
        let wrapper = std::mem::take(&mut state.wrapper);
        let previous = std::mem::take(&mut state.previous);
        if state.nullfont.is_none() {
            state.nullfont = Some(engine.fontsystem.null())
        }
        if state.top_font.is_none() {
            let font = engine.state.get_current_font().clone();
            match state.font_data.entry(font.filename().into()) {
                Entry::Vacant(e) => {
                    e.insert(FontData::new(
                        font.filename(),
                        &mut engine.fontsystem.glyphmaps,
                    ));
                }
                _ => (),
            }
            state.top_font = Some(font)
        }
        if state.top_width.is_none() {
            state.top_width = Some(engine.state.get_primitive_dim(PRIMITIVES.hsize).0);
        }
        if state.page_width.is_none() {
            state.page_width = Some(engine.state.get_primitive_dim(PRIMITIVES.pdfpagewidth).0);
        }
        let mut istate = Shipout {
            engine,
            nodes: output,
            state: Top,
            top_state: &mut state,
            previous,
            wrapper,
        };
        /*for r in wrappers.into_iter() {
            let nodes = std::mem::take(&mut istate.nodes);
            let wrapper = std::mem::replace(&mut istate.wrapper, r);
            istate
                .previous
                .push((ShipoutNodeV::into_nodes(nodes), wrapper));
        }*/
        let r = f(&mut istate);
        //let wrappers = ShipoutWrapper::close_all(&mut istate);
        //state.wrappers = wrappers;
        let wrapper = std::mem::take(&mut istate.wrapper);
        let previous = std::mem::take(&mut istate.previous);
        let nodes = std::mem::take(&mut istate.nodes);
        state.wrapper = wrapper;
        state.previous = previous;
        state.output = nodes;
        engine.aux.extension.state = state;
        r
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourceRef {
    file: Box<str>,
    start: (usize, usize),
    end: (usize, usize),
}
impl SourceRef {
    pub fn new(start: SRef, end: SRef, engine: Refs) -> Self {
        let file = engine
            .filesystem
            .ref_str(start.file)
            .to_string()
            .into_boxed_str();
        let start = (start.line, start.column);
        let end = (end.line, end.column);
        Self { file, start, end }
    }
}
impl Display for SourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}#({};{}):({};{})",
            self.file, self.start.0, self.start.1, self.end.0, self.end.1
        )
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShipoutNodes {
    V(Vec<ShipoutNodeV>),
    H(Vec<ShipoutNodeH>),
    Table(Vec<ShipoutNodeTable>),
    HRow(Vec<ShipoutNodeHRow>),
    Math(Vec<ShipoutNodeM>),
    SVG(Vec<ShipoutNodeSVG>),
}
impl ShipoutNodes {
    fn is_empty(&self) -> bool {
        match self {
            ShipoutNodes::V(v) => v.is_empty(),
            ShipoutNodes::H(v) => v.is_empty(),
            ShipoutNodes::HRow(v) => v.is_empty(),
            ShipoutNodes::Math(v) => v.is_empty(),
            ShipoutNodes::SVG(v) => v.is_empty(),
            ShipoutNodes::Table(v) => v.is_empty(),
        }
    }
    fn clone_new(&self) -> Self {
        match self {
            ShipoutNodes::V(_) => ShipoutNodes::V(Vec::new()),
            ShipoutNodes::H(_) => ShipoutNodes::H(Vec::new()),
            ShipoutNodes::HRow(_) => ShipoutNodes::HRow(Vec::new()),
            ShipoutNodes::Math(_) => ShipoutNodes::Math(Vec::new()),
            ShipoutNodes::SVG(_) => ShipoutNodes::SVG(Vec::new()),
            ShipoutNodes::Table(_) => ShipoutNodes::Table(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShipoutNodeV {
    // "<div class=\"rustex-vkern\" style=\"margin-bottom:{};\"></div>",dim_to_string(kn))
    KernSkip(Margin),
    HRule {
        width: Option<Dim32>,
        height: Option<Dim32>,
        depth: Option<Dim32>,
    },
    Paragraph {
        children: Vec<ShipoutNodeH>,
        uses_color: bool,
        uses_font: bool,
        alignment: Alignment,
        parskip: Margin,
        line_skip: LineSkip,
        left_skip: Margin,
        right_skip: Margin,
        width: i32,
        sref: SourceRef,
    },
    HAlign {
        children: Vec<ShipoutNodeTable>,
        uses_color: bool,
        uses_font: bool,
        num_cols: u8,
        line_skip: LineSkip,
    },
    Common(Common<Self>),
}
impl sealed::Sealed for ShipoutNodeV {}
impl ShipoutNodeT for ShipoutNodeV {
    fn has_ink(&self) -> bool {
        match self {
            Self::KernSkip(_) => false,
            Self::Common(c) => c.has_ink(),
            _ => true,
        }
    }
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes {
        ShipoutNodes::V(v)
    }
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>> {
        match nodes {
            ShipoutNodes::V(v) => Some(v),
            _ => None,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            ShipoutNodeV::KernSkip(..) => false,
            ShipoutNodeV::HRule { .. } => true,
            ShipoutNodeV::Paragraph { uses_color, .. } => *uses_color,
            ShipoutNodeV::HAlign { uses_color, .. } => *uses_color,
            ShipoutNodeV::Common(c) => c.uses_previous_color(),
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            ShipoutNodeV::KernSkip(..) => false,
            ShipoutNodeV::HRule { .. } => false,
            ShipoutNodeV::Paragraph { uses_font, .. } => *uses_font,
            ShipoutNodeV::HAlign { uses_font, .. } => *uses_font,
            ShipoutNodeV::Common(c) => c.uses_previous_font(),
        }
    }
}
impl From<Common<Self>> for ShipoutNodeV {
    fn from(c: Common<Self>) -> Self {
        Self::Common(c)
    }
}

#[derive(Clone)]
pub(crate) enum ShipoutNodeH {
    KernSkip(Margin),
    VRule {
        width: Option<Dim32>,
        height: Option<Dim32>,
        depth: Option<Dim32>,
    },
    LineBreak,
    Common(Common<Self>),
    Char(CharOrStr),
    MissingGlyph {
        name: Box<str>,
        char: u8,
        font_name: Box<str>,
    },
    Space,
    Indent(i32),
    Math {
        sref: SourceRef,
        display: Option<(Margin, Margin)>,
        children: Vec<ShipoutNodeM>,
        uses_color: bool,
        uses_font: bool,
    },
    Img(PDFXImage<Types>),
}
impl std::fmt::Debug for ShipoutNodeH {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ShipoutNodeH::KernSkip(kn) => write!(f, "Skip({})", kn.base),
            ShipoutNodeH::VRule {
                width,
                height,
                depth,
            } => write!(f, "VRule({:?},{:?},{:?})", width, height, depth),
            ShipoutNodeH::LineBreak => write!(f, "\\n"),
            ShipoutNodeH::Common(c) => std::fmt::Debug::fmt(c, f),
            ShipoutNodeH::Char(c) => write!(f, "{c}"),
            ShipoutNodeH::MissingGlyph { .. } => write!(f, "???"),
            ShipoutNodeH::Space => write!(f, " "),
            ShipoutNodeH::Indent(i) => write!(f, "Indent({})", i),
            ShipoutNodeH::Math { children, .. } => write!(f, "<{:?}>", children),
            ShipoutNodeH::Img(img) => write!(f, "Img({})", img.filepath.display()),
        }
    }
}
impl ShipoutNodeH {
    pub(crate) fn char(
        char: u8,
        font: Font,
        engine: Refs,
        data: &mut HMap<Box<str>, FontData>,
    ) -> Self {
        let fs = &mut engine.fontsystem.glyphmaps;
        let glyphtable = fs.get_glyphlist(font.filename());
        let glyph = glyphtable.get(char);
        let data = match data.entry(font.filename().into()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(FontData::new(font.filename(), fs)),
        };
        if !glyph.is_defined() {
            data.missing_glyph(glyph.name(), char);
            ShipoutNodeH::MissingGlyph {
                font_name: font.filename().into(),
                char,
                name: glyph.name().to_string().into(),
            }
        } else {
            let cos = glyph.to_string().into();
            ShipoutNodeH::Char(cos)
        }
    }
    pub(crate) fn accent(
        char: u8,
        accent: u8,
        font: Font,
        engine: Refs,
        data: &mut HMap<Box<str>, FontData>,
    ) -> Self {
        let fs = &mut engine.fontsystem.glyphmaps;
        let glyphtable = fs.get_glyphlist(font.filename());
        let glyph = glyphtable.get(char);
        let accentglyph = glyphtable.get(accent);
        let data = match data.entry(font.filename().into()) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => e.insert(FontData::new(font.filename(), fs)),
        };
        if !glyph.is_defined() {
            data.missing_glyph(glyph.name(), char);
            Self::MissingGlyph {
                font_name: font.filename().into(),
                char,
                name: glyph.name().to_string().into(),
            }
        } else if !accentglyph.is_defined() {
            data.missing_glyph(accentglyph.name(), accent);
            Self::MissingGlyph {
                font_name: font.filename().into(),
                char: accent,
                name: accentglyph.name().to_string().into(),
            }
        } else {
            let cos = accentglyph.as_combinator().map_or_else(
                || format!("{glyph}{accentglyph}"),
                |comb| comb.apply_glyph(&glyph),
            );

            Self::Char(cos.into())
        }
    }
}
impl sealed::Sealed for ShipoutNodeH {}
impl ShipoutNodeT for ShipoutNodeH {
    fn has_ink(&self) -> bool {
        match self {
            ShipoutNodeH::KernSkip(..) => false,
            ShipoutNodeH::LineBreak => false,
            ShipoutNodeH::Indent(_) => false,
            ShipoutNodeH::Char(..) => true,
            ShipoutNodeH::Space => true,
            ShipoutNodeH::Img(_) => true,
            ShipoutNodeH::MissingGlyph { .. } => true,
            ShipoutNodeH::VRule { .. } => true,
            ShipoutNodeH::Math { .. } => true,
            ShipoutNodeH::Common(c) => c.has_ink(),
        }
    }
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes {
        ShipoutNodes::H(v)
    }
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>> {
        match nodes {
            ShipoutNodes::H(v) => Some(v),
            _ => None,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            ShipoutNodeH::KernSkip(..) => false,
            ShipoutNodeH::LineBreak => false,
            ShipoutNodeH::Indent(_) => false,
            ShipoutNodeH::Char(..) => true,
            ShipoutNodeH::Space => false,
            ShipoutNodeH::Img(_) => false,
            ShipoutNodeH::MissingGlyph { .. } => false,
            ShipoutNodeH::VRule { .. } => true,
            ShipoutNodeH::Math { uses_color, .. } => *uses_color,
            ShipoutNodeH::Common(c) => c.uses_previous_color(),
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            ShipoutNodeH::KernSkip(..) => false,
            ShipoutNodeH::LineBreak => false,
            ShipoutNodeH::Img(_) => false,
            ShipoutNodeH::Indent(_) => false,
            ShipoutNodeH::Char(..) => true,
            ShipoutNodeH::Space => false,
            ShipoutNodeH::MissingGlyph { .. } => false,
            ShipoutNodeH::VRule { .. } => false,
            ShipoutNodeH::Math { uses_font, .. } => *uses_font,
            ShipoutNodeH::Common(c) => c.uses_previous_font(),
        }
    }
}
impl From<Common<Self>> for ShipoutNodeH {
    fn from(c: Common<Self>) -> Self {
        Self::Common(c)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShipoutNodeM {
    MissingGlyph {
        name: Box<str>,
        char: u8,
        font_name: Box<str>,
    },
    Glyph {
        char: ModifiedGlyph,
        idx: u8,
        cramped: bool,
        font: Font,
        display: bool,
    },
    Space,
    Phantom {
        width: i32,
        height: i32,
        depth: i32,
    },
    VRule {
        width: Option<Dim32>,
        height: Option<Dim32>,
        depth: Option<Dim32>,
    },
    MSkip {
        base: i32,
        mu: bool,
    },
    Common(Common<Self>),
    Img(PDFXImage<Types>),
    Over {
        sref: SourceRef,
        top: Vec<Self>,
        bottom: Vec<Self>,
        sep: Option<i32>,
        left: Option<Result<ModifiedGlyph, (Box<str>, u8, Box<str>)>>,
        right: Option<Result<ModifiedGlyph, (Box<str>, u8, Box<str>)>>,
        uses_font: bool,
        uses_color: bool,
    },
    Radical {
        children: Vec<ShipoutNodeM>,
        uses_font: bool,
        uses_color: bool,
    },
    WithClass {
        class: MathClass,
        children: Vec<Self>,
        uses_font: bool,
        uses_color: bool,
    },
    Underline {
        children: Vec<Self>,
        uses_font: bool,
        uses_color: bool,
    },
    Overline {
        children: Vec<Self>,
        uses_font: bool,
        uses_color: bool,
    },
    LeftRight {
        sref: SourceRef,
        left: Option<Result<ModifiedGlyph, (Box<str>, u8, Box<str>)>>,
        right: Option<Result<ModifiedGlyph, (Box<str>, u8, Box<str>)>>,
        children: Vec<Self>,
        uses_font: bool,
        uses_color: bool,
    },
    Middle(Result<ModifiedGlyph, (Box<str>, u8, Box<str>)>),
    Accent {
        accent: Result<ModifiedGlyph, (Box<str>, u8, Box<str>)>,
        children: Vec<Self>,
        uses_font: bool,
        uses_color: bool,
    },
    Sub {
        base: Box<Self>,
        sub: Vec<Self>,
        limits: bool,
    },
    Sup {
        base: Box<Self>,
        sup: Vec<Self>,
        limits: bool,
    },
    SubSup {
        base: Box<Self>,
        sub: Vec<Self>,
        sup: Vec<Self>,
        limits: bool,
    },
    VCenter {
        sref: SourceRef,
        width: i32,
        children: Vec<ShipoutNodeV>,
        uses_font: bool,
        uses_color: bool,
    },
}
impl sealed::Sealed for ShipoutNodeM {}
impl ShipoutNodeT for ShipoutNodeM {
    fn has_ink(&self) -> bool {
        true
    }
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes {
        ShipoutNodes::Math(v)
    }
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>> {
        match nodes {
            ShipoutNodes::Math(v) => Some(v),
            _ => None,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            ShipoutNodeM::Common(c) => c.uses_previous_color(),
            ShipoutNodeM::MissingGlyph { .. } => false,
            ShipoutNodeM::Glyph { .. } => true,
            ShipoutNodeM::Space => false,
            ShipoutNodeM::Phantom { .. } => false,
            ShipoutNodeM::VRule { .. } => true,
            ShipoutNodeM::Img(_) => false,
            ShipoutNodeM::Underline { uses_color, .. } => *uses_color,
            ShipoutNodeM::Radical { uses_color, .. } => *uses_color,
            ShipoutNodeM::Over { uses_color, .. } => *uses_color,
            ShipoutNodeM::LeftRight { uses_color, .. } => *uses_color,
            ShipoutNodeM::Middle(..) => true,
            ShipoutNodeM::Accent { uses_color, .. } => *uses_color,
            ShipoutNodeM::Overline { uses_color, .. } => *uses_color,
            ShipoutNodeM::VCenter { uses_color, .. } => *uses_color,
            ShipoutNodeM::WithClass { uses_color, .. } => *uses_color,
            ShipoutNodeM::Sub { base, .. } => base.uses_previous_color(),
            ShipoutNodeM::Sup { base, .. } => base.uses_previous_color(),
            ShipoutNodeM::SubSup { base, .. } => base.uses_previous_color(),
            ShipoutNodeM::MSkip { .. } => false,
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            ShipoutNodeM::Common(c) => c.uses_previous_font(),
            ShipoutNodeM::MissingGlyph { .. } => false,
            ShipoutNodeM::Glyph { .. } => false,
            ShipoutNodeM::Space => false,
            ShipoutNodeM::Phantom { .. } => false,
            ShipoutNodeM::VRule { .. } => false,
            ShipoutNodeM::Img(_) => false,
            ShipoutNodeM::Underline { uses_font, .. } => *uses_font,
            ShipoutNodeM::LeftRight { uses_font, .. } => *uses_font,
            ShipoutNodeM::Radical { uses_font, .. } => *uses_font,
            ShipoutNodeM::Middle(..) => true,
            ShipoutNodeM::Accent { uses_font, .. } => *uses_font,
            ShipoutNodeM::Overline { uses_font, .. } => *uses_font,
            ShipoutNodeM::Over { uses_font, .. } => *uses_font,
            ShipoutNodeM::VCenter { uses_font, .. } => *uses_font,
            ShipoutNodeM::WithClass { uses_font, .. } => *uses_font,
            ShipoutNodeM::Sub { base, .. } => base.uses_previous_font(),
            ShipoutNodeM::Sup { base, .. } => base.uses_previous_font(),
            ShipoutNodeM::SubSup { base, .. } => base.uses_previous_font(),
            ShipoutNodeM::MSkip { .. } => false,
        }
    }
}
impl From<Common<Self>> for ShipoutNodeM {
    fn from(c: Common<Self>) -> Self {
        Self::Common(c)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShipoutNodeSVG {
    Common(Common<Self>),
    SVGNode {
        tag: String,
        attrs: VecMap<&'static str, String>,
        children: Vec<Self>,
        uses_color: bool,
        uses_font: bool,
    },
}
impl ShipoutNodeSVG {
    fn svg_node(tag: String, attrs: VecMap<&'static str, String>, children: Vec<Self>) -> Self {
        let mut uses_color = false;
        let mut uses_font = false;
        for c in children.iter() {
            uses_color = uses_color || c.uses_previous_color();
            uses_font = uses_font || c.uses_previous_font();
            if uses_color && uses_font {
                break;
            }
        }
        Self::SVGNode {
            tag,
            attrs,
            children,
            uses_color,
            uses_font,
        }
    }
}
impl sealed::Sealed for ShipoutNodeSVG {}
impl ShipoutNodeT for ShipoutNodeSVG {
    fn has_ink(&self) -> bool {
        true
    }
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes {
        ShipoutNodes::SVG(v)
    }
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>> {
        match nodes {
            ShipoutNodes::SVG(v) => Some(v),
            _ => None,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            ShipoutNodeSVG::Common(c) => c.uses_previous_color(),
            ShipoutNodeSVG::SVGNode { uses_color, .. } => *uses_color,
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            ShipoutNodeSVG::Common(c) => c.uses_previous_font(),
            ShipoutNodeSVG::SVGNode { uses_font, .. } => *uses_font,
        }
    }
}
impl From<Common<Self>> for ShipoutNodeSVG {
    fn from(c: Common<Self>) -> Self {
        Self::Common(c)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShipoutNodeTable {
    Common(Common<Self>),
    Row {
        sref: SourceRef,
        children: Vec<ShipoutNodeHRow>,
        uses_color: bool,
        uses_font: bool,
        num_cols: u8,
    },
    NoAlign {
        children: Vec<ShipoutNodeV>,
        uses_color: bool,
        uses_font: bool,
    },
}

impl sealed::Sealed for ShipoutNodeTable {}
impl ShipoutNodeT for ShipoutNodeTable {
    fn has_ink(&self) -> bool {
        true
    }
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes {
        ShipoutNodes::Table(v)
    }
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>> {
        match nodes {
            ShipoutNodes::Table(v) => Some(v),
            _ => None,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            ShipoutNodeTable::Common(c) => c.uses_previous_color(),
            ShipoutNodeTable::Row { uses_color, .. } => *uses_color,
            ShipoutNodeTable::NoAlign { uses_color, .. } => *uses_color,
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            ShipoutNodeTable::Common(c) => c.uses_previous_font(),
            ShipoutNodeTable::Row { uses_font, .. } => *uses_font,
            ShipoutNodeTable::NoAlign { uses_font, .. } => *uses_font,
        }
    }
}
impl From<Common<Self>> for ShipoutNodeTable {
    fn from(c: Common<Self>) -> Self {
        Self::Common(c)
    }
}

#[derive(Clone, Debug)]
pub(crate) enum ShipoutNodeHRow {
    Common(Common<Self>),
    Cell {
        sref: SourceRef,
        spans: u8,
        children: Vec<ShipoutNodeH>,
        height: Dim32,
        uses_color: bool,
        uses_font: bool,
    },
}
impl sealed::Sealed for ShipoutNodeHRow {}
impl ShipoutNodeT for ShipoutNodeHRow {
    fn has_ink(&self) -> bool {
        true
    }
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes {
        ShipoutNodes::HRow(v)
    }
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>> {
        match nodes {
            ShipoutNodes::HRow(v) => Some(v),
            _ => None,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            ShipoutNodeHRow::Common(c) => c.uses_previous_color(),
            ShipoutNodeHRow::Cell { uses_color, .. } => *uses_color,
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            ShipoutNodeHRow::Common(c) => c.uses_previous_font(),
            ShipoutNodeHRow::Cell { uses_font, .. } => *uses_font,
        }
    }
}
impl From<Common<Self>> for ShipoutNodeHRow {
    fn from(c: Common<Self>) -> Self {
        Self::Common(c)
    }
}

pub(crate) trait ShipoutNodeT: sealed::Sealed + From<Common<Self>> {
    fn into_nodes(v: Vec<Self>) -> ShipoutNodes;
    fn get(nodes: ShipoutNodes) -> Option<Vec<Self>>;
    fn uses_previous_color(&self) -> bool;
    fn uses_previous_font(&self) -> bool;
    fn has_ink(&self) -> bool;
}

#[derive(Clone, Debug)]
pub(crate) enum Common<T: ShipoutNodeT> {
    Literal(String),
    WithColor {
        color: PDFColor,
        children: Vec<T>,
        uses_font: bool,
    },
    WithFont {
        font: Font,
        children: Vec<T>,
        uses_color: bool,
    },
    WithLink {
        href: String,
        children: Vec<T>,
        uses_color: bool,
        uses_font: bool,
    },
    WithAnnotation {
        attrs: VecMap<Cow<'static, str>, Cow<'static, str>>,
        styles: VecMap<Cow<'static, str>, Cow<'static, str>>,
        classes: VecSet<Cow<'static, str>>,
        tag: Option<String>,
        children: Vec<T>,
        uses_color: bool,
        uses_font: bool,
    },
    WithMatrix {
        scale: f32,
        rotate: f32,
        skewx: f32,
        skewy: f32,
        uses_color: bool,
        uses_font: bool,
        children: Vec<T>,
    },
    PDFDest(NumOrName),
    VBox {
        sref: SourceRef,
        info: VBoxInfo<Types>,
        children: Vec<ShipoutNodeV>,
        uses_color: bool,
        uses_font: bool,
    },
    HBox {
        preskip: Option<Margin>,
        sref: SourceRef,
        info: HBoxInfo<Types>,
        children: Vec<ShipoutNodeH>,
        uses_color: bool,
        uses_font: bool,
    },
    SVG {
        sref: SourceRef,
        minx: i32,
        maxx: i32,
        miny: i32,
        maxy: i32,
        children: Vec<ShipoutNodeSVG>,
        uses_color: bool,
        uses_font: bool,
    },
}
impl<T: ShipoutNodeT> Common<T> {
    pub fn has_ink(&self) -> bool {
        match self {
            Self::Literal(_) | Self::SVG { .. } => true,
            Self::HBox { children, .. } => children.iter().any(|e| e.has_ink()),
            Self::WithColor { children, .. }
            | Self::WithAnnotation { children, .. }
            | Self::WithFont { children, .. }
            | Self::WithLink { children, .. }
            | Self::WithMatrix { children, .. } => children.iter().any(|e| e.has_ink()),
            Self::VBox { children, .. } => children.iter().any(|e| e.has_ink()),
            Self::PDFDest(_) => false,
        }
    }
    fn with_color(c: PDFColor, nodes: Vec<T>) -> Result<Self, Vec<T>> {
        let mut uses_color = false;
        let mut uses_font = false;
        for c in nodes.iter() {
            uses_color = uses_color || c.uses_previous_color();
            uses_font = uses_font || c.uses_previous_font();
            if uses_color && uses_font {
                break;
            }
        }
        if !uses_color {
            return Err(nodes);
        }
        Ok(Common::WithColor {
            color: c,
            children: nodes,
            uses_font,
        })
    }
    fn with_font(
        engine: Refs,
        fonts: &mut HMap<Box<str>, FontData>,
        f: Font,
        nodes: Vec<T>,
    ) -> Result<Self, Vec<T>> {
        let mut uses_color = false;
        let mut uses_font = false;
        for c in nodes.iter() {
            uses_color = uses_color || c.uses_previous_color();
            uses_font = uses_font || c.uses_previous_font();
            if uses_color && uses_font {
                break;
            }
        }
        if !uses_font {
            return Err(nodes);
        }
        let name = f.filename().to_string().into_boxed_str();
        match fonts.entry(name) {
            Entry::Vacant(e) => {
                e.insert(FontData::new(
                    f.filename(),
                    &mut engine.fontsystem.glyphmaps,
                ));
            }
            _ => (),
        }
        Ok(Common::WithFont {
            font: f,
            children: nodes,
            uses_color,
        })
    }
    fn with_link(href: String, nodes: Vec<T>) -> Self {
        let mut uses_color = false;
        let mut uses_font = false;
        for c in nodes.iter() {
            uses_color = uses_color || c.uses_previous_color();
            uses_font = uses_font || c.uses_previous_font();
            if uses_color && uses_font {
                break;
            }
        }
        Common::WithLink {
            href,
            children: nodes,
            uses_color,
            uses_font,
        }
    }
    fn with_matrix(scale: f32, rotate: f32, skewx: f32, skewy: f32, nodes: Vec<T>) -> Self {
        let mut uses_color = false;
        let mut uses_font = false;
        for c in nodes.iter() {
            uses_color = uses_color || c.uses_previous_color();
            uses_font = uses_font || c.uses_previous_font();
            if uses_color && uses_font {
                break;
            }
        }
        Common::WithMatrix {
            scale,
            rotate,
            skewx,
            skewy,
            uses_color,
            uses_font,
            children: nodes,
        }
    }
    fn with_annotation(
        attrs: VecMap<Cow<'static, str>, Cow<'static, str>>,
        styles: VecMap<Cow<'static, str>, Cow<'static, str>>,
        classes: VecSet<Cow<'static, str>>,
        tag: Option<String>,
        nodes: Vec<T>,
    ) -> Self {
        let mut uses_color = false;
        let mut uses_font = false;
        for c in nodes.iter() {
            uses_color = uses_color || c.uses_previous_color();
            uses_font = uses_font || c.uses_previous_font();
            if uses_color && uses_font {
                break;
            }
        }
        Common::WithAnnotation {
            children: nodes,
            attrs,
            styles,
            classes,
            tag,
            uses_color,
            uses_font,
        }
    }
    fn uses_previous_color(&self) -> bool {
        match self {
            Common::WithColor { .. } => false,
            Common::Literal(..) => true,
            Common::WithFont { uses_color, .. } => *uses_color,
            Common::WithLink { uses_color, .. } => *uses_color,
            Common::WithAnnotation { uses_color, .. } => *uses_color,
            Common::PDFDest(..) => false,
            Common::VBox { uses_color, .. } => *uses_color,
            Common::HBox { uses_color, .. } => *uses_color,
            Common::SVG { uses_color, .. } => *uses_color,
            Common::WithMatrix { uses_color, .. } => *uses_color,
        }
    }
    fn uses_previous_font(&self) -> bool {
        match self {
            Common::WithColor { uses_font, .. } => *uses_font,
            Common::Literal(..) => true,
            Common::WithFont { .. } => false,
            Common::WithLink { uses_font, .. } => *uses_font,
            Common::WithAnnotation { uses_font, .. } => *uses_font,
            Common::PDFDest(..) => false,
            Common::VBox { uses_font, .. } => *uses_font,
            Common::HBox { uses_font, .. } => *uses_font,
            Common::SVG { uses_font, .. } => *uses_font,
            Common::WithMatrix { uses_font, .. } => *uses_font,
        }
    }
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub(crate) enum Alignment {
    L,
    R,
    C,
    S,
}
impl Alignment {
    pub fn from(skip1: Skip<Dim32>, skip2: Skip<Dim32>) -> Self {
        use Alignment::*;
        match (skip1.stretch, skip2.stretch) {
            (None | Some(StretchShrink::Dim(_)), None | Some(StretchShrink::Dim(_))) => S,
            (Some(StretchShrink::Fil(_)), Some(StretchShrink::Fil(_))) => C,
            (Some(StretchShrink::Fill(_)), Some(StretchShrink::Fill(_))) => C,
            (Some(StretchShrink::Filll(_)), Some(StretchShrink::Filll(_))) => C,
            (Some(StretchShrink::Filll(_)), _) => R,
            (_, Some(StretchShrink::Filll(_))) => L,
            (Some(StretchShrink::Fill(_)), _) => R,
            (_, Some(StretchShrink::Fill(_))) => L,
            (Some(StretchShrink::Fil(_)), _) => R,
            (_, Some(StretchShrink::Fil(_))) => L,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ModifiedGlyph {
    pub modifiers: Option<ModifierSeq>,
    pub glyph: Glyph,
}
impl ModifiedGlyph {
    #[inline]
    pub fn as_combinator(&self) -> Option<tex_glyphs::Combinator> {
        self.glyph.as_combinator()
    }
}
// TODO this can be optimized (no String allocation!)
impl std::fmt::Display for ModifiedGlyph {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use tex_glyphs::fontstyles::FontModifiable;
        if let Some(m) = self.modifiers {
            self.glyph.to_string().apply(m).fmt(f)
        } else {
            self.glyph.fmt(f)
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum CharOrStr {
    Char(char),
    Str(Box<str>),
}
impl Display for CharOrStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CharOrStr::Char(c) => f.write_char(*c),
            CharOrStr::Str(s) => f.write_str(s),
        }
    }
}
impl From<String> for CharOrStr {
    fn from(s: String) -> Self {
        if s.chars().count() == 1 {
            CharOrStr::Char(s.chars().next().unwrap())
        } else {
            CharOrStr::Str(s.into_boxed_str())
        }
    }
}
impl From<&ModifiedGlyph> for CharOrStr {
    // TODO optimize
    fn from(value: &ModifiedGlyph) -> Self {
        value.to_string().into()
    }
}

#[derive(Debug, Clone)]
pub struct FontData {
    pub web: Option<(String, String)>,
    pub missing: VecSet<(String, u8)>,
    pub modifiers: Option<ModifierSeq>,
}
impl FontData {
    fn new(name: &str, store: &mut FontStore) -> Self {
        let info = store.get_info(name);

        let web = match store.get_info(name) {
            Some(info) => match &info.weblink {
                Some((css, l)) => Some((l.to_string(), css.to_string())),
                _ => None,
            },
            None => None,
        };

        FontData {
            web,
            missing: VecSet::default(),
            modifiers: info.map(|i| i.styles),
        }
    }
    fn missing_glyph(&mut self, name: GlyphName, char: u8) {
        self.missing.insert((name.to_string(), char));
        /*
            let s = match state.mode() {
            ShipoutMode::H{..} | ShipoutMode::Par => "span",
            ShipoutMode::Math => "mtext",
            _ => return
        };
        state.missing_glyphs.insert((name.to_string(),char,font.filename().to_string()));
        state.push_child(HTMLChild::Comment(
            format!("<{} class=\"rustex-missing\" title=\"Missing Glyph: {} (pos. {}) in font {}\"></{}>",s,name,char,font.filename(),s)
        ));
         */
    }
}
