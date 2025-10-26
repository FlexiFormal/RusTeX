/*! Utility methods for [`TeXCommand`]s.
*/

use crate::commands::primitives::{PrimitiveIdentifier, PRIMITIVES};
use crate::commands::{
    CharOrPrimitive, Macro, MacroSignature, PrimitiveCommand, ResolvedToken, TeXCommand,
};
use crate::engine::filesystem::FileSystem;
use crate::engine::fontsystem::{Font, FontSystem};
use crate::engine::gullet::hvalign::{AlignColumn, AlignData};
use crate::engine::gullet::Gullet;
use crate::engine::mouth::Mouth;
use crate::engine::state::{GroupType, State};
use crate::engine::stomach::TeXMode;
use crate::engine::stomach::{Stomach, StomachData};
use crate::engine::{EngineAux, EngineReferences, EngineTypes};
use crate::expand_loop;
use crate::tex::catcodes::CommandCode;
use crate::tex::nodes::boxes::{BoxType, HBoxInfo, TeXBox, ToOrSpread, VBoxInfo};
use crate::tex::nodes::horizontal::{HNode, HorizontalNodeListType};
use crate::tex::nodes::math::{
    Delimiter, MathAtom, MathClass, MathKernel, MathNode, MathNucleus, UnresolvedMathFontStyle,
};
use crate::tex::nodes::vertical::{VNode, VerticalNodeListType};
use crate::tex::nodes::NodeTrait;
use crate::tex::nodes::{
    BoxTarget, LeaderBody, LeaderSkip, LeaderType, Leaders, ListTarget, NodeList,
};
use crate::tex::numerics::Skip;
use crate::tex::tokens::control_sequences::CSHandler;
use crate::tex::tokens::token_lists::TokenList;
use crate::tex::tokens::Token;
use crate::utils::errors::{TeXError, TeXResult};
use crate::utils::HMap;

pub(crate) struct MacroParser<T: Token> {
    arity: u8,
    params: shared_vector::Vector<T>,
    inparam: bool,
    ends_with_brace: Option<T>,
    exp: shared_vector::Vector<T>,
}
impl<T: Token> MacroParser<T> {
    pub(crate) fn new() -> Self {
        Self {
            arity: 0,
            params: shared_vector::Vector::new(),
            inparam: false,
            ends_with_brace: None,
            exp: shared_vector::Vector::new(),
        }
    }
    pub(crate) fn do_signature_token<ET: EngineTypes<Token = T>>(
        &mut self,
        t: T,
    ) -> TeXResult<Option<()>, ET> {
        match t.command_code() {
            CommandCode::BeginGroup => {
                if self.inparam {
                    self.inparam = false;
                    self.params.push(t.clone());
                    self.ends_with_brace = Some(t);
                }
                return Ok(Some(()));
            }
            CommandCode::Parameter if self.inparam => {
                self.inparam = false;
                self.params.push(t);
            }
            CommandCode::Parameter => {
                self.inparam = true;
            }
            _ if self.inparam => {
                self.inparam = false;
                match t.char_value() {
                    Some(c) => match c.try_into() {
                        Ok(u) if u > 48 && u == 49 + self.arity => {
                            self.params.push(T::argument_marker(self.arity))
                        }
                        _ => {
                            return Err(TeXError::General(
                                "Invalid argument number\nTODO: Better error message".to_string(),
                            ))
                        }
                    },
                    None => {
                        return Err(TeXError::General(
                            "Missing argument number\nTODO: Better error message".to_string(),
                        ))
                    }
                }
                self.arity += 1;
            }
            _ => self.params.push(t),
        }
        Ok(None)
    }

    pub(crate) fn do_expansion_token<ET: EngineTypes>(&mut self, t: T) -> TeXResult<(), ET> {
        match t.command_code() {
            CommandCode::Parameter if self.inparam => {
                self.inparam = false;
                self.exp.push(t);
            }
            CommandCode::Parameter => self.inparam = true,
            _ if self.inparam => {
                self.inparam = false;
                match t.char_value() {
                    Some(c) => match c.try_into() {
                        Ok(u) if u > 48 && u - 49 < self.arity => {
                            self.exp.push(T::argument_marker(u - 49))
                        }
                        _ => {
                            return Err(TeXError::General(
                                "Invalid argument number\nTODO: Better error message".to_string(),
                            ))
                        }
                    },
                    None => {
                        return Err(TeXError::General(
                            "Missing argument number\nTODO: Better error message".to_string(),
                        ))
                    }
                }
            }
            _ => self.exp.push(t),
        }
        Ok(())
    }

    pub(crate) fn close(mut self, long: bool, outer: bool, protected: bool) -> Macro<T> {
        if let Some(t) = self.ends_with_brace {
            self.exp.push(t);
        }
        Macro {
            long,
            outer,
            protected,
            expansion: self.exp.into(),
            signature: MacroSignature {
                arity: self.arity,
                params: self.params.into(),
            },
        }
    }
}

pub(in crate::commands) fn modify_int_register<
    ET: EngineTypes,
    O: FnOnce(ET::Int, &mut EngineReferences<ET>) -> TeXResult<ET::Int, ET>,
>(
    engine: &mut EngineReferences<ET>,
    idx: usize,
    globally: bool,
    op: O,
) -> TeXResult<(), ET> {
    engine.read_keyword(b"by")?;
    let old = engine.state.get_int_register(idx);
    let new = op(old, engine)?;
    engine
        .state
        .set_int_register(engine.aux, idx, new, globally);
    Ok(())
}

pub(in crate::commands) fn modify_primitive_int<
    ET: EngineTypes,
    O: FnOnce(ET::Int, &mut EngineReferences<ET>) -> TeXResult<ET::Int, ET>,
>(
    engine: &mut EngineReferences<ET>,
    name: PrimitiveIdentifier,
    globally: bool,
    op: O,
) -> TeXResult<(), ET> {
    engine.read_keyword(b"by")?;
    let old = engine.state.get_primitive_int(name);
    let new = op(old, engine)?;
    engine
        .state
        .set_primitive_int(engine.aux, name, new, globally);
    Ok(())
}

pub(in crate::commands) fn modify_dim_register<
    ET: EngineTypes,
    O: FnOnce(ET::Dim, &mut EngineReferences<ET>) -> TeXResult<ET::Dim, ET>,
>(
    engine: &mut EngineReferences<ET>,
    idx: usize,
    globally: bool,
    op: O,
) -> TeXResult<(), ET> {
    engine.read_keyword(b"by")?;
    let old = engine.state.get_dim_register(idx);
    let new = op(old, engine)?;
    engine
        .state
        .set_dim_register(engine.aux, idx, new, globally);
    Ok(())
}

pub(in crate::commands) fn modify_primitive_dim<
    ET: EngineTypes,
    O: FnOnce(ET::Dim, &mut EngineReferences<ET>) -> TeXResult<ET::Dim, ET>,
>(
    engine: &mut EngineReferences<ET>,
    name: PrimitiveIdentifier,
    globally: bool,
    op: O,
) -> TeXResult<(), ET> {
    engine.read_keyword(b"by")?;
    let old = engine.state.get_primitive_dim(name);
    let new = op(old, engine)?;
    engine
        .state
        .set_primitive_dim(engine.aux, name, new, globally);
    Ok(())
}

pub(in crate::commands) fn modify_skip_register<
    ET: EngineTypes,
    O: FnOnce(Skip<ET::Dim>, &mut EngineReferences<ET>) -> TeXResult<Skip<ET::Dim>, ET>,
>(
    engine: &mut EngineReferences<ET>,
    idx: usize,
    globally: bool,
    op: O,
) -> TeXResult<(), ET> {
    engine.read_keyword(b"by")?;
    let old = engine.state.get_skip_register(idx);
    let new = op(old, engine)?;
    engine
        .state
        .set_skip_register(engine.aux, idx, new, globally);
    Ok(())
}

pub(in crate::commands) fn modify_primitive_skip<
    ET: EngineTypes,
    O: FnOnce(Skip<ET::Dim>, &mut EngineReferences<ET>) -> TeXResult<Skip<ET::Dim>, ET>,
>(
    engine: &mut EngineReferences<ET>,
    name: PrimitiveIdentifier,
    globally: bool,
    op: O,
) -> TeXResult<(), ET> {
    engine.read_keyword(b"by")?;
    let old = engine.state.get_primitive_skip(name);
    let new = op(old, engine)?;
    engine
        .state
        .set_primitive_skip(engine.aux, name, new, globally);
    Ok(())
}

pub(in crate::commands) fn do_box_start<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tp: GroupType,
    every: PrimitiveIdentifier,
    tk: &ET::Token,
) -> TeXResult<ToOrSpread<ET::Dim>, ET> {
    let scaled = match engine.read_keywords(&[b"to", b"spread"])? {
        Some(b"to") => {
            let to = engine.read_dim(false, tk)?;
            ToOrSpread::To(to)
        }
        Some(b"spread") => {
            let spread = engine.read_dim(false, tk)?;
            ToOrSpread::Spread(spread)
        }
        _ => ToOrSpread::None,
    };
    crate::expand_loop!(engine,token,
        ResolvedToken::Tk {code:CommandCode::Space,..} => (),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Relax,..})) => (),
        ResolvedToken::Tk {code:CommandCode::BeginGroup,..} |
        ResolvedToken::Cmd(Some(TeXCommand::Char{code:CommandCode::BeginGroup,..})) => {
            engine.state.push(engine.aux,tp,engine.mouth.line_number());
            engine.push_every(every);
            return Ok(scaled)
        }
        _ => return Err(TeXError::General("Begin group expected in box start\nTODO: Better error message".to_string()))
    );
    Err(TeXError::General(
        "File ended unexpectedly\nTODO: Better error message".to_string(),
    ))
}

pub(in crate::commands) fn get_if_token<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    in_token: &ET::Token,
) -> TeXResult<(Option<ET::Char>, CommandCode), ET> {
    let mut exp = true;
    loop {
        let token = engine.need_next(!exp, in_token)?;
        if token.is_primitive() == Some(PRIMITIVES.noexpand) {
            exp = false;
            continue;
        }
        match engine.resolve(&token) {
            ResolvedToken::Tk { char, code } => return Ok((Some(char), code)),
            ResolvedToken::Cmd(cmd) => match cmd {
                Some(TeXCommand::Macro(m)) if exp => {
                    ET::Gullet::do_macro(engine, m.clone(), token)?
                }
                Some(TeXCommand::Primitive {
                    name,
                    cmd: PrimitiveCommand::Conditional(cond),
                }) if exp => ET::Gullet::do_conditional(engine, *name, token, *cond, false)?,
                Some(TeXCommand::Primitive {
                    name,
                    cmd: PrimitiveCommand::Expandable(e),
                }) if exp => ET::Gullet::do_expandable(engine, *name, token, *e)?,
                Some(TeXCommand::Primitive {
                    name,
                    cmd: PrimitiveCommand::SimpleExpandable(e),
                }) if exp => ET::Gullet::do_simple_expandable(engine, *name, token, *e)?,
                Some(TeXCommand::Char { char, code }, ..) => return Ok((Some(*char), *code)),
                _ => return Ok((None, CommandCode::Escape)),
            },
        }
    }
}

pub(in crate::commands) enum IfxCmd<ET: EngineTypes> {
    Char(ET::Char, CommandCode),
    Undefined,
    Primitive(PrimitiveIdentifier),
    Noexpand(ET::Token),
    Chardef(ET::Char),
    Font(<ET::FontSystem as FontSystem>::Font),
    MathChar(u32),
    Macro(Macro<ET::Token>),
    IntRegister(usize),
    DimRegister(usize),
    SkipRegister(usize),
    MuSkipRegister(usize),
    ToksRegister(usize),
}
impl<ET: EngineTypes> IfxCmd<ET> {
    pub(in crate::commands) fn read(
        engine: &mut EngineReferences<ET>,
        in_token: &ET::Token,
    ) -> TeXResult<Self, ET> {
        let next = engine.need_next(true, in_token)?;
        Ok(if next.is_primitive() == Some(PRIMITIVES.noexpand) {
            IfxCmd::Noexpand(engine.need_next(true, &next)?)
        } else {
            Self::resolve(engine.resolve(&next))
        })
    }

    fn resolve(r: ResolvedToken<ET>) -> Self {
        match r {
            ResolvedToken::Tk { char, code, .. } => Self::Char(char, code),
            ResolvedToken::Cmd(cmd) => match cmd {
                Some(TeXCommand::Char { char, code }) => Self::Char(*char, *code),
                None => Self::Undefined,
                Some(TeXCommand::Macro(m)) => Self::Macro(m.clone()),
                Some(TeXCommand::CharDef(c)) => Self::Chardef(*c),
                Some(TeXCommand::Font(f)) => Self::Font(f.clone()),
                Some(TeXCommand::MathChar(u)) => Self::MathChar(*u),
                Some(TeXCommand::IntRegister(u)) => Self::IntRegister(*u),
                Some(TeXCommand::DimRegister(u)) => Self::DimRegister(*u),
                Some(TeXCommand::SkipRegister(u)) => Self::SkipRegister(*u),
                Some(TeXCommand::MuSkipRegister(u)) => Self::MuSkipRegister(*u),
                Some(TeXCommand::ToksRegister(u)) => Self::ToksRegister(*u),
                Some(TeXCommand::Primitive { name, .. }) => Self::Primitive(*name),
            },
        }
    }
}

impl<ET: EngineTypes> PartialEq for IfxCmd<ET> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Char(_, CommandCode::Space), Self::Char(_, CommandCode::Space)) => true,
            (Self::Char(c1, cc1), Self::Char(c2, cc2)) => c1 == c2 && cc1 == cc2,
            (Self::Undefined, Self::Undefined) => true,
            (Self::Primitive(id), Self::Primitive(id2)) => id == id2,
            (Self::Noexpand(t1), Self::Noexpand(t2)) => t1 == t2,
            (Self::Chardef(c), Self::Chardef(c2)) => c == c2,
            (Self::Font(f1), Self::Font(f2)) => f1.name() == f2.name(),
            (Self::MathChar(u1), Self::MathChar(u2)) => u1 == u2,
            (Self::IntRegister(u1), Self::IntRegister(u2)) => u1 == u2,
            (Self::DimRegister(u1), Self::DimRegister(u2)) => u1 == u2,
            (Self::SkipRegister(u1), Self::SkipRegister(u2)) => u1 == u2,
            (Self::MuSkipRegister(u1), Self::MuSkipRegister(u2)) => u1 == u2,
            (Self::ToksRegister(u1), Self::ToksRegister(u2)) => u1 == u2,
            (Self::Macro(m1), Self::Macro(m2)) => {
                m1.long == m2.long
                    && m1.outer == m2.outer
                    && m1.protected == m2.protected
                    && m1.signature.params == m2.signature.params
                    && m1.expansion == m2.expansion
            }
            _ => false,
        }
    }
}

pub(crate) fn do_marks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    idx: usize,
    in_token: &ET::Token,
) -> TeXResult<(), ET> {
    let mut v = shared_vector::Vector::new();
    engine.expand_until_bgroup(false, in_token)?;
    engine.expand_until_endgroup(true, true, in_token, |_, _, t| {
        v.push(t);
        Ok(())
    })?;
    let data = engine.stomach.data_mut();
    for list in data.open_lists.iter_mut().rev() {
        match list {
            NodeList::Horizontal {
                children,
                tp: HorizontalNodeListType::Paragraph(..),
            } => {
                children.push(HNode::Mark(idx, v.into()));
                return Ok(());
            }
            NodeList::Vertical { children, .. } => {
                children.push(VNode::Mark(idx, v.into()));
                return Ok(());
            }
            NodeList::Math { children, .. } => {
                children.push(MathNode::Mark(idx, v.into()));
                return Ok(());
            }
            _ => (),
        }
    }
    data.page.push(VNode::Mark(idx, v.into()));
    Ok(())
}

pub(crate) fn get_marks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    f: fn(&mut StomachData<ET>) -> &mut HMap<usize, TokenList<ET::Token>>,
    idx: usize,
) {
    if let Some(v) = f(engine.stomach.data_mut()).get(&idx) {
        exp.extend(v.0.iter().cloned())
    }
}

pub(crate) fn do_align<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    inner: BoxType,
    between: BoxType,
    _to: Option<ET::Dim>,
    tk: &ET::Token,
) -> TeXResult<(), ET> {
    // TODO use to
    engine.gullet.push_align(AlignData::dummy());
    engine.expand_until_bgroup(true, tk)?;
    let data = read_align_preamble(engine, inner, between, tk)?;
    *engine.gullet.get_align_data().unwrap() = data;
    ET::Stomach::open_align(engine, inner, between);
    start_align_row(engine, inner)
}

fn read_align_preamble<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    inner_mode: BoxType,
    between_mode: BoxType,
    in_token: &ET::Token,
) -> TeXResult<AlignData<ET::Token, ET::Dim>, ET> {
    struct AlignmentDataBuilder<ET: EngineTypes> {
        columns: Vec<AlignColumn<ET::Token, ET::Dim>>,
        recindex: Option<usize>,
        current_u: Vec<ET::Token>,
        current_v: Vec<ET::Token>,
        ingroups: u8,
        in_v: bool,
        tabskip: Skip<ET::Dim>,
        inner_mode: BoxType,
        between_mode: BoxType,
    }
    impl<ET: EngineTypes> AlignmentDataBuilder<ET> {
        fn push(&mut self, tk: ET::Token) {
            if self.in_v {
                self.current_v.push(tk)
            } else {
                self.current_u.push(tk)
            }
        }
        fn build(mut self, token: ET::Token) -> AlignData<ET::Token, ET::Dim> {
            self.columns.push(AlignColumn::new(
                self.current_u,
                self.current_v,
                self.tabskip,
                self.ingroups,
            ));
            AlignData {
                token,
                columns: self.columns.into(),
                ingroups: 0,
                currindex: 0,
                repeat_index: self.recindex,
                omit: false,
                span: false,
                inner_mode: self.inner_mode,
                outer_mode: self.between_mode,
            }
        }
    }

    let tabskip = engine.state.get_primitive_skip(PRIMITIVES.tabskip);
    let mut cols = AlignmentDataBuilder::<ET> {
        columns: Vec::new(),
        recindex: None,
        ingroups: 0,
        current_u: Vec::new(),
        current_v: Vec::new(),
        in_v: false,
        tabskip,
        inner_mode,
        between_mode,
    };
    let mut ingroups = 0;

    while let Some(next) = engine.mouth.get_next(engine.aux, engine.state)? {
        if next.is_primitive() == Some(PRIMITIVES.noexpand) {
            let Ok(Some(n)) = engine.mouth.get_next(engine.aux, engine.state) else {
                unreachable!()
            };
            cols.push(n);
            continue;
        }
        match next.command_code() {
            CommandCode::BeginGroup => {
                ingroups += 1;
                cols.push(next);
                continue;
            }
            CommandCode::EndGroup => {
                ingroups -= 1;
                cols.push(next);
                continue;
            }
            _ => (),
        }
        match ET::Gullet::char_or_primitive(engine.state, &next) {
            Some(CharOrPrimitive::Char(_, Some(CommandCode::Parameter))) => {
                if cols.in_v {
                    return Err(TeXError::General(
                        "Unexpected # in alignment\nTODO: Better error message".to_string(),
                    ));
                }
                cols.in_v = true;
                cols.ingroups = ingroups;
            }
            Some(CharOrPrimitive::Char(_, Some(CommandCode::AlignmentTab))) => {
                if ingroups != 0 {
                    return Err(TeXError::General(
                        "Unbalanced number of braces in alignment\nTODO: Better error message"
                            .to_string(),
                    ));
                }
                if !cols.in_v && cols.current_u.is_empty() {
                    cols.recindex = Some(cols.columns.len());
                } else {
                    let (u, v, g) = (
                        std::mem::take(&mut cols.current_u),
                        std::mem::take(&mut cols.current_v),
                        std::mem::take(&mut cols.ingroups),
                    );
                    cols.columns.push(AlignColumn::new(u, v, cols.tabskip, g));
                    cols.tabskip = tabskip;
                    cols.in_v = false;
                }
            }
            Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.tabskip => {
                cols.tabskip = engine.read_skip(true, in_token)?
            }
            Some(CharOrPrimitive::Primitive(name))
                if name == PRIMITIVES.cr || name == PRIMITIVES.crcr =>
            {
                if ingroups != 0 {
                    return Err(TeXError::General(
                        "Unbalanced number of braces in alignment\nTODO: Better error message"
                            .to_string(),
                    ));
                }
                engine.push_every(PRIMITIVES.everycr);
                return Ok(cols.build(in_token.clone()));
            }
            Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.span => {
                let t = match engine.mouth.get_next(engine.aux, engine.state)? {
                    Some(t) => t,
                    _ => {
                        TeXError::file_end_while_use(
                            engine.aux,
                            engine.state,
                            engine.mouth,
                            in_token,
                        )?;
                        continue;
                    }
                };
                engine.expand(t)?;
            }
            _ => cols.push(next),
        }
    }
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, in_token)?;
    Ok(cols.build(in_token.clone()))
}

pub(in crate::commands) fn start_align_row<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    mode: BoxType,
) -> TeXResult<(), ET> {
    if let Some(d) = engine.gullet.get_align_data() {
        d.currindex = 0
    } else {
        unreachable!()
    }
    // avoid the gullet here as to not throw an error on '}'!
    while let Some(token) = engine.mouth.get_next(engine.aux, engine.state)? {
        crate::expand!(engine,token;
            ResolvedToken::Tk{code:CommandCode::EndGroup,..} |
            ResolvedToken::Cmd(Some(TeXCommand::Char {code:CommandCode::EndGroup,..})) => {
                engine.gullet.pop_align();
                ET::Stomach::close_align(engine)?;
                return Ok(())
            }
            ResolvedToken::Tk{code:CommandCode::Space,..} => (),
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.crcr => (),
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.noalign => {
                engine.expand_until_bgroup(true,&token)?;
                engine.state.push(engine.aux,GroupType::Noalign,engine.mouth.line_number());
                engine.stomach.data_mut().open_lists.push(
                    match mode {
                        BoxType::Vertical => NodeList::Horizontal {
                            children:Vec::new(),
                            tp:HorizontalNodeListType::Box(HBoxInfo::HAlignRow,engine.mouth.start_ref(),BoxTarget::new(
                                move |engine,l| {
                                    if let TeXBox::H {children,info:HBoxInfo::HAlignRow,..} = l  {
                                        for c in children.into_vec() {
                                            ET::Stomach::add_node_h(engine,c);
                                        }
                                    } else {unreachable!()}
                                    start_align_row(engine,mode)
                                }
                            ))
                        },
                        _ => NodeList::Vertical {
                            children:Vec::new(),
                            tp:VerticalNodeListType::Box(VBoxInfo::VAlignColumn,engine.mouth.start_ref(),BoxTarget::new(
                                move |engine,l| {
                                    if let TeXBox::V {children,info:VBoxInfo::VAlignColumn,..} = l  {
                                        for c in children.into_vec() {
                                            ET::Stomach::add_node_v(engine,c)?;
                                        }
                                    } else {unreachable!()}
                                    start_align_row(engine,mode)
                                }
                            ))
                        }
                    }
                );
                return Ok(())
            }
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.omit => {
                engine.stomach.data_mut().open_lists.push(
                    match mode {
                        BoxType::Vertical => NodeList::Vertical {
                            children:Vec::new(),
                            tp:VerticalNodeListType::VAlignColumn(engine.mouth.start_ref())
                        },
                        _ => NodeList::Horizontal {
                            children:Vec::new(),
                            tp:HorizontalNodeListType::HAlignRow(engine.mouth.start_ref())
                        }
                    }
                );
                engine.gullet.get_align_data().unwrap().omit = true;
                open_align_cell(engine,mode);
                return Ok(())
            }
            _ => {
                engine.stomach.data_mut().open_lists.push(
                    match mode {
                        BoxType::Vertical => NodeList::Vertical {
                            children:Vec::new(),
                            tp:VerticalNodeListType::VAlignColumn(engine.mouth.start_ref())
                        },
                        _ => NodeList::Horizontal {
                            children:Vec::new(),
                            tp:HorizontalNodeListType::HAlignRow(engine.mouth.start_ref())
                        }
                    }
                );
                engine.mouth.requeue(token);
                open_align_cell(engine,mode);
                return Ok(())
            }
        );
    }
    let Some(ad) = engine.gullet.get_align_data() else {
        return Err(TeXError::General("Not in align".to_string()));
    };
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &ad.token)
}

pub(in crate::commands) fn open_align_cell<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    mode: BoxType,
) {
    match engine.gullet.get_align_data() {
        None => unreachable!(),
        Some(data) => {
            if !data.omit {
                engine
                    .mouth
                    .push_slice_rev(&data.columns[data.currindex].left);
            }
            if data.span {
                data.span = false
            } else {
                engine
                    .state
                    .push(engine.aux, GroupType::Align, engine.mouth.line_number());
                engine.stomach.data_mut().open_lists.push(match mode {
                    BoxType::Vertical => NodeList::Vertical {
                        children: vec![],
                        tp: VerticalNodeListType::VAlignCell(engine.mouth.start_ref(), 0),
                    },
                    _ => NodeList::Horizontal {
                        children: vec![],
                        tp: HorizontalNodeListType::HAlignCell(engine.mouth.start_ref(), 0),
                    },
                })
            }
        }
    }
}

pub(in crate::commands) fn pop_align_cell<ET: EngineTypes>(
    state: &mut ET::State,
    aux: &mut EngineAux<ET>,
    stomach: &mut ET::Stomach,
    mouth: &mut ET::Mouth,
    inner_mode: BoxType,
) {
    match inner_mode {
        BoxType::Vertical => pop_align_cell_v(state, aux, stomach, mouth),
        _ => pop_align_cell_h(state, aux, stomach, mouth),
    }
}

fn pop_align_cell_v<ET: EngineTypes>(
    state: &mut ET::State,
    aux: &mut EngineAux<ET>,
    stomach: &mut ET::Stomach,
    mouth: &mut ET::Mouth,
) {
    let (children, start, spans) = match stomach.data_mut().open_lists.pop() {
        Some(NodeList::Vertical {
            children,
            tp: VerticalNodeListType::VAlignCell(start, i),
        }) => (children, start, i),
        _ => unreachable!(),
    };
    state.pop(aux, mouth);
    let bx = TeXBox::V {
        children: children.into(),
        start,
        info: VBoxInfo::VAlignCell { to: None, spans },
        end: mouth.current_sourceref(),
    };
    match stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Vertical {
            children,
            tp: VerticalNodeListType::VAlignColumn(..),
        }) => children.push(VNode::Box(bx)),
        _ => unreachable!(),
    }
}
fn pop_align_cell_h<ET: EngineTypes>(
    state: &mut ET::State,
    aux: &mut EngineAux<ET>,
    stomach: &mut ET::Stomach,
    mouth: &mut ET::Mouth,
) {
    let (children, start, spans) = match stomach.data_mut().open_lists.pop() {
        Some(NodeList::Horizontal {
            children,
            tp: HorizontalNodeListType::HAlignCell(start, i),
        }) => (children, start, i),
        _ => unreachable!(),
    };
    state.pop(aux, mouth);
    let bx = TeXBox::H {
        children: children.into(),
        start,
        info: HBoxInfo::new_cell(spans),
        end: mouth.current_sourceref(),
        preskip: None,
    };
    match stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Horizontal {
            children,
            tp: HorizontalNodeListType::HAlignRow(..),
        }) => children.push(HNode::Box(bx)),
        _ => unreachable!(),
    }
}

pub(in crate::commands) fn pop_align_row<ET: EngineTypes>(
    stomach: &mut ET::Stomach,
    mouth: &mut ET::Mouth,
    inner_mode: BoxType,
) {
    match inner_mode {
        BoxType::Vertical => pop_align_row_v::<ET>(stomach, mouth),
        _ => pop_align_row_h::<ET>(stomach, mouth),
    }
}

fn pop_align_row_v<ET: EngineTypes>(stomach: &mut ET::Stomach, mouth: &mut ET::Mouth) {
    let (children, start) = match stomach.data_mut().open_lists.pop() {
        Some(NodeList::Vertical {
            children,
            tp: VerticalNodeListType::VAlignColumn(start),
        }) => (children, start),
        _ => unreachable!(),
    };
    let bx = TeXBox::V {
        children: children.into(),
        start,
        info: VBoxInfo::VAlignColumn,
        end: mouth.current_sourceref(),
    };
    match stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Horizontal {
            children,
            tp: HorizontalNodeListType::VAlign,
        }) => children.push(HNode::Box(bx)),
        _ => unreachable!(),
    }
}

fn pop_align_row_h<ET: EngineTypes>(stomach: &mut ET::Stomach, mouth: &mut ET::Mouth) {
    let (children, start) = match stomach.data_mut().open_lists.pop() {
        Some(NodeList::Horizontal {
            children,
            tp: HorizontalNodeListType::HAlignRow(start),
        }) => (children, start),
        _ => unreachable!(),
    };
    let bx = TeXBox::H {
        children: children.into(),
        start,
        info: HBoxInfo::HAlignRow,
        end: mouth.current_sourceref(),
        preskip: None,
    };
    match stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Vertical {
            children,
            tp: VerticalNodeListType::HAlign,
        }) => children.push(VNode::Box(bx)),
        _ => unreachable!(),
    }
}

pub(crate) const END_TEMPLATE: &str = "!\"$%&/(endtemplate)\\&%$\"!";
pub(crate) const END_TEMPLATE_ROW: &str = "!\"$%&/(endtemplate_row)\\&%$\"!";

pub(crate) fn un_x<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    v: fn(&VNode<ET>) -> bool,
    h: fn(&HNode<ET>) -> bool,
    m: fn(&MathNode<ET, UnresolvedMathFontStyle<ET>>) -> bool,
) {
    let data = engine.stomach.data_mut();
    match data.open_lists.last_mut() {
        None => (), //todo!("throw error: Not allowed in vertical"), <- not entirely true; if there's contributed stuff not yet migrated to the current page, it's allowed
        Some(NodeList::Vertical { children, .. }) => {
            for (i, n) in children.iter().enumerate().rev() {
                if n.opaque() {
                    continue;
                }
                if v(n) {
                    children.remove(i);
                    return;
                }
                break;
            }
        }
        Some(NodeList::Horizontal { children, .. }) => {
            for (i, n) in children.iter().enumerate().rev() {
                if n.opaque() {
                    continue;
                }
                if h(n) {
                    children.remove(i);
                    return;
                }
                break;
            }
        }
        Some(NodeList::Math { children, .. }) => {
            for (i, n) in children.list().iter().enumerate().rev() {
                if n.opaque() {
                    continue;
                }
                if m(n) {
                    children.list_mut().remove(i);
                    return;
                }
                break;
            }
        }
    }
}

pub(crate) fn last_x<R, ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    v: fn(&VNode<ET>) -> Option<R>,
    h: fn(&HNode<ET>) -> Option<R>,
    m: fn(&MathNode<ET, UnresolvedMathFontStyle<ET>>) -> Option<R>,
) -> Option<R> {
    let data = engine.stomach.data_mut();
    match data.open_lists.last_mut() {
        None => {
            for n in data.page.iter().rev() {
                if n.opaque() {
                    continue;
                }
                return v(n);
            }
        }
        Some(NodeList::Vertical { children, .. }) => {
            for n in children.iter().rev() {
                if n.opaque() {
                    continue;
                }
                return v(n);
            }
        }
        Some(NodeList::Horizontal { children, .. }) => {
            for n in children.iter().rev() {
                if n.opaque() {
                    continue;
                }
                return h(n);
            }
        }
        Some(NodeList::Math { children, .. }) => {
            for n in children.list_mut().iter().rev() {
                if n.opaque() {
                    continue;
                }
                return m(n);
            }
        }
    }
    None
}

pub(crate) fn do_leaders<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tp: LeaderType,
    tk: &ET::Token,
) -> TeXResult<(), ET> {
    match engine.read_keywords(&[b"width", b"height", b"depth"])? {
        Some(_) => {
            return Err(TeXError::General(
                "Not yet implemented: leaders with width/height/depth".to_string(),
            ))
        }
        _ => crate::expand_loop!(engine,token,
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {cmd:PrimitiveCommand::Box(read),..})) => {
                match read(engine,token)? {
                    either::Left(None) => return Err(TeXError::General("Box expected for leaders\nTODO: Better error message".to_string())),
                    either::Left(Some(bx)) => return leaders_skip(engine,LeaderBody::Box(bx),tp,tk),
                    either::Right(bi) => {
                        let tk = tk.clone();
                        let target = BoxTarget::<ET>::new(move |e,b| leaders_skip(e,LeaderBody::Box(b),tp,&tk));
                        let mut ls = bi.open_list(engine.mouth.start_ref());
                        match ls {
                            NodeList::Horizontal {tp:HorizontalNodeListType::Box(_,_,ref mut t),..} => *t = target,
                            NodeList::Vertical {tp:VerticalNodeListType::Box(_,_,ref mut t),..} => *t = target,
                            _ => unreachable!()
                        }
                        engine.stomach.data_mut().open_lists.push(ls);
                        return Ok(())
                    }
                }
            }
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.hrule || *name == PRIMITIVES.vrule => {
                let mut width = None;
                let mut height = None;
                let mut depth = None;
                loop {
                    match engine.read_keywords(&[b"width",b"height",b"depth"])? {
                        Some(b"width") => {
                            width = Some(engine.read_dim(false,tk)?);
                        }
                        Some(b"height") => {
                            height = Some(engine.read_dim(false,tk)?);
                        }
                        Some(b"depth") => {
                            depth = Some(engine.read_dim(false,tk)?);
                        }
                        _ => break
                    }
                }
                return leaders_skip(engine,LeaderBody::Rule {width,height,depth},tp,tk)
            }
            _ => return Err(TeXError::General("Box expected for leaders\nTODO: Better error message".to_string()))
        ),
    }
    Err(TeXError::General(
        "File ended unexpectedly\nTODO: Better error message".to_string(),
    ))
}

fn leaders_skip<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    body: LeaderBody<ET>,
    tp: LeaderType,
    tk: &ET::Token,
) -> TeXResult<(), ET> {
    crate::expand_loop!(engine,token,
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,..})) => {
            let skip = match *name {
                n if n == PRIMITIVES.vskip => LeaderSkip::VSkip(engine.read_skip(false,tk)?),
                n if n == PRIMITIVES.hskip => LeaderSkip::HSkip(engine.read_skip(false,tk)?),
                n if n == PRIMITIVES.vfil => LeaderSkip::VFil,
                n if n == PRIMITIVES.hfil => LeaderSkip::HFil,
                n if n == PRIMITIVES.vfill => LeaderSkip::VFill,
                n if n == PRIMITIVES.hfill => LeaderSkip::HFill,
                _ => return Err(TeXError::General("Glue expected for leaders\nTODO: Better error message".to_string()))
            };
            let leaders = Leaders {skip,body,tp};
            crate::add_node!(ET::Stomach;engine,VNode::Leaders(leaders),HNode::Leaders(leaders),MathNode::Leaders(leaders));
            return Ok(())
        }
        _ => return Err(TeXError::General("Glue expected for leaders\nTODO: Better error message".to_string()))
    );
    Err(TeXError::General(
        "File ended unexpectedly\nTODO: Better error message".to_string(),
    ))
}

pub(crate) fn do_math_class<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    cls: Option<MathClass>,
    in_token: &ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        in_token,
        |(), engine, mc| {
            let mut atom = mc.to_atom();
            if let Some(cls) = cls {
                let MathNucleus::Simple { cls: ocls, .. } = &mut atom.nucleus else {
                    unreachable!()
                };
                *ocls = cls;
            }
            ET::Stomach::add_node_m(engine, MathNode::Atom(atom));
            Ok(())
        },
        move |()| {
            ListTarget::<ET, _>::new(move |engine, mut children, start| {
                if children.len() == 1 {
                    let mut child = children.pop().unwrap_or_else(|| unreachable!());
                    if let Some(cls) = cls {
                        if let MathNode::Atom(MathAtom {
                            sub: None,
                            sup: None,
                            nucleus: MathNucleus::Simple { cls: ocls, .. },
                        }) = &mut child
                        {
                            *ocls = cls;
                        }
                    }
                    ET::Stomach::add_node_m(engine, child);
                    return Ok(());
                }

                let node = MathNode::Atom(MathAtom {
                    sub: None,
                    sup: None,
                    nucleus: match cls {
                        None => MathNucleus::Inner(MathKernel::List {
                            start,
                            children: children.into(),
                            end: engine.mouth.current_sourceref(),
                        }),
                        Some(cls) => MathNucleus::Simple {
                            kernel: MathKernel::List {
                                start,
                                children: children.into(),
                                end: engine.mouth.current_sourceref(),
                            },
                            cls,
                            limits: None,
                        },
                    },
                });
                ET::Stomach::add_node_m(engine, node);
                Ok(())
            })
        },
        (),
    )
}

impl<ET: EngineTypes> EngineReferences<'_, ET> {
    /// expands the [`Token`] if it is expandable, otherwise requeues it
    pub fn expand(&mut self, token: ET::Token) -> TeXResult<(), ET> {
        match self.resolve(&token) {
            ResolvedToken::Cmd(Some(cmd)) => match cmd {
                TeXCommand::Macro(m) => ET::Gullet::do_macro(self, m.clone(), token),
                TeXCommand::Primitive {
                    name,
                    cmd: PrimitiveCommand::Conditional(cond),
                } => ET::Gullet::do_conditional(self, *name, token, *cond, false),
                TeXCommand::Primitive {
                    name,
                    cmd: PrimitiveCommand::Expandable(expand),
                } => ET::Gullet::do_expandable(self, *name, token, *expand),
                TeXCommand::Primitive {
                    name,
                    cmd: PrimitiveCommand::SimpleExpandable(exp),
                } => ET::Gullet::do_simple_expandable(self, *name, token, *exp),
                _ => self.requeue(token),
            },
            _ => self.requeue(token),
        }
    }

    /// reads an integer from the input stream and makes sure it's in the range of
    /// a state register
    pub fn read_register_index(
        &mut self,
        skip_eq: bool,
        in_token: &ET::Token,
    ) -> TeXResult<usize, ET> {
        let idx = self.read_int(skip_eq, in_token)?;
        match ET::State::register_index(idx) {
            Some(idx) => Ok(idx),
            None => Err(TeXError::General(
                "Invalid register index\nTODO: Better error message".to_string(),
            )),
        }
    }
    /// reads an integer and makes sure it's in the range of a math font index (0-15)
    pub fn mathfont_index(&mut self, skip_eq: bool, in_token: &ET::Token) -> TeXResult<u8, ET> {
        let idx = self.read_int(skip_eq, in_token)?.into();
        if !(0..=15).contains(&idx) {
            return Err(TeXError::General(
                "Invalid math font index\nTODO: Better error message".to_string(),
            ));
        }
        Ok(idx as u8)
    }
    /// expects a [`BeginGroup`](CommandCode::BeginGroup) token, reads until the
    /// matching [`EndGroup`](CommandCode::EndGroup) token and discards everything
    /// in between.
    pub fn skip_argument(&mut self, token: &ET::Token) -> TeXResult<(), ET> {
        let t = self.need_next(false, token)?;
        if t.command_code() != CommandCode::BeginGroup {
            TeXError::missing_begingroup(self.aux, self.state, self.mouth)?;
        }
        self.read_until_endgroup(token, |_, _, _| Ok(()))?;
        Ok(())
    }

    /// reads the name of a control sequence until `\endcsname` and returns the
    /// corresponding [`CSName`](crate::tex::tokens::control_sequences::CSName) (i.e. what `\csname` and
    /// `\ifcsname` do)
    pub fn read_csname(&mut self) -> TeXResult<ET::CSName, ET> {
        *self.gullet.csnames() += 1;
        let mut namev = vec![];
        crate::expand_loop!(self,token,
            ResolvedToken::Tk {char,..} => namev.push(char),
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.endcsname => {
                *self.gullet.csnames() -= 1;
                let id = self.aux.memory.cs_interner_mut().cs_from_chars(&namev);
                //engine.aux.memory.return_string(name);
                return Ok(id)
            }
            ResolvedToken::Cmd(_) => return Err(TeXError::General("Unexpandable command in \\csname\nTODO: Better error message".to_string()))
        );
        Err(TeXError::General(
            "File ended while reading \\csname\nTODO: Better error message".to_string(),
        ))
    }
    /// reads a number from the input stream and makes sure it's in the range of
    /// a file input/output stream index (0-255)
    pub fn read_file_index(&mut self, in_token: &ET::Token) -> TeXResult<u8, ET> {
        let idx = self.read_int(false, in_token)?.into();
        if !(0..=255).contains(&idx) {
            return Err(TeXError::General(
                "Invalid file stream index\nTODO: Better error message".to_string(),
            ));
        }
        Ok(idx as u8)
    }

    /// reads a number from the input stream, making sure it's in the range of
    /// a file input/output stream index (0-255), and subsequently reads a filename
    /// - i.e. the first two steps of `\openin` or `\openout`
    pub fn read_filename_and_index(
        &mut self,
        prefix: &str,
        in_token: &ET::Token,
    ) -> TeXResult<(u8, ET::File), ET> {
        let idx = self.read_file_index(in_token)?;
        let mut filename = self.aux.memory.get_string();
        self.read_string(true, &mut filename, in_token)?;
        filename.insert_str(0, prefix);
        let file = self.filesystem.get(&filename);
        self.aux.memory.return_string(filename);
        Ok((idx, file))
    }

    /// `\the`, but using a continuation function; this is used for both [`the`](super::tex::the)
    /// as well as in [`expand_until_endgroup`](Self::expand_until_endgroup)
    /// to speed things up
    pub fn do_the<
        F: FnMut(&mut EngineAux<ET>, &ET::State, &mut ET::Gullet, ET::Token) -> TeXResult<(), ET>,
    >(
        &mut self,
        mut cont: F,
    ) -> TeXResult<(), ET> {
        expand_loop!(self,token,
            ResolvedToken::Cmd(Some(c)) => return c.clone().the(self,token,|a,s,g,t|cont(a,s,g,t).expect("the continuation function should not throw errors on Other characters")),
            _ => return Err(TeXError::General("command expected after \\the\nTODO: Better error message".to_string()))
        );
        Err(TeXError::General(
            "File ended while reading command for \\the\nTODO: Better error message".to_string(),
        ))
    }

    /// reads a [`Delimiter`] from the input stream;
    /// e.g. from `\delimiter` or the `\delcode` of the next character
    pub fn read_opt_delimiter(
        &mut self,
        in_token: &ET::Token,
    ) -> TeXResult<Option<Delimiter<ET>>, ET> {
        crate::expand_loop!(self,token,
            ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..}))  if *name == PRIMITIVES.delimiter => {
                let num = self.read_int(false,in_token)?;
                return Ok(Some(match Delimiter::from_int(num,self.state) {
                    either::Left(d) => d,
                    either::Right((d,i)) => {
                        self.general_error(format!("Bad delimiter code ({})",i))?;
                        d
                    }
                }))
            }
            ResolvedToken::Tk{char,code:CommandCode::Letter|CommandCode::Other,..} => {
                let num = self.state.get_delcode(char);
                if num == ET::Int::default() {return Ok(None)}
                return Ok(Some(match Delimiter::from_int(num,self.state) {
                    either::Left(d) => d,
                    either::Right((d,i)) => {
                        self.general_error(format!("Bad delimiter code ({})",i))?;
                        d
                    }
                }))
            }
            _ => return Err(TeXError::General("Unexpected token for delimiter\nTODO: Better error message".to_string()))
        );
        Err(TeXError::General(
            "File ended while expecting delimiter\nTODO: Better error message".to_string(),
        ))
    }
}
