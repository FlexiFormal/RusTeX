/*! Nodes allowed in horizontal lists. */
use crate::engine::EngineTypes;
use crate::engine::filesystem::{File, SourceRef, SourceReference};
use crate::engine::fontsystem::{Font, FontSystem};
use crate::tex::characters::Character;
use crate::tex::nodes::boxes::{HBoxInfo, TeXBox};
use crate::tex::nodes::math::MathGroup;
use crate::tex::nodes::vertical::VNode;
use crate::tex::nodes::{BoxTarget, Leaders, NodeTrait, NodeType, WhatsitNode, display_do_indent};
use crate::tex::numerics::Skip;
use crate::tex::numerics::TeXDimen;
use crate::tex::tokens::token_lists::TokenList;

/// A horizontal list node.
#[derive(Clone, Debug)]
pub enum HNode<ET: EngineTypes> {
    /// A penalty node, as produced by `\penalty`.
    Penalty(i32),
    /// A mark node, as produced by `\mark`.
    Mark(usize, TokenList<ET::Token>),
    /// A whatsit node, as produced by `\special`, `\write`, etc.
    Whatsit(WhatsitNode<ET>),
    /// A glue node, as produced by `\hskip`.
    HSkip(Skip<ET::Dim>),
    /// A glue node, as produced by `\hfil`.
    HFil,
    /// A glue node, as produced by `\hfill`.
    HFill,
    /// A glue node, as produced by `\hfilneg`.
    HFilneg,
    /// A glue node, as produced by `\hss`.
    Hss,
    /// A glue node, as produced by a space character.
    Space,
    /// A kern node, as produced by `\kern`.
    HKern(ET::Dim),
    /// Leaders, as produced by `\leaders` or `\cleaders` or `\xleaders`.
    Leaders(Leaders<ET>),
    /// A box node, as produced by `\hbox`, `\vbox`, `\vtop`, etc.
    Box(TeXBox<ET>),
    /// A rule node, as produced by `\vrule`.
    VRule {
        /// The *provided* width of the rule.
        width: Option<ET::Dim>,
        /// The *provided* height of the rule.
        height: Option<ET::Dim>,
        /// The *provided* depth of the rule.
        depth: Option<ET::Dim>,
        /// The source reference for the start of the rule.
        start: SourceRef<ET>,
        /// The source reference for the end of the rule.
        end: SourceRef<ET>,
    },
    /// An insertion node, as produced by `\insert`.
    Insert(usize, Box<[VNode<ET>]>),
    /// A vadjust node, as produced by `\vadjust`; its contents will migrate to the surrounding vertical list eventually.
    VAdjust(Box<[VNode<ET>]>),
    /// A math list, as produced by `$...$` or `$$...$$`.
    MathGroup(MathGroup<ET>),
    /// A character node, as produced by a character.
    Char {
        /// The character.
        char: ET::Char,
        /// The current font
        font: <ET::FontSystem as FontSystem>::Font,
    },
    /// An `\accent` node.
    Accent {
        /// The accent character.
        accent: ET::Char,
        /// The lower character.
        char: ET::Char,
        /// The current font
        font: <ET::FontSystem as FontSystem>::Font,
    },

    /// An `\accent` node waiting for the actual character.
    AccentChar {
        /// The accent character.
        accent: ET::Char,
        /// The current font
        font: <ET::FontSystem as FontSystem>::Font,
    },
    /// A custom node.
    Custom(ET::CustomNode),
}

impl<ET: EngineTypes> NodeTrait<ET> for HNode<ET> {
    fn display_fmt(&self, indent: usize, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Penalty(p) => {
                display_do_indent(indent, f)?;
                write!(f, "<penalty:{}>", p)
            }
            Self::Leaders(l) => l.display_fmt(indent, f),
            Self::Box(b) => b.display_fmt(indent, f),
            Self::Mark(i, _) => {
                display_do_indent(indent, f)?;
                write!(f, "<mark:{}>", i)
            }
            Self::VRule {
                width,
                height,
                depth,
                ..
            } => {
                write!(f, "<vrule")?;
                if let Some(w) = width {
                    write!(f, " width={}", w)?;
                }
                if let Some(h) = height {
                    write!(f, " height={}", h)?;
                }
                if let Some(d) = depth {
                    write!(f, " depth={}", d)?;
                }
                write!(f, ">")
            }
            Self::Insert(n, ch) => {
                display_do_indent(indent, f)?;
                write!(f, "<insert {}>", n)?;
                for c in ch.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                write!(f, "</insert>")
            }
            Self::VAdjust(ls) => {
                display_do_indent(indent, f)?;
                f.write_str("<vadjust>")?;
                for c in ls.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                f.write_str("</vadjust>")
            }
            Self::MathGroup(mg) => mg.display_fmt(indent, f),
            Self::Char { char, .. } => {
                char.display_fmt(f);
                Ok(())
            }
            Self::Accent { accent, char, .. } => {
                write!(
                    f,
                    "<accent accent=\"{}\" char=\"{}\" />",
                    accent.display(),
                    char.display()
                )
            }
            Self::AccentChar { accent, .. } => {
                write!(f, "<accent accent=\"{}\" />", accent.display(),)
            }
            Self::Whatsit(w) => {
                display_do_indent(indent, f)?;
                write!(f, "{:?}", w)
            }
            Self::HSkip(s) => write!(f, "<hskip:{}>", s),
            Self::HFil => write!(f, "<hfil>"),
            Self::HFill => write!(f, "<hfill>"),
            Self::HFilneg => write!(f, "<hfilneg>"),
            Self::Hss => write!(f, "<hss>"),
            Self::Space => write!(f, "<space>"),
            Self::HKern(d) => write!(f, "<hkern:{}>", d),
            Self::Custom(n) => n.display_fmt(indent, f),
        }
    }
    fn height(&self) -> ET::Dim {
        match self {
            Self::Box(b) => b.height(),
            Self::VRule { height, .. } => height.unwrap_or_default(),
            Self::Char { char, font } => font.get_ht(*char),
            Self::Leaders(l) => l.height(),
            Self::MathGroup(mg) => mg.height(),
            Self::Custom(n) => n.height(),
            Self::Accent { char, font, .. } => {
                font.get_ht(*char) // TODO
            }
            _ => ET::Dim::default(),
        }
    }
    fn width(&self) -> ET::Dim {
        match self {
            Self::Box(b) => b.width(),
            Self::Char { char, font } => font.get_wd(*char),
            Self::VRule { width, .. } => width.unwrap_or(ET::Dim::from_sp(26214)),
            Self::Leaders(l) => l.width(),
            Self::MathGroup(mg) => mg.width(),
            Self::Custom(n) => n.width(),
            Self::HKern(d) => *d,
            Self::HSkip(s) => s.base,
            Self::Accent { char, font, .. } => font.get_wd(*char),
            Self::Space => ET::Dim::from_sp(65536 * 5), // TODO heuristic; use spacefactor instead
            _ => ET::Dim::default(),
        }
    }
    fn depth(&self) -> ET::Dim {
        match self {
            Self::Box(b) => b.depth(),
            Self::Char { char, font } => font.get_dp(*char),
            Self::Accent { char, font, .. } => font.get_dp(*char),
            Self::VRule { depth, .. } => depth.unwrap_or_default(),
            Self::Leaders(l) => l.depth(),
            Self::MathGroup(mg) => mg.depth(),
            Self::Custom(n) => n.depth(),
            _ => ET::Dim::default(),
        }
    }
    fn nodetype(&self) -> NodeType {
        match self {
            Self::Penalty(_) => NodeType::Penalty,
            Self::VRule { .. } => NodeType::Rule,
            Self::Box(b) => b.nodetype(),
            Self::Char { .. } => NodeType::Char,
            Self::HKern(_) => NodeType::Kern,
            Self::Insert(..) => NodeType::Insertion,
            Self::VAdjust(_) => NodeType::Adjust,
            Self::MathGroup { .. } => NodeType::Math,
            Self::Mark(_, _) => NodeType::Mark,
            Self::Whatsit(_) => NodeType::WhatsIt,
            Self::Accent { .. } => NodeType::Char,
            Self::AccentChar { .. } => NodeType::Char,
            Self::Leaders(_) => NodeType::Glue,
            Self::HSkip(_) | Self::Space | Self::HFil | Self::HFill | Self::HFilneg | Self::Hss => {
                NodeType::Glue
            }
            Self::Custom(n) => n.nodetype(),
        }
    }
    fn opaque(&self) -> bool {
        match self {
            Self::Mark(_, _) => true,
            Self::Custom(n) => n.opaque(),
            _ => false,
        }
    }

    fn sourceref(&self) -> Option<(&SourceRef<ET>, &SourceRef<ET>)> {
        match self {
            Self::VRule { start, end, .. } => Some((start, end)),
            Self::Box(b) => b.sourceref(),
            Self::MathGroup(mg) => mg.sourceref(),
            _ => None,
        }
    }
}

/// The kinds of horizontal lists that can occur.
/// TODO: rethink this
#[derive(Clone, Debug)]
pub enum HorizontalNodeListType<ET: EngineTypes> {
    /// A paragraph; will ultimately be broken into lines.
    Paragraph(SourceReference<<ET::File as File>::SourceRefID>),
    /// A horizontal box.
    Box(HBoxInfo<ET>, SourceRef<ET>, BoxTarget<ET>),
    /// A `\valign` list
    VAlign,
    /// A row in an `\halign`. The source ref indicates the start of the row.
    HAlignRow(SourceRef<ET>),
    /// A cell in an `\halign`. The source ref indicates the start of the cell.
    /// The `u8` indicates the number of *additional* columns spanned by this cell
    /// (so by default 0).
    HAlignCell(SourceRef<ET>, u8),
}
