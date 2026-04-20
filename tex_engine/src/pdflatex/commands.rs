#![allow(clippy::missing_errors_doc, clippy::needless_pass_by_value)]

use md5::Digest;

use super::nodes::{
    ColorStackAction, NumOrName, PDFAnnot, PDFBoxSpec, PDFCatalog, PDFColor, PDFDest, PDFExtension,
    PDFImage, PDFLiteral, PDFLiteralOption, PDFNode, PDFObj, PDFOutline, PDFStartLink, PDFXForm,
    PDFXImage,
};
use crate::commands::primitives::*;
use crate::commands::CommandScope;
use crate::engine::filesystem::{File, FileSystem};
use crate::engine::fontsystem::Font;
use crate::engine::gullet::methods::CSOrActiveChar;
use crate::engine::gullet::Gullet;
use crate::engine::state::State;
use crate::engine::stomach::Stomach;
use crate::engine::stomach::TeXMode;
use crate::engine::{EngineReferences, EngineTypes, TeXEngine};
use crate::pdflatex::{FileWithMD5, FontWithLpRp};
use crate::prelude::CSHandler;
use crate::prelude::Character;
use crate::tex::catcodes::CommandCode;
use crate::tex::nodes::horizontal::HNode;
use crate::tex::nodes::math::MathNode;
use crate::tex::nodes::vertical::VNode;
use crate::tex::nodes::WhatsitFunction;
use crate::tex::numerics::NumSet;
use crate::tex::tokens::token_lists::Otherize;
use crate::tex::tokens::{StandardToken, Token};
use crate::utils::errors::{TeXError, TeXResult};
use std::fmt::Write;

pub fn pdftexversion<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(<ET::Num as NumSet>::Int::from(140))
}

pub fn pdfmajorversion<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(<ET::Num as NumSet>::Int::from(1))
}

pub fn pdftexrevision<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    exp.push(ET::Token::from_char_cat(b'2'.into(), CommandCode::Other));
    exp.push(ET::Token::from_char_cat(b'5'.into(), CommandCode::Other));
    Ok(())
}

pub fn pdfcatalog<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let mut literal = String::new();
    engine.read_braced_string(true, true, &tk, &mut literal)?;
    let action = if engine.read_keyword(b"openaction")? {
        Some(super::nodes::action_spec(engine, &tk)?)
    } else {
        None
    };
    let node = PDFNode::PDFCatalog(PDFCatalog { literal, action });
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfcolorstack<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let index = engine.read_int(false, &tk)?.into();
    if index < 0 || index >= (engine.aux.extension.colorstacks().len() as i64) {
        engine.general_error(format!("Unknown color stack number {}", index))?;
    }
    let index = index as usize;
    let kw = engine.read_keywords(&[b"push", b"pop", b"set", b"current"])?;
    match kw {
        Some(b"current") => crate::add_node!(ET::Stomach;engine,
                                 VNode::Custom(PDFNode::Color(ColorStackAction::Current(index)).into()),
                                 HNode::Custom(PDFNode::Color(ColorStackAction::Current(index)).into()),
                                 MathNode::Custom(PDFNode::Color(ColorStackAction::Current(index)).into())
        ),
        Some(b"pop") => {
            crate::add_node!(ET::Stomach;engine,
                                     VNode::Custom(PDFNode::Color(ColorStackAction::Pop(index)).into()),
                                     HNode::Custom(PDFNode::Color(ColorStackAction::Pop(index)).into()),
                                     MathNode::Custom(PDFNode::Color(ColorStackAction::Pop(index)).into())
            )
        }
        Some(b"set") => {
            let mut color = String::new();
            engine.read_braced_string(true, true, &tk, &mut color)?;
            let color = PDFColor::parse(color);
            crate::add_node!(ET::Stomach;engine,
                                     VNode::Custom(PDFNode::Color(ColorStackAction::Set(index,color)).into()),
                                     HNode::Custom(PDFNode::Color(ColorStackAction::Set(index,color)).into()),
                                     MathNode::Custom(PDFNode::Color(ColorStackAction::Set(index,color)).into())
            )
        }
        Some(b"push") => {
            let mut color = String::new();
            engine.read_braced_string(true, true, &tk, &mut color)?;
            let color = PDFColor::parse(color);
            crate::add_node!(ET::Stomach;engine,
                                     VNode::Custom(PDFNode::Color(ColorStackAction::Push(index,color)).into()),
                                     HNode::Custom(PDFNode::Color(ColorStackAction::Push(index,color)).into()),
                                     MathNode::Custom(PDFNode::Color(ColorStackAction::Push(index,color)).into())
            )
        }
        _ => TeXError::missing_keyword(
            engine.aux,
            engine.state,
            engine.mouth,
            &["current", "pop", "set", "push"],
        )?,
    }
    Ok(())
}

pub fn pdfcolorstackinit<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    engine.read_keyword(b"page")?;
    engine.read_keyword(b"direct")?;
    let mut color = String::new();
    engine.read_braced_string(false, true, &tk, &mut color)?;
    let color = PDFColor::parse(color);
    let idx = engine.aux.extension.colorstacks().len() as i32;
    engine.aux.extension.colorstacks().push(vec![color]);
    Ok(ET::Int::from(idx))
}

pub fn pdfdest<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let structnum = if engine.read_keyword(b"struct")? {
        Some(engine.read_int(false, &tk)?.into())
    } else {
        None
    };
    let id = match super::nodes::num_or_name(engine, &tk)? {
        Some(n) => n,
        _ => {
            TeXError::missing_keyword(engine.aux, engine.state, engine.mouth, &["name", "num"])?;
            NumOrName::Num(0)
        }
    };
    let dest = super::nodes::pdfdest_type(engine, &tk)?;
    let node = PDFNode::PDFDest(PDFDest {
        structnum,
        id,
        dest,
    });
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfstartlink<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let mut width = None;
    let mut height = None;
    let mut depth = None;
    loop {
        match engine.read_keywords(&[b"width", b"height", b"depth"])? {
            Some(b"width") => width = Some(engine.read_dim(false, &tk)?),
            Some(b"height") => height = Some(engine.read_dim(false, &tk)?),
            Some(b"depth") => depth = Some(engine.read_dim(false, &tk)?),
            _ => break,
        }
    }
    let attr = if engine.read_keyword(b"attr")? {
        let mut attr = String::new();
        engine.read_braced_string(true, true, &tk, &mut attr)?;
        Some(attr)
    } else {
        None
    };
    let action = super::nodes::action_spec(engine, &tk)?;
    let node = PDFNode::PDFStartLink(PDFStartLink {
        width,
        height,
        depth,
        attr,
        action,
    });
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfendlink<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    crate::add_node!(ET::Stomach;engine,
        VNode::Custom(PDFNode::PDFEndLink.into()),
        HNode::Custom(PDFNode::PDFEndLink.into()),
        MathNode::Custom(PDFNode::PDFEndLink.into())
    );
    Ok(())
}

pub fn pdfsave<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    crate::add_node!(ET::Stomach;engine,
        VNode::Custom(PDFNode::PDFSave.into()),
        HNode::Custom(PDFNode::PDFSave.into()),
        MathNode::Custom(PDFNode::PDFSave.into())
    );
    Ok(())
}
pub fn pdfrestore<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    crate::add_node!(ET::Stomach;engine,
        VNode::Custom(PDFNode::PDFRestore.into()),
        HNode::Custom(PDFNode::PDFRestore.into()),
        MathNode::Custom(PDFNode::PDFRestore.into())
    );
    Ok(())
}

pub fn pdfsetmatrix<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let mut str = engine.aux.memory.get_string();
    engine.read_braced_string(true, true, &tk, &mut str)?;
    let mut scale = 0f32;
    let mut rotate = 0f32;
    let mut skewx = 0f32;
    let mut skewy = 0f32;
    for (i, s) in str.split(|c: char| c.is_ascii_whitespace()).enumerate() {
        let f = match s.parse::<f32>() {
            Ok(f) => f,
            _ => {
                engine.general_error(
                    "pdfTeX error (\\pdfsetmatrix): Unrecognized format".to_string(),
                )?;
                1.0
            }
        };
        match i {
            0 => scale = f,
            1 => rotate = f,
            2 => skewx = f,
            3 => skewy = f,
            _ => engine
                .general_error("pdfTeX error (\\pdfsetmatrix): Unrecognized format".to_string())?,
        }
    }
    engine.aux.memory.return_string(str);
    crate::add_node!(ET::Stomach;engine,
        VNode::Custom(PDFNode::PDFMatrix{scale,rotate,skewx,skewy}.into()),
        HNode::Custom(PDFNode::PDFMatrix{scale,rotate,skewx,skewy}.into()),
        MathNode::Custom(PDFNode::PDFMatrix{scale,rotate,skewx,skewy}.into())
    );
    Ok(())
}

pub fn pdfinfo<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.skip_argument(&tk)
}

pub fn ifincsname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    Ok(*engine.gullet.csnames() > 0)
}
pub fn ifpdfabsnum<ET: EngineTypes>(
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

    let first = if first < <<ET as EngineTypes>::Num as NumSet>::Int::default() {
        -first
    } else {
        first
    };
    let second = if second < <<ET as EngineTypes>::Num as NumSet>::Int::default() {
        -second
    } else {
        second
    };
    Ok(match rel {
        b'=' => first == second,
        b'<' => first < second,
        b'>' => first > second,
        _ => unreachable!(),
    })
}
pub fn ifpdfabsdim<ET: EngineTypes>(
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
    let first = if first < <<ET as EngineTypes>::Num as NumSet>::Dim::default() {
        -first
    } else {
        first
    };
    let second = if second < <<ET as EngineTypes>::Num as NumSet>::Dim::default() {
        -second
    } else {
        second
    };
    Ok(match rel {
        b'=' => first == second,
        b'<' => first < second,
        b'>' => first > second,
        _ => unreachable!(),
    })
}
pub fn ifpdfprimitive<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<bool, ET> {
    use crate::engine::mouth::Mouth;
    engine.general_error(format!(
        "Not yet implemented: \\ifpdfprimitive at {}",
        engine.mouth.current_sourceref().display(engine.filesystem)
    ))?;
    Ok(false)
}

pub fn lpcode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET>
where
    ET::Font: FontWithLpRp,
{
    let fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(fnt.get_lp(char))
}
pub fn lpcode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET>
where
    ET::Font: FontWithLpRp,
{
    let mut fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    let code = engine.read_int(true, &tk)?;
    fnt.set_lp(char, code);
    Ok(())
}
pub fn rpcode_get<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<ET::Int, ET>
where
    ET::Font: FontWithLpRp,
{
    let fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    Ok(fnt.get_rp(char))
}
pub fn rpcode_set<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
    _globally: bool,
) -> TeXResult<(), ET>
where
    ET::Font: FontWithLpRp,
{
    let mut fnt = engine.read_font(false, &tk)?;
    let char = engine.read_charcode(false, &tk)?;
    let code = engine.read_int(true, &tk)?;
    fnt.set_rp(char, code);
    Ok(())
}

pub fn leftmarginkern<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    // todo
    let _ = engine.read_int(false, &tk)?;
    Otherize::new(&mut |t| exp.push(t)).write_str("0pt")?;
    Ok(())
}
pub fn rightmarginkern<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    // todo
    let _ = engine.read_int(false, &tk)?;
    Otherize::new(&mut |t| exp.push(t)).write_str("0pt")?;
    Ok(())
}

pub fn pdfcreationdate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    use chrono::{Datelike, Timelike};
    let dt = engine.aux.start_time;
    let mut f = |t| exp.push(t);
    let mut tk = Otherize::new(&mut f);
    write!(
        tk,
        "D:{}{:02}{:02}{:02}{:02}{:02}{}'",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.offset().to_string().replace(':', "'")
    )?;
    Ok(())
}

pub fn pdffilemoddate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    use chrono::{Datelike, Timelike};
    let mut filename = engine.aux.memory.get_string();
    engine.read_braced_string(true, false, &tk, &mut filename)?;
    let f = engine.filesystem.get(&filename);
    engine.aux.memory.return_string(filename);
    let path = f.path();
    if let Ok(Ok(st)) = std::fs::metadata(path).map(|md| md.modified()) {
        let dt: chrono::DateTime<chrono::Local> = chrono::DateTime::from(st);
        let mut f = |t| exp.push(t);
        let mut tk = Otherize::new(&mut f);
        write!(
            tk,
            "D:{}{:02}{:02}{:02}{:02}{:02}{}'",
            dt.year(),
            dt.month(),
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            dt.offset().to_string().replace(':', "'")
        )?;
    }
    Ok(())
}

pub fn pdfescapestring<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    use crate::tex::characters::Character;
    engine.expand_until_bgroup(false, &tk)?;
    engine.expand_until_endgroup(true, false, &tk, |_, _, t| {
        match t.to_enum() {
            StandardToken::Character(_, CommandCode::Space) => ET::Char::string_to_iter("\\040")
                .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other))),
            StandardToken::Character(c, _) if c == ET::Char::from(b' ') => {
                ET::Char::string_to_iter("\\040")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b'(') => {
                ET::Char::string_to_iter("\\(")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b')') => {
                ET::Char::string_to_iter("\\)")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b'\\') => {
                ET::Char::string_to_iter("\\\\")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b'#') => {
                ET::Char::string_to_iter("\\#")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) => {
                exp.push(ET::Token::from_char_cat(c, CommandCode::Other))
            }
            StandardToken::Primitive(_) => (),
            StandardToken::ControlSequence(_) => (),
        };
        Ok(())
    })?;
    Ok(())
}

pub fn pdfescapename<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    use crate::tex::characters::Character;
    engine.expand_until_bgroup(false, &tk)?;
    engine.expand_until_endgroup(true, false, &tk, |_, _, t| {
        match t.to_enum() {
            StandardToken::Character(_, CommandCode::Space) => ET::Char::string_to_iter("#20")
                .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other))),
            StandardToken::Character(c, _) if c == ET::Char::from(b' ') => {
                ET::Char::string_to_iter("#20")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b'(') => {
                ET::Char::string_to_iter("#28")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b')') => {
                ET::Char::string_to_iter("#29")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b'\\') => {
                ET::Char::string_to_iter("#5C")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) if c == ET::Char::from(b'#') => {
                ET::Char::string_to_iter("#23")
                    .for_each(|c| exp.push(ET::Token::from_char_cat(c, CommandCode::Other)))
            }
            StandardToken::Character(c, _) => {
                exp.push(ET::Token::from_char_cat(c, CommandCode::Other))
            }
            StandardToken::Primitive(_) => (),
            StandardToken::ControlSequence(_) => (),
        };
        Ok(())
    })?;
    Ok(())
}

pub fn pdfescapehex<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    use crate::tex::characters::Character;
    engine.expand_until_bgroup(false, &tk)?;
    engine.expand_until_endgroup(true, false, &tk, |_, _, t| {
        match t.to_enum() {
            StandardToken::Character(c, _) => {
                let num = c.into();
                for c in ET::Char::string_to_iter(&format!("{:02X}", num)) {
                    exp.push(ET::Token::from_char_cat(c, CommandCode::Other))
                }
            }
            StandardToken::Primitive(_) => (),
            StandardToken::ControlSequence(_) => (),
        };
        Ok(())
    })?;
    Ok(())
}
pub fn pdfunescapehex<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    engine.expand_until_bgroup(false, &tk)?;
    let mut s = String::new();
    engine.expand_until_endgroup(true, false, &tk, |_, _, t| {
        match t.to_enum() {
            StandardToken::Character(c, _) if s.is_empty() => {
                let num = c.into();
                if (48..=57).contains(&num) || (65..=70).contains(&num) || (97..=102).contains(&num)
                {
                    s.push((num as u8).into());
                } else {
                    // TODO
                }
            }
            StandardToken::Character(c, _) => {
                let num = c.into();
                if (48..=57).contains(&num) || (65..=70).contains(&num) || (97..=102).contains(&num)
                {
                    s.push((num as u8).into());
                } else {
                    // TODO
                }
                let c = u8::from_str_radix(&s, 16).unwrap();
                s.clear();
                exp.push(ET::Token::from_char_cat(c.into(), CommandCode::Other));
            }
            StandardToken::Primitive(_) => (),
            StandardToken::ControlSequence(_) => (),
        };
        Ok(())
    })?;
    Ok(())
}

pub fn pdffilesize<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut filename = engine.aux.memory.get_string();
    engine.read_braced_string(false, true, &tk, &mut filename)?;
    let file = engine.filesystem.get(&filename);
    engine.aux.memory.return_string(filename);
    if file.exists() {
        let size = file.size();
        for u in size.to_string().bytes() {
            exp.push(ET::Token::from_char_cat(u.into(), CommandCode::Other));
        }
    }
    Ok(())
}

pub fn pdfglyphtounicode<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    // TODO
    engine.skip_argument(&tk)?;
    engine.skip_argument(&tk)
}

pub fn pdfmatch<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
{
    let icase = engine.read_keyword(b"icase")?;
    let _subcount = if engine.read_keyword(b"subcount")? {
        engine.read_int(false, &tk)?.into()
    } else {
        -1
    }; // TODO use subcount
    let mut pattern_string = String::new();
    let mut target_string = String::new();
    if icase {
        pattern_string.push_str("(?i)");
    }
    engine.read_braced_string(false, true, &tk, &mut pattern_string)?;
    engine.read_braced_string(false, true, &tk, &mut target_string)?;
    let pdfmatches = engine.aux.extension.pdfmatches();
    pdfmatches.clear();

    match regex::Regex::new(&pattern_string) {
        Err(_) => {
            exp.push(ET::Token::from_char_cat(b'-'.into(), CommandCode::Other));
            exp.push(ET::Token::from_char_cat(b'1'.into(), CommandCode::Other));
        }
        Ok(reg) => match reg.captures_iter(&target_string).next() {
            None => exp.push(ET::Token::from_char_cat(b'0'.into(), CommandCode::Other)),
            Some(capture) => {
                let cap = capture.get(0).unwrap();
                pdfmatches.push(format!("{}->{}", cap.start(), cap.as_str()));
                for cap in capture.iter().skip(1) {
                    match cap {
                        None => pdfmatches.push("-1".to_string()),
                        Some(cap) => {
                            pdfmatches.push(format!("{}->{}", cap.start(), cap.as_str()));
                        }
                    }
                }
                exp.push(ET::Token::from_char_cat(b'1'.into(), CommandCode::Other));
            }
        },
    }
    Ok(())
}

pub fn pdflastmatch<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
{
    let i = engine.read_int(false, &tk)?.into();
    let i = if i < 0 { 0usize } else { i as usize };
    match engine.aux.extension.pdfmatches().get(i) {
        None => {
            exp.push(ET::Token::from_char_cat(b'-'.into(), CommandCode::Other));
            exp.push(ET::Token::from_char_cat(b'1'.into(), CommandCode::Other));
        }
        Some(s) => {
            let mut f = |t| exp.push(t);
            let mut t = Otherize::new(&mut f);
            write!(t, "{}", s)?;
        }
    }
    Ok(())
}

pub fn pdfmdfivesum<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::File: FileWithMD5,
{
    let mut f = |t| exp.push(t);
    if engine.read_keyword(b"file")? {
        let mut filename = String::new();
        engine.read_braced_string(true, true, &tk, &mut filename)?;
        let file = engine.filesystem.get(&filename);
        let mut t = Otherize::new(&mut f);
        for i in file.md5() {
            write!(t, "{i:02X}")?;
        }
    } else {
        let mut str = String::new();
        engine.read_braced_string(false, true, &tk, &mut str)?;
        let mut t = Otherize::new(&mut f);
        let mut hasher = md5::Md5::default();
        hasher.update(str.as_bytes());
        let r: [u8; 16] = hasher.finalize().into();
        for i in r {
            write!(t, "{i:02X}")?;
        }
    }
    Ok(())
}

pub fn pdfannot<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let num = match engine.read_keywords(&[b"reserveobjnum", b"useobjnum"])? {
        Some(b"reserveobjnum") => {
            engine.aux.extension.pdfannots().push(PDFAnnot {
                width: None,
                height: None,
                depth: None,
                content: String::new(),
            });
            return Ok(());
        }
        Some(b"useobjnum") => {
            let num = engine.read_int(false, &tk)?.into();
            if num < 0 {
                engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
            }
            let num = num as usize;
            if num >= engine.aux.extension.pdfannots().len() {
                engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
            }
            Some(num)
        }
        _ => None,
    };
    let mut width = None;
    let mut height = None;
    let mut depth = None;
    loop {
        match engine.read_keywords(&[b"width", b"height", b"depth"])? {
            Some(b"width") => width = Some(engine.read_dim(false, &tk)?),
            Some(b"height") => height = Some(engine.read_dim(false, &tk)?),
            Some(b"depth") => depth = Some(engine.read_dim(false, &tk)?),
            _ => break,
        }
    }
    let mut content = String::new();
    engine.read_braced_string(true, true, &tk, &mut content)?;
    let annot = PDFAnnot {
        width,
        height,
        depth,
        content,
    };
    match num {
        None => engine.aux.extension.pdfannots().push(annot.clone()),
        Some(num) => engine.aux.extension.pdfannots()[num] = annot.clone(),
    }
    crate::add_node!(ET::Stomach;engine,
        VNode::Custom(PDFNode::PDFAnnot(annot).into()),
        HNode::Custom(PDFNode::PDFAnnot(annot).into()),
        MathNode::Custom(PDFNode::PDFAnnot(annot).into())
    );
    Ok(())
}

pub fn pdflastannot<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<<ET::Num as NumSet>::Int, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    Ok(<ET::Num as NumSet>::Int::from(
        (engine.aux.extension.pdfannots().len() as i32) - 1,
    ))
}

pub fn parse_pdfobj<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: &ET::Token,
) -> TeXResult<usize, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    match engine.read_keywords(&[b"reserveobjnum", b"useobjnum", b"stream"])? {
        Some(b"reserveobjnum") => {
            engine.aux.extension.pdfobjs().push(PDFObj(String::new()));
            Ok(engine.aux.extension.pdfobjs().len() - 1)
        }
        Some(b"useobjnum") => {
            let num = engine.read_int(false, tk)?.into();
            if num < 0 {
                engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
            }
            let num = num as usize;
            if num >= engine.aux.extension.pdfobjs().len() {
                engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
            }
            let mut str = String::new();
            engine.read_braced_string(false, true, tk, &mut str)?;
            engine.aux.extension.pdfobjs()[num] = PDFObj(str);
            Ok(num)
        }
        Some(b"stream") => {
            if engine.read_keyword(b"attr")? {
                // TODO
            }
            let mut str = String::new();
            engine.read_braced_string(false, true, tk, &mut str)?;
            engine.aux.extension.pdfobjs().push(PDFObj(str));
            Ok(engine.aux.extension.pdfobjs().len() - 1)
        }
        _ => {
            TeXError::missing_keyword(
                engine.aux,
                engine.state,
                engine.mouth,
                &["reserveobjnum", "useobjnum", "stream"],
            )?;
            Ok(0)
        }
    }
}

pub fn pdfobj<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    parse_pdfobj(engine, &tk)?;
    Ok(None)
}
pub fn pdfobj_immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let num = parse_pdfobj(engine, &tk)?;
    let node = PDFNode::Obj(engine.aux.extension.pdfobjs()[num].clone());
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfrefobj<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let num = engine.read_int(false, &tk)?.into();
    if num < 0 {
        engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
    }
    match engine.aux.extension.pdfobjs().get(num as usize) {
        None => {
            engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
        }
        Some(o) => {
            let node = PDFNode::Obj(o.clone());
            crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()))
        }
    }
    Ok(())
}

pub fn pdflastobj<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    Ok(<ET::Num as NumSet>::Int::from(
        (engine.aux.extension.pdfobjs().len() as i32) - 1,
    ))
}

pub fn pdfoutline<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let mut attr = String::new();
    if engine.read_keyword(b"attr")? {
        engine.read_braced_string(true, true, &tk, &mut attr)?;
    }
    let action = super::nodes::action_spec(engine, &tk)?;
    let count = if engine.read_keyword(b"count")? {
        Some(engine.read_int(false, &tk)?.into())
    } else {
        None
    };
    let mut content = String::new();
    engine.read_braced_string(true, true, &tk, &mut content)?;
    let node = PDFNode::PDFOutline(PDFOutline {
        attr,
        action,
        count,
        content,
    });
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn parse_pdfxform<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: &ET::Token,
) -> TeXResult<usize, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    let mut attr = String::new();
    if engine.read_keyword(b"attr")? {
        engine.read_braced_string(true, true, tk, &mut attr)?;
    }
    let mut resources = String::new();
    if engine.read_keyword(b"resources")? {
        engine.read_braced_string(true, true, tk, &mut resources)?;
    }
    let idx = engine.read_register_index(false, tk)?;
    let bx = engine.state.take_box_register(idx);
    engine.aux.extension.pdfxforms().push(PDFXForm {
        attr,
        resources,
        bx,
    });
    Ok(engine.aux.extension.pdfxforms().len() - 1)
}
pub fn pdfxform<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    parse_pdfxform(engine, &tk)?;
    Ok(None)
}
pub fn pdfxform_immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let num = parse_pdfxform(engine, &tk)?;
    let form = engine.aux.extension.pdfxforms()[num].clone();
    let node = PDFNode::XForm(form);
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfrefxform<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let num = engine.read_int(false, &tk)?.into();
    if num < 0 {
        engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
    }
    match engine.aux.extension.pdfxforms().get(num as usize) {
        None => {
            engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
        }
        Some(n) => {
            let node = PDFNode::XForm(n.clone());
            crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()))
        }
    }
    Ok(())
}

pub fn pdflastxform<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    Ok(<ET::Num as NumSet>::Int::from(
        (engine.aux.extension.pdfxforms().len() as i32) - 1,
    ))
}

pub fn pdfximage<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
{
    let mut width: Option<ET::Dim> = None;
    let mut height: Option<ET::Dim> = None;
    let mut depth: Option<ET::Dim> = None;
    loop {
        match engine.read_keywords(&[b"width", b"height", b"depth"])? {
            Some(b"width") => width = Some(engine.read_dim(false, &tk)?),
            Some(b"height") => height = Some(engine.read_dim(false, &tk)?),
            Some(b"depth") => depth = Some(engine.read_dim(false, &tk)?),
            _ => break,
        }
    }
    let mut attr = String::new();
    if engine.read_keyword(b"attr")? {
        engine.read_braced_string(true, true, &tk, &mut attr)?
    }
    let page = if engine.read_keyword(b"page")? {
        Some(engine.read_int(false, &tk)?.into())
    } else {
        None
    };
    let colorspace = if engine.read_keyword(b"colorspace")? {
        Some(engine.read_int(false, &tk)?.into())
    } else {
        None
    };
    let boxspec = match engine.read_keywords(&[
        b"mediabox",
        b"cropbox",
        b"bleedbox",
        b"trimbox",
        b"artbox",
    ])? {
        None => None,
        Some(b"mediabox") => Some(PDFBoxSpec::MediaBox),
        Some(b"cropbox") => Some(PDFBoxSpec::CropBox),
        Some(b"bleedbox") => Some(PDFBoxSpec::BleedBox),
        Some(b"trimbox") => Some(PDFBoxSpec::TrimBox),
        Some(b"artbox") => Some(PDFBoxSpec::ArtBox),
        _ => unreachable!(),
    };
    let mut filename = String::new();
    engine.read_braced_string(true, true, &tk, &mut filename)?;
    let file = engine.filesystem.get(&filename);

    let img = if file.path().extension().is_some_and(|ext| ext == "pdf") {
        super::nodes::pdf_as_image(file.path(), &mut engine.aux.extension)
    } else {
        let Ok(img) = image::ImageReader::open(file.path()) else {
            engine.general_error("Unknown type of image".into())?;
            return Ok(());
        };
        let Ok(Ok(img)) = img.with_guessed_format().map(image::ImageReader::decode) else {
            engine.general_error("Unknown type of image".into())?;
            return Ok(());
        };
        PDFImage::Img(img)
    };
    let img = PDFXImage {
        width,
        height,
        depth,
        attr,
        page,
        colorspace,
        boxspec,
        img,
        filepath: file.path().to_path_buf(),
    };
    engine.aux.extension.pdfximages().push(img);
    Ok(())
}

pub fn pdfrefximage<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let num = engine.read_int(false, &tk)?.into();
    if num < 0 {
        engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
    }
    match engine.aux.extension.pdfximages().get(num as usize) {
        None => {
            engine.general_error("pdfTeX error (ext1): invalid object number.".to_string())?;
        }
        Some(n) => {
            let node = PDFNode::XImage(n.clone());
            crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()))
        }
    }
    Ok(())
}

pub fn pdfpageattr<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    engine.expand_until_bgroup(false, &tk)?;
    let mut v = Vec::new();
    engine.read_until_endgroup(&tk, |_, _, t| {
        v.push(t);
        Ok(())
    })?;
    let node = PDFNode::PDFPageAttr(v.into());
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfpagesattr<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    engine.expand_until_bgroup(false, &tk)?;
    let mut v = Vec::new();
    engine.read_until_endgroup(&tk, |_, _, t| {
        v.push(t);
        Ok(())
    })?;
    let node = PDFNode::PDFPagesAttr(v.into());
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfprimitive<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<(), ET> {
    let name = engine.read_csname()?;
    let s = engine
        .aux
        .memory
        .cs_interner_mut()
        .resolve(&name)
        .to_string();
    match engine.state.primitives().get_name(&s) {
        None => (),
        Some(s) => engine.requeue(ET::Token::primitive(s))?,
    }
    Ok(())
}

pub fn pdflastximage<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET>
where
    ET::Extension: PDFExtension<ET>,
{
    Ok((engine.aux.extension.pdfximages().len() as i32 - 1).into())
}

pub fn pdfliteral<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let _ = engine.read_keyword(b"shipout")?; // TODO
    let option = match engine.read_keywords(&[b"direct", b"page"])? {
        Some(b"direct") => PDFLiteralOption::Direct,
        Some(b"page") => PDFLiteralOption::Page,
        _ => PDFLiteralOption::None,
    };
    let mut literal = String::new();
    engine.read_braced_string(true, true, &tk, &mut literal)?;
    let node = PDFNode::PDFLiteral(PDFLiteral { literal, option });
    crate::add_node!(ET::Stomach;engine,VNode::Custom(node.into()),HNode::Custom(node.into()),MathNode::Custom(node.into()));
    Ok(())
}

pub fn pdfshellescape<ET: EngineTypes>(
    _engine: &mut EngineReferences<ET>,
    _tk: ET::Token,
) -> TeXResult<ET::Int, ET> {
    Ok(<ET::Num as NumSet>::Int::from(2))
}

pub fn pdfstrcmp<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let mut first = String::new();
    let mut second = String::new();
    engine.read_braced_string(false, true, &tk, &mut first)?;
    engine.read_braced_string(false, true, &tk, &mut second)?;
    match first.cmp(&second) {
        std::cmp::Ordering::Less => {
            exp.push(ET::Token::from_char_cat(b'-'.into(), CommandCode::Other));
            exp.push(ET::Token::from_char_cat(b'1'.into(), CommandCode::Other));
        }
        std::cmp::Ordering::Equal => {
            exp.push(ET::Token::from_char_cat(b'0'.into(), CommandCode::Other));
        }
        std::cmp::Ordering::Greater => {
            exp.push(ET::Token::from_char_cat(b'1'.into(), CommandCode::Other));
        }
    }
    Ok(())
}

pub fn pdffontsize<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    exp: &mut Vec<ET::Token>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    let dim = engine.read_font(false, &tk)?.get_at();
    let mut f = |t| exp.push(t);
    let mut t = Otherize::new(&mut f);
    write!(t, "{}", dim)?;
    Ok(())
}

// dummy
pub fn showstream<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<Option<Box<WhatsitFunction<ET>>>, ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    showstream_immediate::<ET>(engine, tk)?;
    Ok(None)
}
pub fn showstream_immediate<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    if engine
        .need_next(false, &tk)?
        .char_value()
        .map(|v| v.to_char())
        == Some('=')
    {
        let _ = engine.need_next(false, &tk)?;
    }
    Ok(())
}

pub fn pdffontexpand<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET> {
    // TODO
    let _ = engine.read_font(false, &tk)?;
    let _ = engine.read_int(false, &tk)?;
    let _ = engine.read_int(false, &tk)?;
    let _ = engine.read_int(false, &tk)?;
    engine.read_keyword(b"autoexpand")?;
    Ok(())
}

pub fn partokenname<ET: EngineTypes>(
    engine: &mut EngineReferences<ET>,
    tk: ET::Token,
) -> TeXResult<(), ET>
where
    ET::Extension: PDFExtension<ET>,
    ET::CustomNode: From<PDFNode<ET>>,
{
    let CSOrActiveChar::Name(par) = engine.read_control_sequence(&tk)? else {
        return TeXResult::Err(TeXError::General(
            "expected control sequence after \\partokenname".to_string(),
        ));
    };
    engine.state.set_par_token(par);
    Ok(())
}

const PRIMITIVE_INTS: &[&str] = &[
    "pdfadjustspacing",
    "pdfcompresslevel",
    "pdfdecimaldigits",
    "pdfdraftmode",
    "pdfgentounicode",
    "pdfminorversion",
    "pdfobjcompresslevel",
    "pdfoutput",
    "pdfpkresolution",
    "pdfprotrudechars",
    "pdfprependkern",
    "pdfappendkern",
    "tracingstacklevels",
    "partokencontext",
];

const PRIMITIVE_DIMS: &[&str] = &[
    "pdfhorigin",
    "pdflinkmargin",
    "pdfpageheight",
    "pdfpagewidth",
    "pdfvorigin",
];

const PRIMITIVE_TOKS: &[&str] = &["pdfpageresources"];

pub fn register_pdftex_primitives<E: TeXEngine>(engine: &mut E)
where
    <E::Types as EngineTypes>::Extension: PDFExtension<E::Types>,
    <E::Types as EngineTypes>::CustomNode: From<PDFNode<E::Types>>,
    <E::Types as EngineTypes>::File: FileWithMD5,
    <E::Types as EngineTypes>::Font: FontWithLpRp,
{
    register_expandable(engine, "leftmarginkern", leftmarginkern);
    register_expandable(engine, "rightmarginkern", rightmarginkern);
    register_expandable(engine, "pdfcreationdate", pdfcreationdate);
    register_expandable(engine, "pdffilemoddate", pdffilemoddate);
    register_expandable(engine, "pdfescapestring", pdfescapestring);
    register_expandable(engine, "pdfescapename", pdfescapename);
    register_expandable(engine, "pdfescapehex", pdfescapehex);
    register_expandable(engine, "pdfunescapehex", pdfunescapehex);
    register_expandable(engine, "pdffilesize", pdffilesize);
    register_expandable(engine, "pdfmatch", pdfmatch);
    register_expandable(engine, "pdflastmatch", pdflastmatch);
    register_expandable(engine, "pdfstrcmp", pdfstrcmp);
    register_expandable(engine, "pdftexrevision", pdftexrevision);
    register_expandable(engine, "pdfmdfivesum", pdfmdfivesum);
    register_expandable(engine, "pdffontsize", pdffontsize);

    register_int(engine, "pdftexversion", pdftexversion, None);
    register_int(engine, "pdfmajorversion", pdfmajorversion, None);
    register_int(engine, "pdfshellescape", pdfshellescape, None);
    register_int(engine, "pdfcolorstackinit", pdfcolorstackinit, None);
    register_int(engine, "lpcode", lpcode_get, Some(lpcode_set));
    register_int(engine, "rpcode", rpcode_get, Some(rpcode_set));

    register_conditional(engine, "ifincsname", ifincsname);
    register_conditional(engine, "ifpdfabsdim", ifpdfabsdim);
    register_conditional(engine, "ifpdfabsnum", ifpdfabsnum);
    register_conditional(engine, "ifpdfprimitive", ifpdfprimitive);

    register_unexpandable(engine, "pdfcatalog", CommandScope::Any, pdfcatalog);
    register_unexpandable(
        engine,
        "pdfglyphtounicode",
        CommandScope::Any,
        pdfglyphtounicode,
    );
    register_unexpandable(engine, "pdfcolorstack", CommandScope::Any, pdfcolorstack);
    register_unexpandable(engine, "pdfdest", CommandScope::Any, pdfdest);
    register_unexpandable(engine, "pdfinfo", CommandScope::Any, pdfinfo);
    register_unexpandable(engine, "pdfliteral", CommandScope::Any, pdfliteral);
    register_unexpandable(engine, "pdffontexpand", CommandScope::Any, pdffontexpand);
    register_unexpandable(engine, "pdfoutline", CommandScope::Any, pdfoutline);
    register_unexpandable(engine, "pdfstartlink", CommandScope::Any, pdfstartlink);
    register_unexpandable(engine, "pdfendlink", CommandScope::Any, pdfendlink);
    register_unexpandable(engine, "pdfsave", CommandScope::Any, pdfsave);
    register_unexpandable(engine, "pdfrestore", CommandScope::Any, pdfrestore);
    register_unexpandable(engine, "pdfsetmatrix", CommandScope::Any, pdfsetmatrix);
    register_unexpandable(engine, "pdfannot", CommandScope::Any, pdfannot);

    register_whatsit(engine, "showstream", showstream, showstream_immediate, None);
    register_whatsit(engine, "pdfobj", pdfobj, pdfobj_immediate, None);
    register_whatsit(engine, "pdfxform", pdfxform, pdfxform_immediate, None);
    register_whatsit(
        engine,
        "pdfpageattr",
        |e, t| {
            pdfpageattr(e, t)?;
            Ok(None)
        },
        pdfpageattr,
        Some(|_, _| Ok(Vec::new())),
    );
    register_whatsit(
        engine,
        "pdfpagesattr",
        |e, t| {
            pdfpagesattr(e, t)?;
            Ok(None)
        },
        pdfpagesattr,
        Some(|_, _| Ok(Vec::new())),
    );
    register_unexpandable(engine, "partokenname", CommandScope::Any, partokenname);
    register_unexpandable(engine, "pdfrefobj", CommandScope::Any, pdfrefobj);
    register_int(engine, "pdflastobj", pdflastobj, None);
    register_unexpandable(engine, "pdfrefxform", CommandScope::Any, pdfrefxform);
    register_int(engine, "pdflastxform", pdflastxform, None);
    register_unexpandable(engine, "pdfximage", CommandScope::Any, pdfximage);
    register_unexpandable(engine, "pdfrefximage", CommandScope::Any, pdfrefximage);
    register_simple_expandable(engine, "pdfprimitive", pdfprimitive);
    register_int(engine, "pdflastximage", pdflastximage, None);
    register_int(engine, "pdflastannot", pdflastannot, None);
    register_simple_expandable(engine, "pdfsavepos", |_, _| Ok(()));
    register_int(
        engine,
        "pdflastxpos",
        |_, _| Ok(<E::Types as EngineTypes>::Int::default()),
        None,
    );
    register_int(
        engine,
        "pdflastypos",
        |_, _| Ok(<E::Types as EngineTypes>::Int::default()),
        None,
    );

    register_primitive_int(engine, PRIMITIVE_INTS);
    register_primitive_dim(engine, PRIMITIVE_DIMS);
    register_primitive_toks(engine, PRIMITIVE_TOKS);

    cmtodos!(engine, pdfelapsedtime, pdfresettimer);

    cmtodo!(engine, efcode);
    cmtodo!(engine, knaccode);
    cmtodo!(engine, knbccode);
    cmtodo!(engine, knbscode);
    cmtodo!(engine, pdfadjustinterwordglue);
    cmtodo!(engine, pdfforcepagebox);
    cmtodo!(engine, pdfgamma);
    cmtodo!(engine, pdfimageapplygamma);
    cmtodo!(engine, pdfimagegamma);
    cmtodo!(engine, pdfimagehicolor);
    cmtodo!(engine, pdfimageresolution);
    cmtodo!(engine, pdfinclusioncopyfonts);
    cmtodo!(engine, pdfinclusionerrorlevel);
    cmtodo!(engine, pdfinfoomitdate);
    cmtodo!(engine, pdfomitcharset);
    cmtodo!(engine, pdfomitinfodict);
    cmtodo!(engine, pdfomitprocset);
    cmtodo!(engine, pdfpagebox);
    cmtodo!(engine, pdfsuppressptexinfo);
    cmtodo!(engine, pdfsuppresswarningdupdest);
    cmtodo!(engine, pdfsuppresswarningdupmap);
    cmtodo!(engine, pdfsuppresswarningpagegroup);
    cmtodo!(engine, pdftracingfonts);
    cmtodo!(engine, pdfuniqueresname);
    cmtodo!(engine, shbscode);
    cmtodo!(engine, stbscode);
    cmtodo!(engine, tagcode);
    cmtodo!(engine, pdflastlink);
    cmtodo!(engine, pdflastximagecolordepth);
    cmtodo!(engine, pdflastximagepages);
    cmtodo!(engine, pdfrandomseed);
    cmtodo!(engine, pdfretval);
    cmtodo!(engine, pdfdestmargin);
    cmtodo!(engine, pdfeachlinedepth);
    cmtodo!(engine, pdfeachlineheight);
    cmtodo!(engine, pdffirstlineheight);
    cmtodo!(engine, pdfignoreddimen);
    cmtodo!(engine, pdflastlinedepth);
    cmtodo!(engine, pdfpxdimen);
    cmtodo!(engine, pdfthreadmargin);
    cmtodo!(engine, pdfpkmode);
    cmtodo!(engine, pdffiledump);
    cmtodo!(engine, pdffontname);
    cmtodo!(engine, pdffontobjnum);
    cmtodo!(engine, pdfincludechars);
    cmtodo!(engine, pdfinsertht);
    cmtodo!(engine, pdfnormaldeviate);
    cmtodo!(engine, pdfpageref);
    cmtodo!(engine, pdftexbanner);
    cmtodo!(engine, pdfuniformdeviate);
    cmtodo!(engine, pdfxformname);
    cmtodo!(engine, pdfximagebbox);

    cmtodo!(engine, letterspacefont);
    cmtodo!(engine, pdfcopyfont);
    cmtodo!(engine, pdfendthread);
    cmtodo!(engine, pdffakespace);
    cmtodo!(engine, pdffontattr);
    cmtodo!(engine, pdfinterwordspaceoff);
    cmtodo!(engine, pdfinterwordspaceon);
    cmtodo!(engine, pdfmapfile);
    cmtodo!(engine, pdfmapline);
    cmtodo!(engine, pdfnames);
    cmtodo!(engine, pdfnobuiltintounicode);
    cmtodo!(engine, pdfnoligatures);
    cmtodo!(engine, pdfrunninglinkoff);
    cmtodo!(engine, pdfrunninglinkon);
    cmtodo!(engine, pdfsetrandomseed);
    cmtodo!(engine, pdfspacefont);
    cmtodo!(engine, pdfthread);
    cmtodo!(engine, pdftrailer);
    cmtodo!(engine, pdftrailerid);
    cmtodo!(engine, pdfstartthread);
    cmtodo!(engine, quitvmode);

    /*
    register_conditional!(ifincsname,engine,(e,cmd) =>ifincsname::<ET>(e,&cmd));
    register_conditional!(ifpdfabsdim,engine,(e,cmd) =>ifpdfabsdim::<ET>(e,&cmd));
    register_conditional!(ifpdfabsnum,engine,(e,cmd) =>ifpdfabsnum::<ET>(e,&cmd));
    register_value_assign_int!(lpcode,engine);
    register_unexpandable!(pdfcatalog,engine,None,(e,cmd) =>pdfcatalog::<ET>(e,&cmd));
    register_unexpandable!(pdfcolorstack,engine,None,(e,cmd) =>pdfcolorstack::<ET>(e,&cmd));
    register_int!(pdfcolorstackinit,engine,(e,c) => pdfcolorstackinit::<ET>(e,&c));
    register_expandable!(pdfcreationdate,engine,(e,cmd,f) =>pdfcreationdate::<ET>(e,&cmd,f));
    register_unexpandable!(pdfdest,engine,None,(e,cmd) =>pdfdest::<ET>(e,&cmd));
    register_int!(pdfelapsedtime,engine,(e,c) => pdfelapsedtime::<ET>(e,&c));
    register_unexpandable!(pdfendlink,engine,None,(e,cmd) =>pdfendlink::<ET>(e,&cmd));
    register_expandable!(pdfescapestring,engine,(e,cmd,f) =>pdfescapestring::<ET>(e,&cmd,f));
    register_expandable!(pdffilesize,engine,(e,cmd,f) =>pdffilesize::<ET>(e,&cmd,f));
    register_unexpandable!(pdffontexpand,engine,None,(e,cmd) =>pdffontexpand::<ET>(e,&cmd));
    register_expandable!(pdffontsize,engine,(e,cmd,f) =>pdffontsize::<ET>(e,&cmd,f));
    register_unexpandable!(pdfglyphtounicode,engine,None,(e,cmd) =>pdfglyphtounicode::<ET>(e,&cmd));
    register_int!(pdflastobj,engine,(e,c) => pdflastobj::<ET>(e,&c));
    register_int!(pdflastxform,engine,(e,c) => pdflastxform::<ET>(e,&c));
    register_int!(pdflastximage,engine,(e,c) => pdflastximage::<ET>(e,&c));
    register_unexpandable!(pdfliteral,engine,None,(e,cmd) =>pdfliteral::<ET>(e,&cmd));
    register_int!(pdfmajorversion,engine,(_,c) => pdfmajorversion::<ET>(&c));
    register_expandable!(pdfmatch,engine,(e,cmd,f) =>pdfmatch::<ET>(e,&cmd,f));
    register_expandable!(pdfmdfivesum,engine,(e,cmd,f) =>pdfmdfivesum::<ET>(e,&cmd,f));
    register_unexpandable!(pdfobj,engine,None,(e,cmd) =>pdfobj::<ET>(e,&cmd));
    register_unexpandable!(pdfoutline,engine,None,(e,cmd) =>pdfoutline::<ET>(e,&cmd));
    register_whatsit!(pdfrefxform,engine,(e,cmd) =>pdfrefxform::<ET>(e,&cmd));
    register_unexpandable!(pdfrefximage,engine,None,(e,cmd) =>pdfrefximage::<ET>(e,&cmd));
    register_unexpandable!(pdfresettimer,engine,None,(e,cmd) =>pdfresettimer::<ET>(e,&cmd));
    register_unexpandable!(pdfrestore,engine,None,(e,cmd) => pdfrestore::<ET>(e,&cmd));
    register_unexpandable!(pdfsave,engine,None,(e,cmd) => pdfsave::<ET>(e,&cmd));
    register_unexpandable!(pdfsetmatrix,engine,None,(e,cmd) =>pdfsetmatrix::<ET>(e,&cmd));
    register_int!(pdfshellescape,engine,(_,c) => pdfshellescape::<ET>(&c));
    register_unexpandable!(pdfstartlink,engine,None,(e,cmd) =>pdfstartlink::<ET>(e,&cmd));
    register_expandable!(pdfstrcmp,engine,(e,cmd,f) =>pdfstrcmp::<ET>(e,&cmd,f));
    register_expandable!(pdftexrevision,engine,(e,c,f) =>pdftexrevision::<ET>(e,&c,f));
    register_int!(pdftexversion,engine,(_,c) => pdftexversion::<ET>(&c));
    register_unexpandable!(pdfxform,engine,None,(e,cmd) =>pdfxform::<ET>(e,&cmd));
    register_unexpandable!(pdfximage,engine,None,(e,cmd) =>pdfximage::<ET>(e,&cmd));
    register_value_assign_int!(rpcode,engine);
     */
}
