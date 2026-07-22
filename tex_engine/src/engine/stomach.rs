pub mod methods;

use crate::commands::TeXCommand;
use crate::commands::primitives::{PRIMITIVES, PrimitiveIdentifier};
use crate::commands::{CharOrPrimitive, CommandScope, PrimitiveCommand, ResolvedToken};
use crate::engine::filesystem::File;
use crate::engine::filesystem::SourceReference;
use crate::engine::gullet::Gullet;
use crate::engine::mouth::Mouth;
use crate::engine::state::{GroupType, State};
use crate::engine::stomach::methods::{ParLine, ParLineSpec, SplitResult};
use crate::engine::utils::outputs::Outputs;
use crate::engine::{EngineAux, EngineReferences, EngineTypes};
use crate::tex::catcodes::CommandCode;
use crate::tex::nodes::boxes::{BoxInfo, BoxType, HBoxInfo, TeXBox, ToOrSpread};
use crate::tex::nodes::horizontal::{HNode, HorizontalNodeListType};
use crate::tex::nodes::math::{
    Delimiter, MathAtom, MathChar, MathClass, MathKernel, MathNode, MathNodeList, MathNodeListType,
    MathNucleus, UnresolvedMathFontStyle,
};
use crate::tex::nodes::vertical::{VNode, VerticalNodeListType};
use crate::tex::nodes::{BoxTarget, ListTarget, NodeList, WhatsitFunction, WhatsitNode};
use crate::tex::numerics::{Skip, TeXDimen};
use crate::tex::tokens::token_lists::TokenList;
use crate::tex::tokens::{StandardToken, Token};
use crate::utils::HMap;
use crate::utils::errors::{TeXError, TeXResult};
use either::Either;
use std::fmt::Display;

/// The mode the engine is currently in, e.g. horizontal mode or vertical mode.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum TeXMode {
    /// The mode the engine is in at the start of a document, outside of boxes or paragraphs
    Vertical,
    /// The mode the engine is in inside a vertical box
    InternalVertical,
    /// The mode the engine is in inside a paragraph
    Horizontal,
    /// The mode the engine is in inside a horizontal box
    RestrictedHorizontal,
    /// The mode the engine is in inside an inline math box
    InlineMath,
    /// The mode the engine is in inside a display math box
    DisplayMath,
}
impl Display for TeXMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TeXMode::Vertical => write!(f, "vertical"),
            TeXMode::InternalVertical => write!(f, "internal vertical"),
            TeXMode::Horizontal => write!(f, "horizontal"),
            TeXMode::RestrictedHorizontal => write!(f, "restricted horizontal"),
            TeXMode::InlineMath => write!(f, "inline math"),
            TeXMode::DisplayMath => write!(f, "display math"),
        }
    }
}
impl TeXMode {
    /// Returns true if the mode is vertical or internal vertical
    pub fn is_vertical(&self) -> bool {
        matches!(self, TeXMode::Vertical | TeXMode::InternalVertical)
    }
    /// Returns true if the mode is horizontal or restricted horizontal
    pub fn is_horizontal(&self) -> bool {
        matches!(self, TeXMode::Horizontal | TeXMode::RestrictedHorizontal)
    }
    /// Returns true if the mode is inline math or display math
    pub fn is_math(&self) -> bool {
        matches!(self, TeXMode::InlineMath | TeXMode::DisplayMath)
    }
    /// Returns true if the mode is horizontal, restricted horizontal, inline math, or display math
    pub fn h_or_m(&self) -> bool {
        matches!(
            self,
            TeXMode::Horizontal
                | TeXMode::RestrictedHorizontal
                | TeXMode::InlineMath
                | TeXMode::DisplayMath
        )
    }
}
impl From<BoxType> for TeXMode {
    fn from(bt: BoxType) -> Self {
        match bt {
            BoxType::Horizontal => TeXMode::RestrictedHorizontal,
            BoxType::Vertical => TeXMode::InternalVertical,
        }
    }
}

/// The [`Stomach`] is the part of the engine that processes (unexpandable)
/// commands, collects [nodes](crate::tex::nodes::NodeTrait) in (horizontal or vertical) lists, and builds pages.
///
/// The vast majority of the methods implemented by this trait take a [`EngineReferences`] as their first argument
/// and have a default implementation already.
/// As such, we could have attached them to [`EngineReferences`] directly, but we put them here in a
/// separate trait instead so we can overwrite the methods easily - e.g. add `trigger` code when a paragraph is opened/closed etc.
pub trait Stomach<ET: EngineTypes /*<Stomach = Self>*/> {
    /// Constructs a new [`Stomach`].
    fn new(aux: &mut EngineAux<ET>, state: &mut ET::State) -> Self;
    /// Mutable reference to the current `\afterassignment` [`Token`].
    fn afterassignment(&mut self) -> &mut Option<ET::Token>;
    /// The current list(s)
    fn data_mut(&mut self) -> &mut StomachData<ET>;
    /// To be executed at every iteration of the top-level loop - i.e. in between all unexpandable commands
    #[inline]
    fn every_top(engine: &mut EngineReferences<ET>) {
        engine.mouth.update_start_ref();
    }
    /// To be executed at the end of a document - flushes the current page
    fn flush(engine: &mut EngineReferences<ET>) -> TeXResult<(), ET> {
        use crate::engine::utils::outputs::Outputs;
        let open_groups = std::mem::take(&mut engine.stomach.data_mut().open_lists);
        if !open_groups.is_empty() {
            engine.aux.outputs.message(format_args!(
                "(\\end occurred inside a group at level {})",
                engine.state.get_group_level()
            ));
            if let Some(g) = engine.state.get_group_type() {
                engine.aux.outputs.message(format_args!("## {} group", g))
            }
            while engine.state.get_group_level() > 0 {
                engine.state.pop(engine.aux, engine.mouth);
            }
        }
        Self::add_node_v(engine, VNode::Penalty(-10000))?;
        engine.stomach.data_mut().page.clear();
        Ok(())
    }
    /// Execute the provided [Unexpandable](PrimitiveCommand::Unexpandable) command
    fn do_unexpandable(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        scope: CommandScope,
        token: ET::Token,
        apply: fn(&mut EngineReferences<ET>, ET::Token) -> TeXResult<(), ET>,
    ) -> TeXResult<(), ET> {
        if Self::maybe_switch_mode(engine, scope, token.clone(), name)? {
            engine.trace_command(|engine| name.display(engine.state.get_escape_char()));
            apply(engine, token)
        } else {
            Ok(())
        }
    }

    /// Execute the provided [Assignment](PrimitiveCommand::Assignment) command and insert `\afterassignment` if necessary
    fn do_assignment(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        token: ET::Token,
        assign: fn(&mut EngineReferences<ET>, ET::Token, bool) -> TeXResult<(), ET>,
        global: bool,
    ) -> TeXResult<(), ET> {
        engine.trace_command(|engine| name.display(engine.state.get_escape_char()));
        assign(engine, token, global)?;
        methods::insert_afterassignment(engine);
        Ok(())
    }

    /// Execute the provided [Font](TeXCommand::Font) assignment and insert `\afterassignment` if necessary
    fn assign_font(
        engine: &mut EngineReferences<ET>,
        _token: ET::Token,
        f: ET::Font,
        global: bool,
    ) -> TeXResult<(), ET> {
        engine.state.set_current_font(engine.aux, f, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [count register](TeXCommand::IntRegister) and insert `\afterassignment` if necessary
    fn assign_int_register(
        engine: &mut EngineReferences<ET>,
        register: usize,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        let val = engine.read_int(true, &in_token)?;
        engine
            .state
            .set_int_register(engine.aux, register, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [dimen register](TeXCommand::DimRegister) and insert `\afterassignment` if necessary
    fn assign_dim_register(
        engine: &mut EngineReferences<ET>,
        register: usize,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        let val = engine.read_dim(true, &in_token)?;
        engine
            .state
            .set_dim_register(engine.aux, register, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [skip register](TeXCommand::SkipRegister) and insert `\afterassignment` if necessary
    fn assign_skip_register(
        engine: &mut EngineReferences<ET>,
        register: usize,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        let val = engine.read_skip(true, &in_token)?;
        engine
            .state
            .set_skip_register(engine.aux, register, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [muskip register](TeXCommand::MuSkipRegister) and insert `\afterassignment` if necessary
    fn assign_muskip_register(
        engine: &mut EngineReferences<ET>,
        register: usize,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        let val = engine.read_muskip(true, &in_token)?;
        engine
            .state
            .set_muskip_register(engine.aux, register, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [token register](TeXCommand::ToksRegister) and insert `\afterassignment` if necessary
    #[inline]
    fn assign_toks_register(
        engine: &mut EngineReferences<ET>,
        token: ET::Token,
        register: usize,
        global: bool,
    ) -> TeXResult<(), ET> {
        methods::assign_toks_register(engine, token, register, global)
    }
    /// Assign a value to a [primitive token list](PrimitiveCommand::PrimitiveToks) and insert `\afterassignment` if necessary
    #[inline]
    fn assign_primitive_toks(
        engine: &mut EngineReferences<ET>,
        token: ET::Token,
        name: PrimitiveIdentifier,
        global: bool,
    ) -> TeXResult<(), ET> {
        methods::assign_primitive_toks(engine, token, name, global)
    }
    /// Assign a value to a [primitive integer value](PrimitiveCommand::PrimitiveInt) and insert `\afterassignment` if necessary
    fn assign_primitive_int(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        engine.trace_command(|engine| format!("{}", name.display(engine.state.get_escape_char())));
        let val = engine.read_int(true, &in_token)?;
        engine
            .state
            .set_primitive_int(engine.aux, name, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [primitive dimension value](PrimitiveCommand::PrimitiveDim) and insert `\afterassignment` if necessary
    fn assign_primitive_dim(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        engine.trace_command(|engine| format!("{}", name.display(engine.state.get_escape_char())));
        let val = engine.read_dim(true, &in_token)?;
        engine
            .state
            .set_primitive_dim(engine.aux, name, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [primitive skip value](PrimitiveCommand::PrimitiveSkip) and insert `\afterassignment` if necessary
    fn assign_primitive_skip(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        engine.trace_command(|engine| format!("{}", name.display(engine.state.get_escape_char())));
        let val = engine.read_skip(true, &in_token)?;
        engine
            .state
            .set_primitive_skip(engine.aux, name, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Assign a value to a [primitive muskip value](PrimitiveCommand::PrimitiveMuSkip) and insert `\afterassignment` if necessary
    fn assign_primitive_muskip(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        global: bool,
        in_token: ET::Token,
    ) -> TeXResult<(), ET> {
        engine.trace_command(|engine| format!("{}", name.display(engine.state.get_escape_char())));
        let val = engine.read_muskip(true, &in_token)?;
        engine
            .state
            .set_primitive_muskip(engine.aux, name, val, global);
        methods::insert_afterassignment(engine);
        Ok(())
    }
    /// Executes a [Whatsit](PrimitiveCommand::Whatsit) command
    fn do_whatsit(
        engine: &mut EngineReferences<ET>,
        name: PrimitiveIdentifier,
        token: ET::Token,
        read: fn(
            &mut EngineReferences<ET>,
            ET::Token,
        ) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET>,
    ) -> TeXResult<(), ET> {
        if let Some(ret) = read(engine, token)? {
            let wi = WhatsitNode::new(ret, name);
            match engine.stomach.data_mut().mode() {
                TeXMode::Vertical | TeXMode::InternalVertical => {
                    Self::add_node_v(engine, VNode::Whatsit(wi))?
                }
                TeXMode::Horizontal | TeXMode::RestrictedHorizontal => {
                    Self::add_node_h(engine, HNode::Whatsit(wi))
                }
                TeXMode::InlineMath | TeXMode::DisplayMath => {
                    Self::add_node_m(engine, MathNode::Whatsit(wi))
                }
            }
        }
        Ok(())
    }
    /// Executes a [Box](PrimitiveCommand::Box) command
    fn do_box(
        engine: &mut EngineReferences<ET>,
        _name: PrimitiveIdentifier,
        token: ET::Token,
        bx: fn(
            &mut EngineReferences<ET>,
            ET::Token,
        ) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET>,
    ) -> TeXResult<(), ET> {
        match bx(engine, token)? {
            either::Left(Some(bx)) => methods::add_box(engine, bx, BoxTarget::none()),
            either::Left(None) => Ok(()),
            either::Right(bi) => {
                engine
                    .stomach
                    .data_mut()
                    .open_lists
                    .push(bi.open_list(engine.mouth.start_ref()));
                Ok(())
            }
        }
    }

    /// Processes a character depending on the current [`TeXMode`] and its [`CommandCode`]
    fn do_char(
        engine: &mut EngineReferences<ET>,
        token: ET::Token,
        char: ET::Char,
        code: CommandCode,
    ) -> TeXResult<(), ET> {
        methods::do_char(engine, token, char, code)
    }

    /// Processes a character depending on the current [`TeXMode`] and its [`CommandCode`]
    fn do_defed_char(
        engine: &mut EngineReferences<ET>,
        token: ET::Token,
        char: ET::Char,
    ) -> TeXResult<(), ET> {
        if engine.stomach.data_mut().mode().is_math() {
            Self::do_char_in_math(engine, char)
        } else {
            methods::do_char(engine, token, char, CommandCode::Other)
        }
    }
    fn do_char_in_math(engine: &mut EngineReferences<ET>, char: ET::Char) -> TeXResult<(), ET> {
        ET::Stomach::add_node_m(
            engine,
            MathNode::Atom(MathAtom {
                sup: None,
                sub: None,
                nucleus: MathNucleus::Simple {
                    cls: MathClass::Ord,
                    limits: None,
                    kernel: MathKernel::Char {
                        char,
                        style: UnresolvedMathFontStyle::of_fam(17), // use current font
                    },
                },
            }),
        );
        Ok(())
    }
    /// Processes a mathchar value (assumes we are in math mode)
    fn do_mathchar(engine: &mut EngineReferences<ET>, code: u32, token: Option<ET::Token>) {
        let ret = match token.map(|t| t.to_enum()) {
            Some(StandardToken::Character(char, _)) if code == 32768 => {
                engine
                    .mouth
                    .requeue(ET::Token::from_char_cat(char, CommandCode::Active));
                return;
            }
            Some(StandardToken::Character(char, _)) => {
                MathChar::from_u32(code, engine.state, Some(char))
            }
            _ => MathChar::from_u32(code, engine.state, None),
        };
        ET::Stomach::add_node_m(engine, MathNode::Atom(ret.to_atom()));
    }

    /// Closes a node list belonging to a [`TeXBox`] and adds it to the
    /// corresponding node list
    fn close_box(engine: &mut EngineReferences<ET>, bt: BoxType) -> TeXResult<(), ET> {
        methods::close_box(engine, bt)
    }

    /// Switches the current [`TeXMode`] (if necessary) by opening/closing a paragraph, or throws an error
    /// if neither action is possible or would not result in a compatible mode.
    /// If a paragraph is opened or closed, the provided token is requeued to be reprocessed afterwards in
    /// horizontal/vertical mode, and `false` is returned (as to not process the triggering command
    /// further). Otherwise, all is well and `true` is returned.
    fn maybe_switch_mode(
        engine: &mut EngineReferences<ET>,
        scope: CommandScope,
        token: ET::Token,
        name: PrimitiveIdentifier,
    ) -> TeXResult<bool, ET> {
        match (scope, engine.stomach.data_mut().mode()) {
            (CommandScope::Any, _) => Ok(true),
            (
                CommandScope::SwitchesToHorizontal | CommandScope::SwitchesToHorizontalOrMath,
                TeXMode::Horizontal | TeXMode::RestrictedHorizontal,
            ) => Ok(true),
            (CommandScope::SwitchesToVertical, TeXMode::Vertical | TeXMode::InternalVertical) => {
                Ok(true)
            }
            (CommandScope::SwitchesToVertical, TeXMode::Horizontal) => {
                engine.requeue(token)?;
                Self::close_paragraph(engine)?;
                Ok(false)
            }
            (
                CommandScope::MathOnly | CommandScope::SwitchesToHorizontalOrMath,
                TeXMode::InlineMath | TeXMode::DisplayMath,
            ) => Ok(true),
            (
                CommandScope::SwitchesToHorizontal | CommandScope::SwitchesToHorizontalOrMath,
                TeXMode::Vertical | TeXMode::InternalVertical,
            ) => {
                Self::open_paragraph(engine, token);
                Ok(false)
            }
            (_, _) => {
                TeXError::not_allowed_in_mode(
                    engine.aux,
                    engine.state,
                    engine.mouth,
                    name,
                    engine.stomach.data_mut().mode(),
                )?;
                Ok(false)
            }
        }
    }

    /// Opens an `\halign` or `\valign`
    fn open_align(engine: &mut EngineReferences<ET>, _inner: BoxType, between: BoxType) {
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
    }

    /// Closes an `\halign` or `\valign`
    fn close_align(engine: &mut EngineReferences<ET>) -> TeXResult<(), ET> {
        match engine.stomach.data_mut().open_lists.pop() {
            Some(NodeList::Vertical {
                children,
                tp: VerticalNodeListType::HAlign,
            }) => {
                engine.state.pop(engine.aux, engine.mouth);
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
                engine.state.pop(engine.aux, engine.mouth);
                for c in children {
                    Self::add_node_h(engine, c);
                }
            }
            _ => unreachable!("Stomach::close_align called outside of an align"),
        };
        Ok(())
    }

    /// Adds a node to the current math list (i.e. assumes we're in math mode)
    fn add_node_m(
        engine: &mut EngineReferences<ET>,
        node: MathNode<ET, UnresolvedMathFontStyle<ET>>,
    ) {
        match engine.stomach.data_mut().open_lists.last_mut() {
            Some(NodeList::Math { children, .. }) => {
                children.push(node);
            }
            _ => unreachable!("Stomach::add_node_m called outside of math mode"),
        }
    }

    /// Adds a node to the current horizontal list (i.e. assumes we're in (restricted) horizontal mode)
    fn add_node_h(engine: &mut EngineReferences<ET>, node: HNode<ET>) {
        if let HNode::Penalty(i) = node {
            engine.stomach.data_mut().lastpenalty = i;
        }
        match engine.stomach.data_mut().open_lists.last_mut() {
            Some(NodeList::Horizontal { children, .. }) => {
                children.push(node);
            }
            _ => unreachable!("Stomach::add_node_h called outside of horizontal mode"),
        }
    }

    /// Adds a node to the current vertical list (i.e. assumes we're in (internal) vertical mode)
    #[inline]
    fn add_node_v(engine: &mut EngineReferences<ET>, node: VNode<ET>) -> TeXResult<(), ET> {
        methods::add_node_v(engine, node)
    }

    /// Checks whether the output routine should occur; either because the page is
    /// full enough, or because the provided penalty is `Some`
    /// (and assumed to be <= -10000) and the page is not empty.
    fn maybe_do_output(
        engine: &mut EngineReferences<ET>,
        penalty: Option<i32>,
    ) -> TeXResult<(), ET> {
        let data = engine.stomach.data_mut();
        if !data.in_output
            && data.open_lists.is_empty()
            && !data.page.is_empty()
            && (data.pagetotal >= data.pagegoal || penalty.is_some())
        {
            Self::do_output(engine, penalty)
        } else {
            Ok(())
        }
    }

    /// Actually calls the output routine
    fn do_output(
        engine: &mut EngineReferences<ET>,
        caused_penalty: Option<i32>,
    ) -> TeXResult<(), ET> {
        methods::do_output(engine, caused_penalty)
    }

    /// Split a vertical list for the provided target height
    fn split_vertical(
        engine: &mut EngineReferences<ET>,
        nodes: Vec<VNode<ET>>,
        target: <ET as EngineTypes>::Dim,
    ) -> SplitResult<ET> {
        methods::vsplit_roughly(engine, nodes, target)
    }

    /// Open a new paragraph; assumed to be called in (internal) vertical mode
    fn open_paragraph(engine: &mut EngineReferences<ET>, token: ET::Token) {
        let sref = engine.mouth.start_ref();
        let data = engine.stomach.data_mut();
        data.prevgraf = 0;
        data.open_lists.push(NodeList::Horizontal {
            tp: HorizontalNodeListType::Paragraph(sref),
            children: vec![],
        });
        match <ET as EngineTypes>::Gullet::char_or_primitive(engine.state, &token) {
            Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.indent => {
                Self::add_node_h(
                    engine,
                    HNode::Box(TeXBox::H {
                        children: vec![].into(),
                        info: HBoxInfo::ParIndent(
                            engine.state.get_primitive_dim(PRIMITIVES.parindent),
                        ),
                        start: sref,
                        end: sref,
                        preskip: None,
                    }),
                )
            }
            Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.noindent => (),
            _ => {
                engine.mouth.requeue(token);
                Self::add_node_h(
                    engine,
                    HNode::Box(TeXBox::H {
                        children: vec![].into(),
                        info: HBoxInfo::ParIndent(
                            engine.state.get_primitive_dim(PRIMITIVES.parindent),
                        ),
                        start: sref,
                        end: sref,
                        preskip: None,
                    }),
                )
            }
        }
        engine.push_every(PRIMITIVES.everypar)
    }

    /// Close a paragraph; assumed to be called in horizontal mode
    fn close_paragraph(engine: &mut EngineReferences<ET>) -> TeXResult<(), ET> {
        let ls = &mut engine.stomach.data_mut().open_lists;
        match ls.pop() {
            Some(NodeList::Horizontal {
                tp: HorizontalNodeListType::Paragraph(sourceref),
                children,
            }) => {
                if children.is_empty() {
                    let _ = engine.state.take_parshape();
                    engine.state.set_primitive_int(
                        engine.aux,
                        PRIMITIVES.hangafter,
                        ET::Int::default(),
                        false,
                    );
                    engine.state.set_primitive_dim(
                        engine.aux,
                        PRIMITIVES.hangindent,
                        ET::Dim::default(),
                        false,
                    );
                    return Ok(());
                }
                let spec = ParLineSpec::make(engine.state, engine.aux);
                Self::split_paragraph(engine, spec, children, sourceref)?;
            }
            _ => unreachable!("Stomach::close_paragraph called outside of horizontal mode"),
        }
        Ok(())
    }

    /// Split a paragraph into lines and add them (as horizontal boxes) to the current vertical list
    fn split_paragraph(
        engine: &mut EngineReferences<ET>,
        specs: Vec<ParLineSpec<ET>>,
        children: Vec<HNode<ET>>,
        start_ref: SourceReference<<<ET as EngineTypes>::File as File>::SourceRefID>,
    ) -> TeXResult<(), ET> {
        if children.is_empty() {
            return Ok(());
        }
        let parskip = engine.state.get_primitive_skip(PRIMITIVES.parskip);
        if parskip != Skip::default() {
            Self::add_node_v(engine, VNode::VSkip(parskip))?;
        }
        let ret = methods::split_paragraph_roughly(engine, specs, children, start_ref);
        for line in ret {
            match line {
                ParLine::Adjust(n) => Self::add_node_v(engine, n)?,
                ParLine::Line(bx) => Self::add_node_v(engine, VNode::Box(bx))?,
            }
        }
        Ok(())
    }
}

/// All the mutable data of the [`Stomach`] - i.e. the current page, the current list(s), etc.
///
/// TODO: should be overhauled; this is just a rough approximation of what needs to happen and can be made
/// more efficient *and* more correct.
#[derive(Clone, Debug)]
pub struct StomachData<ET: EngineTypes> {
    pub page: Vec<VNode<ET>>,
    pub open_lists: Vec<NodeList<ET>>,
    pub pagegoal: ET::Dim,
    pub pagetotal: ET::Dim,
    pub pagestretch: ET::Dim,
    pub pagefilstretch: ET::Dim,
    pub pagefillstretch: ET::Dim,
    pub pagefilllstretch: ET::Dim,
    pub pageshrink: ET::Dim,
    pub pagedepth: ET::Dim,
    pub prevdepth: ET::Dim,
    pub spacefactor: i32,
    pub topmarks: HMap<usize, TokenList<ET::Token>>,
    pub firstmarks: HMap<usize, TokenList<ET::Token>>,
    pub botmarks: HMap<usize, TokenList<ET::Token>>,
    pub splitfirstmarks: HMap<usize, TokenList<ET::Token>>,
    pub splitbotmarks: HMap<usize, TokenList<ET::Token>>,
    pub page_contains_boxes: bool,
    pub lastpenalty: i32,
    pub prevgraf: u16,
    pub in_output: bool,
    pub deadcycles: usize,
    pub vadjusts: Vec<VNode<ET>>,
    pub inserts: Vec<(usize, Box<[VNode<ET>]>)>,
}
impl<ET: EngineTypes> StomachData<ET> {
    /// The current [`TeXMode`] (indicating the type of node list currently open)
    pub fn mode(&self) -> TeXMode {
        match self.open_lists.last() {
            Some(NodeList::Horizontal {
                tp: HorizontalNodeListType::Paragraph(..),
                ..
            }) => TeXMode::Horizontal,
            Some(NodeList::Horizontal { .. }) => TeXMode::RestrictedHorizontal,
            Some(NodeList::Vertical { .. }) => TeXMode::InternalVertical,
            Some(NodeList::Math { .. }) => {
                for ls in self.open_lists.iter().rev() {
                    if let NodeList::Math {
                        tp: MathNodeListType::Top { display },
                        ..
                    } = ls
                    {
                        if *display {
                            return TeXMode::DisplayMath;
                        } else {
                            return TeXMode::InlineMath;
                        }
                    }
                }
                unreachable!()
            }
            None => TeXMode::Vertical,
        }
    }
}

impl<ET: EngineTypes> Default for StomachData<ET> {
    fn default() -> Self {
        StomachData {
            page: vec![],
            open_lists: vec![],
            pagegoal: ET::Dim::from_sp(i32::MAX),
            pagetotal: ET::Dim::default(),
            pagestretch: ET::Dim::default(),
            pagefilstretch: ET::Dim::default(),
            pagefillstretch: ET::Dim::default(),
            pagefilllstretch: ET::Dim::default(),
            pageshrink: ET::Dim::default(),
            pagedepth: ET::Dim::default(),
            prevdepth: ET::Dim::from_sp(-65536000),
            spacefactor: 1000,
            topmarks: HMap::default(),
            firstmarks: HMap::default(),
            botmarks: HMap::default(),
            splitfirstmarks: HMap::default(),
            splitbotmarks: HMap::default(),
            prevgraf: 0,
            lastpenalty: 0,
            page_contains_boxes: false,
            in_output: false,
            deadcycles: 0,
            vadjusts: vec![],
            inserts: vec![],
        }
    }
}

/// Default implementation of a [`Stomach`]
pub struct DefaultStomach<ET: EngineTypes /*<Stomach=Self>*/> {
    afterassignment: Option<ET::Token>,
    data: StomachData<ET>,
}
impl<ET: EngineTypes /*<Stomach=Self>*/> Stomach<ET> for DefaultStomach<ET> {
    fn new(_aux: &mut EngineAux<ET>, _state: &mut ET::State) -> Self {
        DefaultStomach {
            afterassignment: None,
            data: StomachData::default(),
        }
    }

    fn afterassignment(&mut self) -> &mut Option<ET::Token> {
        &mut self.afterassignment
    }

    fn data_mut(&mut self) -> &mut StomachData<ET> {
        &mut self.data
    }
}

impl<ET: EngineTypes> EngineReferences<'_, ET> {
    /// read a box from the current input stream
    pub fn read_box(
        &mut self,
        skip_eq: bool,
    ) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
        let mut read_eq = !skip_eq;
        crate::expand_loop!(self,token,
            ResolvedToken::Tk {char,code:CommandCode::Other} if !read_eq && matches!(char.try_into(),Ok(b'=')) => read_eq = true,
            ResolvedToken::Tk { code:CommandCode::Space,..} => (),
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {cmd:PrimitiveCommand::Box(b),..})) =>
                return b(self,token),
            _ => break
        );
        self.general_error("A <box> was supposed to be here".to_string())?;
        Ok(Either::Left(None))
    }

    /// read a math char or group from the current input stream. Assumes we are in math mode.
    /// (e.g. `\mathop X` or `\mathop{ \alpha + \beta }`).
    /// In the latter case a new list is opened and processed "asynchronously". When the list is closed,
    /// the second continuation is called with the list as argument.
    pub fn read_char_or_math_group<
        S,
        F1: FnOnce(S, &mut Self, MathChar<ET>) -> TeXResult<(), ET>,
        F2: FnOnce(S) -> ListTarget<ET, MathNode<ET, UnresolvedMathFontStyle<ET>>>,
    >(
        &mut self,
        in_token: &ET::Token,
        f: F1,
        tp: F2,
        s: S,
    ) -> TeXResult<(), ET> {
        crate::expand_loop!(self,token,
            ResolvedToken::Tk {code:CommandCode::Space,..} => (),
            ResolvedToken::Tk {code:CommandCode::BeginGroup,..} |
            ResolvedToken::Cmd(Some(TeXCommand::Char {code:CommandCode::BeginGroup,..})) => {
                self.state.push(self.aux,GroupType::Math,self.mouth.line_number());
                let list = NodeList::Math{children:MathNodeList::default(),start:self.mouth.start_ref(),
                    tp:MathNodeListType::Target(tp(s))};
                self.stomach.data_mut().open_lists.push(list);
                return Ok(())
            },
            ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Relax,..})) => (),
            ResolvedToken::Tk {char,code:CommandCode::Other | CommandCode::Letter} => {
                let code = self.state.get_mathcode(char);
                if code == 32768 {
                    self.mouth.requeue(ET::Token::from_char_cat(char, CommandCode::Active));
                    continue
                }
                let mc = MathChar::from_u32(code,self.state, Some(char));
                return f(s,self,mc)
            },
            ResolvedToken::Cmd(Some(TeXCommand::MathChar(u))) => {
                let mc = MathChar::from_u32(*u, self.state,None);
                return f(s,self,mc)
            }
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.delimiter => {
                let int = self.read_int(true,&token)?;
                match Delimiter::from_int(int,self.state) {
                    either::Left(d) => return f(s,self,d.small),
                    either::Right((d,i)) => {
                        self.general_error(format!("Bad delimiter code ({})",i))?;
                        return f(s,self,d.small)
                    }
                }
            }
            _ => return Err(TeXError::General("Begingroup or math character expected\nTODO: Better error message".to_string()))
        );
        TeXError::file_end_while_use(self.aux, self.state, self.mouth, in_token)
    }
}
