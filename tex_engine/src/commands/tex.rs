use super::primitives::*;
use crate::commands::methods::{END_TEMPLATE, END_TEMPLATE_ROW, IfxCmd, MacroParser};
use crate::commands::{
    ActiveConditional, CharOrPrimitive, CommandScope, Macro, MacroSignature, PrimitiveCommand,
    ResolvedToken, TeXCommand,
};
use crate::engine::filesystem::{File, FileSystem};
use crate::engine::fontsystem::Font;
use crate::engine::fontsystem::FontSystem;
use crate::engine::gullet::Gullet;
use crate::engine::gullet::methods::CSOrActiveChar;
use crate::engine::mouth::Mouth;
use crate::engine::state::{GroupType, State};
use crate::engine::stomach::methods::SplitResult;
use crate::engine::stomach::{Stomach, TeXMode};
use crate::engine::utils::outputs::Outputs;
use crate::engine::{EngineReferences, EngineTypes, TeXEngine};
use crate::tex::catcodes::{CategoryCode, CommandCode};
use crate::tex::characters::{Character, CharacterMap};
use crate::tex::nodes::boxes::{BoxInfo, BoxType, HBoxInfo, TeXBox, ToOrSpread, VBoxInfo};
use crate::tex::nodes::horizontal::{HNode, HorizontalNodeListType};
use crate::tex::nodes::math::{
    Delimiter, EqNoPosition, MathAtom, MathChar, MathClass, MathKernel, MathNode, MathNodeList,
    MathNodeListType, MathNucleus, UnresolvedMarkers, UnresolvedMathChoice,
    UnresolvedMathFontStyle,
};
use crate::tex::nodes::vertical::{VNode, VerticalNodeListType};
use crate::tex::nodes::{BoxTarget, LeaderType, ListTarget, NodeList, NodeTrait, WhatsitFunction};
use crate::tex::numerics::TeXDimen;
use crate::tex::numerics::{MuSkip, NumSet, Skip};
use crate::tex::tokens::control_sequences::{CSHandler, ResolvedCSName};
use crate::tex::tokens::token_lists::CharWrite;
use crate::tex::tokens::token_lists::Otherize;
use crate::tex::tokens::{StandardToken, Token};
use crate::utils::errors::{TeXError, TeXResult};
use crate::{add_node, expand_loop};
use either::Either;
use std::cmp::Ordering;
use std::fmt::Write;

pub fn accent<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let accent = engine.read_charcode(false, &tk)?;
    crate::expand_loop!(engine,token,
        ResolvedToken::Tk{char,code:CommandCode::Other|CommandCode::Letter} => {
            ET::Stomach::add_node_h(engine,HNode::Accent {accent,char,font:engine.state.get_current_font().clone()});
            return Ok(())
        }
        ResolvedToken::Cmd(Some(TeXCommand::Char {char,code:CommandCode::Other|CommandCode::Letter} | TeXCommand::CharDef(char))) => {
            ET::Stomach::add_node_h(engine,HNode::Accent {accent,char:*char,font:engine.state.get_current_font().clone()});
            return Ok(())
        }
        _ => {
            engine.requeue(token)?;
            ET::Stomach::add_node_h(engine,HNode::AccentChar {accent,font:engine.state.get_current_font().clone()});
            return Ok(())
        }
    );
    engine.general_error("Unexpected token after accent command".to_string())
}

pub fn afterassignment<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let next = engine.need_next(true, &tk)?;
    *engine.stomach.afterassignment() = Some(next);
    Ok(())
}

pub fn aftergroup<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let next = engine.need_next(true, &tk)?;
    engine.state.aftergroup(next);
    Ok(())
}

pub fn begingroup<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.state.push(
        engine.aux,
        GroupType::SemiSimple,
        engine.mouth.line_number(),
    );
    Ok(())
}
pub fn endgroup<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.state.get_group_type() {
        Some(GroupType::SemiSimple) => (),
        _ => TeXError::missing_endgroup(engine.aux, engine.state, engine.mouth)?,
    }
    engine.state.pop(engine.aux, engine.mouth);
    Ok(())
}

pub fn end<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::flush(engine)?;
    engine.mouth.finish();
    Ok(())
}

pub fn discretionary<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.skip_argument(&tk)?;
    engine.skip_argument(&tk)?;
    engine.skip_argument(&tk)
    // TODO
}

pub fn endinput<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.mouth.endinput(engine.aux, engine.state)?;
    Ok(())
}

pub fn errorstopmode<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    Ok(())
}

pub fn expandafter<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let first = engine.need_next(false, &tk)?;
    let second = engine.need_next(false, &tk)?;
    engine.expand(second)?;
    engine.requeue(first)
}

pub fn catcode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let char = engine.read_charcode(false, &tk)?;
    let u: u8 = (*engine.state.get_catcode_scheme().get(char)).into();
    Ok(ET::Int::from(u as i32))
}
pub fn catcode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    let val: i64 = engine.read_int(true, &tk)?.into();
    if !(0..=15).contains(&val) {
        return engine.general_error(format!("Illegal category code {val}"));
    }
    let cc: CategoryCode = (val as u8).try_into().unwrap();
    engine.state.set_catcode(engine.aux, char, cc, globally);
    Ok(())
}

pub fn sfcode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let char = engine.read_charcode(false, &tk)?;
    let u: u16 = engine.state.get_sfcode(char);
    Ok(ET::Int::from(u as i32))
}
pub fn sfcode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    let val: i64 = engine.read_int(true, &tk)?.into();
    if !(0..=32767).contains(&val) {
        return engine.general_error(format!("Illegal spacefactor code {val}"));
    }
    let sf = val as u16;
    engine.state.set_sfcode(engine.aux, char, sf, globally);
    Ok(())
}

pub fn spacefactor_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(ET::Int::from(engine.stomach.data_mut().spacefactor))
}
pub fn spacefactor_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let val = match engine.read_int(true, &tk)?.try_into() {
        Ok(v) => v,
        _ => return engine.general_error("Illegal spacefactor code".to_string()),
    };
    engine.stomach.data_mut().spacefactor = val;
    Ok(())
}

pub fn parshape_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(ET::Int::from(engine.state.get_parshape().len() as i32))
}
pub fn parshape_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let len = engine.read_int(false, &tk)?.into();
    if len < 0 {
        return engine.general_error(format!("Illegal parshape length {len}"));
    }
    let mut shape = Vec::with_capacity(len as usize);
    for _ in 0..len {
        let a = engine.read_dim(false, &tk)?;
        let b = engine.read_dim(false, &tk)?;
        shape.push((a, b))
    }
    engine.state.set_parshape(engine.aux, shape, globally);
    Ok(())
}

pub fn lccode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let char = engine.read_charcode(false, &tk)?;
    let u = engine.state.get_lccode(char).into();
    match ET::Int::try_from(u as i64) {
        Ok(v) => Ok(v),
        _ => {
            engine.general_error(format!("Illegal lowercase code {u}"))?;
            Ok(ET::Int::from(0))
        }
    }
}
pub fn lccode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    let val = engine.read_charcode(true, &tk)?;
    engine.state.set_lccode(engine.aux, char, val, globally);
    Ok(())
}

pub fn uccode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let char = engine.read_charcode(false, &tk)?;
    let u = engine.state.get_uccode(char).into();
    match ET::Int::try_from(u as i64) {
        Ok(v) => Ok(v),
        _ => {
            engine.general_error(format!("Illegal uppercase code {u}"))?;
            Ok(ET::Int::from(0))
        }
    }
}
pub fn uccode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    let val = engine.read_charcode(true, &tk)?;
    engine.state.set_uccode(engine.aux, char, val, globally);
    Ok(())
}

pub fn mathcode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let char = engine.read_charcode(false, &tk)?;
    let u = engine.state.get_mathcode(char);
    match ET::Int::try_from(u as i64) {
        Ok(v) => Ok(v),
        _ => {
            engine.general_error(format!("Illegal mathcode {u}"))?;
            Ok(ET::Int::from(0))
        }
    }
}
pub fn mathcode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    let mut val = engine.read_int(true, &tk)?.into();
    if val < 0 || val > 32768 {
        engine.general_error(format!("Illegal mathcode {val}"))?;
        val = 0;
    }
    engine
        .state
        .set_mathcode(engine.aux, char, val as u32, globally);
    Ok(())
}

pub fn delcode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let char = engine.read_charcode(false, &tk)?;
    Ok(engine.state.get_delcode(char))
}
pub fn delcode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    let val = engine.read_int(true, &tk)?;
    engine.state.set_delcode(engine.aux, char, val, globally);
    Ok(())
}

pub fn chardef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let name = engine.read_control_sequence(&tk)?;
    let char = engine.read_charcode(true, &tk)?;
    let cmd = TeXCommand::CharDef(char);
    engine.set_command(&name, Some(cmd), globally);
    Ok(())
}

pub fn r#char<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let char = engine.read_charcode(false, &tk)?;
    match engine.stomach.data_mut().mode() {
        TeXMode::DisplayMath | TeXMode::InlineMath => ET::Stomach::do_char_in_math(engine, char),
        _ => {
            let tk = <ET::Token as Token>::from_char_cat(char, CommandCode::Other);
            ET::Stomach::do_char(engine, tk, char, CommandCode::Other)
        }
    }
}

pub fn csname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let name = engine.read_csname()?;
    if engine.state.get_command(&name).is_none() {
        engine.state.set_command(
            engine.aux,
            name.clone(),
            Some(TeXCommand::Primitive {
                name: PRIMITIVES.relax,
                cmd: PrimitiveCommand::Relax,
            }),
            false,
        )
    }
    engine.mouth.requeue(ET::Token::from_cs(name));
    Ok(())
}

pub fn endcsname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.general_error("Unexpected `\\endcsname`".to_string())
}

pub fn count_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(engine.state.get_int_register(idx))
}
pub fn count_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let val = engine.read_int(true, &tk)?;
    engine
        .state
        .set_int_register(engine.aux, idx, val, globally);
    Ok(())
}

pub fn dimen_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(engine.state.get_dim_register(idx))
}
pub fn dimen_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let val = engine.read_dim(true, &tk)?;
    engine
        .state
        .set_dim_register(engine.aux, idx, val, globally);
    Ok(())
}

pub fn skip_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Skip<ET::Dim>, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(engine.state.get_skip_register(idx))
}
pub fn skip_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let val = engine.read_skip(true, &tk)?;
    engine
        .state
        .set_skip_register(engine.aux, idx, val, globally);
    Ok(())
}

pub fn muskip_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<MuSkip<ET::MuDim>, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(engine.state.get_muskip_register(idx))
}
pub fn muskip_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let val = engine.read_muskip(true, &tk)?;
    engine
        .state
        .set_muskip_register(engine.aux, idx, val, globally);
    Ok(())
}

pub fn countdef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let name = engine.read_control_sequence(&tk)?;
    let i = engine.read_register_index(true, &tk)?;
    let cmd = TeXCommand::IntRegister(i);
    engine.set_command(&name, Some(cmd), globally);
    Ok(())
}

pub fn dimendef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let name = engine.read_control_sequence(&tk)?;
    let i = engine.read_register_index(true, &tk)?;
    let cmd = TeXCommand::DimRegister(i);
    engine.set_command(&name, Some(cmd), globally);
    Ok(())
}

pub fn skipdef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let name = engine.read_control_sequence(&tk)?;
    let i = engine.read_register_index(true, &tk)?;
    let cmd = TeXCommand::SkipRegister(i);
    engine.set_command(&name, Some(cmd), globally);
    Ok(())
}

pub fn muskipdef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let name = engine.read_control_sequence(&tk)?;
    let i = engine.read_register_index(true, &tk)?;
    let cmd = TeXCommand::MuSkipRegister(i);
    engine.set_command(&name, Some(cmd), globally);
    Ok(())
}

pub fn toksdef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let name = engine.read_control_sequence(&tk)?;
    let i = engine.read_register_index(true, &tk)?;
    let cmd = TeXCommand::ToksRegister(i);
    engine.set_command(&name, Some(cmd), globally);
    Ok(())
}

pub fn def<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    long: bool,
    protected: bool,
    globally: bool,
) -> TeXResult<(), ET> {
    let cm = loop {
        let t = engine.need_next(false, &tk)?;
        match t.to_enum() {
            StandardToken::Character(c, CommandCode::Active) => break CSOrActiveChar::Active(c),
            StandardToken::Character(_, CommandCode::Space) => (),
            StandardToken::ControlSequence(cs) => break CSOrActiveChar::Name(cs),
            _ => {
                return engine.general_error("Expected control sequence after `\\def`".to_string());
            }
        }
    };
    let mut parser = MacroParser::new();
    engine.iterate(
        |_, _, t| parser.do_signature_token(t),
        |_, _, _| {
            Err(TeXError::General(
                "Unexpected Token in `\\def`-signature".to_string(),
            ))
        },
    )?;
    engine.read_until_endgroup(&tk, |_, _, t| parser.do_expansion_token(t))?;
    let cmd = parser.close(long, outer, protected);
    engine.set_command(&cm, Some(TeXCommand::Macro(cmd)), globally);
    Ok(())
}

pub fn edef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    long: bool,
    protected: bool,
    globally: bool,
) -> TeXResult<(), ET> {
    let cm = loop {
        let t = engine.need_next(false, &tk)?;
        match t.to_enum() {
            StandardToken::Character(c, CommandCode::Active) => break CSOrActiveChar::Active(c),
            StandardToken::Character(_, CommandCode::Space) => (),
            StandardToken::ControlSequence(cs) => break CSOrActiveChar::Name(cs),
            _ => {
                return engine
                    .general_error("Expected control sequence after `\\edef`".to_string());
            }
        }
    };

    let mut parser = MacroParser::new();
    engine.iterate(
        |_, _, t| parser.do_signature_token(t),
        |_, _, _| {
            Err(TeXError::General(
                "Unexpected token in `\\edef`-signature".to_string(),
            ))
        },
    )?;
    engine.expand_until_endgroup(false, true, &tk, |_, _, t| parser.do_expansion_token(t))?;
    let cmd = parser.close(long, outer, protected);
    engine.set_command(&cm, Some(TeXCommand::Macro(cmd)), globally);
    Ok(())
}

pub fn xdef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    long: bool,
    protected: bool,
    _globally: bool,
) -> TeXResult<(), ET> {
    edef(engine, tk, outer, long, protected, true)
}

pub fn gdef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    long: bool,
    protected: bool,
    _globally: bool,
) -> TeXResult<(), ET> {
    def(engine, tk, outer, long, protected, true)
}

pub fn dp_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(match engine.state.get_box_register(idx) {
        None => ET::Dim::default(),
        Some(b) => b.depth(),
    })
}
pub fn dp_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let dim = engine.read_dim(true, &tk)?;
    if let Some(b) = engine.state.get_box_register_mut(idx) {
        b.assign_depth(dim)
    }
    Ok(())
}

pub fn ht_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(match engine.state.get_box_register(idx) {
        None => ET::Dim::default(),
        Some(b) => b.height(),
    })
}
pub fn ht_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let dim = engine.read_dim(true, &tk)?;
    if let Some(b) = engine.state.get_box_register_mut(idx) {
        b.assign_height(dim)
    }
    Ok(())
}

pub fn wd_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(match engine.state.get_box_register(idx) {
        None => ET::Dim::default(),
        Some(b) => b.width(),
    })
}
pub fn wd_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let dim = engine.read_dim(true, &tk)?;
    if let Some(b) = engine.state.get_box_register_mut(idx) {
        b.assign_width(dim)
    }
    Ok(())
}

pub fn global<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    long: bool,
    protected: bool,
    _globally: bool,
) -> TeXResult<(), ET> {
    let allow_others = !outer && !long && !protected;
    crate::expand_loop!(engine,token,
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,cmd:PrimitiveCommand::Assignment(a)})) => match *name {
            n if n == PRIMITIVES.outer => return self::outer(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.long => return self::long(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.protected => return super::etex::protected(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.global => return self::global(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.def => return self::def(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.edef => return self::edef(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.xdef => return self::xdef(engine,token,outer,long,protected,true),
            n if n == PRIMITIVES.gdef => return self::gdef(engine,token,outer,long,protected,true),
            n if allow_others => return ET::Stomach::do_assignment(engine,n,token,*a,true),
            _ => return engine.general_error(format!("Illegal command after `\\global`: {}",name.display::<ET::Char>(Some(b'\\'.into()))))
        }
        ResolvedToken::Cmd(Some(TeXCommand::IntRegister(u))) if allow_others =>
            return ET::Stomach::assign_int_register(engine,*u,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveInt})) if allow_others =>
            return ET::Stomach::assign_primitive_int(engine,*name,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Int {assign:Some(f),..},name})) if allow_others =>
            return ET::Stomach::do_assignment(engine,*name,token,*f,true),
        ResolvedToken::Cmd(Some(TeXCommand::DimRegister(u))) if allow_others =>
            return ET::Stomach::assign_dim_register(engine,*u,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveDim})) if allow_others =>
            return ET::Stomach::assign_primitive_dim(engine,*name,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Dim {assign:Some(f),..},name})) if allow_others =>
            return ET::Stomach::do_assignment(engine,*name,token,*f,true),
        ResolvedToken::Cmd(Some(TeXCommand::SkipRegister(u))) if allow_others =>
            return ET::Stomach::assign_skip_register(engine,*u,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveSkip})) if allow_others =>
            return ET::Stomach::assign_primitive_skip(engine,*name,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Skip {assign:Some(f),..},name})) if allow_others =>
            return ET::Stomach::do_assignment(engine,*name,token,*f,true),
        ResolvedToken::Cmd(Some(TeXCommand::MuSkipRegister(u))) if allow_others =>
            return ET::Stomach::assign_muskip_register(engine,*u,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveMuSkip})) if allow_others =>
            return ET::Stomach::assign_primitive_muskip(engine,*name,true,tk),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::MuSkip {assign:Some(f),..},name})) if allow_others =>
            return ET::Stomach::do_assignment(engine,*name,token,*f,true),
        ResolvedToken::Cmd(Some(TeXCommand::ToksRegister(u))) if allow_others =>
            return ET::Stomach::assign_toks_register(engine,token,*u,true),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveToks})) if allow_others =>
            return ET::Stomach::assign_primitive_toks(engine,token,*name,true),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::FontCmd {assign:Some(f),..},name})) if allow_others =>
            return ET::Stomach::do_assignment(engine,*name,token,*f,true),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Whatsit {get,..},name})) if allow_others =>
            return ET::Stomach::do_whatsit(engine,*name,token,*get),
        ResolvedToken::Cmd(Some(TeXCommand::Primitive {cmd:PrimitiveCommand::Relax,..})) => (),
        ResolvedToken::Cmd(Some(_)) => {
            let s = token.display(engine.aux.memory.cs_interner(),engine.state.get_catcode_scheme(),engine.state.get_escape_char()).to_string();
            return engine.general_error(format!("Illegal command after `\\global`: {s}"))
        }
        _ => {
            let s = token.display(engine.aux.memory.cs_interner(),engine.state.get_catcode_scheme(),engine.state.get_escape_char()).to_string();
            return engine.general_error(format!("Illegal command after `\\global`: {s}"))
        }
    );
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &tk)
}
pub fn outer<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _outer: bool,
    long: bool,
    protected: bool,
    globally: bool,
) -> TeXResult<(), ET> {
    crate::expand_loop!(engine,token,
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,..})) => match *name {
            n if n == PRIMITIVES.outer => return self::outer(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.long => return self::long(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.protected => return super::etex::protected(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.global => return self::global(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.def => return self::def(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.edef => return self::edef(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.xdef => return self::xdef(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.gdef => return self::gdef(engine,token,true,long,protected,globally),
            n if n == PRIMITIVES.relax => (),
            n => {
                let s = n.display(engine.state.get_escape_char()).to_string();
                engine.requeue(token)?;
                return Err(TeXError::General(format!("You can't use a prefix with '{}'",s)))
            }
        }
        _ => {
            let s = token.display(engine.aux.memory.cs_interner(),engine.state.get_catcode_scheme(),engine.state.get_escape_char()).to_string();
            engine.requeue(token)?;
            return Err(TeXError::General(format!("You can't use a prefix with '{}'",s)))
        }
    );
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &tk)
}

pub fn long<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    _long: bool,
    protected: bool,
    globally: bool,
) -> TeXResult<(), ET> {
    crate::expand_loop!(engine,token,
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,..})) => match *name {
            n if n == PRIMITIVES.outer => return self::outer(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.long => return self::long(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.protected => return super::etex::protected(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.global => return self::global(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.def => return self::def(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.edef => return self::edef(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.xdef => return self::xdef(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.gdef => return self::gdef(engine,token,outer,true,protected,globally),
            n if n == PRIMITIVES.relax => (),
            n => {
                let s = n.display(engine.state.get_escape_char()).to_string();
                engine.requeue(token)?;
                return Err(TeXError::General(format!("You can't use a prefix with '{}'",s)))
            }
        }
        _ => {
            let s = token.display(engine.aux.memory.cs_interner(),engine.state.get_catcode_scheme(),engine.state.get_escape_char()).to_string();
            engine.requeue(token)?;
            return Err(TeXError::General(format!("You can't use a prefix with '{}'",s)))
        }
    );
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &tk)
}

macro_rules! modify_num {
    ($engine:ident,$globally:ident,$int:expr,$dim:expr,$skip:expr) => {
        crate::expand_loop!($engine,token,
            ResolvedToken::Cmd(Some(cm)) => match cm {
                TeXCommand::Primitive{name,..} if *name == PRIMITIVES.count => {
                    let idx = $engine.read_register_index(false,&token)?;
                    return crate::commands::methods::modify_int_register($engine,idx,$globally,$int)
                }
                TeXCommand::IntRegister(idx) => {
                    return crate::commands::methods::modify_int_register($engine,*idx,$globally,$int)
                }
                TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveInt} => {
                    return crate::commands::methods::modify_primitive_int($engine,*name,$globally,$int)
                }
                TeXCommand::Primitive{name,..} if *name == PRIMITIVES.dimen => {
                    let idx = $engine.read_register_index(false,&token)?;
                    return crate::commands::methods::modify_dim_register($engine,idx,$globally,$dim)
                }
                TeXCommand::DimRegister(idx) => {
                    return crate::commands::methods::modify_dim_register($engine,*idx,$globally,$dim)
                }
                TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveDim} => {
                    return crate::commands::methods::modify_primitive_dim($engine,*name,$globally,$dim)
                }
                TeXCommand::Primitive{name,..} if *name == PRIMITIVES.skip => {
                    let idx = $engine.read_register_index(false,&token)?;
                    return crate::commands::methods::modify_skip_register($engine,idx,$globally,$skip)
                }
                TeXCommand::SkipRegister(idx) => {
                    return crate::commands::methods::modify_skip_register($engine,*idx,$globally,$skip)
                }
                TeXCommand::Primitive{name,cmd:PrimitiveCommand::PrimitiveSkip} => {
                    return crate::commands::methods::modify_primitive_skip($engine,*name,$globally,$skip)
                }
                _ => {
                    let s = token.display($engine.aux.memory.cs_interner(),$engine.state.get_catcode_scheme(),$engine.state.get_escape_char()).to_string();
                    $engine.requeue(token)?;
                    return Err(TeXError::General(format!("Unexpected token in \\divide/\\multiply: {s}")))
                }
            }
            _ => {
                let s = token.display($engine.aux.memory.cs_interner(),$engine.state.get_catcode_scheme(),$engine.state.get_escape_char()).to_string();
                $engine.requeue(token)?;
                return Err(TeXError::General(format!("Unexpected token in \\divide/\\multiply: {s}")))
            }
        );
    };
}

pub fn advance<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    modify_num!(
        engine,
        globally,
        |a, e| Ok(a + e.read_int(false, &tk)?),
        |a, e| Ok(a + e.read_dim(false, &tk)?),
        |a, e| Ok(a + e.read_skip(false, &tk)?)
    );
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &tk)
}
pub fn divide<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    modify_num!(
        engine,
        globally,
        |a, e| {
            let b = e.read_int(false, &tk)?;
            if b == ET::Int::default() {
                return Err(TeXError::General("Division by zero".to_string()));
            }
            Ok(a / b)
        },
        |a, e| {
            let b = e.read_int(false, &tk)?;
            if b == ET::Int::default() {
                return Err(TeXError::General("Division by zero".to_string()));
            }
            Ok(a / b)
        },
        |a, e| {
            let b = e.read_int(false, &tk)?;
            if b == ET::Int::default() {
                return Err(TeXError::General("Division by zero".to_string()));
            }
            Ok(a / b)
        }
    );
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &tk)
}
pub fn multiply<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    modify_num!(
        engine,
        globally,
        |a, e| Ok(a * e.read_int(false, &tk)?),
        |a, e| {
            let b = e.read_int(false, &tk)?;
            Ok(a * b)
        },
        |a, e| {
            let b = e.read_int(false, &tk)?;
            Ok(a * b)
        }
    );
    TeXError::file_end_while_use(engine.aux, engine.state, engine.mouth, &tk)
}

pub fn r#else<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let conds = engine.gullet.get_conditionals();
    let name = match conds.pop() {
        Some(ActiveConditional::True(id)) => {
            conds.push(ActiveConditional::Else(id));
            id
        }
        Some(c @ ActiveConditional::Case(_)) => {
            conds.push(c);
            PRIMITIVES.ifcase
        }
        Some(u @ ActiveConditional::Unfinished(_)) => {
            conds.push(u);
            engine.mouth.requeue(tk);
            let relax = engine.aux.memory.cs_interner_mut().cs_from_str("relax");
            engine.mouth.requeue(ET::Token::from_cs(relax));
            return Ok(());
        }
        Some(ActiveConditional::Else(_)) => {
            return Err(TeXError::General(
                "Unexpected `\\else` in `\\else`-branch".to_string(),
            ));
        }
        _ => {
            return Err(TeXError::General(
                "Unexpected `\\else` outside of a condition".to_string(),
            ));
        }
    };
    let trace = engine.state.get_primitive_int(PRIMITIVES.tracingifs) > ET::Int::default();
    let index = conds.len();
    if trace {
        engine.aux.outputs.write_neg1(format_args!(
            "{{{}else: {} (level {}) entered on line {}}}",
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            name.display(engine.state.get_escape_char()),
            index,
            engine.mouth.line_number()
        ));
    }
    crate::engine::gullet::methods::false_loop(engine, index, false, false)?;
    if trace {
        engine.aux.outputs.write_neg1(format_args!(
            "{{{}fi: {} (level {}) entered on line {}}}",
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            name.display(engine.state.get_escape_char()),
            index,
            engine.mouth.line_number()
        ));
    }
    Ok(())
}

pub fn or<ET: EngineTypes>(engine: &mut EngineReferences<ET>, tk: ET::Token) -> TeXResult<(), ET> {
    let conds = engine.gullet.get_conditionals();
    match conds.pop() {
        Some(ActiveConditional::Case(_)) => {
            conds.push(ActiveConditional::Else(PRIMITIVES.ifcase));
        }
        Some(u @ ActiveConditional::Unfinished(_)) => {
            conds.push(u);
            engine.mouth.requeue(tk);
            let relax = engine.aux.memory.cs_interner_mut().cs_from_str("relax");
            engine.mouth.requeue(ET::Token::from_cs(relax));
            return Ok(());
        }
        _ => return engine.general_error("`\\or` outside of `\\case`".to_string()),
    };
    let trace = engine.state.get_primitive_int(PRIMITIVES.tracingifs) > ET::Int::default();
    let index = conds.len();
    if trace {
        engine.aux.outputs.write_neg1(format_args!(
            "{{{}or: {}ifcase (level {}) entered on line {}}}",
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            index,
            engine.mouth.line_number()
        ));
    }
    crate::engine::gullet::methods::false_loop(engine, index, false, true)?;
    if trace {
        engine.aux.outputs.write_neg1(format_args!(
            "{{{}fi: {}ifcase (level {}) entered on line {}}}",
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            index,
            engine.mouth.line_number()
        ));
    }
    Ok(())
}

pub fn endlinechar_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(ET::Int::from(match engine.state.get_endline_char() {
        Some(c) => c.into() as i32,
        _ => -1,
    }))
}
pub fn endlinechar_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let val: i64 = engine.read_int(true, &tk)?.into();
    let val = match val.cmp(&-1) {
        Ordering::Less => return engine.general_error(format!("Illegal endline character: {val}")),
        Ordering::Equal => None,
        Ordering::Greater => match ET::Char::try_from(val as u64) {
            Ok(c) => Some(c),
            _ => return engine.general_error(format!("Illegal endline character: {val}")),
        },
    };
    engine.state.set_endline_char(engine.aux, val, globally);
    Ok(())
}

pub fn escapechar_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(ET::Int::from(match engine.state.get_escape_char() {
        Some(c) => c.into() as i32,
        _ => -1,
    }))
}
pub fn escapechar_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let val: i64 = engine.read_int(true, &tk)?.into();
    let val = match val.cmp(&-1) {
        Ordering::Less => return engine.general_error(format!("Illegal escape character: {val}")),
        Ordering::Equal => None,
        Ordering::Greater => match ET::Char::try_from(val as u64) {
            Ok(c) => Some(c),
            _ => return engine.general_error(format!("Illegal escape character: {val}")),
        },
    };
    engine.state.set_escape_char(engine.aux, val, globally);
    Ok(())
}

pub fn font_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Font, ET> {
    Ok(engine.state.get_current_font().clone())
}

pub fn font_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    global: bool,
) -> TeXResult<(), ET> {
    let cs = match engine.read_control_sequence(&tk)? {
        CSOrActiveChar::Name(name) => name,
        _ => return engine.general_error("control sequence expected after \\font".to_string()),
    };
    let mut name = engine.aux.memory.get_string();
    engine.read_string(true, &mut name, &tk)?;
    let mut font = engine
        .fontsystem
        .new_font(&name, cs.clone(), engine.filesystem);
    engine.aux.memory.return_string(name);
    match engine.read_keywords(&[b"at", b"scaled"])? {
        Some(b"at") => {
            let size = engine.read_dim(false, &tk)?;
            font.set_at(size);
        }
        Some(b"scaled") => {
            let i = engine.read_int(false, &tk)?.into();
            let at = font.get_at();
            font.set_at(at.scale_float(i as f64 / 1000.0));
        }
        _ => (),
    }
    engine
        .state
        .set_command(engine.aux, cs, Some(TeXCommand::Font(font)), global);
    Ok(())
}

pub fn textfont_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Font, ET> {
    let num = engine.mathfont_index(false, &tk)?;
    Ok(engine.state.get_textfont(num).clone())
}

pub fn textfont_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    global: bool,
) -> TeXResult<(), ET> {
    let num = engine.mathfont_index(false, &tk)?;
    let fnt = engine.read_font(true, &tk)?;
    engine.state.set_textfont(engine.aux, num, fnt, global);
    Ok(())
}

pub fn scriptfont_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Font, ET> {
    let num = engine.mathfont_index(false, &tk)?;
    Ok(engine.state.get_scriptfont(num).clone())
}

pub fn scriptfont_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    global: bool,
) -> TeXResult<(), ET> {
    let num = engine.mathfont_index(false, &tk)?;
    let fnt = engine.read_font(true, &tk)?;
    engine.state.set_scriptfont(engine.aux, num, fnt, global);
    Ok(())
}

pub fn scriptscriptfont_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Font, ET> {
    let num = engine.mathfont_index(false, &tk)?;
    Ok(engine.state.get_scriptscriptfont(num).clone())
}

pub fn scriptscriptfont_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    global: bool,
) -> TeXResult<(), ET> {
    let num = engine.mathfont_index(false, &tk)?;
    let fnt = engine.read_font(true, &tk)?;
    engine
        .state
        .set_scriptscriptfont(engine.aux, num, fnt, global);
    Ok(())
}

pub fn fontdimen_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let idx = match engine.read_int(false, &tk)?.try_into() {
        Ok(i) if i > 0 && i - 1 <= u16::MAX.into() => (i - 1) as u16,
        _ => {
            engine.general_error("Illegal \\fontdimen index".to_string())?;
            return Ok(ET::Dim::default());
        }
    };
    let font = engine.read_font(false, &tk)?;
    Ok(font.get_dim(idx))
}
pub fn fontdimen_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?;
    let idx = match i.try_into() {
        Ok(i) if i > 0 && i - 1 <= u16::MAX.into() => (i - 1) as u16,
        _ => return engine.general_error("Illegal \\fontdimen index".to_string()),
    };
    let mut font = engine.read_font(false, &tk)?;
    let dim = engine.read_dim(true, &tk)?;
    font.set_dim(idx, dim);
    Ok(())
}

pub fn box_<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(either::Left(engine.state.take_box_register(idx)))
}
pub fn copy<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(either::Left(engine.state.get_box_register(idx).cloned()))
}

pub fn unbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    tp: BoxType,
    copy: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let bx = if copy {
        engine.state.get_box_register(idx).cloned()
    } else {
        engine.state.take_box_register(idx)
    };
    match bx {
        None => (),
        Some(TeXBox::V { children, .. }) if tp == BoxType::Vertical => {
            for c in children.into_vec() {
                ET::Stomach::add_node_v(engine, c)?
            }
        }
        Some(TeXBox::H { children, .. }) if tp == BoxType::Horizontal => {
            match engine.stomach.data_mut().open_lists.last_mut() {
                Some(NodeList::Horizontal { children: ls, .. }) => ls.extend(children.into_vec()),
                _ => {
                    return engine.general_error(
                        "Cannot unbox \\hbox outside of horizontal mode".to_string(),
                    );
                }
            }
        }
        _ => return engine.general_error(format!("Cannot unbox box {idx} in {tp} mode")),
    }
    Ok(())
}

pub fn unhbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    unbox(engine, tk, BoxType::Horizontal, false)
}
pub fn unhcopy<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    unbox(engine, tk, BoxType::Horizontal, true)
}
pub fn unvbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    unbox(engine, tk, BoxType::Vertical, false)
}
pub fn unvcopy<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    unbox(engine, tk, BoxType::Vertical, true)
}

pub fn hbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    let scaled = super::methods::do_box_start(engine, GroupType::HBox, PRIMITIVES.everyhbox, &tk)?;
    Ok(either::Right(BoxInfo::H(HBoxInfo::new_box(scaled))))
}

pub fn vbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    let scaled = super::methods::do_box_start(engine, GroupType::VBox, PRIMITIVES.everyvbox, &tk)?;
    Ok(either::Right(BoxInfo::V(VBoxInfo::new_box(scaled))))
}

pub fn vtop<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    let scaled = super::methods::do_box_start(engine, GroupType::VTop, PRIMITIVES.everyvbox, &tk)?;
    Ok(either::Right(BoxInfo::V(VBoxInfo::new_top(scaled))))
}

pub fn vcenter<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let scaled =
        super::methods::do_box_start(engine, GroupType::VCenter, PRIMITIVES.everyvbox, &tk)?;
    engine
        .stomach
        .data_mut()
        .open_lists
        .push(NodeList::Vertical {
            children: Vec::new(),
            tp: VerticalNodeListType::VCenter(engine.mouth.start_ref(), scaled),
        });
    Ok(())
}

pub fn halign<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.stomach.data_mut().mode() {
        TeXMode::Horizontal => {
            engine.requeue(tk)?;
            return ET::Stomach::close_paragraph(engine);
        }
        TeXMode::DisplayMath | TeXMode::Vertical | TeXMode::InternalVertical => (),
        _ => return engine.general_error("\\halign not allowed in math mode".to_string()),
    }
    let wd = if engine.read_keyword(b"to")? {
        Some(engine.read_dim(false, &tk)?)
    } else {
        None
    };
    super::methods::do_align(engine, BoxType::Horizontal, BoxType::Vertical, wd, &tk)
}

pub fn valign<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let wd = if engine.read_keyword(b"to")? {
        Some(engine.read_dim(false, &tk)?)
    } else {
        None
    };
    super::methods::do_align(engine, BoxType::Vertical, BoxType::Horizontal, wd, &tk)
}

pub fn hyphenchar_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let font = engine.read_font(false, &tk)?;
    Ok(font.get_hyphenchar())
}
pub fn hyphenchar_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let mut font = engine.read_font(false, &tk)?;
    let val = engine.read_int(true, &tk)?;
    font.set_hyphenchar(val);
    Ok(())
}

pub fn skewchar_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let font = engine.read_font(false, &tk)?;
    Ok(font.get_skewchar())
}
pub fn skewchar_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let mut font = engine.read_font(false, &tk)?;
    let val = engine.read_int(true, &tk)?;
    font.set_skewchar(val);
    Ok(())
}

pub fn r#if<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let first = super::methods::get_if_token(engine, &tk)?;
    let second = super::methods::get_if_token(engine, &tk)?;
    Ok(first.0 == second.0)
}

pub fn ifcase<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let index = engine.gullet.get_conditionals().len() - 1;
    let num = engine.read_int(false, &tk)?;
    *engine.gullet.get_conditionals().get_mut(index).unwrap() = ActiveConditional::Case(num);
    Ok(num == <ET::Num as NumSet>::Int::default())
}

pub fn ifcat<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let first = super::methods::get_if_token(engine, &tk)?;
    let second = super::methods::get_if_token(engine, &tk)?;
    Ok(first.1 == second.1)
}

pub fn ifdim<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let first = engine.read_dim(false, &tk)?;
    let rel = match engine.read_chars(b"=<>")? {
        either::Left(b) => b,
        _ => {
            TeXError::missing_keyword(engine.aux, engine.state, engine.mouth, &["=", "<", ">"])?;
            b'='
        }
    };
    let second = engine.read_dim(false, &tk)?;
    Ok(match rel {
        b'=' => first == second,
        b'<' => first < second,
        b'>' => first > second,
        _ => unreachable!(),
    })
}

pub fn ifeof<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let idx = engine.read_file_index(&tk)?;
    Ok(engine.filesystem.eof(idx))
}

pub fn ifhmode<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(matches!(
        engine.stomach.data_mut().mode(),
        TeXMode::Horizontal | TeXMode::RestrictedHorizontal
    ))
}
pub fn ifinner<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(matches!(
        engine.stomach.data_mut().mode(),
        TeXMode::RestrictedHorizontal | TeXMode::InternalVertical | TeXMode::InlineMath
    ))
}
pub fn ifmmode<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(matches!(
        engine.stomach.data_mut().mode(),
        TeXMode::InlineMath | TeXMode::DisplayMath
    ))
}

pub fn ifnum<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let first = engine.read_int(false, &tk)?;
    let rel = match engine.read_chars(b"=<>")? {
        either::Left(b) => b,
        _ => {
            TeXError::missing_keyword(engine.aux, engine.state, engine.mouth, &["=", "<", ">"])?;
            b'='
        }
    };
    let second = engine.read_int(false, &tk)?;
    Ok(match rel {
        b'=' => first == second,
        b'<' => first < second,
        b'>' => first > second,
        _ => unreachable!(),
    })
}

pub fn ifodd<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(engine.read_int(false, &tk)?.into() % 2 != 0)
}

pub fn iftrue<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(true)
}

pub fn iffalse<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(false)
}

pub fn ifvmode<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(matches!(
        engine.stomach.data_mut().mode(),
        TeXMode::Vertical | TeXMode::InternalVertical
    ))
}
pub fn ifvbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(matches!(
        engine.state.get_box_register(idx),
        Some(TeXBox::V { .. })
    ))
}
pub fn ifvoid<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(engine.state.get_box_register(idx).is_none())
}
pub fn ifhbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    Ok(matches!(
        engine.state.get_box_register(idx),
        Some(TeXBox::H { .. })
    ))
}

pub fn ifx<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let first = IfxCmd::read(engine, &tk)?;
    let second = IfxCmd::read(engine, &tk)?;
    Ok(first == second)
}

pub fn ignorespaces<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    while let Some(next) = engine.mouth.get_next(engine.aux, engine.state)? {
        if next.command_code() != CommandCode::Space {
            match ET::Gullet::char_or_primitive(engine.state, &next) {
                Some(CharOrPrimitive::Char(_, Some(CommandCode::Space))) => (),
                _ => {
                    engine.mouth.requeue(next);
                    break;
                }
            }
        }
    }
    Ok(())
}

pub fn insert<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let n = engine.read_int(false, &tk)?.into();
    if n < 0 {
        return engine.general_error(format!("Illegal insert index {n}"));
    }
    engine.expand_until_bgroup(false, &tk)?;
    engine
        .state
        .push(engine.aux, GroupType::Insert, engine.mouth.line_number());
    engine
        .stomach
        .data_mut()
        .open_lists
        .push(NodeList::Vertical {
            children: vec![],
            tp: VerticalNodeListType::Insert(n as usize),
        });
    Ok(())
}

pub fn immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    expand_loop!(engine,token,
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{cmd:PrimitiveCommand::Whatsit { immediate,.. },..})) => {
            return immediate(engine,token)
        },
        ResolvedToken::Tk {char,code} => return ET::Stomach::do_char(engine,token,char,code),
        ResolvedToken::Cmd(Some(TeXCommand::Char {char,code})) => return ET::Stomach::do_char(engine,token,*char,*code),
        ResolvedToken::Cmd(None) =>
            return TeXError::undefined(engine.aux,engine.state,engine.mouth,&token),
        ResolvedToken::Cmd(Some(c)) => {
            crate::do_cmd!(engine,token,c);
            return Ok(())
        }
    );
    Ok(())
}

pub fn input<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut filename = engine.aux.memory.get_string();
    engine.read_string(false, &mut filename, &tk)?;
    if filename.is_empty() {
        return engine.general_error("empty file name in \\input".to_string());
    }
    let is_file = !filename.starts_with('|');
    let file = engine.filesystem.get(&filename);
    engine.aux.memory.return_string(filename);
    if is_file && !file.exists() {
        return engine.general_error(format!("File does not exist: {}", file.path().display()));
    }
    engine.aux.outputs.file_open(&file);
    engine.push_file(file);
    Ok(())
}

pub fn fi<ET: EngineTypes>(engine: &mut EngineReferences<ET>, tk: ET::Token) -> TeXResult<(), ET> {
    let conds = engine.gullet.get_conditionals();
    let name = match conds.pop() {
        Some(ActiveConditional::True(id) | ActiveConditional::Else(id)) => id,
        Some(ActiveConditional::Case(_)) => PRIMITIVES.ifcase,
        Some(u @ ActiveConditional::Unfinished(_)) => {
            conds.push(u);
            engine.mouth.requeue(tk);
            let relax = engine.aux.memory.cs_interner_mut().cs_from_str("relax");
            engine.mouth.requeue(ET::Token::from_cs(relax));
            return Ok(());
        }
        _ => return engine.general_error("Unexpected \\fi".to_string()),
    };
    let trace = engine.state.get_primitive_int(PRIMITIVES.tracingifs) > ET::Int::default();
    let index = conds.len() + 1;
    if trace {
        engine.aux.outputs.write_neg1(format_args!(
            "{{{}fi: {} (level {}) entered on line {}}}",
            <ET::Char as Character>::display_opt(engine.state.get_escape_char()),
            name.display(engine.state.get_escape_char()),
            index,
            engine.mouth.line_number()
        ));
    }
    Ok(())
}

pub fn jobname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut fi = |t| exp.push(t);
    let mut f = Otherize::new(&mut fi);
    let escape = engine
        .aux
        .jobname
        .as_bytes()
        .iter()
        .any(|c| c.is_ascii_whitespace());
    if escape {
        write!(f, "\"{}\"", engine.aux.jobname)?;
    } else {
        write!(f, "{}", engine.aux.jobname)?;
    }
    Ok(())
}

pub fn fontname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let font = engine.read_font(false, &tk)?;
    let mut fi = |t| exp.push(t);
    let mut f = Otherize::new(&mut fi);
    if font.has_at_set() {
        write!(f, "{} at {}", font.filename(), font.get_at())?;
    } else {
        write!(f, "{}", font.filename())?;
    }
    Ok(())
}

pub fn let_<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let t = engine.need_next(false, &tk)?;
    let cm = match t.to_enum() {
        StandardToken::Character(c, CommandCode::Active) => CSOrActiveChar::Active(c),
        StandardToken::ControlSequence(cs) => CSOrActiveChar::Name(cs),
        _ => return engine.general_error("Control sequence expected after \\let".to_string()),
    };
    let mut after_eq = false;
    let mut after_space = false;
    loop {
        let next = engine.need_next(false, &tk)?;
        let cmd = match next.to_enum() {
            StandardToken::Character(_, CommandCode::Space) if !after_eq => continue,
            StandardToken::Character(_, CommandCode::Space) if !after_space => {
                after_space = true;
                continue;
            }
            StandardToken::Character(c, CommandCode::Other) if matches!(c.try_into(), Ok(b'=')) => {
                after_eq = true;
                continue;
            }
            StandardToken::ControlSequence(cs) => engine.state.get_command(&cs).cloned(),
            StandardToken::Primitive(id) => engine.state.primitives().get_id(id).cloned(),
            StandardToken::Character(c, CommandCode::Active) => {
                engine.state.get_ac_command(c).cloned()
            }
            StandardToken::Character(c, cc) => Some(TeXCommand::Char { char: c, code: cc }),
        };
        engine.set_command(&cm, cmd, globally);
        return Ok(());
    }
}

pub fn futurelet<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let t = engine.need_next(false, &_tk)?;
    let cm = match t.to_enum() {
        StandardToken::Character(c, CommandCode::Active) => CSOrActiveChar::Active(c),
        StandardToken::ControlSequence(cs) => CSOrActiveChar::Name(cs),
        _ => {
            return Err(TeXError::General(
                "Control sequence expected after \\futurelet".to_string(),
            ));
        }
    };
    let first = match engine.mouth.get_next(engine.aux, engine.state)? {
        Some(t) => t,
        _ => {
            return Err(TeXError::FileEndedWhileScanningUseOf(
                "futurelet".to_string(),
            ));
        }
    };
    let second = match engine.mouth.get_next(engine.aux, engine.state)? {
        Some(t) => t,
        _ => {
            return Err(TeXError::FileEndedWhileScanningUseOf(
                "futurelet".to_string(),
            ));
        }
    };
    let cmd = match second.to_enum() {
        StandardToken::ControlSequence(cs) => engine.state.get_command(&cs).cloned(),
        StandardToken::Character(c, CommandCode::Active) => engine.state.get_ac_command(c).cloned(),
        StandardToken::Character(c, cc) => Some(TeXCommand::Char { char: c, code: cc }),
        StandardToken::Primitive(id) => engine.state.primitives().get_id(id).cloned(),
    };
    engine.set_command(&cm, cmd, globally);
    engine.mouth.requeue(second);
    engine.mouth.requeue(first);
    Ok(())
}

pub fn lowercase<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(false, &tk)?;
    let mut exp = engine.mouth.get_vec(); // ET::Gullet::get_expansion_container(engine);
    engine.read_until_endgroup(&tk, |_, state, t| match t.to_enum() {
        StandardToken::Character(c, cc) => {
            let lccode = state.get_lccode(c);
            if lccode == ET::Char::default() {
                exp.push(t)
            } else {
                exp.push(ET::Token::from_char_cat(lccode, cc))
            }
            Ok(())
        }
        _ => {
            exp.push(t);
            Ok(())
        }
    })?;
    engine.mouth.push_vec(exp);
    Ok(())
}

pub fn uppercase<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(false, &tk)?;
    let mut exp = engine.mouth.get_vec(); //ET::Gullet::get_expansion_container(engine);
    engine.read_until_endgroup(&tk, |_, state, t| match t.to_enum() {
        StandardToken::Character(c, cc) => {
            let uccode = state.get_uccode(c);
            if uccode == ET::Char::default() {
                exp.push(t)
            } else {
                exp.push(ET::Token::from_char_cat(uccode, cc))
            }
            Ok(())
        }
        _ => {
            exp.push(t);
            Ok(())
        }
    })?;
    engine.mouth.push_vec(exp);
    Ok(())
}

pub fn mathchardef<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let t = engine.need_next(false, &tk)?;
    let cm = match t.to_enum() {
        StandardToken::Character(c, CommandCode::Active) => CSOrActiveChar::Active(c),
        StandardToken::ControlSequence(cs) => CSOrActiveChar::Name(cs),
        _ => {
            return Err(TeXError::General(
                "Expected control sequence after \\mathchardef".to_string(),
            ));
        }
    };
    let i = engine.read_int(true, &tk)?.into();
    if i < 0 || i > u32::MAX as i64 {
        return engine.general_error(format!("Illegal math char: {i}"));
    }
    let i = i as u32;
    engine.set_command(&cm, Some(TeXCommand::MathChar(i)), globally);
    Ok(())
}

pub fn mathchar<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 || i > 32767 as i64 {
        return engine.general_error(format!("Illegal math char: {i}"));
    }
    let i = i as u32;
    let ret = MathChar::from_u32(i, engine.state, None);
    ET::Stomach::add_node_m(engine, MathNode::Atom(ret.to_atom()));
    Ok(())
}

pub fn left<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let del = engine.read_opt_delimiter(&tk)?;
    engine.stomach.data_mut().open_lists.push(NodeList::Math {
        children: MathNodeList::default(),
        start: engine.mouth.start_ref(),
        tp: MathNodeListType::LeftRight(del),
    });
    engine
        .state
        .push(engine.aux, GroupType::LeftRight, engine.mouth.line_number());
    Ok(())
}
pub fn right<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    if engine.state.get_group_type() != Some(GroupType::LeftRight) {
        return Err(TeXError::General(
            "\\right outside of \\left\\right group".to_string(),
        ));
    }
    let del = engine.read_opt_delimiter(&tk)?;
    match engine.stomach.data_mut().open_lists.pop() {
        Some(NodeList::Math {
            children,
            start,
            tp: MathNodeListType::LeftRight(left),
        }) => {
            engine.state.pop(engine.aux, engine.mouth);
            let (children, None) = children.close(start, engine.mouth.current_sourceref()) else {
                unreachable!()
            };
            ET::Stomach::add_node_m(
                engine,
                MathNode::Atom(MathAtom {
                    nucleus: MathNucleus::LeftRight {
                        left: left.map(|d| (d.large.char, d.large.style)),
                        right: del.map(|d| (d.large.char, d.large.style)),
                        start,
                        end: engine.mouth.current_sourceref(),
                        children: children.into(),
                    },
                    sub: None,
                    sup: None,
                }),
            );
        }
        _ => {
            return Err(TeXError::General(
                "Unexpected open list in \\right".to_string(),
            ));
        }
    }
    Ok(())
}

pub fn meaning<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut fi = |t| exp.push(t);
    let mut f = Otherize::new(&mut fi);
    let t = engine.need_next(false, &tk)?;
    match engine.resolve(&t) {
        ResolvedToken::Cmd(None) => {
            if let Some(c) = engine.state.get_escape_char() {
                f.push_char(c);
            }
            write!(f, "undefined")?;
        }
        ResolvedToken::Cmd(Some(cmd)) => cmd
            .meaning(
                engine.aux.memory.cs_interner(),
                engine.state.get_catcode_scheme(),
                engine.state.get_escape_char(),
            )
            .write_chars(&mut f)?,
        ResolvedToken::Tk { char, code, .. } => code.meaning(char, f)?,
    }
    Ok(())
}

pub fn leaders<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_leaders(engine, LeaderType::Normal, &tk)
}
pub fn xleaders<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_leaders(engine, LeaderType::X, &tk)
}
pub fn cleaders<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_leaders(engine, LeaderType::C, &tk)
}

pub fn message<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut out = engine.aux.memory.get_string();
    engine.read_braced_string(false, true, &tk, &mut out)?;
    engine.aux.outputs.message(&out);
    engine.aux.memory.return_string(out);
    Ok(())
}

pub fn errmessage<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut out = String::new();
    engine.read_braced_string(false, true, &tk, &mut out)?;
    write!(out, " (line {})", engine.mouth.line_number()).unwrap();
    engine.general_error(out)
}

pub fn newlinechar_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(ET::Int::from(match engine.state.get_newline_char() {
        Some(c) => c.into() as i32,
        _ => -1,
    }))
}
pub fn newlinechar_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let val: i64 = engine.read_int(true, &tk)?.into();
    let val = match val.cmp(&-1) {
        Ordering::Less => return engine.general_error(format!("Illegal newline character: {val}")),
        Ordering::Equal => None,
        Ordering::Greater => match ET::Char::try_from(val as u64) {
            Ok(c) => Some(c),
            _ => return engine.general_error(format!("Illegal newline character: {val}")),
        },
    };
    engine.state.set_newline_char(engine.aux, val, globally);
    Ok(())
}

pub fn number<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let val = engine.read_int(false, &tk)?;
    write!(Otherize::new(&mut |t| exp.push(t)), "{}", val)?;
    Ok(())
}

pub fn noexpand<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let token = match engine.mouth.get_next(engine.aux, engine.state)? {
        Some(t) if t == ET::Token::eof() => return Ok(()),
        Some(t) => t,
        _ => {
            return Err(TeXError::General(
                "control sequence expected after \\noexpand".to_string(),
            ));
        }
    };
    match engine.resolve(&token) {
        ResolvedToken::Tk {
            code: CommandCode::AlignmentTab,
            ..
        } => {
            engine.mouth.requeue(token);
            engine
                .mouth
                .requeue(ET::Token::primitive(PRIMITIVES.noexpand));
        }
        ResolvedToken::Tk { .. } => engine.mouth.requeue(token),
        ResolvedToken::Cmd(Some(cm)) => {
            match cm {
                TeXCommand::Macro(_)
                | TeXCommand::Primitive {
                    cmd:
                        PrimitiveCommand::Expandable(_)
                        | PrimitiveCommand::SimpleExpandable(_)
                        | PrimitiveCommand::Conditional(_),
                    ..
                } => {
                    engine.mouth.requeue(token);
                    engine
                        .mouth
                        .requeue(ET::Token::primitive(PRIMITIVES.noexpand));
                }
                TeXCommand::Primitive { name, .. }
                    if *name == PRIMITIVES.cr || *name == PRIMITIVES.crcr =>
                {
                    engine.mouth.requeue(token);
                    engine
                        .mouth
                        .requeue(ET::Token::primitive(PRIMITIVES.noexpand));
                }
                _ => engine.mouth.requeue(token),
            };
        }
        ResolvedToken::Cmd(_) => engine.mouth.requeue(token),
    }
    Ok(())
}

pub fn noindent<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    if ET::Stomach::maybe_switch_mode(
        engine,
        CommandScope::SwitchesToHorizontal,
        tk,
        PRIMITIVES.noindent,
    )? {
        match engine.stomach.data_mut().open_lists.last_mut() {
            Some(NodeList::Horizontal { children, .. }) => {
                if let Some(HNode::Box(TeXBox::H {
                    info: HBoxInfo::ParIndent { .. },
                    ..
                })) = children.last_mut()
                {
                    children.pop();
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(())
}

pub fn openout<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET> {
    let (idx, file) = engine.read_filename_and_index("./", &tk)?;
    Ok(Some(Box::new(move |engine| {
        engine.filesystem.open_out(idx, file);
        Ok(())
    })))
}
pub fn openout_immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let (idx, file) = engine.read_filename_and_index("./", &tk)?;
    engine.filesystem.open_out(idx, file);
    Ok(())
}

pub fn prevdepth_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(engine.stomach.data_mut().prevdepth)
}
pub fn prevdepth_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let val = engine.read_dim(true, &tk)?;
    engine.stomach.data_mut().prevdepth = val;
    Ok(())
}

pub fn read<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_file_index(&tk)?;
    if !engine.read_keyword("to".as_bytes())? {
        TeXError::missing_keyword(engine.aux, engine.state, engine.mouth, &["to"])?;
    }
    let cs = engine.read_control_sequence(&tk)?;
    let mut ret = shared_vector::Vector::new();
    engine.filesystem.read::<ET, _>(
        idx,
        engine.aux.memory.cs_interner_mut(),
        engine.state,
        |t| ret.push(t),
    )?;

    let m = Macro {
        long: false,
        outer: false,
        protected: false,
        expansion: ret.into(),
        signature: MacroSignature {
            arity: 0,
            params: engine.aux.memory.empty_list(),
        },
    };
    engine.set_command(&cs, Some(TeXCommand::Macro(m)), globally);
    Ok(())
}

const ROMAN: &[(u8, i64)] = &[
    (b'm', 0),
    (b'd', 2),
    (b'c', 5),
    (b'l', 2),
    (b'x', 5),
    (b'v', 2),
    (b'i', 5),
];
#[allow(clippy::many_single_char_names)]
pub fn romannumeral<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut n = engine.read_int(false, &tk)?.into();
    if n < 0 {
        return Ok(());
    }
    let mut v = 1000;
    let mut j = 0;
    loop {
        while n >= v {
            exp.push(ET::Token::from_char_cat(
                ET::Char::from(ROMAN[j].0),
                CommandCode::Other,
            ));
            n -= v;
        }
        if n <= 0 {
            return Ok(());
        }
        let mut k = ROMAN[j + 1];
        let mut u = v / k.1;
        if k.1 == 2 {
            k = ROMAN[j + 2];
            u /= k.1;
        }
        if n + u >= v {
            exp.push(ET::Token::from_char_cat(
                ET::Char::from(k.0),
                CommandCode::Other,
            ));
            n += u;
        } else {
            j += 1;
            v /= ROMAN[j].1;
        }
    }
}

pub fn setbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let index = engine.read_register_index(false, &tk)?;
    match engine.read_box(true)? {
        either::Left(bx) => engine
            .state
            .set_box_register(engine.aux, index, bx, globally),
        either::Right(bi) => {
            let target = BoxTarget::<ET>::new(move |e, b| {
                e.state.set_box_register(e.aux, index, Some(b), globally);
                Ok(())
            });
            let mut ls = bi.open_list(engine.mouth.start_ref());
            match ls {
                NodeList::Horizontal {
                    tp: HorizontalNodeListType::Box(_, _, ref mut t),
                    ..
                } => *t = target,
                NodeList::Vertical {
                    tp: VerticalNodeListType::Box(_, _, ref mut t),
                    ..
                } => *t = target,
                _ => unreachable!(),
            }
            engine.stomach.data_mut().open_lists.push(ls);
        }
    }
    Ok(())
}

pub fn moveright<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.read_dim(false, &tk)?;
    match engine.read_box(false)? {
        either::Left(Some(mut bx)) => {
            match bx {
                TeXBox::H { ref mut info, .. } => info.move_left(-dim),
                TeXBox::V { ref mut info, .. } => info.move_left(-dim),
            }
            ET::Stomach::add_node_v(engine, VNode::Box(bx))?;
        }
        either::Left(None) => (),
        either::Right(mut bi) => {
            bi.move_left(-dim);
            engine
                .stomach
                .data_mut()
                .open_lists
                .push(bi.open_list(engine.mouth.start_ref()));
        }
    }
    Ok(())
}

pub fn moveleft<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.read_dim(false, &tk)?;
    match engine.read_box(false)? {
        either::Left(Some(mut bx)) => {
            match bx {
                TeXBox::H { ref mut info, .. } => info.move_left(dim),
                TeXBox::V { ref mut info, .. } => info.move_left(dim),
            }
            ET::Stomach::add_node_v(engine, VNode::Box(bx))?;
        }
        either::Left(None) => (),
        either::Right(mut bi) => {
            bi.move_left(dim);
            engine
                .stomach
                .data_mut()
                .open_lists
                .push(bi.open_list(engine.mouth.start_ref()));
        }
    }
    Ok(())
}

pub fn raise<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.read_dim(false, &tk)?;
    match engine.read_box(false)? {
        either::Left(Some(mut bx)) => {
            match bx {
                TeXBox::H { ref mut info, .. } => info.raise(dim),
                TeXBox::V { ref mut info, .. } => info.raise(dim),
            }
            match engine.stomach.data_mut().mode() {
                TeXMode::Horizontal | TeXMode::RestrictedHorizontal => {
                    ET::Stomach::add_node_h(engine, HNode::Box(bx));
                }
                TeXMode::InlineMath | TeXMode::DisplayMath => {
                    ET::Stomach::add_node_m(engine, bx.to_math());
                }
                _ => unreachable!(),
            }
        }
        either::Left(None) => (),
        either::Right(mut bi) => {
            bi.raise(dim);
            engine
                .stomach
                .data_mut()
                .open_lists
                .push(bi.open_list(engine.mouth.start_ref()));
        }
    }
    Ok(())
}

pub fn lower<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.read_dim(false, &tk)?;
    match engine.read_box(false)? {
        either::Left(Some(mut bx)) => {
            match bx {
                TeXBox::H { ref mut info, .. } => info.raise(-dim),
                TeXBox::V { ref mut info, .. } => info.raise(-dim),
            }
            match engine.stomach.data_mut().mode() {
                TeXMode::Horizontal | TeXMode::RestrictedHorizontal => {
                    ET::Stomach::add_node_h(engine, HNode::Box(bx));
                }
                TeXMode::InlineMath | TeXMode::DisplayMath => {
                    ET::Stomach::add_node_m(engine, bx.to_math());
                }
                _ => unreachable!(),
            }
        }
        either::Left(None) => (),
        either::Right(mut bi) => {
            bi.raise(-dim);
            engine
                .stomach
                .data_mut()
                .open_lists
                .push(bi.open_list(engine.mouth.start_ref()));
        }
    }
    Ok(())
}

pub fn string<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let t = engine.need_next(false, &tk)?;
    if t.command_code() == CommandCode::Space {
        exp.push(t)
    } else {
        match t.to_enum() {
            StandardToken::Character(c, _) => {
                exp.push(ET::Token::from_char_cat(c, CommandCode::Other))
            }
            StandardToken::ControlSequence(cs) => {
                let res = engine.aux.memory.cs_interner().resolve(&cs);
                if let Some(c) = engine.state.get_escape_char() {
                    exp.push(ET::Token::from_char_cat(c, CommandCode::Other))
                }
                for u in res.iter() {
                    match u.try_into() {
                        Ok(b' ') => exp.push(ET::Token::space()),
                        _ => exp.push(ET::Token::from_char_cat(u, CommandCode::Other)),
                    }
                }
            }
            _ => return Err(TeXError::EmergencyStop),
        }
    }
    Ok(())
}

pub fn openin<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let (idx, file) = engine.read_filename_and_index("", &tk)?;
    engine.filesystem.open_in(idx, file);
    Ok(())
}

pub fn closeout<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET> {
    let idx = engine.read_file_index(&tk)?;
    Ok(Some(Box::new(move |engine| {
        engine.filesystem.close_out(idx);
        Ok(())
    })))
}
pub fn closeout_immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let idx = engine.read_file_index(&tk)?;
    engine.filesystem.close_out(idx);
    Ok(())
}

pub fn closein<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let idx = engine.read_file_index(&tk)?;
    engine.filesystem.close_in(idx);
    Ok(())
}

pub fn write<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET> {
    let idx = engine.read_int(false, &tk)?.into();
    let mut tks = engine.mouth.get_vec();
    tks.push(ET::Token::from_char_cat(
        b'{'.into(),
        CommandCode::BeginGroup,
    ));
    let t = engine.need_next(false, &tk)?;
    if t.command_code() != CommandCode::BeginGroup {
        TeXError::missing_begingroup(engine.aux, engine.state, engine.mouth)?;
    }
    engine.read_until_endgroup(&tk, |_, _, t| {
        tks.push(t);
        Ok(())
    })?;
    tks.push(ET::Token::from_char_cat(b'}'.into(), CommandCode::EndGroup));
    Ok(Some(Box::new(move |engine| do_write(engine, tk, idx, tks))))
}

pub fn write_immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let idx = engine.read_int(false, &tk)?.into();
    let mut out = engine.aux.memory.get_string();
    engine.read_braced_string(false, false, &tk, &mut out)?;
    engine
        .filesystem
        .write(idx, &out, engine.state.get_newline_char(), engine.aux);
    engine.aux.memory.return_string(out);
    Ok(())
}
pub fn do_write<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    i: i64,
    v: Vec<ET::Token>,
) -> TeXResult<(), ET> {
    engine.mouth.push_vec(v);
    let mut out = engine.aux.memory.get_string();
    engine.read_braced_string(false, false, &tk, &mut out)?;
    engine
        .filesystem
        .write(i, &out, engine.state.get_newline_char(), engine.aux);
    engine.aux.memory.return_string(out);
    Ok(())
}

pub fn par<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let mode = engine.stomach.data_mut().mode();
    if mode.is_vertical() {
        return Ok(());
    }
    if mode == TeXMode::Horizontal {
        return ET::Stomach::close_paragraph(engine);
    }
    Err(TeXError::General(
        "\\par not allowed in math mode".to_string(),
    ))
}

pub fn the<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.do_the(|_, _, _, t| {
        exp.push(t);
        Ok(())
    })
}

pub fn toks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    global: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_register_index(false, &tk)?;
    ET::Stomach::assign_toks_register(engine, tk, idx, global)
}

pub fn penalty<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = match engine.read_int(false, &tk)?.try_into() {
        Ok(i) => i,
        Err(_) => return engine.general_error("Number expected after \\penalty".to_string()),
    };
    crate::add_node!(ET::Stomach;engine, VNode::Penalty(i), HNode::Penalty(i), MathNode::Penalty(i));
    Ok(())
}

pub fn kern<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.read_dim(false, &tk)?;
    crate::add_node!(ET::Stomach;engine,VNode::VKern(dim), HNode::HKern(dim), MathNode::HKern(dim));
    Ok(())
}

pub fn vrule<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let start = engine.mouth.start_ref();
    let mut width = None;
    let mut height = None;
    let mut depth = None;
    loop {
        match engine.read_keywords(&[b"width", b"height", b"depth"])? {
            Some(b"width") => {
                width = Some(engine.read_dim(false, &tk)?);
            }
            Some(b"height") => {
                height = Some(engine.read_dim(false, &tk)?);
            }
            Some(b"depth") => {
                depth = Some(engine.read_dim(false, &tk)?);
            }
            _ => break,
        }
    }
    let end = engine.mouth.current_sourceref();
    match engine.stomach.data_mut().mode() {
        TeXMode::Horizontal | TeXMode::RestrictedHorizontal => ET::Stomach::add_node_h(
            engine,
            HNode::VRule {
                width,
                height,
                depth,
                start,
                end,
            },
        ),
        TeXMode::InlineMath | TeXMode::DisplayMath => ET::Stomach::add_node_m(
            engine,
            MathNode::VRule {
                width,
                height,
                depth,
                start,
                end,
            },
        ),
        _ => unreachable!(),
    }
    Ok(())
}

pub fn hrule<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let start = engine.mouth.start_ref();
    let mut width = None;
    let mut height = None;
    let mut depth = None;
    loop {
        match engine.read_keywords(&[b"width", b"height", b"depth"])? {
            Some(b"width") => {
                width = Some(engine.read_dim(false, &tk)?);
            }
            Some(b"height") => {
                height = Some(engine.read_dim(false, &tk)?);
            }
            Some(b"depth") => {
                depth = Some(engine.read_dim(false, &tk)?);
            }
            _ => break,
        }
    }
    let end = engine.mouth.current_sourceref();
    ET::Stomach::add_node_v(
        engine,
        VNode::HRule {
            width,
            height,
            depth,
            start,
            end,
        },
    )
}

pub fn vskip<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let sk = engine.read_skip(false, &tk)?;
    ET::Stomach::add_node_v(engine, VNode::VSkip(sk))
}

pub fn vfil<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_v(engine, VNode::VFil)
}
pub fn vfill<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_v(engine, VNode::VFill)
}
pub fn vfilneg<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_v(engine, VNode::VFilneg)
}
pub fn vss<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_v(engine, VNode::Vss)
}

#[allow(unreachable_code)]
pub fn hskip<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let sk = engine.read_skip(false, &tk)?;
    add_node!(ET::Stomach;engine,unreachable!(),HNode::HSkip(sk),MathNode::HSkip(sk));
    Ok(())
}
#[allow(unreachable_code)]
pub fn hfil<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    add_node!(ET::Stomach;engine,unreachable!(),HNode::HFil,MathNode::HFil);
    Ok(())
}
#[allow(unreachable_code)]
pub fn hfill<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    add_node!(ET::Stomach;engine,unreachable!(),HNode::HFill,MathNode::HFill);
    Ok(())
}
#[allow(unreachable_code)]
pub fn hfilneg<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    add_node!(ET::Stomach;engine,unreachable!(),HNode::HFilneg,MathNode::HFilneg);
    Ok(())
}
#[allow(unreachable_code)]
pub fn hss<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    add_node!(ET::Stomach;engine,unreachable!(),HNode::Hss,MathNode::Hss);
    Ok(())
}

pub fn indent<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.state.get_primitive_dim(PRIMITIVES.parindent);
    ET::Stomach::add_node_h(
        engine,
        HNode::Box(TeXBox::H {
            children: vec![].into(),
            info: HBoxInfo::ParIndent(dim),
            start: engine.mouth.start_ref(),
            end: engine.mouth.current_sourceref(),
            preskip: None,
        }),
    );
    Ok(())
}

pub fn delimiter<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let num = engine.read_int(false, &tk)?;
    let delim = match Delimiter::from_int(num, engine.state) {
        either::Left(d) => d,
        either::Right((d, i)) => {
            engine.general_error(format!("Bad delimiter code ({})", i))?;
            d
        }
    };
    ET::Stomach::add_node_m(engine, MathNode::Atom(delim.small.to_atom()));
    Ok(())
}

pub fn mskip<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let skip = engine.read_muskip(false, &tk)?;
    ET::Stomach::add_node_m(
        engine,
        MathNode::MSkip {
            skip,
            style: UnresolvedMathFontStyle::of_fam(0),
        },
    );
    Ok(())
}

pub fn mkern<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let kern = engine.read_mudim(false, &tk)?;
    ET::Stomach::add_node_m(
        engine,
        MathNode::MKern {
            kern,
            style: UnresolvedMathFontStyle::of_fam(0),
        },
    );
    Ok(())
}

pub fn unskip<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::un_x(
        engine,
        |v| {
            matches!(
                v,
                VNode::VSkip(_) | VNode::VFil | VNode::VFill | VNode::VFilneg | VNode::Vss
            )
        },
        |h| {
            matches!(
                h,
                HNode::HSkip(_) | HNode::HFil | HNode::HFill | HNode::HFilneg | HNode::Hss
            )
        },
        |m| {
            matches!(
                m,
                MathNode::MSkip { .. }
                    | MathNode::HSkip(_)
                    | MathNode::HFil
                    | MathNode::HFill
                    | MathNode::HFilneg
                    | MathNode::Hss
            )
        },
    );
    Ok(())
}
pub fn unkern<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::un_x(
        engine,
        |v| matches!(v, VNode::VKern(_)),
        |h| matches!(h, HNode::HKern(_)),
        |m| matches!(m, MathNode::MKern { .. } | MathNode::HKern(_)),
    );
    Ok(())
}
pub fn unpenalty<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::un_x(
        engine,
        |v| matches!(v, VNode::Penalty(_)),
        |h| matches!(h, HNode::Penalty(_)),
        |m| matches!(m, MathNode::Penalty(_)),
    );
    Ok(())
}

pub fn lastbox<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    if engine.stomach.data_mut().mode() == TeXMode::Vertical {
        return Err(TeXError::General(
            "\\lastbox not allowed outside of vertical mode".to_string(),
        ));
    }
    let data = engine.stomach.data_mut();
    match data.open_lists.last_mut() {
        None => unreachable!(),
        Some(NodeList::Vertical { children, .. }) => {
            for (i, n) in children.iter().enumerate().rev() {
                if n.opaque() {
                    continue;
                }
                match n {
                    VNode::Box(_) => {
                        if let VNode::Box(bi) = children.remove(i) {
                            return Ok(either::Left(Some(bi)));
                        }
                        unreachable!()
                    }
                    _ => return Ok(either::Left(None)),
                }
            }
        }
        Some(NodeList::Horizontal { children, .. }) => {
            for (i, n) in children.iter().enumerate().rev() {
                if n.opaque() {
                    continue;
                }
                match n {
                    HNode::Box(_) => {
                        if let HNode::Box(bi) = children.remove(i) {
                            return Ok(either::Left(Some(bi)));
                        } else {
                            unreachable!()
                        }
                    }
                    _ => return Ok(either::Left(None)),
                }
            }
        }
        _ => (),
    }
    Ok(either::Left(None))
}

pub fn lastkern<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(super::methods::last_x(
        engine,
        |v| match v {
            VNode::VKern(k) => Some(*k),
            _ => None,
        },
        |h| match h {
            HNode::HKern(k) => Some(*k),
            _ => None,
        },
        |m| match m {
            MathNode::HKern(k) => Some(*k),
            _ => None,
        },
    )
    .unwrap_or_default())
}

pub fn lastskip<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<Skip<ET::Dim>, ET> {
    Ok(super::methods::last_x(
        engine,
        |v| match v {
            VNode::VSkip(k) => Some(*k),
            _ => None,
        },
        |h| match h {
            HNode::HSkip(k) => Some(*k),
            _ => None,
        },
        |m| match m {
            MathNode::HSkip(k) => Some(*k),
            _ => None,
        },
    )
    .unwrap_or_default())
}

pub fn lastpenalty<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(super::methods::last_x(
        engine,
        |v| match v {
            VNode::Penalty(k) => Some(*k),
            _ => None,
        },
        |h| match h {
            HNode::Penalty(k) => Some(*k),
            _ => None,
        },
        |m| match m {
            MathNode::Penalty(k) => Some(*k),
            _ => None,
        },
    )
    .unwrap_or_default()
    .into())
}

pub fn pagegoal_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(engine.stomach.data_mut().pagegoal)
}
pub fn pagegoal_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let d = engine.read_dim(true, &tk)?;
    let data = engine.stomach.data_mut();
    if data.pagegoal != ET::Dim::from_sp(i32::MAX) {
        engine.stomach.data_mut().pagegoal = d;
    }
    Ok(())
}
pub fn pagetotal_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(engine.stomach.data_mut().pagetotal)
}
pub fn pagetotal_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let d = engine.read_dim(true, &tk)?;
    engine.stomach.data_mut().pagetotal = d;
    Ok(())
}
pub fn pagestretch_get<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(ET::Dim::default()) // TODO
}
pub fn pagestretch_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let _ = engine.read_dim(true, &tk)?;
    // TODO
    Ok(())
}
pub fn pagefilstretch_get<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(ET::Dim::default()) // TODO
}
pub fn pagefilstretch_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let _ = engine.read_dim(true, &tk)?;
    // TODO
    Ok(())
}
pub fn pagefillstretch_get<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(ET::Dim::default()) // TODO
}
pub fn pagefillstretch_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let _ = engine.read_dim(true, &tk)?;
    // TODO
    Ok(())
}
pub fn pageshrink_get<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(ET::Dim::default()) // TODO
}
pub fn pageshrink_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let _ = engine.read_dim(true, &tk)?;
    // TODO
    Ok(())
}
pub fn pagefilshrink_get<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(ET::Dim::default()) // TODO
}
pub fn pagefilshrink_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let _ = engine.read_dim(true, &tk)?;
    // TODO
    Ok(())
}
pub fn pagefillshrink_get<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(ET::Dim::default()) // TODO
}
pub fn pagefillshrink_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let _ = engine.read_dim(true, &tk)?;
    // TODO
    Ok(())
}

pub fn pagedepth_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    Ok(engine.stomach.data_mut().pagedepth)
}
pub fn pagedepth_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let d = engine.read_dim(true, &tk)?;
    engine.stomach.data_mut().pagedepth = d;
    Ok(())
}

pub fn prevgraf_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok((engine.stomach.data_mut().prevgraf as i32).into())
}
pub fn prevgraf_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let d = engine.read_int(true, &tk)?.into();
    if d < 0 {
        engine.general_error(format!("Bad prevgraf ({})", d))?;
    } else {
        engine.stomach.data_mut().prevgraf = d as u16;
    }
    Ok(())
}

pub fn deadcycles_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok((engine.stomach.data_mut().deadcycles as i32).into())
}
pub fn deadcycles_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET> {
    let i = engine.read_int(true, &tk)?.into();
    if i >= 0 {
        engine.stomach.data_mut().deadcycles = i as usize
    }
    Ok(())
}

pub fn vsplit<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Either<Option<TeXBox<ET>>, BoxInfo<ET>>, ET> {
    let idx = engine.read_register_index(false, &tk)?;
    let (mut info, ls, start, end) = match engine.state.get_box_register_mut(idx) {
        Some(TeXBox::V {
            info,
            children,
            start,
            end,
        }) => (
            info.clone_for_split(),
            std::mem::take(children).into_vec(),
            *start,
            *end,
        ),
        _ => {
            if !engine.read_keyword(b"to")? {
                return Err(TeXError::General(
                    "`to` expected after \\vsplit".to_string(),
                ));
            }
            let _ = engine.read_dim(false, &tk)?;
            return Ok(either::Left(None));
        }
    };
    if !engine.read_keyword(b"to")? {
        return Err(TeXError::General(
            "`to` expected after \\vsplit".to_string(),
        ));
    }
    let target = engine.read_dim(false, &tk)?;
    match &mut info {
        VBoxInfo::VBox { scaled, .. } => {
            *scaled = ToOrSpread::To(target);
        }
        VBoxInfo::VTop { scaled, .. } => {
            *scaled = ToOrSpread::To(target);
        }
        _ => unreachable!(),
    }
    let SplitResult { first, rest, .. } = ET::Stomach::split_vertical(engine, ls, target);
    let ret = TeXBox::V {
        children: first.into(),
        info,
        start,
        end,
    };
    if rest.is_empty() {
        engine.state.set_box_register(engine.aux, idx, None, false);
    } else if let Some(TeXBox::V { children, .. }) = engine.state.get_box_register_mut(idx) {
        *children = rest.into();
    } else {
        unreachable!()
    }
    Ok(either::Left(Some(ret)))
}

pub fn vadjust<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(true, &tk)?;
    engine
        .stomach
        .data_mut()
        .open_lists
        .push(NodeList::Vertical {
            children: vec![],
            tp: VerticalNodeListType::VAdjust,
        });
    engine
        .state
        .push(engine.aux, GroupType::VAdjust, engine.mouth.line_number());
    Ok(())
}

pub fn inputlineno<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(ET::Int::from(engine.mouth.line_number() as i32))
}

pub fn mark<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_marks(engine, 0, &tk)
}

pub fn topmark<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::get_marks(engine, exp, |d| &mut d.topmarks, 0);
    Ok(())
}

pub fn firstmark<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::get_marks(engine, exp, |d| &mut d.firstmarks, 0);
    Ok(())
}

pub fn botmark<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::get_marks(engine, exp, |d| &mut d.botmarks, 0);
    Ok(())
}

pub fn splitfirstmark<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::get_marks(engine, exp, |d| &mut d.splitfirstmarks, 0);
    Ok(())
}

pub fn splitbotmark<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::get_marks(engine, exp, |d| &mut d.splitbotmarks, 0);
    Ok(())
}

pub fn shipout<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.stomach.data_mut().deadcycles = 0;
    match engine.read_box(false)? {
        either::Left(Some(bx)) => {
            engine.shipout(VNode::Box(bx))?;
            /*if let Some(n) = bx.as_node().shipout_top(engine) {
                engine.colon.out(n)
            }*/
        }
        either::Left(None) => (),
        either::Right(bi) => {
            let mut list = bi.open_list(engine.mouth.start_ref());
            let target = BoxTarget::new(|e, b| e.shipout(VNode::Box(b)));
            match &mut list {
                NodeList::Horizontal {
                    tp: HorizontalNodeListType::Box(_, _, t),
                    ..
                } => *t = target,
                NodeList::Vertical {
                    tp: VerticalNodeListType::Box(_, _, t),
                    ..
                } => *t = target,
                _ => unreachable!(),
            }
            let data = engine.stomach.data_mut();
            data.open_lists.push(list);
        }
    }
    Ok(())
}

pub fn displaylimits<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let ls = engine.stomach.data_mut().open_lists.last_mut().unwrap();
    match ls {
        NodeList::Math { children, .. } => {
            if let Some(MathNode::Atom(MathAtom {
                sub: None,
                sup: None,
                nucleus: MathNucleus::Simple { limits, .. },
                ..
            })) = children.list_mut().last_mut()
            {
                *limits = None;
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
pub fn limits<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let ls = engine.stomach.data_mut().open_lists.last_mut().unwrap();
    match ls {
        NodeList::Math { children, .. } => {
            if let Some(MathNode::Atom(MathAtom {
                sub: None,
                sup: None,
                nucleus:
                    MathNucleus::Simple {
                        cls: MathClass::Op,
                        limits,
                        ..
                    },
                ..
            })) = children.list_mut().last_mut()
            {
                *limits = Some(true);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}
pub fn nolimits<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let ls = engine.stomach.data_mut().open_lists.last_mut().unwrap();
    match ls {
        NodeList::Math { children, .. } => {
            if let Some(MathNode::Atom(MathAtom {
                sub: None,
                sup: None,
                nucleus:
                    MathNucleus::Simple {
                        cls: MathClass::Op,
                        limits,
                        ..
                    },
                ..
            })) = children.list_mut().last_mut()
            {
                *limits = Some(false);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn underline<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        &tk,
        |_, engine, mc| {
            ET::Stomach::add_node_m(
                engine,
                MathNode::Atom(MathAtom {
                    nucleus: MathNucleus::Underline(MathKernel::Char {
                        char: mc.char,
                        style: mc.style,
                    }),
                    sub: None,
                    sup: None,
                }),
            );
            Ok(())
        },
        |_| {
            ListTarget::<ET, _>::new(|engine, children, start| {
                ET::Stomach::add_node_m(
                    engine,
                    MathNode::Atom(MathAtom {
                        sup: None,
                        sub: None,
                        nucleus: MathNucleus::Underline(MathKernel::List {
                            children: children.into(),
                            start,
                            end: engine.mouth.current_sourceref(),
                        }),
                    }),
                );
                Ok(())
            })
        },
        (),
    )
}
pub fn overline<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        &tk,
        |_, engine, mc| {
            ET::Stomach::add_node_m(
                engine,
                MathNode::Atom(MathAtom {
                    nucleus: MathNucleus::Overline(MathKernel::Char {
                        char: mc.char,
                        style: mc.style,
                    }),
                    sub: None,
                    sup: None,
                }),
            );
            Ok(())
        },
        |_| {
            ListTarget::<ET, _>::new(|engine, children, start| {
                ET::Stomach::add_node_m(
                    engine,
                    MathNode::Atom(MathAtom {
                        sup: None,
                        sub: None,
                        nucleus: MathNucleus::Overline(MathKernel::List {
                            children: children.into(),
                            start,
                            end: engine.mouth.current_sourceref(),
                        }),
                    }),
                );
                Ok(())
            })
        },
        (),
    )
}

pub fn mathaccent<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 || i > 32767 as i64 {
        return Err(TeXError::General(format!("Illegal math accent number {i}")));
    }
    let char = MathChar::from_u32(i as u32, engine.state, None);
    engine.read_char_or_math_group(
        &tk,
        |(char, style), engine, mc| {
            ET::Stomach::add_node_m(
                engine,
                MathNode::Atom(MathAtom {
                    nucleus: MathNucleus::Accent {
                        accent: (char, style),
                        inner: vec![MathNode::Atom(mc.to_atom())].into(),
                    },
                    sub: None,
                    sup: None,
                }),
            );
            Ok(())
        },
        |(char, style)| {
            ListTarget::<ET, _>::new(move |engine, children, _| {
                ET::Stomach::add_node_m(
                    engine,
                    MathNode::Atom(MathAtom {
                        sup: None,
                        sub: None,
                        nucleus: MathNucleus::Accent {
                            accent: (char, style),
                            inner: children.into(),
                        },
                    }),
                );
                Ok(())
            })
        },
        (char.char, char.style),
    )
}

pub fn radical<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?;
    if i.into() < 0 || i.into() > u32::MAX as i64 {
        return Err(TeXError::General(format!("Illegal radical number {i}")));
    }
    let char = Delimiter::from_int(i, engine.state).left().unwrap().small; // MathChar::from_u32(i as u32, engine.state, None);
    engine.read_char_or_math_group(
        &tk,
        |(char, style), engine, mc| {
            ET::Stomach::add_node_m(
                engine,
                MathNode::Atom(MathAtom {
                    nucleus: MathNucleus::Radical {
                        rad: (char, style),
                        inner: vec![MathNode::Atom(mc.to_atom())].into(),
                    },
                    sub: None,
                    sup: None,
                }),
            );
            Ok(())
        },
        |(char, style)| {
            ListTarget::<ET, _>::new(move |engine, children, _| {
                ET::Stomach::add_node_m(
                    engine,
                    MathNode::Atom(MathAtom {
                        sup: None,
                        sub: None,
                        nucleus: MathNucleus::Radical {
                            rad: (char, style),
                            inner: children.into(),
                        },
                    }),
                );
                Ok(())
            })
        },
        (char.char, char.style),
    )
}

fn do_eqno<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    pos: EqNoPosition,
) -> TeXResult<(), ET> {
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_) | ch @ MathNodeList::Over { .. },
            tp: MathNodeListType::Top { .. },
            start,
        }) => {
            let old = std::mem::replace(
                ch,
                MathNodeList::EqNo {
                    pos,
                    main: vec![],
                    eqno: vec![],
                },
            );
            let (children, None) = old.close(*start, engine.mouth.current_sourceref()) else {
                unreachable!()
            };
            let MathNodeList::EqNo { main, .. } = ch else {
                unreachable!()
            };
            *main = children;
        }
        _ => return Err(TeXError::General("\\eqno outside of math mode".to_string())),
    }
    Ok(())
}

pub fn eqno<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    do_eqno(engine, EqNoPosition::Right)
}

pub fn leqno<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    do_eqno(engine, EqNoPosition::Left)
}

pub fn over<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_),
            ..
        }) => {
            //let v = std::mem::take(v);
            let old = std::mem::replace(
                ch,
                MathNodeList::Over {
                    top: vec![],
                    bottom: vec![],
                    left: None,
                    right: None,
                    sep: None,
                },
            );
            if let MathNodeList::Over { top, .. } = ch {
                *top = if let MathNodeList::Simple(v) = old {
                    v
                } else {
                    unreachable!()
                };
            } else {
                unreachable!()
            }
        }
        Some(NodeList::Math { .. }) => {
            return Err(TeXError::General(
                "Incompatible list for \\over".to_string(),
            ));
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn overwithdelims<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let left = engine
        .read_opt_delimiter(&tk)?
        .map(|d| (d.small.char, d.small.style));
    let right = engine
        .read_opt_delimiter(&tk)?
        .map(|d| (d.small.char, d.small.style));
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_),
            ..
        }) => {
            //let v = std::mem::take(v);
            let old = std::mem::replace(
                ch,
                MathNodeList::Over {
                    top: vec![],
                    bottom: vec![],
                    left,
                    right,
                    sep: None,
                },
            );
            if let MathNodeList::Over { top, .. } = ch {
                *top = if let MathNodeList::Simple(v) = old {
                    v
                } else {
                    unreachable!()
                };
            } else {
                unreachable!()
            }
        }
        Some(NodeList::Math { .. }) => {
            return Err(TeXError::General(
                "Incompatible list for \\overwithdelims".to_string(),
            ));
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn above<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let sep = engine.read_dim(false, &tk)?;
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_),
            ..
        }) => {
            //let v = std::mem::take(v);
            let old = std::mem::replace(
                ch,
                MathNodeList::Over {
                    top: vec![],
                    bottom: vec![],
                    left: None,
                    right: None,
                    sep: Some(sep),
                },
            );
            if let MathNodeList::Over { top, .. } = ch {
                *top = if let MathNodeList::Simple(v) = old {
                    v
                } else {
                    unreachable!()
                };
            } else {
                unreachable!()
            }
        }
        Some(NodeList::Math { .. }) => {
            return Err(TeXError::General(
                "Incompatible list for \\above".to_string(),
            ));
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn abovewithdelims<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let left = engine
        .read_opt_delimiter(&tk)?
        .map(|d| (d.small.char, d.small.style));
    let right = engine
        .read_opt_delimiter(&tk)?
        .map(|d| (d.small.char, d.small.style));
    let sep = engine.read_dim(false, &tk)?;
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_),
            ..
        }) => {
            //let v = std::mem::take(v);
            let old = std::mem::replace(
                ch,
                MathNodeList::Over {
                    top: vec![],
                    bottom: vec![],
                    left,
                    right,
                    sep: Some(sep),
                },
            );
            if let MathNodeList::Over { top, .. } = ch {
                *top = if let MathNodeList::Simple(v) = old {
                    v
                } else {
                    unreachable!()
                };
            } else {
                unreachable!()
            }
        }
        Some(NodeList::Math { .. }) => {
            return Err(TeXError::General(
                "Incompatible list for \\abovewithdelims".to_string(),
            ));
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn atop<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_),
            ..
        }) => {
            //let v = std::mem::take(v);
            let old = std::mem::replace(
                ch,
                MathNodeList::Over {
                    top: vec![],
                    bottom: vec![],
                    left: None,
                    right: None,
                    sep: Some(ET::Dim::default()),
                },
            );
            if let MathNodeList::Over { top, .. } = ch {
                *top = if let MathNodeList::Simple(v) = old {
                    v
                } else {
                    unreachable!()
                };
            } else {
                unreachable!()
            }
        }
        Some(NodeList::Math { .. }) => {
            return Err(TeXError::General(
                "Incompatible list for \\atop".to_string(),
            ));
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn atopwithdelims<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let left = engine
        .read_opt_delimiter(&tk)?
        .map(|d| (d.small.char, d.small.style));
    let right = engine
        .read_opt_delimiter(&tk)?
        .map(|d| (d.small.char, d.small.style));
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Math {
            children: ch @ MathNodeList::Simple(_),
            ..
        }) => {
            //let v = std::mem::take(v);
            let old = std::mem::replace(
                ch,
                MathNodeList::Over {
                    top: vec![],
                    bottom: vec![],
                    left,
                    right,
                    sep: Some(ET::Dim::default()),
                },
            );
            if let MathNodeList::Over { top, .. } = ch {
                *top = if let MathNodeList::Simple(v) = old {
                    v
                } else {
                    unreachable!()
                };
            } else {
                unreachable!()
            }
        }
        Some(NodeList::Math { .. }) => {
            return Err(TeXError::General(
                "Incompatible list for \\atopwithdelims".to_string(),
            ));
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub fn mathord<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Ord), &tk)
}
pub fn mathop<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Op), &tk)
}
pub fn mathbin<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Bin), &tk)
}
pub fn mathrel<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Rel), &tk)
}
pub fn mathopen<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Open), &tk)
}
pub fn mathclose<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Close), &tk)
}
pub fn mathpunct<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, Some(MathClass::Punct), &tk)
}
pub fn mathinner<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    super::methods::do_math_class(engine, None, &tk)
}
pub fn mathchoice<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        &tk.clone(),
        |tk, engine, mc| mathchoice_i(engine, vec![MathNode::Atom(mc.to_atom())].into(), tk),
        |tk| {
            ListTarget::<ET, _>::new(|engine, children, _| {
                mathchoice_i(engine, children.into(), tk)
            })
        },
        tk,
    )
}
type ML<ET> = Box<[MathNode<ET, UnresolvedMathFontStyle<ET>>]>;
pub fn mathchoice_i<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    d: ML<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        &tk.clone(),
        |(d, tk), engine, mc| {
            mathchoice_ii(engine, d, vec![MathNode::Atom(mc.to_atom())].into(), tk)
        },
        |(d, tk)| {
            ListTarget::<ET, _>::new(move |engine, children, _| {
                mathchoice_ii(engine, d, children.into(), tk)
            })
        },
        (d, tk),
    )
}
pub fn mathchoice_ii<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    d: ML<ET>,
    t: ML<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        &tk.clone(),
        |(d, t, tk), engine, mc| {
            mathchoice_iii(engine, d, t, vec![MathNode::Atom(mc.to_atom())].into(), tk)
        },
        |(d, t, tk)| {
            ListTarget::<ET, _>::new(|engine, children, _| {
                mathchoice_iii(engine, d, t, children.into(), tk)
            })
        },
        (d, t, tk),
    )
}
pub fn mathchoice_iii<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    d: ML<ET>,
    t: ML<ET>,
    s: ML<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.read_char_or_math_group(
        &tk,
        |(d, t, s), engine, mc| {
            ET::Stomach::add_node_m(
                engine,
                MathNode::Choice(UnresolvedMathChoice {
                    display: d,
                    text: t,
                    script: s,
                    scriptscript: vec![MathNode::Atom(mc.to_atom())].into(),
                }),
            );
            Ok(())
        },
        |(d, t, s)| {
            ListTarget::<ET, _>::new(|engine, children, _| {
                ET::Stomach::add_node_m(
                    engine,
                    MathNode::Choice(UnresolvedMathChoice {
                        display: d,
                        text: t,
                        script: s,
                        scriptscript: children.into(),
                    }),
                );
                Ok(())
            })
        },
        (d, t, s),
    )
}

pub fn displaystyle<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_m(engine, MathNode::Marker(UnresolvedMarkers::Display));
    Ok(())
}
pub fn textstyle<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_m(engine, MathNode::Marker(UnresolvedMarkers::Text));
    Ok(())
}
pub fn scriptstyle<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_m(engine, MathNode::Marker(UnresolvedMarkers::Script));
    Ok(())
}
pub fn scriptscriptstyle<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    ET::Stomach::add_node_m(engine, MathNode::Marker(UnresolvedMarkers::ScriptScript));
    Ok(())
}

const PRIMITIVE_INTS: &[&str] = &[
    "year",
    "month",
    "day",
    "time",
    "adjdemerits",
    "badness",
    "binoppenalty",
    "brokenpenalty",
    "clubpenalty",
    "defaulthyphenchar",
    "defaultskewchar",
    "delimiterfactor",
    "displaywidowpenalty",
    "doublehyphendemerits",
    "errorcontextlines",
    "exhyphenpenalty",
    "fam",
    "finalhyphendemerits",
    "floatingpenalty",
    "globaldefs",
    "hangafter",
    "hbadness",
    "holdinginserts",
    "hyphenpenalty",
    "interlinepenalty",
    "language",
    "lefthyphenmin",
    "linepenalty",
    "looseness",
    "mag",
    "maxdeadcycles",
    "outputpenalty",
    "pausing",
    "postdisplaypenalty",
    "predisplaypenalty",
    "relpenalty",
    "righthyphenmin",
    "pretolerance",
    "showboxbreadth",
    "showboxdepth",
    "tolerance",
    "tracingcommands",
    "tracinglostchars",
    "tracingmacros",
    "tracingonline",
    "tracingoutput",
    "tracingpages",
    "tracingparagraphs",
    "tracingrestores",
    "tracingstats",
    "uchyph",
    "vbadness",
    "widowpenalty",
    "synctex",         // not technically plain tex, but supported in basically all engines
    "insertpenalties", // todo
];

const PRIMITIVE_DIMS: &[&str] = &[
    "boxmaxdepth",
    "delimitershortfall",
    "displayindent",
    "displaywidth",
    "hangindent",
    "emergencystretch",
    "hfuzz",
    "hoffset",
    "hsize",
    "lineskiplimit",
    "maxdepth",
    "mathsurround",
    "nulldelimiterspace",
    "overfullrule",
    "parindent",
    "predisplaysize",
    "scriptspace",
    "splitmaxdepth",
    "vfuzz",
    "voffset",
    "vsize",
];

const PRIMITIVE_SKIPS: &[&str] = &[
    "abovedisplayshortskip",
    "abovedisplayskip",
    "baselineskip",
    "belowdisplayshortskip",
    "belowdisplayskip",
    "leftskip",
    "lineskip",
    "parfillskip",
    "parskip",
    "rightskip",
    "spaceskip",
    "splittopskip",
    "tabskip",
    "topskip",
    "xspaceskip",
];

const PRIMITIVE_MUSKIPS: &[&str] = &["thinmuskip", "medmuskip", "thickmuskip"];

const PRIMITIVE_TOKS: &[&str] = &[
    "everypar",
    "everymath",
    "everydisplay",
    "everyhbox",
    "everyvbox",
    "everyjob",
    "everycr",
    "output",
    "errhelp",
];

#[allow(unreachable_code)]
pub fn char_space<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    crate::add_node!(ET::Stomach;engine,unreachable!(), HNode::Space, MathNode::Space);
    Ok(())
}
pub fn char_slash<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    // TODO
    Ok(())
}
pub fn char_dash<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    // TODO
    Ok(())
}

pub fn end_template<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.gullet.get_align_data() {
        None => unreachable!(),
        Some(data) => {
            data.omit = false;
            let skip = data.columns[data.currindex].tabskip;
            match engine.stomach.data_mut().open_lists.last_mut() {
                Some(NodeList::Horizontal { children, .. }) => {
                    children.push(HNode::HSkip(skip));
                }
                Some(NodeList::Vertical { children, .. }) => {
                    children.push(VNode::VSkip(skip));
                }
                _ => unreachable!(),
            }
            data.currindex += 1;
            if data.columns.len() <= data.currindex {
                if let Some(i) = data.repeat_index {
                    data.currindex = i
                }
            }
            if data.span {
                match engine.stomach.data_mut().open_lists.last_mut() {
                    Some(NodeList::Horizontal {
                        tp: HorizontalNodeListType::HAlignCell(_, span),
                        ..
                    })
                    | Some(NodeList::Vertical {
                        tp: VerticalNodeListType::VAlignCell(_, span),
                        ..
                    }) => {
                        *span += 1;
                    }
                    _ => unreachable!(),
                }
            } else {
                super::methods::pop_align_cell(
                    engine.state,
                    engine.aux,
                    engine.stomach,
                    engine.mouth,
                    data.inner_mode,
                );
            }
            let mode = data.inner_mode;
            crate::expand_loop!(engine,token,
                ResolvedToken::Tk{code:CommandCode::EndGroup,..} |
                ResolvedToken::Cmd(Some(TeXCommand::Char {code:CommandCode::EndGroup,..})) => {
                    return Err(TeXError::General("Unexpected end group character in alignment".to_string()))
                }
                ResolvedToken::Tk{code:CommandCode::Space,..} => (),
                /*ResolvedToken::Cmd {cmd:Some(Command::Unexpandable(Unexpandable {name,..})),..}
                    if *name == PRIMITIVES.crcr => (),*/
                ResolvedToken::Cmd(Some(TeXCommand::Primitive {name,..})) if *name == PRIMITIVES.omit => {
                    engine.gullet.get_align_data().unwrap().omit = true;
                    super::methods::open_align_cell(engine,mode);
                    return Ok(())
                },
                ResolvedToken::Tk{..} | ResolvedToken::Cmd(_) => {
                    engine.requeue(token)?;
                    super::methods::open_align_cell(engine,mode);
                    return Ok(())
                }
            );
            super::methods::open_align_cell(engine, mode);
            Ok(())
        }
    }
}

pub fn end_template_row<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.gullet.get_align_data() {
        None => unreachable!(),
        Some(data) => {
            data.omit = false;
            let skip = data.columns[data.currindex].tabskip;
            match engine.stomach.data_mut().open_lists.last_mut() {
                Some(NodeList::Horizontal { children, .. }) => {
                    children.push(HNode::HSkip(skip));
                }
                Some(NodeList::Vertical { children, .. }) => {
                    children.push(VNode::VSkip(skip));
                }
                _ => unreachable!(),
            }
            let mode = data.inner_mode;
            super::methods::pop_align_cell(
                engine.state,
                engine.aux,
                engine.stomach,
                engine.mouth,
                mode,
            );
            super::methods::pop_align_row::<ET>(engine.stomach, engine.mouth, mode);
            super::methods::start_align_row(engine, mode)
        }
    }
}

pub fn register_tex_primitives<E: TeXEngine>(engine: &mut E) {
    register_int(engine, "catcode", catcode_get, Some(catcode_set));
    register_int(engine, "hyphenchar", hyphenchar_get, Some(hyphenchar_set));
    register_int(engine, "skewchar", skewchar_get, Some(skewchar_set));
    register_int(engine, "sfcode", sfcode_get, Some(sfcode_set));
    register_int(engine, "lccode", lccode_get, Some(lccode_set));
    register_int(engine, "uccode", uccode_get, Some(uccode_set));
    register_int(engine, "mathcode", mathcode_get, Some(mathcode_set));
    register_int(engine, "delcode", delcode_get, Some(delcode_set));
    register_int(engine, "count", count_get, Some(count_set));
    register_int(
        engine,
        "endlinechar",
        endlinechar_get,
        Some(endlinechar_set),
    );
    register_int(engine, "escapechar", escapechar_get, Some(escapechar_set));
    register_int(
        engine,
        "newlinechar",
        newlinechar_get,
        Some(newlinechar_set),
    );
    register_int(engine, "inputlineno", inputlineno, None);
    register_int(
        engine,
        "spacefactor",
        spacefactor_get,
        Some(spacefactor_set),
    );
    register_int(engine, "lastpenalty", lastpenalty, None);
    register_int(engine, "parshape", parshape_get, Some(parshape_set));
    register_int(engine, "deadcycles", deadcycles_get, Some(deadcycles_set));
    register_int(engine, "prevgraf", prevgraf_get, Some(prevgraf_set));

    register_dim(engine, "fontdimen", fontdimen_get, Some(fontdimen_set));
    register_dim(engine, "dimen", dimen_get, Some(dimen_set));
    register_dim(engine, "prevdepth", prevdepth_get, Some(prevdepth_set));
    register_dim(engine, "dp", dp_get, Some(dp_set));
    register_dim(engine, "ht", ht_get, Some(ht_set));
    register_dim(engine, "wd", wd_get, Some(wd_set));
    register_dim(engine, "lastkern", lastkern, None);
    register_dim(engine, "pagegoal", pagegoal_get, Some(pagegoal_set));
    register_dim(engine, "pagetotal", pagetotal_get, Some(pagegoal_set));
    register_dim(
        engine,
        "pagestretch",
        pagestretch_get,
        Some(pagestretch_set),
    );
    register_dim(
        engine,
        "pagefilstretch",
        pagefilstretch_get,
        Some(pagefilstretch_set),
    );
    register_dim(
        engine,
        "pagefillstretch",
        pagefillstretch_get,
        Some(pagefillstretch_set),
    );
    register_dim(engine, "pageshrink", pageshrink_get, Some(pageshrink_set));
    register_dim(
        engine,
        "pagefilshrink",
        pagefilshrink_get,
        Some(pagefilshrink_set),
    );
    register_dim(
        engine,
        "pagefillshrink",
        pagefillshrink_get,
        Some(pagefillshrink_set),
    );
    register_dim(engine, "pagedepth", pagedepth_get, Some(pagedepth_set));

    register_skip(engine, "skip", skip_get, Some(skip_set));
    register_skip(engine, "lastskip", lastskip, None);

    register_muskip(engine, "muskip", muskip_get, Some(muskip_set));

    register_font(engine, "font", font_get, Some(font_set));
    register_font(engine, "textfont", textfont_get, Some(textfont_set));
    register_font(engine, "scriptfont", scriptfont_get, Some(scriptfont_set));
    register_font(
        engine,
        "scriptscriptfont",
        scriptscriptfont_get,
        Some(scriptscriptfont_set),
    );

    register_assignment(engine, "advance", advance);
    register_assignment(engine, "chardef", chardef);
    register_assignment(engine, "countdef", countdef);
    register_assignment(engine, "dimendef", dimendef);
    register_assignment(engine, "skipdef", skipdef);
    register_assignment(engine, "muskipdef", muskipdef);
    register_assignment(engine, "toksdef", toksdef);
    register_assignment(engine, "def", |e, cmd, g| {
        def(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "divide", divide);
    register_assignment(engine, "edef", |e, cmd, g| {
        edef(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "xdef", |e, cmd, g| {
        xdef(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "gdef", |e, cmd, g| {
        gdef(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "outer", |e, cmd, g| {
        outer(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "long", |e, cmd, g| {
        long(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "global", |e, cmd, g| {
        global(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "let", let_);
    register_assignment(engine, "futurelet", futurelet);
    register_assignment(engine, "mathchardef", mathchardef);
    register_assignment(engine, "multiply", multiply);
    register_assignment(engine, "read", read);
    register_assignment(engine, "setbox", setbox);
    register_assignment(engine, "toks", toks);

    register_simple_expandable(engine, "csname", csname);
    register_simple_expandable(engine, "else", r#else);
    register_simple_expandable(engine, "endinput", endinput);
    register_simple_expandable(engine, "or", or);
    register_simple_expandable(engine, "expandafter", expandafter);
    register_simple_expandable(engine, "fi", fi);
    register_simple_expandable(engine, "input", input);
    register_simple_expandable(engine, "noexpand", noexpand);

    register_expandable(engine, "jobname", jobname);
    register_expandable(engine, "fontname", fontname);
    register_expandable(engine, "meaning", meaning);
    register_expandable(engine, "number", number);
    register_expandable(engine, "romannumeral", romannumeral);
    register_expandable(engine, "string", string);
    register_expandable(engine, "the", the);
    register_expandable(engine, "topmark", topmark);
    register_expandable(engine, "firstmark", firstmark);
    register_expandable(engine, "botmark", botmark);
    register_expandable(engine, "splitfirstmark", splitfirstmark);
    register_expandable(engine, "splitbotmark", splitbotmark);

    register_conditional(engine, "if", r#if);
    register_conditional(engine, "ifcase", ifcase);
    register_conditional(engine, "ifcat", ifcat);
    register_conditional(engine, "ifdim", ifdim);
    register_conditional(engine, "ifeof", ifeof);
    register_conditional(engine, "ifhbox", ifhbox);
    register_conditional(engine, "ifhmode", ifhmode);
    register_conditional(engine, "ifinner", ifinner);
    register_conditional(engine, "ifmmode", ifmmode);
    register_conditional(engine, "ifnum", ifnum);
    register_conditional(engine, "ifodd", ifodd);
    register_conditional(engine, "ifx", ifx);
    register_conditional(engine, "iftrue", iftrue);
    register_conditional(engine, "iffalse", iffalse);
    register_conditional(engine, "ifvbox", ifvbox);
    register_conditional(engine, "ifvmode", ifvmode);
    register_conditional(engine, "ifvoid", ifvoid);

    register_unexpandable(
        engine,
        "afterassignment",
        CommandScope::Any,
        afterassignment,
    );
    register_unexpandable(engine, "aftergroup", CommandScope::Any, aftergroup);
    register_unexpandable(engine, "begingroup", CommandScope::Any, begingroup);
    register_unexpandable(engine, "closein", CommandScope::Any, closein);
    register_unexpandable(
        engine,
        "char",
        CommandScope::SwitchesToHorizontalOrMath,
        r#char,
    );
    register_unexpandable(engine, "discretionary", CommandScope::Any, discretionary);
    register_unexpandable(engine, "dump", CommandScope::Any, |_, _| Ok(()));
    register_unexpandable(engine, "endcsname", CommandScope::Any, endcsname);
    register_unexpandable(engine, "endgroup", CommandScope::Any, endgroup);
    register_unexpandable(engine, "end", CommandScope::Any, end);
    register_unexpandable(engine, "errorstopmode", CommandScope::Any, errorstopmode);
    register_unexpandable(engine, "halign", CommandScope::Any, halign);
    register_unexpandable(engine, "valign", CommandScope::SwitchesToHorizontal, valign);
    register_unexpandable(engine, "hyphenation", CommandScope::Any, |e, t| {
        e.skip_argument(&t)
    });
    register_unexpandable(engine, "ignorespaces", CommandScope::Any, ignorespaces);
    register_unexpandable(engine, "insert", CommandScope::Any, insert);
    register_unexpandable(engine, "immediate", CommandScope::Any, immediate);
    register_unexpandable(engine, "lowercase", CommandScope::Any, lowercase);
    register_unexpandable(engine, "uppercase", CommandScope::Any, uppercase);
    register_unexpandable(engine, "leaders", CommandScope::Any, leaders);
    register_unexpandable(engine, "xleaders", CommandScope::Any, xleaders);
    register_unexpandable(engine, "cleaders", CommandScope::Any, cleaders);
    register_unexpandable(engine, "message", CommandScope::Any, message);
    register_unexpandable(engine, "errmessage", CommandScope::Any, errmessage);
    register_unexpandable(engine, "noindent", CommandScope::Any, noindent);
    register_unexpandable(engine, "openin", CommandScope::Any, openin);
    register_unexpandable(engine, "par", CommandScope::Any, par);
    register_unexpandable(
        engine,
        "unhbox",
        CommandScope::SwitchesToHorizontalOrMath,
        unhbox,
    );
    register_unexpandable(engine, "unvbox", CommandScope::SwitchesToVertical, unvbox);
    register_unexpandable(
        engine,
        "unhcopy",
        CommandScope::SwitchesToHorizontalOrMath,
        unhcopy,
    );
    register_unexpandable(engine, "unvcopy", CommandScope::SwitchesToVertical, unvcopy);
    register_unexpandable(engine, "unskip", CommandScope::Any, unskip);
    register_unexpandable(engine, "unkern", CommandScope::Any, unkern);
    register_unexpandable(engine, "unpenalty", CommandScope::Any, unpenalty);
    register_unexpandable(
        engine,
        "moveleft",
        CommandScope::SwitchesToVertical,
        moveleft,
    );
    register_unexpandable(
        engine,
        "moveright",
        CommandScope::SwitchesToVertical,
        moveright,
    );
    register_unexpandable(
        engine,
        "raise",
        CommandScope::SwitchesToHorizontalOrMath,
        raise,
    );
    register_unexpandable(
        engine,
        "lower",
        CommandScope::SwitchesToHorizontalOrMath,
        lower,
    );
    register_unexpandable(engine, "shipout", CommandScope::Any, shipout);
    register_unexpandable(engine, "patterns", CommandScope::Any, |e, t| {
        e.skip_argument(&t)
    });
    register_unexpandable(
        engine,
        "vadjust",
        CommandScope::SwitchesToHorizontalOrMath,
        vadjust,
    );
    {
        let refs = engine.get_engine_refs();
        let relax = refs.aux.memory.cs_interner_mut().cs_from_str("relax");
        let nullfont = refs.aux.memory.cs_interner_mut().cs_from_str("nullfont");
        refs.state.set_command(
            refs.aux,
            relax,
            Some(TeXCommand::Primitive {
                name: PRIMITIVES.relax,
                cmd: PrimitiveCommand::Relax,
            }),
            true,
        );
        refs.state.set_command(
            refs.aux,
            nullfont,
            Some(TeXCommand::Font(refs.fontsystem.null())),
            true,
        )
    }
    register_unexpandable(engine, "mark", CommandScope::Any, mark);
    register_unexpandable(engine, "/", CommandScope::Any, char_slash);
    register_unexpandable(engine, "-", CommandScope::Any, char_dash);
    register_unexpandable(engine, "showlists", CommandScope::Any, |_, _| Ok(())); // TODO
    register_unexpandable(engine, "crcr", CommandScope::Any, |_, _| Ok(()));
    register_unexpandable(engine, "cr", CommandScope::Any, |_, _| {
        Err(TeXError::General("Unexpected \\cr".to_string()))
    });
    register_unexpandable(engine, END_TEMPLATE, CommandScope::Any, end_template);
    register_unexpandable(
        engine,
        END_TEMPLATE_ROW,
        CommandScope::Any,
        end_template_row,
    );

    register_unexpandable(engine, "delimiter", CommandScope::MathOnly, delimiter);
    register_unexpandable(engine, "mskip", CommandScope::MathOnly, mskip);
    register_unexpandable(engine, "mkern", CommandScope::MathOnly, mkern);
    register_unexpandable(engine, "mathchar", CommandScope::MathOnly, mathchar);
    register_unexpandable(engine, "left", CommandScope::MathOnly, left);
    register_unexpandable(engine, "right", CommandScope::MathOnly, right);
    register_unexpandable(
        engine,
        "displaylimits",
        CommandScope::MathOnly,
        displaylimits,
    );
    register_unexpandable(engine, "limits", CommandScope::MathOnly, limits);
    register_unexpandable(engine, "nolimits", CommandScope::MathOnly, nolimits);
    register_unexpandable(engine, "penalty", CommandScope::Any, penalty);
    register_unexpandable(engine, "kern", CommandScope::Any, kern);
    register_unexpandable(
        engine,
        "vrule",
        CommandScope::SwitchesToHorizontalOrMath,
        vrule,
    );
    register_unexpandable(engine, "vskip", CommandScope::SwitchesToVertical, vskip);
    register_unexpandable(engine, "vfil", CommandScope::SwitchesToVertical, vfil);
    register_unexpandable(engine, "vfill", CommandScope::SwitchesToVertical, vfill);
    register_unexpandable(engine, "vfilneg", CommandScope::SwitchesToVertical, vfilneg);
    register_unexpandable(engine, "vss", CommandScope::SwitchesToVertical, vss);
    register_unexpandable(engine, "hrule", CommandScope::SwitchesToVertical, hrule);
    register_unexpandable(
        engine,
        "hskip",
        CommandScope::SwitchesToHorizontalOrMath,
        hskip,
    );
    register_unexpandable(
        engine,
        "hfil",
        CommandScope::SwitchesToHorizontalOrMath,
        hfil,
    );
    register_unexpandable(
        engine,
        "hfill",
        CommandScope::SwitchesToHorizontalOrMath,
        hfill,
    );
    register_unexpandable(
        engine,
        "hfilneg",
        CommandScope::SwitchesToHorizontalOrMath,
        hfilneg,
    );
    register_unexpandable(engine, "hss", CommandScope::SwitchesToHorizontalOrMath, hss);
    register_unexpandable(engine, "indent", CommandScope::SwitchesToHorizontal, indent);
    register_unexpandable(
        engine,
        " ",
        CommandScope::SwitchesToHorizontalOrMath,
        char_space,
    );
    register_unexpandable(engine, "vcenter", CommandScope::MathOnly, vcenter);
    register_unexpandable(engine, "accent", CommandScope::SwitchesToHorizontal, accent);

    register_unexpandable(engine, "mathord", CommandScope::MathOnly, mathord);
    register_unexpandable(engine, "mathop", CommandScope::MathOnly, mathop);
    register_unexpandable(engine, "mathbin", CommandScope::MathOnly, mathbin);
    register_unexpandable(engine, "mathrel", CommandScope::MathOnly, mathrel);
    register_unexpandable(engine, "mathopen", CommandScope::MathOnly, mathopen);
    register_unexpandable(engine, "mathclose", CommandScope::MathOnly, mathclose);
    register_unexpandable(engine, "mathpunct", CommandScope::MathOnly, mathpunct);
    register_unexpandable(engine, "mathinner", CommandScope::MathOnly, mathinner);
    register_unexpandable(engine, "mathchoice", CommandScope::MathOnly, mathchoice);
    register_unexpandable(engine, "underline", CommandScope::MathOnly, underline);
    register_unexpandable(engine, "overline", CommandScope::MathOnly, overline);
    register_unexpandable(engine, "mathaccent", CommandScope::MathOnly, mathaccent);
    register_unexpandable(engine, "radical", CommandScope::MathOnly, radical);
    register_unexpandable(engine, "eqno", CommandScope::MathOnly, eqno);
    register_unexpandable(engine, "leqno", CommandScope::MathOnly, leqno);

    register_unexpandable(engine, "over", CommandScope::MathOnly, over);
    register_unexpandable(
        engine,
        "overwithdelims",
        CommandScope::MathOnly,
        overwithdelims,
    );
    register_unexpandable(engine, "above", CommandScope::MathOnly, above);
    register_unexpandable(
        engine,
        "abovewithdelims",
        CommandScope::MathOnly,
        abovewithdelims,
    );
    register_unexpandable(engine, "atop", CommandScope::MathOnly, atop);
    register_unexpandable(
        engine,
        "atopwithdelims",
        CommandScope::MathOnly,
        atopwithdelims,
    );

    register_unexpandable(engine, "displaystyle", CommandScope::MathOnly, displaystyle);
    register_unexpandable(engine, "textstyle", CommandScope::MathOnly, textstyle);
    register_unexpandable(engine, "scriptstyle", CommandScope::MathOnly, scriptstyle);
    register_unexpandable(
        engine,
        "scriptscriptstyle",
        CommandScope::MathOnly,
        scriptscriptstyle,
    );

    register_unexpandable(engine, "nonscript", CommandScope::MathOnly, |_, _| Ok(())); // TODO

    register_whatsit(engine, "closeout", closeout, closeout_immediate, None);
    register_whatsit(engine, "openout", openout, openout_immediate, None);
    register_whatsit(engine, "write", write, write_immediate, None);

    register_box(engine, "hbox", hbox);
    register_box(engine, "vbox", vbox);
    register_box(engine, "vtop", vtop);
    register_box(engine, "box", box_);
    register_box(engine, "copy", copy);
    register_box(engine, "lastbox", lastbox);
    register_box(engine, "vsplit", vsplit);

    cmstodos!(engine, noalign, omit, span);

    cmtodos!(
        engine,
        scrollmode,
        nonstopmode,
        batchmode,
        show,
        showbox,
        showthe,
        noboundary,
        setlanguage,
        bye,
        italiccorr
    );

    register_primitive_int(engine, PRIMITIVE_INTS);
    register_primitive_dim(engine, PRIMITIVE_DIMS);
    register_primitive_skip(engine, PRIMITIVE_SKIPS);
    register_primitive_muskip(engine, PRIMITIVE_MUSKIPS);
    register_primitive_toks(engine, PRIMITIVE_TOKS);

    register_unexpandable(engine, "special", CommandScope::Any, |e, t| {
        e.skip_argument(&t)
    });
}
