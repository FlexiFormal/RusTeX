use crate::engine::nodes::{LineSkip, RusTeXNode};
use crate::engine::state::RusTeXState;
use crate::engine::{Font, Refs, Res, Types};
use tex_engine::commands::primitives::PRIMITIVES;
use tex_engine::commands::{PrimitiveCommand, TeXCommand};
use tex_engine::engine::filesystem::{File, SourceReference};
use tex_engine::engine::mouth::Mouth;
use tex_engine::engine::state::{GroupType, State};
use tex_engine::engine::stomach::methods::{
    ParLine, ParLineSpec, SplitResult, insert_afterassignment, split_paragraph_roughly,
};
use tex_engine::engine::stomach::{Stomach, StomachData};
use tex_engine::engine::{EngineAux, EngineReferences, EngineTypes};
use tex_engine::prelude::*;
use tex_engine::tex::nodes::NodeList;
use tex_engine::tex::nodes::NodeTrait;
use tex_engine::tex::nodes::boxes::{BoxType, ToOrSpread};
use tex_engine::tex::nodes::horizontal::{HNode, HorizontalNodeListType};
use tex_engine::tex::nodes::math::{MathAtom, MathNode, MathNucleus};
use tex_engine::tex::nodes::vertical::{VNode, VerticalNodeListType};
use tex_engine::tex::numerics::Dim32;
use tex_engine::tex::numerics::Skip;
use tex_engine::tex::numerics::TeXDimen;
use tex_engine::tex::tokens::CompactToken;

pub struct RusTeXStomach {
    afterassignment: Option<CompactToken>,
    data: StomachData<Types>,
    prevent_shipout: bool,
    pub continuous: bool,
}
impl Stomach<Types> for RusTeXStomach {
    fn new(_aux: &mut EngineAux<Types>, _state: &mut RusTeXState) -> Self {
        Self {
            afterassignment: None,
            data: StomachData::default(),
            prevent_shipout: false,
            continuous: false,
        }
    }

    fn afterassignment(&mut self) -> &mut Option<CompactToken> {
        &mut self.afterassignment
    }

    fn data_mut(&mut self) -> &mut StomachData<Types> {
        &mut self.data
    }

    fn split_vertical(engine: Refs, nodes: Vec<VNode<Types>>, target: Dim32) -> SplitResult<Types> {
        vsplit(engine, nodes, target)
    }

    fn assign_font(engine: Refs, _token: CompactToken, f: Font, global: bool) -> Res<()> {
        let g = engine.state.get_group_level() == 0
            || global
            || engine.aux.extension.change_markers.is_empty();
        tex_engine::add_node!(Self;engine,
                       VNode::Custom(RusTeXNode::FontChange(f.clone(),g)),
                       HNode::Custom(RusTeXNode::FontChange(f.clone(),g)),
                       MathNode::Custom(RusTeXNode::FontChange(f.clone(),g))
        );
        if !g {
            *engine.aux.extension.change_markers.last_mut().unwrap() += 1;
        }
        engine.state.set_current_font(engine.aux, f, global);
        insert_afterassignment(engine);
        Ok(())
    }

    fn close_box(engine: &mut EngineReferences<Types>, bt: BoxType) -> Res<()> {
        let markers = std::mem::take(engine.aux.extension.change_markers.last_mut().unwrap());
        for _ in 0..markers {
            tex_engine::add_node!(Self;engine,
                            VNode::Custom(RusTeXNode::FontChangeEnd),
                            HNode::Custom(RusTeXNode::FontChangeEnd),
                           MathNode::Custom(RusTeXNode::FontChangeEnd)
            )
        }
        tex_engine::engine::stomach::methods::close_box(engine, bt)
    }

    fn split_paragraph(
        engine: Refs,
        specs: Vec<ParLineSpec<Types>>,
        children: Vec<HNode<Types>>,
        sourceref: SourceReference<<<Types as EngineTypes>::File as File>::SourceRefID>,
    ) -> Res<()> {
        if children.is_empty() {
            return Ok(());
        }
        let ret = split_paragraph_roughly(engine, specs.clone(), children, sourceref.clone());
        engine.stomach.prevent_shipout = true;
        Self::add_node_v(
            engine,
            VNode::Custom(RusTeXNode::ParagraphBegin {
                specs,
                start: sourceref,
                end: engine.mouth.current_sourceref(),
                lineskip: LineSkip::get(engine.state),
                parskip: engine.state.get_primitive_skip(PRIMITIVES.parskip),
            }),
        )?;
        let mut redo = vec![];
        for line in ret {
            match line {
                ParLine::Adjust(n) => redo.push(n),
                ParLine::Line(bx) => Self::add_node_v(engine, VNode::Box(bx))?,
            }
        }
        engine.stomach.prevent_shipout = false;
        Self::add_node_v(engine, VNode::Custom(RusTeXNode::ParagraphEnd))?;
        for r in redo.into_iter() {
            Self::add_node_v(engine, r)?;
        }
        Ok(())
    }
    /*
    fn add_node_v(engine: &mut EngineReferences<Types>, node: VNode<Types>) {
        if engine.stomach.data.in_output
        match node {
            VNode::Custom(RusTeXNode::PageBegin | RusTeXNode::PageEnd) => (),
            _ => add_node_v(engine,node)
        }
    }
     */

    fn maybe_do_output(engine: &mut EngineReferences<Types>, penalty: Option<i32>) -> Res<()> {
        if engine.stomach.prevent_shipout {
            return Ok(());
        }
        let continuous = engine.stomach.continuous;
        let data = engine.stomach.data_mut();
        if !data.in_output && data.open_lists.is_empty() && !data.page.is_empty() {
            if continuous {
                //data.pagegoal = <Types as EngineTypes>::Dim::from_sp(180224000);
                //engine.state.set_primitive_dim(engine.aux,PRIMITIVES.vsize,data.pagegoal,true);
                if data.page_contains_boxes
                    && data.pagetotal > <Types as EngineTypes>::Dim::from_sp(6553600 * 5)
                {
                    do_shipout(engine, penalty.or(Some(-10000)), |_| ())?;
                    engine.stomach.data_mut().page_contains_boxes = true;
                    Ok(())
                } else if penalty.is_some() {
                    do_shipout(engine, penalty, |data| {
                        data.page
                            .push(VNode::VSkip(Skip::new(Dim32(655360), None, None)))
                    })
                } else {
                    Ok(())
                }
            } else if data.pagetotal >= data.pagegoal || penalty.is_some() {
                RusTeXStomach::do_output(engine, penalty)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    fn open_align(engine: Refs, _inner: BoxType, between: BoxType) {
        engine
            .state
            .push(engine.aux, GroupType::Align, engine.mouth.line_number());
        engine
            .stomach
            .data_mut()
            .open_lists
            .push(if between == BoxType::Vertical {
                NodeList::Vertical {
                    tp: VerticalNodeListType::HAlign,
                    children: vec![],
                }
            } else {
                NodeList::Horizontal {
                    tp: HorizontalNodeListType::VAlign,
                    children: vec![],
                }
            });
        let lineskip = LineSkip::get(engine.state);
        Self::add_node_v(engine, VNode::Custom(RusTeXNode::HAlignBegin { lineskip })).unwrap()
    }
    fn close_align(engine: &mut EngineReferences<Types>) -> Res<()> {
        Self::add_node_v(engine, VNode::Custom(RusTeXNode::HAlignEnd))?;
        match engine.state.get_group_type() {
            Some(GroupType::Align) => (),
            _ => todo!("throw error"),
        }
        match engine.stomach.data_mut().open_lists.pop() {
            Some(NodeList::Vertical {
                children,
                tp: VerticalNodeListType::HAlign,
            }) => {
                engine.state.pop(engine.aux, &mut engine.mouth);
                match engine.stomach.data_mut().open_lists.last_mut() {
                    Some(NodeList::Math { .. }) => {
                        Self::add_node_m(
                            engine,
                            MathNode::Atom(MathAtom {
                                nucleus: MathNucleus::VCenter {
                                    children: children.into(),
                                    start: engine.mouth.start_ref(),
                                    end: engine.mouth.current_sourceref(),
                                    scaled: ToOrSpread::None,
                                },
                                sup: None,
                                sub: None,
                            }),
                        );
                    }
                    _ => {
                        for c in children {
                            Self::add_node_v(engine, c)?;
                        }
                    }
                }
            }
            Some(NodeList::Horizontal {
                children,
                tp: HorizontalNodeListType::VAlign,
            }) => {
                engine.state.pop(engine.aux, &mut engine.mouth);
                for c in children {
                    Self::add_node_h(engine, c);
                }
            }
            _ => todo!("throw error"),
        };
        Ok(())
    }
}

pub fn vsplit(engine: Refs, mut nodes: Vec<VNode<Types>>, mut target: Dim32) -> SplitResult<Types> {
    let data = engine.stomach.data_mut();
    data.topmarks.clear();
    std::mem::swap(&mut data.botmarks, &mut data.topmarks);
    data.firstmarks.clear();
    data.splitfirstmarks.clear();
    data.splitbotmarks.clear();
    let mut in_par = None;
    let mut split = nodes.len();
    let iter = nodes.iter().enumerate();
    for (i, n) in iter {
        match n {
            VNode::Custom(r @ RusTeXNode::ParagraphBegin { .. }) => {
                in_par = Some(r.clone());
            }
            VNode::Custom(RusTeXNode::ParagraphEnd) => {
                in_par = None;
            }
            VNode::Mark(i, v) => {
                if !data.firstmarks.contains_key(&i) {
                    data.firstmarks.insert(*i, v.clone());
                }
                data.botmarks.insert(*i, v.clone());
            }
            VNode::Insert(_, bx) => {
                target = target - bx.iter().map(|c| c.height() + c.depth()).sum(); // - n.depth() ?
                if target < Dim32(0) {
                    split = i;
                    break;
                }
            }
            _ => {
                target = target - (n.height() + n.depth()); // - n.depth() ?
                if target < Dim32(0) {
                    split = i;
                    break;
                }
            }
        }
    }
    let mut rest = nodes.split_off(split);
    if let Some(b) = in_par {
        rest.insert(0, VNode::Custom(b));
        nodes.push(VNode::Custom(RusTeXNode::ParagraphEnd));
    }
    let split_penalty = match rest.first() {
        Some(VNode::Penalty(p)) => {
            let p = *p;
            rest.drain(..1).next();
            Some(p)
        }
        _ => None,
    };

    for n in &rest {
        match n {
            VNode::Mark(i, v) => {
                if !data.splitfirstmarks.contains_key(&i) {
                    data.splitfirstmarks.insert(*i, v.clone());
                }
                data.splitbotmarks.insert(*i, v.clone());
            }
            _ => (),
        }
    }
    SplitResult {
        first: nodes,
        rest,
        split_penalty,
    }
}

fn do_shipout<F: FnOnce(&mut StomachData<Types>)>(
    engine: &mut EngineReferences<Types>,
    penalty: Option<i32>,
    f: F,
) -> Res<()> {
    macro_rules! set_empty {
        ($id:ident) => {
            engine.state.set_command(
                &engine.aux,
                engine.aux.extension.$id,
                Some(TeXCommand::Macro(engine.aux.extension.empty.clone())),
                true,
            )
        };
    }
    set_empty!(oddhead);
    set_empty!(oddfoot);
    set_empty!(evenhead);
    set_empty!(evenfoot);
    engine.state.set_command(
        &engine.aux,
        engine.aux.extension.mkboth,
        Some(TeXCommand::Macro(engine.aux.extension.gobbletwo.clone())),
        true,
    );

    let iffalse = TeXCommand::Primitive {
        cmd: PrimitiveCommand::Conditional(tex_engine::commands::tex::iffalse::<Types>),
        name: PRIMITIVES.iffalse,
    };
    engine.state.set_command(
        &engine.aux,
        engine.aux.extension.specialpage,
        Some(iffalse),
        true,
    );
    let data = engine.stomach.data_mut();
    data.page.insert(0, VNode::Custom(RusTeXNode::PageBegin));
    f(data);
    data.page.push(VNode::Custom(RusTeXNode::PageEnd));
    RusTeXStomach::do_output(engine, penalty)
}
