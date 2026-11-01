use crate::fontstyles::ModifierSeq;
use crate::glyphs::UNDEFINED_LIST;
use crate::parsing::Parser;
use crate::{Glyph, GlyphList};

pub fn parse_pfb(f: &str, mods: &mut ModifierSeq) -> Option<GlyphList> {
    let disas = match std::str::from_utf8(
        std::process::Command::new("t1disasm")
            .args(vec![f])
            .output()
            .expect("t1disasm not found!")
            .stdout
            .as_slice(),
    ) {
        Ok(s) => s.trim().to_string(),
        _ => return Some(UNDEFINED_LIST.clone()),
    };
    let mut s = Parser::new(&disas);
    let _ = s.read_until_strs(&["dict begin", "dict dup begin"]);
    assert!(!s.is_empty(), "Empty pfb file!");
    parse_dict(&mut s, mods)
}

#[allow(clippy::cast_possible_truncation)]
fn parse_dict(s: &mut Parser<'_>, mods: &mut ModifierSeq) -> Option<GlyphList> {
    if parse_dict_header(s, mods) {
        return Some(crate::STANDARD_ENCODING.clone());
    }
    let mut dict = UNDEFINED_LIST.clone();
    while !s.is_empty() {
        if s.drop("dup") {
            if s.drop("dup") {
                s.read_until_strs(&["putinterval", "put"]);
                continue;
            }
            let i = s.read_digit() as u8;
            if !s.drop("/") {
                return None;
            }
            let name = s.read_until_ws();
            dict.0[i as usize] = Glyph::get(name);
            if !s.drop("put") {
                return None;
            }
            continue;
        }
        while s.starts_with('%') {
            s.skip_until_endline()
        }
        if s.drop("readonly def") {
            return Some(dict);
        }
        return None;
    }
    None
}

fn parse_dict_header(s: &mut Parser<'_>, mods: &mut ModifierSeq) -> bool {
    'a: loop {
        if s.starts_with('%') {
            s.skip_until_endline();
            continue;
        }
        if s.drop("/FontInfo") {
            s.read_until_str("begin");
            loop {
                if s.starts_with('%') {
                    s.skip_until_endline();
                    continue;
                }
                if s.drop("end") {
                    s.read_until_str(" def");
                    break;
                }
                if s.drop("/Weight") {
                    let w = s.read_until_str(" def");
                    if w.contains("Semibold")
                        || w.contains("Bold")
                        || w.contains("Heavy")
                        || w.contains("bold")
                    {
                        *mods = mods.add(crate::fontstyles::FontModifier::Bold);
                    }
                    if w.contains("SmallCaps") {
                        *mods = mods.add(crate::fontstyles::FontModifier::Capitals);
                    }
                    if w.contains("Italic") {
                        *mods = mods.add(crate::fontstyles::FontModifier::Italic);
                    }
                    if w.contains("Oblique") {
                        *mods = mods.add(crate::fontstyles::FontModifier::Oblique);
                    }
                    continue;
                }
                if s.drop("/ItalicAngle") {
                    let i = s.read_digit();
                    if i != 0 && i >= 10 {
                        *mods = mods.add(crate::fontstyles::FontModifier::Italic);
                    }
                    s.read_until_str(" def");
                    continue;
                }
                if s.starts_with('/') {
                    s.read_until_str(" def");
                    continue;
                }
                panic!("???: {}", s.0)
            }
            continue 'a;
        }
        if s.drop("/Encoding") {
            if s.drop("StandardEncoding") {
                return true;
            }
            while !s.starts_with_str("dup") {
                let _ = s.read_until_ws();
            }
            //s.skip_until_endline();//s.read_until_str("for");
            return false;
        }
        if s.starts_with('/') {
            s.read_until_str(" def");
            continue;
        }
        panic!("???: {}", s.0)
    }
}
