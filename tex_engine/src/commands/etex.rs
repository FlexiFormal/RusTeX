use super::primitives::*;
use crate::commands::{
    CharOrPrimitive, CommandScope, Macro, MacroSignature, PrimitiveCommand, ResolvedToken,
    TeXCommand,
};
use crate::engine::filesystem::FileSystem;
use crate::engine::fontsystem::Font;
use crate::engine::gullet::Gullet;
use crate::engine::gullet::methods::NumContinuation;
use crate::engine::mouth::Mouth;
use crate::engine::state::{GroupType, State};
use crate::engine::stomach::Stomach;
use crate::engine::{EngineAux, EngineReferences, EngineTypes, TeXEngine};
use crate::tex::catcodes::{CategoryCode, CommandCode};
use crate::tex::characters::Character;
use crate::tex::characters::CharacterMap;
use crate::tex::nodes::math::MathAtom;
use crate::tex::nodes::math::{MathNode, MathNucleus};
use crate::tex::nodes::{NodeList, NodeTrait};
use crate::tex::numerics::{MuSkip, NumSet, Numeric, Skip, StretchShrink};
use crate::tex::tokens::control_sequences::{CSHandler, ResolvedCSName};
use crate::tex::tokens::token_lists::{CharWrite, Otherize};
use crate::tex::tokens::{StandardToken, Token};
use crate::utils::errors::{TeXError, TeXResult};
use either::Either;

#[allow(non_snake_case)]
pub fn eTeXversion<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(2.into())
}
#[allow(non_snake_case)]
pub fn eTeXrevision<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    exp.push(ET::Token::from_char_cat(b'.'.into(), CommandCode::Other));
    exp.push(ET::Token::from_char_cat(b'6'.into(), CommandCode::Other));
    Ok(())
}

fn expr_inner<ET: EngineTypes, R: Numeric<<ET::Num as NumSet>::Int>>(
    engine: &mut EngineReferences<ET>,
    byte: fn(&mut EngineReferences<ET>, bool, u8) -> TeXResult<R, ET>,
    cmd: fn(&mut EngineReferences<ET>, bool, TeXCommand<ET>, ET::Token) -> TeXResult<R, ET>,
    tk: &ET::Token,
) -> TeXResult<R, ET> {
    let NumContinuation { is_negative, next } =
        crate::engine::gullet::methods::read_numeric(engine, false, tk)?;
    match next {
        Either::Left(b) => {
            if b == b'(' {
                let (int, ret) = expr_loop(engine, byte, cmd, tk)?;
                match ret {
                    Some(ret) => match ret.to_enum() {
                        StandardToken::Character(char, CommandCode::Other)
                            if matches!(char.try_into(), Ok(b')')) =>
                        {
                            if is_negative {
                                Ok(-int)
                            } else {
                                Ok(int)
                            }
                        }
                        _ => Err(TeXError::General(
                            "Closing Parenthesis expected\nTODO: Better error message".to_string(),
                        )),
                    },
                    _ => Err(TeXError::General(
                        "Closing Parenthesis expected\nTODO: Better error message".to_string(),
                    )),
                }
            } else {
                byte(engine, is_negative, b)
            }
        }
        Either::Right((c, token)) => cmd(engine, is_negative, c, token),
    }
}

struct Summand<ET: EngineTypes, R: Numeric<ET::Int>> {
    base: R,
    times: Vec<ET::Int>,
    div: Vec<ET::Int>,
}
impl<ET: EngineTypes, R: Numeric<ET::Int>> Summand<ET, R> {
    fn new(r: R) -> Self {
        Self {
            base: r,
            times: vec![],
            div: vec![],
        }
    }
    fn resolve(self) -> R {
        let times = if self.times.is_empty() {
            ET::Int::from(1)
        } else {
            self.times.into_iter().reduce(|a, b| a * b).unwrap()
        };
        let div = if self.div.is_empty() {
            ET::Int::from(1)
        } else {
            self.div.into_iter().reduce(|a, b| a * b).unwrap()
        };
        self.base.scale(times, div)
    }
    fn mult(&mut self, i: ET::Int) {
        self.times.push(i)
    }
    fn div(&mut self, i: ET::Int) {
        self.div.push(i)
    }
}
impl<ET: EngineTypes, R: Numeric<ET::Int>> std::fmt::Display for Summand<ET, R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}", self.base)?;
        for x in self.times.iter() {
            write!(f, " *{}", x)?;
        }
        for x in self.div.iter() {
            write!(f, " /{}", x)?;
        }
        write!(f, ")")
    }
}

fn expr_loop<ET: EngineTypes, R: Numeric<<ET::Num as NumSet>::Int>>(
    engine: &mut EngineReferences<ET>,
    byte: fn(&mut EngineReferences<ET>, bool, u8) -> TeXResult<R, ET>,
    cmd: fn(&mut EngineReferences<ET>, bool, TeXCommand<ET>, ET::Token) -> TeXResult<R, ET>,
    tk: &ET::Token,
) -> TeXResult<(R, Option<ET::Token>), ET> {
    let mut prev: Option<R> = None;
    let mut curr = Summand::<ET, R>::new(expr_inner(engine, byte, cmd, tk)?);
    loop {
        match engine.read_chars(b"+-*/")? {
            Either::Right(r) => {
                match prev {
                    Some(p) => return Ok((p + curr.resolve(), r)),
                    _ => return Ok((curr.resolve(), r)),
                }
                //return (Summand::reduce(adds),r)
            }
            Either::Left(b'+') => {
                let old =
                    std::mem::replace(&mut curr, Summand::new(expr_inner(engine, byte, cmd, tk)?))
                        .resolve();
                prev = match prev {
                    Some(s) => Some(s + old),
                    None => Some(old),
                }
            }
            Either::Left(b'-') => {
                let old =
                    std::mem::replace(&mut curr, Summand::new(-expr_inner(engine, byte, cmd, tk)?))
                        .resolve();
                prev = match prev {
                    Some(s) => Some(s + old),
                    None => Some(old),
                }
            }
            Either::Left(b'*') => curr.mult(expr_inner(
                engine,
                crate::engine::gullet::methods::read_int_byte,
                crate::engine::gullet::methods::read_int_command,
                tk,
            )?),
            Either::Left(b'/') => curr.div(expr_inner(
                engine,
                crate::engine::gullet::methods::read_int_byte,
                crate::engine::gullet::methods::read_int_command,
                tk,
            )?),
            Either::Left(_) => unreachable!(),
        }
    }
}

pub fn currentgrouplevel<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok((engine.state.get_group_level() as i32).into())
}
/*\currentgrouptype returns a number representing the type of the innermost
group:
0: bottom level (no group)
1: simple group
2: hbox group
// 3: adjusted hbox group
4: vbox group
// 5: vtop group
// 6: align group
// 7: no align group
// 8: output group
// 9: math group
// 10: disc group
// 11: insert group
// 12: vcenter group
// 13: math choice group
14: semi simple group
15: math shift group
16: math left group
 */
pub fn currentgrouptype<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(match engine.state.get_group_type() {
        None => ET::Int::default(),
        Some(gt) => ET::Int::from(gt.to_byte() as i32),
    })
}

pub fn detokenize<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(false, &tk)?;
    let mut f = |t| exp.push(t);
    let escapechar = engine.state.get_escape_char();
    let g = |a: &mut EngineAux<ET>, st: &<ET as EngineTypes>::State, t: ET::Token, f: &mut _| {
        let mut tokenizer = Otherize::new(f);
        match t.to_enum() {
            StandardToken::Character(c, _) => tokenizer.push_char(c),
            StandardToken::ControlSequence(cs) => tokenizer.push_cs(
                cs,
                a.memory.cs_interner(),
                st.get_catcode_scheme(),
                escapechar,
            ),
            StandardToken::Primitive(id) => {
                tokenizer.push_cs(
                    a.memory.cs_interner_mut().cs_from_str("pdfprimitive"),
                    a.memory.cs_interner(),
                    st.get_catcode_scheme(),
                    escapechar,
                );
                let name = a
                    .memory
                    .cs_interner_mut()
                    .cs_from_str(&id.display::<ET::Char>(None).to_string());
                tokenizer.push_cs(
                    name,
                    a.memory.cs_interner(),
                    st.get_catcode_scheme(),
                    escapechar,
                );
            }
        }
    };
    engine.read_until_endgroup(&tk, |a, st, t| {
        match t.command_code() {
            CommandCode::Space => f(t),
            CommandCode::Parameter => {
                g(a, st, t.clone(), &mut f);
                g(a, st, t, &mut f)
            }
            _ => g(a, st, t, &mut f),
        }
        Ok(())
    })?;
    Ok(())
}

pub fn expanded<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    if engine.need_next(false, &tk)?.command_code() == CommandCode::BeginGroup {
    } else {
        TeXError::missing_begingroup(engine.aux, engine.state, engine.mouth)?;
    }
    ET::Gullet::expand_until_endgroup(engine, false, false, &tk, |_, _, t| {
        exp.push(t);
        Ok(())
    })
}

pub fn fontchardp<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(fnt.get_dp(char))
}
pub fn fontcharht<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(fnt.get_ht(char))
}
pub fn fontcharwd<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(fnt.get_wd(char))
}
pub fn fontcharic<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(fnt.get_ic(char))
}

pub fn ifcsname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    let name = engine.read_csname()?;
    Ok(engine.state.get_command(&name).is_some())
}

pub fn ifdefined<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(match engine.need_next(false, &tk)?.to_enum() {
        StandardToken::Character(c, CommandCode::Active) => {
            engine.state.get_ac_command(c).is_some()
        }
        StandardToken::ControlSequence(name) => engine.state.get_command(&name).is_some(),
        _ => true,
    })
}

pub fn iffontchar<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<bool, ET> {
    let font = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(font.has_char(char))
}

pub fn numexpr<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let (i, r) = expr_loop(
        engine,
        crate::engine::gullet::methods::read_int_byte,
        crate::engine::gullet::methods::read_int_command,
        &tk,
    )?;
    if let Some(r) = r {
        if !r.is_cs_or_active() {
            engine.requeue(r)?
        } else {
            match ET::Gullet::char_or_primitive(engine.state, &r) {
                Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.relax => (),
                _ => engine.requeue(r)?,
            }
        }
    }
    Ok(i)
}

pub fn dimexpr<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    let (i, r) = expr_loop(
        engine,
        crate::engine::gullet::methods::read_dim_byte,
        crate::engine::gullet::methods::read_dim_command,
        &tk,
    )?;
    if let Some(r) = r {
        if !r.is_cs_or_active() {
            engine.requeue(r)?
        } else {
            match ET::Gullet::char_or_primitive(engine.state, &r) {
                Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.relax => (),
                _ => engine.requeue(r)?,
            }
        }
    }
    Ok(i)
}

pub fn glueexpr<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Skip<ET::Dim>, ET> {
    let (i, r) = expr_loop(
        engine,
        crate::engine::gullet::methods::read_skip_byte,
        crate::engine::gullet::methods::read_skip_command,
        &tk,
    )?;
    if let Some(r) = r {
        if !r.is_cs_or_active() {
            engine.requeue(r)?
        } else {
            match ET::Gullet::char_or_primitive(engine.state, &r) {
                Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.relax => (),
                _ => engine.requeue(r)?,
            }
        }
    }
    Ok(i)
}

pub fn muexpr<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<MuSkip<ET::MuDim>, ET> {
    fn muskip_byte<ET: EngineTypes>(
        engine: &mut EngineReferences<ET>,
        is_negative: bool,
        b: u8,
    ) -> TeXResult<MuSkip<ET::MuDim>, ET> {
        crate::engine::gullet::methods::read_muskip_byte(engine, is_negative, b, |d, e| {
            crate::engine::gullet::methods::read_muskip_ii(e, d)
        })
    }
    fn muskip_cmd<ET: EngineTypes>(
        engine: &mut EngineReferences<ET>,
        is_negative: bool,
        cmd: TeXCommand<ET>,
        tk: ET::Token,
    ) -> TeXResult<MuSkip<ET::MuDim>, ET> {
        crate::engine::gullet::methods::read_muskip_command(
            engine,
            is_negative,
            cmd,
            tk,
            |d, e| crate::engine::gullet::methods::read_muskip_ii(e, d),
            Ok,
        )
    }
    let (i, r) = expr_loop(engine, muskip_byte, muskip_cmd, &tk)?;
    if let Some(r) = r {
        if !r.is_cs_or_active() {
            engine.requeue(r)?
        } else {
            match ET::Gullet::char_or_primitive(engine.state, &r) {
                Some(CharOrPrimitive::Primitive(name)) if name == PRIMITIVES.relax => (),
                _ => engine.requeue(r)?,
            }
        }
    }
    Ok(i)
}

pub fn lastnodetype<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let data = engine.stomach.data_mut();
    Ok(match data.open_lists.last() {
        None => match data.page.last() {
            None => (-1).into(),
            Some(n) => (n.nodetype().to_u8() as i32).into(),
        },
        Some(NodeList::Vertical { children, .. }) => match children.last() {
            None => (-1).into(),
            Some(n) => (n.nodetype().to_u8() as i32).into(),
        },
        Some(NodeList::Horizontal { children, .. }) => match children.last() {
            None => (-1).into(),
            Some(n) => (n.nodetype().to_u8() as i32).into(),
        },
        Some(NodeList::Math { children, .. }) => match children.list().last() {
            None => (-1).into(),
            Some(n) => (n.nodetype().to_u8() as i32).into(),
        },
    })
}

pub fn protected<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    outer: bool,
    long: bool,
    _protected: bool,
    globally: bool,
) -> TeXResult<(), ET> {
    crate::expand_loop!(engine,token,
        ResolvedToken::Cmd(Some(TeXCommand::Primitive{name,..})) => match *name {
            n if n == PRIMITIVES.outer => return super::tex::outer(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.long => return super::tex::long(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.protected => return self::protected(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.global => return super::tex::global(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.def => return super::tex::def(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.edef => return super::tex::edef(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.xdef => return super::tex::xdef(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.gdef => return super::tex::gdef(engine,token,outer,long,true,globally),
            n if n == PRIMITIVES.relax => (),
            r => {
                let s = r.display(engine.state.get_escape_char()).to_string();
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

pub fn readline<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    globally: bool,
) -> TeXResult<(), ET> {
    let idx = engine.read_file_index(&tk)?;
    if !engine.read_keyword("to".as_bytes())? {
        TeXError::missing_keyword(engine.aux, engine.state, engine.mouth, &["to"])?
    }
    let cs = engine.read_control_sequence(&tk)?;
    let mut ret = shared_vector::Vector::new();
    engine
        .filesystem
        .readline(idx, engine.state, |t| ret.push(t))?;
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

pub fn scantokens<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(false, &tk)?;
    let mut ret: Vec<Box<[ET::Char]>> = vec![];
    let mut curr = vec![];
    let mut f = |c: ET::Char| {
        if matches!(c.try_into(), Ok(b'\n')) {
            if curr.last().copied() == Some(b'\r'.into()) {
                curr.pop();
            }
            ret.push(std::mem::take(&mut curr).into());
        } else {
            curr.push(c)
        }
    };
    let escapechar = engine.state.get_escape_char();
    engine.read_until_endgroup(&tk, |a, state, t| {
        match t.to_enum() {
            //StandardToken::Character(c,CommandCode::Parameter) => {f(c);f(c)},
            StandardToken::Character(c, _) => f(c),
            StandardToken::ControlSequence(cs) => {
                if let Some(esc) = escapechar {
                    f(esc);
                }
                let res = a.memory.cs_interner().resolve(&cs);
                for c in res.iter() {
                    f(c)
                }
                if res.len() == 1 {
                    let c = res.iter().next().unwrap();
                    if state.get_catcode_scheme().get(c) == &CategoryCode::Letter {
                        f(b' '.into())
                    }
                } else {
                    f(b' '.into())
                }
            }
            StandardToken::Primitive(id) => {
                if let Some(esc) = escapechar {
                    f(esc);
                }
                for c in ET::Char::string_to_iter("pdfprimitive") {
                    f(c)
                }
                f(b' '.into());
                let s = id.display::<ET::Char>(None).to_string();
                let res = ET::Char::string_to_iter(&s);
                for c in res {
                    f(c)
                }
                let mut res = ET::Char::string_to_iter(&s);
                if res.len() == 1 {
                    let c = res.next().unwrap();
                    if state.get_catcode_scheme().get(c) == &CategoryCode::Letter {
                        f(b' '.into())
                    }
                } else {
                    f(b' '.into())
                }
            }
        }
        Ok(())
    })?;
    if !curr.is_empty() {
        ret.push(curr.into())
    }
    engine.mouth.push_string(ret.into());
    Ok(())
}

pub fn unexpanded<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(false, &tk)?;
    engine.read_until_endgroup(&tk, |_, _, t| {
        exp.push(t);
        Ok(())
    })?;
    Ok(())
}

pub fn unless<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let t = engine.need_next(false, &tk)?;
    match engine.resolve(&t) {
        ResolvedToken::Cmd(Some(TeXCommand::Primitive {
            name,
            cmd: PrimitiveCommand::Conditional(cnd),
        })) => ET::Gullet::do_conditional(engine, *name, t, *cnd, true),
        _ => {
            let s = t
                .display(
                    engine.aux.memory.cs_interner(),
                    engine.state.get_catcode_scheme(),
                    engine.state.get_escape_char(),
                )
                .to_string();
            engine.general_error(format!("You can't use `\\unless` before {}", s))
        }
    }
}

pub fn middle<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    match engine.state.get_group_type() {
        Some(GroupType::LeftRight) => (),
        _ => {
            return engine.general_error(
                "You can't use `\\middle` outside of a `\\left`-`\\right` pair".to_string(),
            );
        }
    }
    let del = match engine.read_opt_delimiter(&tk)? {
        None => return engine.general_error("Delimiter expected after `\\middle`".to_string()),
        Some(c) => c,
    };
    ET::Stomach::add_node_m(
        engine,
        MathNode::Atom(MathAtom {
            sub: None,
            sup: None,
            nucleus: MathNucleus::Middle(del.small.char, del.small.style),
        }),
    );
    Ok(())
}

pub fn marks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 {
        return engine.general_error(format!("Illegal \\marks register: {i}"));
    }
    super::methods::do_marks(engine, i as usize, &tk)
}

pub fn topmarks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 {
        return engine.general_error(format!("Illegal \\topmarks register: {i}"));
    }
    super::methods::get_marks(engine, exp, |d| &mut d.topmarks, i as usize);
    Ok(())
}
pub fn firstmarks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 {
        return engine.general_error(format!("Illegal \\firstmarks register: {i}"));
    }
    super::methods::get_marks(engine, exp, |d| &mut d.firstmarks, i as usize);
    Ok(())
}
pub fn botmarks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 {
        return engine.general_error(format!("Illegal \\botmarks register: {i}"));
    }
    super::methods::get_marks(engine, exp, |d| &mut d.botmarks, i as usize);
    Ok(())
}
pub fn splitfirstmarks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 {
        return engine.general_error(format!("Illegal \\splitfirstmarks register: {i}"));
    }
    super::methods::get_marks(engine, exp, |d| &mut d.splitfirstmarks, i as usize);
    Ok(())
}
pub fn splitbotmarks<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let i = engine.read_int(false, &tk)?.into();
    if i < 0 {
        return engine.general_error(format!("Illegal \\splitbotmarks register: {i}"));
    }
    super::methods::get_marks(engine, exp, |d| &mut d.splitbotmarks, i as usize);
    Ok(())
}

pub fn glueshrinkorder<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let dim = engine.read_skip(false, &tk)?;
    Ok(match dim.shrink {
        None | Some(StretchShrink::Dim(_)) => ET::Int::from(0),
        Some(StretchShrink::Fil(_)) => ET::Int::from(1),
        Some(StretchShrink::Fill(_)) => ET::Int::from(2),
        Some(StretchShrink::Filll(_)) => ET::Int::from(2),
    })
}
pub fn gluestretchorder<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    let dim = engine.read_skip(false, &tk)?;
    Ok(match dim.stretch {
        None | Some(StretchShrink::Dim(_)) => ET::Int::from(0),
        Some(StretchShrink::Fil(_)) => ET::Int::from(1),
        Some(StretchShrink::Fill(_)) => ET::Int::from(2),
        Some(StretchShrink::Filll(_)) => ET::Int::from(2),
    })
}
pub fn glueshrink<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    use crate::tex::numerics::TeXDimen;
    let dim = engine.read_skip(false, &tk)?;
    Ok(match dim.shrink {
        None => ET::Dim::default(),
        Some(StretchShrink::Dim(d)) => d,
        Some(StretchShrink::Fil(i)) => ET::Dim::from_sp(i),
        Some(StretchShrink::Fill(i)) => ET::Dim::from_sp(i),
        Some(StretchShrink::Filll(i)) => ET::Dim::from_sp(i),
    })
}
pub fn gluestretch<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Dim, ET> {
    use crate::tex::numerics::TeXDimen;
    let dim = engine.read_skip(false, &tk)?;
    Ok(match dim.stretch {
        None => ET::Dim::default(),
        Some(StretchShrink::Dim(d)) => d,
        Some(StretchShrink::Fil(i)) => ET::Dim::from_sp(i),
        Some(StretchShrink::Fill(i)) => ET::Dim::from_sp(i),
        Some(StretchShrink::Filll(i)) => ET::Dim::from_sp(i),
    })
}

pub fn splitdiscards<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    Ok(()) // dummy
}

const PRIMITIVE_INTS: &[&str] = &[
    "savinghyphcodes",
    "tracingassigns",
    "tracinggroups",
    "tracingifs",
    "tracingnesting",
    "tracingscantokens",
    "savingvdiscards",
    "predisplaydirection",
    "interactionmode",
];

const PRIMITIVE_TOKS: &[&str] = &["everyeof"];

pub fn register_etex_primitives<E: TeXEngine>(engine: &mut E) {
    register_primitive_int(engine, PRIMITIVE_INTS);
    register_primitive_toks(engine, PRIMITIVE_TOKS);

    register_int(engine, "numexpr", numexpr, None);
    register_dim(engine, "dimexpr", dimexpr, None);
    register_skip(engine, "glueexpr", glueexpr, None);
    register_muskip(engine, "muexpr", muexpr, None);

    register_int(engine, "currentgrouplevel", currentgrouplevel, None);
    register_int(engine, "currentgrouptype", currentgrouptype, None);
    register_int(engine, "lastnodetype", lastnodetype, None);
    register_int(engine, "eTeXversion", eTeXversion, None);
    register_int(engine, "glueshrinkorder", glueshrinkorder, None);
    register_int(engine, "gluestretchorder", gluestretchorder, None);

    register_dim(engine, "fontchardp", fontchardp, None);
    register_dim(engine, "fontcharht", fontcharht, None);
    register_dim(engine, "fontcharwd", fontcharwd, None);
    register_dim(engine, "fontcharic", fontcharic, None);
    register_dim(engine, "glueshrink", glueshrink, None);
    register_dim(engine, "gluestretch", gluestretch, None);

    register_assignment(engine, "protected", |e, cmd, g| {
        protected(e, cmd, false, false, false, g)
    });
    register_assignment(engine, "readline", readline);

    register_conditional(engine, "ifcsname", ifcsname);
    register_conditional(engine, "ifdefined", ifdefined);
    register_conditional(engine, "iffontchar", iffontchar);

    register_expandable(engine, "detokenize", detokenize);
    register_expandable(engine, "expanded", expanded);
    register_expandable(engine, "unexpanded", unexpanded);
    register_expandable(engine, "eTeXrevision", eTeXrevision);

    register_unexpandable(engine, "marks", CommandScope::Any, marks);
    register_unexpandable(engine, "middle", CommandScope::MathOnly, middle);

    register_simple_expandable(engine, "unless", unless);
    register_simple_expandable(engine, "scantokens", scantokens);

    register_expandable(engine, "topmarks", topmarks);
    register_expandable(engine, "firstmarks", firstmarks);
    register_expandable(engine, "botmarks", botmarks);
    register_expandable(engine, "splitfirstmarks", splitfirstmarks);
    register_expandable(engine, "splitbotmarks", splitbotmarks);

    register_unexpandable(engine, "splitdiscards", CommandScope::Any, splitdiscards);

    cmtodo!(engine, beginL);
    cmtodo!(engine, beginR);
    cmtodo!(engine, clubpenalties);
    cmtodo!(engine, currentifbranch);
    cmtodo!(engine, currentiflevel);
    cmtodo!(engine, currentiftype);
    cmtodo!(engine, displaywidowpenalties);
    cmtodo!(engine, endL);
    cmtodo!(engine, endR);
    cmtodo!(engine, gluetomu);
    cmtodo!(engine, interlinepenalties);
    cmtodo!(engine, lastlinefit);
    cmtodo!(engine, mutoglue);
    cmtodo!(engine, pagediscards);
    cmtodo!(engine, parshapedimen);
    cmtodo!(engine, parshapeindent);
    cmtodo!(engine, parshapelength);
    cmtodo!(engine, showgroups);
    cmtodo!(engine, showifs);
    cmtodo!(engine, showtokens);
    //cmtodo!(engine, splitdiscards);
    cmtodo!(engine, TeXXeTstate);
    cmtodo!(engine, widowpenalties);
    /*
       register_int!(currentgrouplevel,engine,(e,c) => currentgrouplevel::<ET>(e,&c));
       register_expandable!(detokenize,engine,(e,c,f) =>detokenize::<ET>(e,&c,f));
       register_dim!(dimexpr,engine,(e,c) => dimexpr::<ET>(e,&c));
       register_expandable!(eTeXrevision,engine,(e,c,f) => eTeXrevision::<ET>(e,&c,f));
       register_int!(eTeXversion,engine,(e,c) => eTeXversion::<ET>(&c));
       register_tok_assign!(everyeof,engine);
       register_expandable!(expanded,engine,(e,c,f) => expanded::<ET>(e,&c,f));
       register_dim!(fontchardp,engine,(e,c) => fontchardp::<ET>(e,&c));
       register_dim!(fontcharht,engine,(e,c) => fontcharht::<ET>(e,&c));
       register_dim!(fontcharic,engine,(e,c) => fontcharic::<ET>(e,&c));
       register_dim!(fontcharwd,engine,(e,c) => fontcharwd::<ET>(e,&c));
       register_skip!(glueexpr,engine,(e,c) => glueexpr::<ET>(e,&c));
       register_conditional!(ifcsname,engine,(eu,cmd) =>ifcsname::<ET>(eu,&cmd));
       register_conditional!(ifdefined,engine,(eu,cmd) =>ifdefined::<ET>(eu,&cmd));
       register_conditional!(iffontchar,engine,(e,cmd) =>iffontchar::<ET>(e,&cmd));
       register_int!(lastnodetype,engine,(e,c) => lastnodetype::<ET>(e,&c));
       register_unexpandable!(marks,engine,None,(e,cmd) =>marks::<ET>(e,&cmd));
       register_expandable!(topmarks,engine,(e,c,f) =>topmarks::<ET>(e,&c,f));
       register_expandable!(firstmarks,engine,(e,c,f) =>firstmarks::<ET>(e,&c,f));
       register_expandable!(botmarks,engine,(e,c,f) =>botmarks::<ET>(e,&c,f));
       register_expandable!(splitfirstmarks,engine,(e,c,f) =>splitfirstmarks::<ET>(e,&c,f));
       register_expandable!(splitbotmarks,engine,(e,c,f) =>splitbotmarks::<ET>(e,&c,f));
       register_muskip!(muexpr,engine,(e,c) => muexpr::<ET>(e,&c));
       register_int!(numexpr,engine,(e,c) => numexpr::<ET>(e,&c));
       register_assign!(readline,engine,(eu,cmd,global) =>readline::<ET>(eu,&cmd,global));
       register_expandable_notk!(scantokens,engine,(e,c) =>scantokens::<ET>(e,&c));
       register_assign!(protected,engine,(eu,cmd,g) =>protected::<ET>(eu,&cmd,g,false,false,false));
       register_expandable!(unexpanded,engine,(e,c,f) => unexpanded::<ET>(e,&c,f));
       register_expandable_notk!(unless,engine,(eu,cmd) =>unless::<ET>(eu,&cmd));


    */
}
