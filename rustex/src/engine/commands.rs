use crate::engine::extension::CSS;
use crate::engine::nodes::RusTeXNode;
use crate::engine::stomach::RusTeXStomach;
use crate::engine::{Refs, Res, Types, register_command};
use crate::utils::{VecMap, VecSet};
use tex_engine::add_node;
use tex_engine::commands::primitives::{register_simple_expandable, register_unexpandable};
use tex_engine::commands::{CommandScope, PrimitiveCommand};
use tex_engine::engine::DefaultEngine;
use tex_engine::engine::mouth::Mouth;
use tex_engine::engine::state::State;
use tex_engine::engine::stomach::Stomach;
use tex_engine::prelude::*;
use tex_engine::tex::nodes::horizontal::HNode;
use tex_engine::tex::nodes::math::{
    MathAtom, MathClass, MathKernel, MathNode, MathNucleus, UnresolvedMarkers,
};
use tex_engine::tex::nodes::vertical::VNode;
use tex_engine::tex::nodes::{ListTarget, NodeList};
use tex_engine::tex::tokens::CompactToken;
use tex_engine::utils::errors::TeXError;

pub fn register_primitives_preinit(engine: &mut DefaultEngine<Types>) {
    register_simple_expandable(engine, CLOSE_FONT, close_font);
    engine.state.register_primitive(
        &mut engine.aux,
        "rustexBREAK",
        PrimitiveCommand::Unexpandable {
            scope: CommandScope::Any,
            apply: |_, _| {
                println!("HERE!");
                Ok(())
            },
        },
    );
}

pub fn register_primitives_postinit(engine: &mut DefaultEngine<Types>) {
    /*register_command(engine, true, "LaTeX", "",
                     "L\\kern-.3em\\raise.5ex\\hbox{\\check@mathfonts\\fontsize\\sf@size\\z@\\math@fontsfalse\\selectfont A}\\kern-.15em\\TeX",
                     true, false
    );*/
    crate::engine::pgf::register_pgf(engine);
    register_unexpandable(
        engine,
        "rustex@annotateHTML",
        CommandScope::Any,
        annot_begin,
    );
    register_unexpandable(engine, "rustex@HTMLNode", CommandScope::Any, node_begin);
    register_unexpandable(
        engine,
        "rustex@annotateHTMLEnd",
        CommandScope::Any,
        annot_end,
    );
    engine.state.register_primitive(
        &mut engine.aux,
        "if@rustex",
        PrimitiveCommand::Conditional(tex_engine::commands::tex::iftrue),
    );
    register_unexpandable(
        engine,
        "rustex@addNamespaceAbbrev",
        CommandScope::Any,
        namespace,
    );
    register_unexpandable(engine, "rustex@addMeta", CommandScope::Any, meta);
    register_unexpandable(engine, "rustex@annotateTop", CommandScope::Any, annot_top);
    register_unexpandable(engine, "rustex@cssLink", CommandScope::Any, css_link);
    register_unexpandable(engine, "rustex@cssLiteral", CommandScope::Any, css_literal);
    register_unexpandable(
        engine,
        "rustex@HTMLLiteral",
        CommandScope::Any,
        html_literal,
    );
    register_unexpandable(
        engine,
        "rustex@annotateInvisible",
        CommandScope::Any,
        invisible_begin,
    );
    register_unexpandable(
        engine,
        "rustex@annotateInvisibleEnd",
        CommandScope::Any,
        invisible_end,
    );
    register_unexpandable(
        engine,
        "rustex@@underbrace",
        CommandScope::MathOnly,
        underbrace,
    );
    register_unexpandable(
        engine,
        "rustex@@overbrace",
        CommandScope::MathOnly,
        overbrace,
    );
    // if@rustex
    // rustex@directHTML
}

fn html_literal(engine: Refs, token: CompactToken) -> Res<()> {
    let mut lit = String::new();
    engine.read_braced_string(true, true, &token, &mut lit)?;
    let node = RusTeXNode::Literal(lit);
    add_node!(RusTeXStomach;engine, VNode::Custom(node),HNode::Custom(node),MathNode::Custom(node));
    Ok(())
}
fn css_link(engine: Refs, token: CompactToken) -> Res<()> {
    let mut file = String::new();
    engine.read_braced_string(true, true, &token, &mut file)?;
    engine.aux.extension.css.insert(CSS::File(file));
    Ok(())
}
fn css_literal(engine: Refs, token: CompactToken) -> Res<()> {
    let mut literal = String::new();
    engine.read_braced_string(true, true, &token, &mut literal)?;
    engine.aux.extension.css.insert(CSS::Literal(literal));
    Ok(())
}

fn namespace(engine: Refs, token: CompactToken) -> Res<()> {
    let mut key = String::new();
    engine.read_braced_string(true, true, &token, &mut key)?;
    let mut value = String::new();
    engine.read_braced_string(true, true, &token, &mut value)?;
    engine.aux.extension.namespaces.insert(key, value);
    Ok(())
}

fn meta(engine: Refs, token: CompactToken) -> Res<()> {
    let mut attrs = VecMap::default();
    let mut str = String::new();
    engine.read_braced_string(true, true, &token, &mut str)?;
    let mut s = str[..].trim();
    while !s.is_empty() {
        let key = if let Some(i) = s.find('=') {
            let (n, v) = s.split_at(i);
            s = v[1..].trim_start();
            n.trim()
        } else {
            todo!()
        };
        let delim = if s.starts_with('"') {
            s = &s[1..];
            '"'
        } else if s.starts_with('\'') {
            s = &s[1..];
            '\''
        } else {
            todo!()
        };
        let val = if let Some(i) = s.find(delim) {
            let (v, r) = s.split_at(i);
            s = r[1..].trim_start();
            v.trim()
        } else {
            todo!()
        };
        attrs.insert(key.to_string(), val.to_string());
    }
    engine.aux.extension.metas.push(attrs);
    Ok(())
}

fn annot_top(engine: Refs, token: CompactToken) -> Res<()> {
    let mut str = String::new();
    engine.read_braced_string(true, true, &token, &mut str)?;
    let mut s = str[..].trim();
    let top = &mut engine.aux.extension.top;
    while !s.is_empty() {
        let key = if let Some(i) = s.find('=') {
            let (n, v) = s.split_at(i);
            s = v[1..].trim_start();
            n.trim()
        } else {
            todo!()
        };
        let delim = if s.starts_with('"') {
            s = &s[1..];
            '"'
        } else if s.starts_with('\'') {
            s = &s[1..];
            '\''
        } else {
            todo!()
        };
        let val = if let Some(i) = s.find(delim) {
            let (v, r) = s.split_at(i);
            s = r[1..].trim_start();
            v.trim()
        } else {
            todo!()
        };
        top.insert(key.to_string(), val.to_string());
    }
    Ok(())
}

fn parse_annotations(
    orig: &str,
) -> Res<(
    VecMap<String, String>,
    VecMap<String, String>,
    VecSet<String>,
)> {
    let mut attrs = VecMap::default();
    let mut styles = VecMap::default();
    let mut classes = VecSet::default();
    let mut s = orig.trim();
    while !s.is_empty() {
        let style = if s.starts_with("style:") {
            s = &s[6..];
            Some(true)
        } else if s.starts_with("class:") {
            s = &s[6..];
            Some(false)
        } else {
            None
        };
        let key = if let Some(i) = s.find('=') {
            let (n, v) = s.split_at(i);
            s = v[1..].trim_start();
            n.trim()
        } else {
            return Err(TeXError::General(format!("Invalid annotation: {orig}")));
        };
        let delim = if s.starts_with('"') {
            s = &s[1..];
            '"'
        } else if s.starts_with('\'') {
            s = &s[1..];
            '\''
        } else {
            return Err(TeXError::General(format!("Invalid annotation: {orig}")));
        };
        let val = if let Some(i) = s.find(delim) {
            let (v, r) = s.split_at(i);
            s = r[1..].trim_start();
            v.trim()
        } else {
            return Err(TeXError::General(format!("Invalid annotation: {orig}")));
        };
        if style == Some(true) {
            styles.insert(key.to_string(), val.to_string());
        } else if style == Some(false) {
            if !val.is_empty() {
                return Err(TeXError::General("Invalid class annotation".to_string()));
            }
            classes.insert(key.to_string());
        } else {
            attrs.insert(key.to_string(), val.to_string());
        }
    }
    Ok((attrs, styles, classes))
}

fn node_begin(engine: Refs, token: CompactToken) -> Res<()> {
    let start = engine.mouth.start_ref();
    let mut tag = String::new();
    engine.read_braced_string(true, true, &token, &mut tag)?;
    let mut str = String::new();
    engine.read_braced_string(true, true, &token, &mut str)?;
    let (attrs, styles, classes) = parse_annotations(&str)?;
    let node = RusTeXNode::AnnotBegin {
        attrs,
        styles,
        start,
        classes,
        tag: Some(tag),
    };
    add_node!(RusTeXStomach;engine, VNode::Custom(node),HNode::Custom(node),MathNode::Custom(node));
    Ok(())
}
fn invisible_begin(engine: Refs, _token: CompactToken) -> Res<()> {
    let node = RusTeXNode::InvisibleBegin;
    add_node!(RusTeXStomach;engine, VNode::Custom(node),HNode::Custom(node),MathNode::Custom(node));
    Ok(())
}
fn invisible_end(engine: Refs, _token: CompactToken) -> Res<()> {
    let node = RusTeXNode::InvisibleEnd;
    add_node!(RusTeXStomach;engine, VNode::Custom(node),HNode::Custom(node),MathNode::Custom(node));
    Ok(())
}
fn annot_begin(engine: Refs, token: CompactToken) -> Res<()> {
    let start = engine.mouth.start_ref();
    let mut str = String::new();
    engine.read_braced_string(true, true, &token, &mut str)?;
    let (attrs, styles, classes) = parse_annotations(&str)?;
    let node = RusTeXNode::AnnotBegin {
        attrs,
        styles,
        start,
        classes,
        tag: None,
    };
    add_node!(RusTeXStomach;engine, VNode::Custom(node),HNode::Custom(node),MathNode::Custom(node));
    Ok(())
}
fn annot_end(engine: Refs, _token: CompactToken) -> Res<()> {
    let node = RusTeXNode::AnnotEnd(engine.mouth.current_sourceref());
    add_node!(RusTeXStomach;engine, VNode::Custom(node),HNode::Custom(node),MathNode::Custom(node));
    Ok(())
}

pub const CLOSE_FONT: &str = "!\"$%&/(closefont)\\&%$\"!";
pub fn close_font(engine: Refs, _token: CompactToken) -> Res<()> {
    match engine.stomach.data_mut().open_lists.last_mut() {
        Some(NodeList::Vertical { children, .. }) => match children.last() {
            Some(VNode::Custom(RusTeXNode::FontChange(_, _))) => {
                children.pop();
            }
            _ => children.push(VNode::Custom(RusTeXNode::FontChangeEnd)),
        },
        Some(NodeList::Horizontal { children, .. }) => match children.last() {
            Some(HNode::Custom(RusTeXNode::FontChange(_, _))) => {
                children.pop();
            }
            _ => children.push(HNode::Custom(RusTeXNode::FontChangeEnd)),
        },
        Some(NodeList::Math { children, .. }) => match children.list_mut().last() {
            Some(MathNode::Custom(RusTeXNode::FontChange(_, _))) => {
                children.list_mut().pop();
            }
            _ => children.push(MathNode::Custom(RusTeXNode::FontChangeEnd)),
        },
        _ => engine
            .stomach
            .data_mut()
            .page
            .push(VNode::Custom(RusTeXNode::FontChangeEnd)),
    }
    Ok(())
}

// ------------------------------------------------------------------

fn underbrace(engine: Refs, token: CompactToken) -> Res<()> {
    const LITERAL: &str = "<mo stretchy=\"true\">⏟</mo>";
    //engine.state.push(engine.aux, GroupType::Math, engine.mouth.line_number());
    engine.read_char_or_math_group(
        &token,
        |(), engine, char| {
            let node = MathNode::Atom(MathAtom {
                sup: None,
                sub: Some(Box::new([MathNode::Custom(RusTeXNode::Literal(
                    LITERAL.to_string(),
                ))])),
                nucleus: MathNucleus::Simple {
                    cls: char.cls,
                    kernel: MathKernel::Char {
                        char: char.char,
                        style: char.style,
                    },
                    limits: Some(true),
                },
            });
            let node = MathNode::Atom(MathAtom {
                sup: None,
                sub: None,
                nucleus: MathNucleus::Simple {
                    cls: MathClass::Ord,
                    limits: Some(true),
                    kernel: MathKernel::List {
                        start: engine.mouth.current_sourceref(),
                        end: engine.mouth.current_sourceref(),
                        children: Box::new([node]),
                    },
                },
            });
            //engine.state.pop(engine.aux, engine.mouth);
            RusTeXStomach::add_node_m(engine, node);
            Ok(())
        },
        |()| {
            ListTarget::<Types, _>::new(|engine, mut children, rf| {
                children.insert(0, MathNode::Marker(UnresolvedMarkers::Display));
                let node = MathNode::Atom(MathAtom {
                    sup: None,
                    sub: Some(Box::new([MathNode::Custom(RusTeXNode::Literal(
                        LITERAL.to_string(),
                    ))])),
                    nucleus: MathNucleus::Simple {
                        cls: MathClass::Ord,
                        limits: Some(true),
                        kernel: MathKernel::List {
                            start: rf,
                            children: children.into(),
                            end: engine.mouth.current_sourceref(),
                        },
                    },
                });
                let node = MathNode::Atom(MathAtom {
                    sup: None,
                    sub: None,
                    nucleus: MathNucleus::Simple {
                        cls: MathClass::Ord,
                        limits: Some(true),
                        kernel: MathKernel::List {
                            start: engine.mouth.current_sourceref(),
                            end: engine.mouth.current_sourceref(),
                            children: Box::new([node]),
                        },
                    },
                });
                RusTeXStomach::add_node_m(engine, node);
                Ok(())
            })
        },
        (),
    )
}
fn overbrace(engine: Refs, token: CompactToken) -> Res<()> {
    const LITERAL: &str = "<mo stretchy=\"true\">⏞</mo>";
    RusTeXStomach::add_node_m(engine, MathNode::Marker(UnresolvedMarkers::Display));
    engine.read_char_or_math_group(
        &token,
        |(), engine, char| {
            let node = MathNode::Atom(MathAtom {
                sub: None,
                sup: Some(Box::new([MathNode::Custom(RusTeXNode::Literal(
                    LITERAL.to_string(),
                ))])),
                nucleus: MathNucleus::Simple {
                    cls: char.cls,
                    kernel: MathKernel::Char {
                        char: char.char,
                        style: char.style,
                    },
                    limits: Some(true),
                },
            });
            let node = MathNode::Atom(MathAtom {
                sup: None,
                sub: None,
                nucleus: MathNucleus::Simple {
                    cls: MathClass::Ord,
                    limits: Some(true),
                    kernel: MathKernel::List {
                        start: engine.mouth.current_sourceref(),
                        end: engine.mouth.current_sourceref(),
                        children: Box::new([node]),
                    },
                },
            });
            RusTeXStomach::add_node_m(engine, node);
            Ok(())
        },
        |()| {
            ListTarget::<Types, _>::new(|engine, mut children, rf| {
                children.insert(0, MathNode::Marker(UnresolvedMarkers::Display));
                let node = MathNode::Atom(MathAtom {
                    sub: None,
                    sup: Some(Box::new([MathNode::Custom(RusTeXNode::Literal(
                        LITERAL.to_string(),
                    ))])),
                    nucleus: MathNucleus::Simple {
                        cls: MathClass::Ord,
                        limits: Some(true),
                        kernel: MathKernel::List {
                            start: rf,
                            children: children.into(),
                            end: engine.mouth.current_sourceref(),
                        },
                    },
                });
                let node = MathNode::Atom(MathAtom {
                    sup: None,
                    sub: None,
                    nucleus: MathNucleus::Simple {
                        cls: MathClass::Ord,
                        limits: Some(true),
                        kernel: MathKernel::List {
                            start: engine.mouth.current_sourceref(),
                            end: engine.mouth.current_sourceref(),
                            children: Box::new([node]),
                        },
                    },
                });
                RusTeXStomach::add_node_m(engine, node);
                Ok(())
            })
        },
        (),
    )
}
