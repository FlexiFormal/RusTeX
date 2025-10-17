pub(crate) mod annotations;
pub(crate) mod html;
pub(crate) mod nodes;
pub(crate) mod state;
pub(crate) mod utils;

use crate::engine::nodes::{LineSkip, RusTeXNode};
use crate::engine::{Refs, Res, SRef, Types};
use crate::shipout::state::{
    Common, HLike, Math, ModeKind, Row, SVG, Shipout, ShipoutNodeH, ShipoutNodeM, ShipoutNodeT,
    ShipoutNodeTable, ShipoutNodeV, ShipoutState, VLike,
};
use crate::shipout::utils::{HNodes, MNode, MNodes, VNodes};
use crate::utils::{Flex, Margin};
use tex_engine::engine::stomach::methods::ParLineSpec;
use tex_engine::pdflatex::nodes::{PDFDest, PDFNode};
use tex_engine::tex::nodes::NodeTrait;
use tex_engine::tex::nodes::boxes::{HBoxInfo, TeXBox, ToOrSpread, VBoxInfo};
use tex_engine::tex::nodes::horizontal::HNode;
use tex_engine::tex::nodes::math::{
    MathAtom, MathFontStyle, MathGroup, MathKernel, MathNode, MathNucleus, MathStyle, MathStyleType,
};
use tex_engine::tex::nodes::vertical::VNode;
use tex_engine::tex::numerics::{Dim32, Skip};
use tex_engine::utils::errors::TeXError;
/*
pub(crate) fn make_page<F:FnOnce(Refs,&mut ShipoutState) -> Res<()>>(engine:Refs,state:&mut ShipoutState,f:F) -> Res<HTMLNode> {
    let mut page = state.do_in_and(HTMLNode::page(),None,|state| {
        f(engine,state)
    })?;
    let page_width = engine.state.get_primitive_dim(PRIMITIVES.pdfpagewidth);
    let text_width = engine.state.get_primitive_dim(PRIMITIVES.hsize);
    let top_font = state.fonts.first().unwrap();
    page.font = Some((top_font.clone(),true));
    page.styles.insert("--rustex-text-width".into(),dim_to_num(text_width.0).into());
    page.styles.insert("--rustex-page-width".into(),dim_to_num(page_width.0).into());
    page.styles.insert("line-height".into(),"1.2".into()); // TODO
    Ok(page)
}

 */

pub fn shipout(engine: Refs, n: VNode<Types>) -> Res<()> {
    //println!("Here: {}\n\n-------------------------------------------\n\n",n.display());
    match n {
        VNode::Box(TeXBox::V { children, .. }) => {
            let children = get_page_inner(children.into_vec());
            /*println!("--------------------------------------------");
            for c in &children {
                println!("{}",c.display());
            }*/
            ShipoutState::split_state(engine, |state| state.do_vlist(&mut children.into()))
                .map_err(|e| TeXError::General(format!("Not allowed in V-Mode: {e:?}")))?;
            /*
                        println!("--------------------------------------------");
                        println!("{:?}",engine.aux.extension.state.output);
                        println!("--------------------------------------------");
            */
        }
        _ => unreachable!(),
    }
    Ok(())
}

impl<Mode: VLike> Shipout<'_, '_, Mode> {
    fn do_vlist(&mut self, children: &mut VNodes) -> Result<(), Option<VNode<Types>>> {
        //let mut empty: bool = true;
        while let Some(c) = children.next() {
            match c {
                VNode::Custom(RusTeXNode::PDFNode(
                    PDFNode::PDFOutline(_)
                    | PDFNode::PDFPageAttr(_)
                    | PDFNode::PDFPagesAttr(_)
                    | PDFNode::PDFCatalog(_)
                    | PDFNode::PDFSave
                    | PDFNode::PDFAnnot(_)
                    | PDFNode::PDFLiteral(_)
                    | PDFNode::XForm(_)
                    | PDFNode::Obj(_),
                ))
                | VNode::Penalty(_)
                | VNode::Mark(..)
                | VNode::Custom(
                    RusTeXNode::PageBegin | RusTeXNode::PageEnd | RusTeXNode::HAlignEnd,
                ) => (),
                VNode::Custom(RusTeXNode::PGFEscape(bx)) => children.prefix(vec![VNode::Box(bx)]), // TODO?
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(act))) => self.do_color(act),
                VNode::Custom(RusTeXNode::FontChange(font, global)) => self.open_font(font, global),
                VNode::Custom(RusTeXNode::FontChangeEnd) => self.close_font(),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFStartLink(link))) => {
                    self.open_link(link)
                }
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFEndLink)) => self.close_link(),
                VNode::Custom(RusTeXNode::AnnotBegin {
                    start,
                    attrs,
                    styles,
                    classes,
                    tag,
                }) => self.open_annot(
                    start,
                    attrs.into(),
                    styles.into(),
                    classes.into_iter().map(String::into).collect(),
                    tag,
                ),
                VNode::Custom(RusTeXNode::AnnotEnd(end)) => self.close_annot(end),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFMatrix {
                    scale,
                    rotate,
                    skewx,
                    skewy,
                })) => self.open_matrix(scale, rotate, skewx, skewy),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFRestore)) => self.close_matrix(),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFDest(PDFDest { id, .. }))) => {
                    self.push(Common::PDFDest(id).into())
                }
                VNode::Whatsit(wi) => wi.call(self.engine).map_err(|_| None)?,
                VNode::Custom(RusTeXNode::PGFGBegin { .. } | RusTeXNode::PGFGEnd) => (), // TODO?
                VNode::Custom(RusTeXNode::PGFSvg {
                    bx,
                    minx,
                    miny,
                    maxx,
                    maxy,
                }) => {
                    if let TeXBox::H {
                        children,
                        start,
                        end,
                        ..
                    } = bx
                    {
                        self.in_svg(start, end, minx.0, miny.0, maxx.0, maxy.0, |state| {
                            state.do_svglist(&mut children.into()).map_err(|_| None)
                        })
                        .map_err(|_| None)?
                    } else {
                        unreachable!()
                    }
                }
                VNode::Leaders(_) => (), // TODO?
                VNode::Custom(RusTeXNode::Literal(s)) => self.push(Common::Literal(s).into()),

                // ----------------------------------------------------------
                VNode::VKern(d) => self.skipv(d.into()),
                VNode::VSkip(s) => self.skipv(s.into()),
                VNode::VFil => self.skipv(Margin::fil()),
                VNode::VFill => self.skipv(Margin::fill()),
                VNode::Vss => self.skipv(Margin::ss()),
                VNode::VFilneg => self.skipv(Margin {
                    base: 0,
                    stretch: Flex::Fil(-1),
                    shrink: Flex::Fixed(0),
                }),
                VNode::Custom(RusTeXNode::ParagraphBegin {
                    specs,
                    start,
                    end,
                    lineskip,
                    parskip,
                }) => self.do_par(children, specs, start, end, lineskip, parskip)?,
                // empty = false
                VNode::Custom(RusTeXNode::HAlignBegin { lineskip }) => {
                    self.do_halign(children, lineskip)?
                }
                // empty = false
                // ----------------------------------------------------------
                /*VNode::Box(TeXBox::H {info:HBoxInfo::HAlignRow,children,start,end,..}) => {
                    let mut redos = Vec::new();
                    while let Some(c) = self.nodes.pop() { match c {
                        ShipoutNodeV::HAlign {children,num_cols,uses_color,uses_font} => {
                            todo!()
                        }
                        c => redos.push(c)
                    } }
                    if !redos.is_empty() {
                        todo!()
                    }
                }*/
                VNode::Box(tb) => {
                    let _ = tb.height();
                    let _ = tb.width();
                    let _ = tb.depth();
                    match tb {
                        TeXBox::V {
                            info:
                                VBoxInfo::VBox {
                                    scaled: ToOrSpread::None,
                                    assigned_width: None,
                                    assigned_height: None,
                                    assigned_depth: None,
                                    moved_left: None,
                                    raised: None,
                                    ..
                                },
                            children: next,
                            ..
                        } => {
                            children.prefix(next.into());
                        }
                        TeXBox::V {
                            info,
                            children,
                            start,
                            end,
                        } if matches!(info, VBoxInfo::VBox { .. } | VBoxInfo::VTop { .. }) => self
                            .in_v(start, end, info, |state| {
                                state.do_vlist(&mut children.into())
                            })?,
                        TeXBox::H {
                            info,
                            children,
                            start,
                            end,
                            preskip,
                        } if matches!(info, HBoxInfo::HBox { .. }) => self
                            .in_h(start, end, info, preskip, |state| {
                                state.do_hlist(&mut children.into())
                            })
                            .map_err(|_| None)?,
                        TeXBox::H {
                            info: HBoxInfo::HAlignRow,
                            children,
                            start,
                            end,
                            ..
                        } => {
                            let mut redos = Vec::new();
                            let mut done = false;
                            while let Some(mut c) = self.nodes.pop() {
                                match &mut c {
                                    ShipoutNodeV::HAlign {
                                        children: chs,
                                        num_cols,
                                        uses_color,
                                        uses_font,
                                        ..
                                    } => {
                                        chs.push(ShipoutNodeTable::NoAlign {
                                            uses_font: redos
                                                .iter()
                                                .any(|c: &ShipoutNodeV| c.uses_previous_font()),
                                            uses_color: redos
                                                .iter()
                                                .any(|c: &ShipoutNodeV| c.uses_previous_color()),
                                            children: std::mem::take(&mut redos),
                                        });
                                        self.reopen_halign(
                                            start,
                                            end,
                                            chs,
                                            num_cols,
                                            uses_color,
                                            uses_font,
                                            |state| state.do_row(children),
                                        )
                                        .map_err(|_| None)?;
                                        done = true;
                                        break;
                                    }
                                    _ => redos.push(c),
                                }
                            }
                            if !done {
                                for r in redos.into_iter().rev() {
                                    self.push(r)
                                }
                            }
                            // TODO lost table row
                        }
                        _ => todo!("{tb:?}"),
                    }
                }
                VNode::HRule {
                    width,
                    height,
                    depth,
                    ..
                } => self.push(ShipoutNodeV::HRule {
                    width,
                    height,
                    depth,
                }),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::XImage(_))) => todo!(),

                _ => todo!("{c:?}"),
            }
        }
        /*
         */
        /*
        while let Some(c) = children.next() {
            match c {
                VNode::Custom(RusTeXNode::PGFGBegin {..}|RusTeXNode::PGFGEnd) => (), // TODO?
                VNode::Custom(RusTeXNode::PGFEscape(bx)) => children.prefix(vec!(VNode::Box(bx))),
                c => {
                    empty = false;
                    do_v(engine,state,c)?;
                }

                VNode::VKern(kn) => state.push_comment(format!("<div class=\"rustex-vkern\" style=\"margin-bottom:{};\"></div>",dim_to_string(kn))),
                VNode::VSkip(sk) => state.push_comment(format!("<div class=\"rustex-vskip\" style=\"margin-bottom:{};\"></div>",dim_to_string(sk.base))),
                VNode::VFil => state.push_comment("<div class=\"rustex-vfil\"></div>"),
                VNode::VFill => state.push_comment("<div class=\"rustex-vfill\"></div>"),
                VNode::Vss => state.push_comment("<div class=\"rustex-vss\"></div>"),
                VNode::VFilneg => (),
                VNode::Custom(RusTeXNode::ParagraphBegin{specs,start,end,lineskip,parskip}) => {
                    let parskip = if empty {None} else {Some(parskip)};
                    empty = false;
                    nodes::do_paragraph(engine,state,children,specs,start,end,lineskip,parskip)?;
                }
                VNode::Custom(RusTeXNode::HAlignBegin) => {
                    empty = false;
                    nodes::do_halign(engine,state,children)?;
                }
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFOutline(_) | PDFNode::PDFPageAttr(_) | PDFNode::PDFPagesAttr(_) |
                PDFNode::PDFCatalog(_)| PDFNode::PDFSave| PDFNode::PDFAnnot(_) | PDFNode::PDFLiteral(_) | PDFNode::XForm(_) | PDFNode::Obj(_))) | VNode::Penalty(_) => (),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(act))) => annotations::do_color(state,engine,act),
                VNode::Custom(RusTeXNode::FontChange(font,true)) => {
                    /*if state.in_content {
                        todo!()
                    }*/
                    *state.fonts.last_mut().unwrap() = font;
                }
                VNode::Custom(RusTeXNode::FontChange(font,false)) => annotations::do_font(state,&engine.fontsystem.glyphmaps,font),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFStartLink(link))) =>
                    annotations::do_link(link,state),
                VNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFEndLink)) =>
                    annotations::close_link(state),
                VNode::Custom(RusTeXNode::FontChangeEnd) => annotations::close_font(state),
                VNode::Custom(RusTeXNode::AnnotBegin {start,attrs,styles,tag}) => annotations::do_annot(state,start,tag,attrs,styles),
                VNode::Custom(RusTeXNode::AnnotEnd(end)) => annotations::close_annot(state,end),
            }
        }

         */
        Ok(())
    }

    fn do_par(
        &mut self,
        children: &mut VNodes,
        specs: Vec<ParLineSpec<Types>>,
        start: SRef,
        end: SRef,
        lineskip: LineSkip,
        parskip: Skip<Dim32>,
    ) -> Result<(), Option<VNode<Types>>> {
        // hack for parlines that may have been \lastboxed
        let ret = self.in_par(specs, start, end, lineskip, parskip, |state| {
            let mut later = Vec::new();
            let mut emergency_break = false;
            let mut is_empty = true;
            while let Some(c) = children.next() {
                match c {
                    VNode::VSkip(_)
                    | VNode::VFil
                    | VNode::VFill
                    | VNode::VFilneg
                    | VNode::Vss
                    | VNode::Mark(..)
                    | VNode::VKern(_) => (),
                    VNode::Custom(RusTeXNode::ParagraphEnd) if is_empty => emergency_break = true,
                    VNode::Custom(RusTeXNode::ParagraphEnd) => return Ok(later),
                    VNode::Box(TeXBox::H {
                        info:
                            HBoxInfo::ParLine {
                                ends_with_line_break,
                                ..
                            },
                        children,
                        ..
                    }) => {
                        is_empty = false;
                        state.do_hlist(&mut children.into()).map_err(|_| None)?;
                        if ends_with_line_break {
                            state.push(ShipoutNodeH::LineBreak)
                        }
                    }
                    VNode::Box(TeXBox::H {
                        info:
                            HBoxInfo::HBox {
                                moved_left: None,
                                raised: None,
                                ..
                            },
                        children,
                        ..
                    }) if emergency_break => {
                        is_empty = false;
                        state.do_hlist(&mut children.into()).map_err(|_| None)?
                    }
                    _ => {
                        later.push(c);
                        if emergency_break {
                            return Ok(later);
                        }
                    }
                }
            }
            Ok(later)
        })?;
        children.prefix(ret);
        Ok(())
    }

    fn do_halign(
        &mut self,
        children: &mut VNodes,
        lineskip: LineSkip,
    ) -> Result<(), Option<VNode<Types>>> {
        self.in_halign(lineskip, move |state| {
            while let Some(row) = children.next() {
                match row {
                    VNode::Custom(RusTeXNode::HAlignEnd) => break,
                    VNode::Box(TeXBox::H {
                        info: HBoxInfo::HAlignRow,
                        children,
                        start,
                        end,
                        ..
                    }) => {
                        state
                            .in_row(start, end, |state| state.do_row(children))
                            .map_err(|_| None)?;
                    }
                    VNode::Box(
                        b @ TeXBox::H {
                            info: HBoxInfo::HAlignCell { .. },
                            start,
                            end,
                            ..
                        },
                    ) => {
                        let c = vec![HNode::Box(b)].into();
                        state
                            .in_row(start, end, |state| state.do_row(c))
                            .map_err(|_| None)?;
                    }
                    v => {
                        let children = vec![v];
                        match state.in_noalign(|s| s.do_vlist(&mut children.into())) {
                            Ok(()) => (),
                            Err(Some(VNode::Custom(RusTeXNode::HAlignEnd))) => break,
                            Err(Some(VNode::Box(TeXBox::H {
                                info: HBoxInfo::HAlignRow,
                                children,
                                start,
                                end,
                                ..
                            }))) => {
                                state
                                    .in_row(start, end, |state| state.do_row(children))
                                    .map_err(|_| None)?;
                            }
                            o => return o,
                        }
                    }
                }
            }
            Ok(())
        })
    }
}

impl Shipout<'_, '_, Row> {
    fn do_row(&mut self, children: Box<[HNode<Types>]>) -> Result<(), Option<HNode<Types>>> {
        for c in children {
            match c {
                HNode::Box(TeXBox::H {
                    info: HBoxInfo::HAlignCell { spans, .. },
                    children,
                    start,
                    end,
                    ..
                }) => self
                    .in_cell(
                        start,
                        end,
                        children
                            .iter()
                            .max_by_key(|e| e.height().0 + e.depth().0)
                            .map(|e| e.height() + e.depth())
                            .unwrap_or_default(),
                        spans,
                        |state| state.do_hlist(&mut children.into()),
                    )
                    .map_err(|_| None)?,
                _ => todo!(),
            }
        }
        Ok(())
    }
}

impl<Mode: HLike> Shipout<'_, '_, Mode> {
    fn do_hlist(&mut self, children: &mut HNodes) -> Result<(), Option<HNode<Types>>> {
        while let Some(c) = children.next() {
            match c {
                HNode::Custom(RusTeXNode::PDFNode(
                    PDFNode::PDFOutline(_)
                    | PDFNode::PDFPageAttr(_)
                    | PDFNode::PDFPagesAttr(_)
                    | PDFNode::PDFCatalog(_)
                    | PDFNode::PDFSave
                    | PDFNode::PDFAnnot(_)
                    | PDFNode::PDFLiteral(_)
                    | PDFNode::XForm(_)
                    | PDFNode::Obj(_),
                ))
                | HNode::Penalty(_)
                | HNode::Mark(..)
                | HNode::Custom(
                    RusTeXNode::PageBegin | RusTeXNode::PageEnd | RusTeXNode::HAlignEnd,
                ) => (),
                HNode::Custom(RusTeXNode::PGFEscape(bx)) => children.prefix(vec![HNode::Box(bx)]),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(act))) => self.do_color(act),
                HNode::Custom(RusTeXNode::FontChange(font, global)) => self.open_font(font, global),
                HNode::Custom(RusTeXNode::FontChangeEnd) => self.close_font(),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFStartLink(link))) => {
                    self.open_link(link)
                }
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFEndLink)) => self.close_link(),
                HNode::Custom(RusTeXNode::AnnotBegin {
                    start,
                    attrs,
                    styles,
                    classes,
                    tag,
                }) => self.open_annot(
                    start,
                    attrs.into(),
                    styles.into(),
                    classes.into_iter().map(String::into).collect(),
                    tag,
                ),
                HNode::Custom(RusTeXNode::AnnotEnd(end)) => self.close_annot(end),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFMatrix {
                    scale,
                    rotate,
                    skewx,
                    skewy,
                })) => self.open_matrix(scale, rotate, skewx, skewy),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFRestore)) => self.close_matrix(),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFDest(PDFDest { id, .. }))) => {
                    self.push(Common::PDFDest(id).into())
                }
                HNode::Whatsit(wi) => wi.call(self.engine).map_err(|_| None)?,
                HNode::Custom(RusTeXNode::Literal(s)) => self.push(Common::Literal(s).into()),
                HNode::Custom(RusTeXNode::PGFGBegin { .. } | RusTeXNode::PGFGEnd) => (), // TODO maybe? Only seems to happen in escape-boxes in svg
                HNode::Custom(RusTeXNode::PGFSvg {
                    bx,
                    minx,
                    miny,
                    maxx,
                    maxy,
                }) => {
                    if let TeXBox::H {
                        children,
                        start,
                        end,
                        ..
                    } = bx
                    {
                        self.in_svg(start, end, minx.0, miny.0, maxx.0, maxy.0, |state| {
                            state.do_svglist(&mut children.into()).map_err(|_| None)
                        })
                        .map_err(|_| None)?
                    } else {
                        unreachable!()
                    }
                }
                HNode::Leaders(_) => (), // TODO?

                HNode::HSkip(sk) => self.skiph(sk.into()),
                HNode::HKern(kn) => self.skiph(kn.into()),
                HNode::HFil => self.skiph(Margin::fil()),
                HNode::HFill => self.skiph(Margin::fill()),
                HNode::Hss => self.skiph(Margin::ss()),
                HNode::HFilneg => self.skiph(Margin {
                    base: 0,
                    stretch: Flex::Fil(-1),
                    shrink: Flex::Fixed(0),
                }),

                HNode::Box(bx) => {
                    let _ = bx.height();
                    let _ = bx.width();
                    let _ = bx.depth();
                    match bx {
                        TeXBox::V {
                            info,
                            children,
                            start,
                            end,
                        } if matches!(info, VBoxInfo::VBox { .. } | VBoxInfo::VTop { .. }) => self
                            .in_v(start, end, info, |state| {
                                state.do_vlist(&mut children.into())
                            })
                            .map_err(|_| None)?,
                        TeXBox::H {
                            info: HBoxInfo::ParIndent(d),
                            ..
                        } => self.indent(d.0),
                        TeXBox::H {
                            info:
                                HBoxInfo::HBox {
                                    scaled: ToOrSpread::None,
                                    assigned_width: None,
                                    assigned_height: None,
                                    assigned_depth: None,
                                    moved_left: None,
                                    raised: None,
                                    ..
                                },
                            children: next,
                            ..
                        } if Mode::kind() == ModeKind::H => {
                            children.prefix(next.into());
                        }
                        TeXBox::H {
                            info,
                            children,
                            start,
                            end,
                            preskip,
                        } if matches!(info, HBoxInfo::HBox { .. }) => self
                            .in_h(start, end, info, preskip, |state| {
                                state.do_hlist(&mut children.into())
                            })
                            .map_err(|_| None)?,
                        TeXBox::H {
                            info: mut info @ HBoxInfo::ParLine { .. },
                            children,
                            start,
                            end,
                            ..
                        } => {
                            info.to_hbox();
                            self.in_h(start, end, info, None, |state| {
                                state.do_hlist(&mut children.into())
                            })
                            .map_err(|_| None)?
                        }
                        _ => todo!("{bx:?}"),
                    }
                }
                HNode::MathGroup(MathGroup {
                    start,
                    end,
                    display,
                    children,
                    ..
                }) => self
                    .in_math(
                        start,
                        end,
                        display.map(|(a, b)| (a.into(), b.into())).into(),
                        |state| state.do_mathlist(&mut children.into()),
                    )
                    .map_err(|_| None)?,

                HNode::VRule {
                    width,
                    height,
                    depth,
                    ..
                } => self.push(ShipoutNodeH::VRule {
                    width,
                    height,
                    depth,
                }),
                HNode::Char { char, font } => {
                    let r =
                        ShipoutNodeH::char(char, font, self.engine, &mut self.top_state.font_data);
                    self.push(r)
                }
                HNode::Space => self.push(ShipoutNodeH::Space),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::XImage(img))) => {
                    self.push(ShipoutNodeH::Img(img))
                }
                HNode::Accent { accent, char, font } => {
                    let r = ShipoutNodeH::accent(
                        char,
                        accent,
                        font,
                        self.engine,
                        &mut self.top_state.font_data,
                    );
                    self.push(r)
                }
                _ => todo!("{c:?}"),
            }
        }
        /*
        while let Some(c) = children.next() {
            match c {
                HNode::HKern(kn) => {
                    if kn!= Dim32(0) {state.push_comment(format!("<div class=\"rustex-hkern\" style=\"margin-left:{};\"></div>",dim_to_string(kn)))}
                }
                HNode::Space if state.mode() == ShipoutMode::H{escape:true} =>
                    state.push_comment(format_args!("<div class=\"rustex-space-in-hbox\"></div>")),
                HNode::Space => state.push_space(),
                HNode::HSkip(sk) => {
                    state.push_comment(format!("<div class=\"rustex-hskip\" style=\"margin-left:{};\"></div>",dim_to_string(sk.base)))
                }
                HNode::HFil => state.push_comment("<div class=\"rustex-hfil\"></div>"),
                HNode::HFill => state.push_comment("<div class=\"rustex-hfill\"></div>"),
                HNode::Hss => state.push_comment("<div class=\"rustex-hss\"></div>"),
                HNode::HFilneg => (),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(act))) =>  annotations::do_color(state,engine,act),
                HNode::Custom(RusTeXNode::FontChange(font,true)) => {
                    if state.in_content {
                        //TODO ???
                    }
                    *state.fonts.last_mut().unwrap() = font;
                }
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFStartLink(link))) =>
                    annotations::do_link(link,state),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFEndLink)) =>
                    annotations::close_link(state),
                HNode::Custom(RusTeXNode::FontChange(font,false)) => annotations::do_font(state,&engine.fontsystem.glyphmaps,font),
                HNode::Custom(RusTeXNode::FontChangeEnd) => annotations::close_font(state),
                HNode::Custom(RusTeXNode::AnnotBegin {start,attrs,styles,tag}) => annotations::do_annot(state,start,tag,attrs,styles),
                HNode::Custom(RusTeXNode::AnnotEnd(end)) => annotations::close_annot(state,end),
                HNode::Custom(RusTeXNode::InvisibleBegin) => annotations::do_invisible(state),
                HNode::Custom(RusTeXNode::InvisibleEnd) => annotations::close_invisible(state),
                HNode::Custom(RusTeXNode::PGFGBegin{..}|RusTeXNode::PGFGEnd) => (),  // TODO???
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFOutline(_) | PDFNode::PDFPageAttr(_) | PDFNode::PDFPagesAttr(_) |
                PDFNode::PDFCatalog(_) | PDFNode::PDFSave | PDFNode::PDFAnnot(_) | PDFNode::PDFLiteral(_) | PDFNode::XForm(_) | PDFNode::Obj(_))) | HNode::Penalty(_) => (),
                HNode::Custom(RusTeXNode::PGFEscape(bx)) => do_h(engine,state,HNode::Box(bx))?,
                c => do_h(engine,state,c)?
            }
        }

         */
        Ok(())
    }
}

impl Shipout<'_, '_, Math> {
    fn do_mathlist(
        &mut self,
        children: &mut MNodes,
    ) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>> {
        while let Some(c) = children.next() {
            match c {
                MNode::Custom(RusTeXNode::PDFNode(
                    PDFNode::PDFOutline(_)
                    | PDFNode::PDFPageAttr(_)
                    | PDFNode::PDFPagesAttr(_)
                    | PDFNode::PDFCatalog(_)
                    | PDFNode::PDFSave
                    | PDFNode::PDFAnnot(_)
                    | PDFNode::PDFLiteral(_)
                    | PDFNode::XForm(_)
                    | PDFNode::Obj(_),
                ))
                | MNode::Penalty(_)
                | MNode::Mark(..)
                | MNode::Custom(
                    RusTeXNode::PageBegin | RusTeXNode::PageEnd | RusTeXNode::HAlignEnd,
                ) => (),
                MNode::Custom(RusTeXNode::PGFEscape(_bx)) => todo!(), // children.prefix(vec!(VNode::Box(bx))),
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(act))) => self.do_color(act),
                MNode::Custom(RusTeXNode::FontChange(font, global)) => self.open_font(font, global),
                MNode::Custom(RusTeXNode::FontChangeEnd) => self.close_font(),
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFStartLink(link))) => {
                    self.open_link(link)
                }
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFEndLink)) => self.close_link(),
                MNode::Custom(RusTeXNode::AnnotBegin {
                    start,
                    attrs,
                    styles,
                    classes,
                    tag,
                }) => self.open_annot(
                    start,
                    attrs.into(),
                    styles.into(),
                    classes.into_iter().map(String::into).collect(),
                    tag,
                ),
                MNode::Custom(RusTeXNode::AnnotEnd(end)) => self.close_annot(end),
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFMatrix {
                    scale,
                    rotate,
                    skewx,
                    skewy,
                })) => self.open_matrix(scale, rotate, skewx, skewy),
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFRestore)) => self.close_matrix(),
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFDest(PDFDest { id, .. }))) => {
                    self.push(Common::PDFDest(id).into())
                }
                MNode::Whatsit(wi) => wi.call(self.engine).map_err(|_| None)?,
                MNode::Custom(RusTeXNode::Literal(s)) => self.push(Common::Literal(s).into()),
                MNode::Custom(RusTeXNode::PDFNode(PDFNode::XImage(img))) => {
                    self.push(ShipoutNodeM::Img(img))
                }
                MNode::Custom(RusTeXNode::PGFGBegin { .. } | RusTeXNode::PGFGEnd) => todo!(),
                MNode::Space => self.push(ShipoutNodeM::Space),
                MNode::MSkip { skip, .. } => self.push(ShipoutNodeM::MSkip {
                    base: skip.base.0,
                    mu: true,
                }),
                MNode::MKern { kern, .. } => self.push(ShipoutNodeM::MSkip {
                    base: kern.0,
                    mu: true,
                }),
                MNode::HSkip(skip) => self.push(ShipoutNodeM::MSkip {
                    base: skip.base.0,
                    mu: false,
                }),
                MNode::HKern(kern) => self.push(ShipoutNodeM::MSkip {
                    base: kern.0,
                    mu: false,
                }),
                MNode::HFil | MNode::HFill | MNode::Hss | MNode::HFilneg => (), // TODO maybe?
                MNode::Choice(ls) => children.prefix(ls.0.into_vec()),
                MNode::Leaders(..) => (), // TODO?

                MNode::Atom(MathAtom { nucleus, sub, sup }) => {
                    let sub = sub.map(|inner| {
                        |state: &mut Shipout<Math>| state.do_mathlist(&mut inner.into())
                    });
                    let sup = sup.map(|inner| {
                        |state: &mut Shipout<Math>| state.do_mathlist(&mut inner.into())
                    });
                    match nucleus {
                        MathNucleus::Accent { accent, inner } => {
                            self.accent(
                                accent.0,
                                accent.1,
                                |state| state.do_mathlist(&mut inner.into()),
                                sub,
                                sup,
                            )?;
                        }
                        MathNucleus::Inner(kernel) => {
                            self.inner(|state| state.do_kernel(kernel), sub, sup)?;
                        }
                        MathNucleus::Middle(char, s) => {
                            self.middle(char, s, sub, sup)?;
                        }
                        MathNucleus::Overline(kernel) => {
                            self.overline(|state| state.do_kernel(kernel), sub, sup)?;
                        }
                        MathNucleus::Underline(kernel) => {
                            self.underline(|state| state.do_kernel(kernel), sub, sup)?;
                        }
                        MathNucleus::LeftRight {
                            start,
                            end,
                            left,
                            right,
                            children,
                        } => {
                            self.left_right(
                                start,
                                end,
                                left,
                                right,
                                |state| state.do_mathlist(&mut children.into()),
                                sub,
                                sup,
                            )?;
                        }
                        MathNucleus::Radical { rad, inner } => {
                            self.radical(
                                rad.0,
                                rad.1,
                                |state| state.do_mathlist(&mut inner.into()),
                                sub,
                                sup,
                            )?;
                        }
                        MathNucleus::Simple {
                            cls,
                            kernel,
                            limits,
                        } => {
                            self.with_class(
                                cls,
                                limits.unwrap_or(false),
                                |state| state.do_kernel(kernel),
                                sub,
                                sup,
                            )?;
                        }
                        c @ MathNucleus::VCenter { .. } => {
                            let wd = c.width().0;
                            let MathNucleus::VCenter {
                                start,
                                end,
                                children,
                                ..
                            } = c
                            else {
                                unreachable!()
                            };
                            self.vcenter(
                                start,
                                end,
                                wd,
                                |state| state.do_vlist(&mut children.into()),
                                sub,
                                sup,
                            )?;
                        }
                    }
                }
                MathNode::Over {
                    start,
                    end,
                    top,
                    bottom,
                    sep,
                    left,
                    right,
                } => {
                    self.over(
                        start,
                        end,
                        left,
                        right,
                        sep,
                        |state| state.do_mathlist(&mut top.into()).map_err(|_| ()),
                        |state| state.do_mathlist(&mut bottom.into()).map_err(|_| ()),
                    )?;
                }
                MathNode::VRule {
                    width,
                    height,
                    depth,
                    ..
                } => self.push(ShipoutNodeM::VRule {
                    width,
                    height,
                    depth,
                }),

                _ => todo!("{c:?}"),
            }
        }
        Ok(())
    }
    fn do_kernel(
        &mut self,
        kernel: MathKernel<Types, MathFontStyle<Types>>,
    ) -> Result<(), Option<MathNode<Types, MathFontStyle<Types>>>> {
        match kernel {
            MathKernel::Char { char, style } => self.do_mathchar(
                char,
                style.font,
                style.cramped,
                style.style == MathStyleType::Display,
            ),
            MathKernel::List { children, .. } => self.do_mathlist(&mut children.into())?,
            MathKernel::Empty => (),
            MathKernel::Box(bx) => {
                let _ = bx.width();
                let _ = bx.height();
                let _ = bx.depth();
                let wd = bx.assigned_width();
                let ht = bx.assigned_height();
                let dp = bx.assigned_depth();
                if bx.is_empty() {
                    match (wd, ht, dp) {
                        (Some(Dim32(0)) | None, Some(Dim32(0)) | None, Some(Dim32(0)) | None) => (),
                        _ => self.push(ShipoutNodeM::Phantom {
                            width: wd.map(|d| d.0).unwrap_or_default(),
                            height: ht.map(|d| d.0).unwrap_or_default(),
                            depth: dp.map(|d| d.0).unwrap_or_default(),
                        }),
                    }
                } else {
                    match bx {
                        TeXBox::V {
                            start,
                            end,
                            info,
                            children,
                            ..
                        } => self
                            .in_v(start, end, info, |state| {
                                state.do_vlist(&mut children.into())
                            })
                            .map_err(|_| None)?,
                        TeXBox::H {
                            start,
                            end,
                            preskip,
                            info,
                            children,
                            ..
                        } => self
                            .in_h(start, end, info, preskip, |state| {
                                state.do_hlist(&mut children.into())
                            })
                            .map_err(|_| None)?,
                    }
                }
            }
        }
        Ok(())
    }
}

impl Shipout<'_, '_, SVG> {
    fn do_svglist(&mut self, children: &mut HNodes) -> Result<(), Option<HNode<Types>>> {
        while let Some(c) = children.next() {
            match c {
                HNode::Custom(RusTeXNode::PDFNode(
                    PDFNode::PDFOutline(_)
                    | PDFNode::PDFPageAttr(_)
                    | PDFNode::PDFPagesAttr(_)
                    | PDFNode::PDFCatalog(_)
                    | PDFNode::PDFSave
                    | PDFNode::PDFAnnot(_)
                    | PDFNode::PDFLiteral(_)
                    | PDFNode::XForm(_)
                    | PDFNode::Obj(_),
                ))
                | HNode::Penalty(_)
                | HNode::Mark(..)
                | HNode::Custom(
                    RusTeXNode::PageBegin | RusTeXNode::PageEnd | RusTeXNode::HAlignEnd,
                ) => (),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(act))) => self.do_color(act),
                HNode::Custom(RusTeXNode::FontChange(font, global)) => self.open_font(font, global),
                HNode::Custom(RusTeXNode::FontChangeEnd) => self.close_font(),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFStartLink(link))) => {
                    self.open_link(link)
                }
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFEndLink)) => self.close_link(),
                HNode::Custom(RusTeXNode::AnnotBegin {
                    start,
                    attrs,
                    styles,
                    classes,
                    tag,
                }) => self.open_annot(
                    start,
                    attrs.into(),
                    styles.into(),
                    classes.into_iter().map(String::into).collect(),
                    tag,
                ),
                HNode::Custom(RusTeXNode::AnnotEnd(end)) => self.close_annot(end),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFMatrix {
                    scale,
                    rotate,
                    skewx,
                    skewy,
                })) => self.open_matrix(scale, rotate, skewx, skewy),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFRestore)) => self.close_matrix(),
                HNode::Custom(RusTeXNode::PDFNode(PDFNode::PDFDest(PDFDest { id, .. }))) => {
                    self.push(Common::PDFDest(id).into())
                }

                HNode::Whatsit(wi) => wi.call(self.engine).map_err(|_| None)?,
                HNode::Leaders(_) => (), // TODO?
                HNode::Custom(RusTeXNode::Literal(s)) => self.push(Common::Literal(s).into()),

                HNode::Custom(RusTeXNode::PGFGBegin { attrs, tag }) => self.open_node(attrs, tag),
                HNode::Custom(RusTeXNode::PGFGEnd) => self.close_node(),
                HNode::Box(TeXBox::H { children: chs, .. }) => children.prefix(chs.into_vec()),
                HNode::Space | HNode::Hss => (),
                HNode::Custom(RusTeXNode::PGFEscape(bx @ TeXBox::H { .. })) => {
                    let _ = bx.height();
                    let _ = bx.width();
                    let _ = bx.depth();
                    let TeXBox::H {
                        children,
                        start,
                        end,
                        info,
                        preskip,
                        ..
                    } = bx
                    else {
                        unreachable!()
                    };
                    self.in_h(start, end, info, preskip, |state| {
                        state.do_hlist(&mut children.into())
                    })
                    .map_err(|_| None)?
                }
                HNode::Custom(RusTeXNode::PGFEscape(bx @ TeXBox::V { .. })) => {
                    let _ = bx.height();
                    let _ = bx.width();
                    let _ = bx.depth();
                    let TeXBox::V {
                        children,
                        start,
                        end,
                        info,
                        ..
                    } = bx
                    else {
                        unreachable!()
                    };
                    self.in_v(start, end, info, |state| {
                        state.do_vlist(&mut children.into())
                    })
                    .map_err(|_| None)?
                }
                HNode::MathGroup(ref mg) => {
                    //TODO should not happen
                    let bx = HBoxInfo::new_box(ToOrSpread::None);
                    let MathGroup { start, end, .. } = mg;
                    let start = start.clone();
                    let end = end.clone();
                    let children = vec![c];
                    self.in_h(start, end, bx, None, |state| {
                        state.do_hlist(&mut children.into())
                    })
                    .map_err(|_| None)?
                }
                HNode::Char { .. } => (), // TODO?
                _ => todo!("{c:?}"),
            }
        }
        Ok(())
    }
}

fn get_page_inner(children: Vec<VNode<Types>>) -> Vec<VNode<Types>> {
    let mut ret = Vec::new();
    let mut list: VNodes = children.into();
    while let Some(c) = list.next() {
        match c {
            VNode::Box(TeXBox::V { children, .. })
                if children
                    .iter()
                    .any(|n| matches!(n, VNode::Custom(RusTeXNode::PageBegin))) =>
            {
                ret.extend(children.into_vec().into_iter().filter(|p| {
                    !matches!(
                        p,
                        VNode::Custom(RusTeXNode::PageBegin | RusTeXNode::PageEnd)
                    )
                }))
            }
            VNode::Box(TeXBox::V { children, .. }) => list.prefix(children.into_vec()),
            VNode::Box(TeXBox::H { children, .. }) if hbox_works(&children) => {
                get_page_hbox(children, &mut ret, &mut list)
            }
            VNode::VSkip(_)
            | VNode::VKern(_)
            | VNode::VFil
            | VNode::VFill
            | VNode::VFilneg
            | VNode::Vss
            | VNode::Penalty(_)
            | VNode::Mark(..) => (),
            VNode::Custom(RusTeXNode::PDFNode(
                PDFNode::PDFDest(..)
                | PDFNode::PDFCatalog(_)
                | PDFNode::PDFLiteral(_)
                | PDFNode::XForm(..)
                | PDFNode::Obj(..)
                | PDFNode::PDFOutline(_),
            )) => (),
            VNode::Custom(RusTeXNode::PageBegin) => {
                for c in list.by_ref() {
                    if let VNode::Custom(RusTeXNode::PageEnd) = c {
                        break;
                    } else {
                        ret.push(c)
                    }
                }
            }
            _ => ret.push(c),
        }
    }
    ret
}

fn get_page_hbox(children: Box<[HNode<Types>]>, ret: &mut Vec<VNode<Types>>, list: &mut VNodes) {
    for c in children.into_vec().into_iter() {
        match c {
            HNode::HSkip(_)
            | HNode::Hss
            | HNode::Space
            | HNode::HKern(_)
            | HNode::HFil
            | HNode::HFill
            | HNode::HFilneg
            | HNode::Penalty(_)
            | HNode::Mark(_, _)
            | HNode::Custom(RusTeXNode::PDFNode(
                PDFNode::PDFDest(..)
                | PDFNode::PDFCatalog(_)
                | PDFNode::PDFLiteral(_)
                | PDFNode::XForm(..)
                | PDFNode::Obj(..)
                | PDFNode::PDFOutline(_),
            )) => (),
            HNode::Custom(
                n @ (RusTeXNode::PDFNode(PDFNode::Color(_))
                | RusTeXNode::FontChange(..)
                | RusTeXNode::FontChangeEnd),
            ) => ret.push(VNode::Custom(n)),
            HNode::Box(TeXBox::H { children, .. }) if hbox_works(&children) => {
                get_page_hbox(children, ret, list)
            }
            HNode::Box(b) => ret.push(VNode::Box(b)), //list.prefix(vec![VNode::Box(b)]),
            _ => unreachable!(),
        }
    }
}

fn hbox_works(children: &[HNode<Types>]) -> bool {
    children.iter().all(|n| {
        matches!(
            n,
            HNode::HSkip(_)
                | HNode::Hss
                | HNode::Space
                | HNode::HKern(_)
                | HNode::HFil
                | HNode::HFill
                | HNode::HFilneg
                | HNode::Penalty(_)
                | HNode::Mark(_, _)
                | HNode::Custom(RusTeXNode::PDFNode(
                    PDFNode::PDFDest(..)
                        | PDFNode::PDFCatalog(_)
                        | PDFNode::PDFLiteral(_)
                        | PDFNode::XForm(..)
                        | PDFNode::Obj(..)
                        | PDFNode::PDFOutline(_)
                ))
                //| HNode::Custom(RusTeXNode::PDFNode(PDFNode::Color(_)))
                //| HNode::Custom(RusTeXNode::FontChange(..) | RusTeXNode::FontChangeEnd)
                | HNode::Box(TeXBox::H { .. })
                | HNode::Box(TeXBox::V { .. })
        )
    })
}
