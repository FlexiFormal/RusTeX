/*! [`TeXBox`]es */

use crate::engine::EngineTypes;
use crate::engine::filesystem::SourceRef;
use crate::engine::stomach::methods::ParLineSpec;
use crate::tex::nodes::horizontal::{HNode, HorizontalNodeListType};
use crate::tex::nodes::math::{
    MathAtom, MathClass, MathKernel, MathNode, MathNucleus, UnresolvedMathFontStyle,
};
use crate::tex::nodes::vertical::{VNode, VerticalNodeListType};
use crate::tex::nodes::{BoxTarget, NodeList, NodeTrait, NodeType, display_do_indent};
use crate::tex::numerics::Skip;
use crate::tex::numerics::TeXDimen;
use std::fmt::{Display, Formatter};

#[cfg(feature = "multithreaded")]
type Once<A> = std::sync::OnceLock<A>;
#[cfg(not(feature = "multithreaded"))]
type Once<A> = std::cell::OnceCell<A>;

/// The type of a box, e.g. `\hbox` or `\vbox`.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum BoxType {
    /// A horizontal box, e.g. `\hbox`.
    Horizontal,
    /// A vertical box, e.g. `\vbox`.
    Vertical,
}
impl BoxType {
    /// Horizontal -> Vertical, Vertical -> Horizontal
    pub fn other(&self) -> Self {
        match self {
            BoxType::Horizontal => BoxType::Vertical,
            BoxType::Vertical => BoxType::Horizontal,
        }
    }
}
impl Display for BoxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoxType::Horizontal => write!(f, "hbox"),
            BoxType::Vertical => write!(f, "vbox"),
            //BoxType::InlineMath | BoxType::DisplayMath => write!(f, "math shift")
        }
    }
}

/// The "scaling factor" of a box, e.g. `\hbox to 50pt` or `\hbox spread 10pt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToOrSpread<D: TeXDimen> {
    /// unscaled
    None,
    /// e.g. `\hbox to 50pt`
    To(D),
    /// e.g. `\hbox spread 10pt`
    Spread(D),
}

/// "Metadata" of a horizontal box
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HBoxInfo<ET: EngineTypes> {
    /// A "normal" `\hbox`
    HBox {
        /// scaling factor
        scaled: ToOrSpread<ET::Dim>,
        /// width, if assigned via e.g. `\wd0=50pt`
        assigned_width: Option<ET::Dim>,
        /// height, if assigned via e.g. `\ht0=50pt`
        assigned_height: Option<ET::Dim>,
        /// depth, if assigned via e.g. `\dp0=50pt`
        assigned_depth: Option<ET::Dim>,
        /// horizontal movement, if moved via e.g. `\moveleft 50pt` (or `\moveright`)
        moved_left: Option<ET::Dim>,
        /// vertical movement, if raised via e.g. `\raise 50pt` (or `\lower`)
        raised: Option<ET::Dim>,
        /// computed width by summing the widths of all children
        computed_width: Once<ET::Dim>,
        /// computed height by taking the maximum height of all children
        computed_height: Once<ET::Dim>,
        /// computed depth by taking the maximum depth of all children
        computed_depth: Once<ET::Dim>,
    },
    /// A line in a paragraph
    ParLine {
        /// The paragraph line specification used to determine the break point of this line
        spec: ParLineSpec<ET>,
        /// Whether this line was forcefully ended via `\penalty-10000`
        ends_with_line_break: bool,
        /// The height of the line as computed via the maximum height of all children
        inner_height: ET::Dim,
        /// The depth of the line as computed via the maximum depth of all children
        inner_depth: ET::Dim,
    },
    /// A row in an `\halign`; should only contain [HBoxInfo::HAlignCell]s
    HAlignRow,
    /// A cell in an `\halign`
    HAlignCell {
        /// The width of the cell, as computed by comparing all cells in the same columng (not yet implemented)
        to: Option<ET::Dim>,
        /// The computed width of the cell as the sum of the widths of the children
        computed_width: Once<ET::Dim>,
        /// The computed height of the cell as the maximum height of the children
        computed_height: Once<ET::Dim>,
        /// The computed depth of the cell as the maximum depth of the children
        computed_depth: Once<ET::Dim>,
        /// The number of *additional* columns this cell spans (i.e. by default 0)
        spans: u8,
    },
    /// A paragraph indent box
    ParIndent(ET::Dim),
}
impl<ET: EngineTypes> Display for HBoxInfo<ET> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use HBoxInfo::*;
        match self {
            HBox { .. } => write!(f, "hbox"),
            ParLine { .. } => write!(f, "parline"),
            ParIndent(_) => write!(f, "parindent"),
            HAlignRow => write!(f, "halignrow"),
            HAlignCell { .. } => write!(f, "haligncell"),
        }
    }
}
impl<ET: EngineTypes> HBoxInfo<ET> {
    /// Create a new `\hbox` box info with the given scaling factor
    pub fn new_box(scaled: ToOrSpread<ET::Dim>) -> Self {
        HBoxInfo::HBox {
            scaled,
            assigned_width: None,
            assigned_height: None,
            assigned_depth: None,
            moved_left: None,
            raised: None,
            computed_width: Once::new(),
            computed_height: Once::new(),
            computed_depth: Once::new(),
        }
    }
    /// Convert this to a simple `\hbox` box info
    pub fn to_hbox(&mut self) {
        match self {
            HBoxInfo::HBox { .. } => (),
            HBoxInfo::ParLine { spec, .. } => {
                *self = HBoxInfo::new_box(ToOrSpread::To(spec.target))
            }
            HBoxInfo::ParIndent(d) => *self = HBoxInfo::new_box(ToOrSpread::To(*d)),
            HBoxInfo::HAlignRow => *self = HBoxInfo::new_box(ToOrSpread::None),
            HBoxInfo::HAlignCell { to, .. } => {
                *self = HBoxInfo::new_box(match to {
                    None => ToOrSpread::None,
                    Some(to) => ToOrSpread::To(*to),
                })
            }
        }
    }

    /// Create a new `\halign` cell box info with the given number of column spans (default 0)
    pub fn new_cell(spans: u8) -> Self {
        HBoxInfo::HAlignCell {
            to: None,
            computed_width: Once::new(),
            computed_height: Once::new(),
            computed_depth: Once::new(),
            spans,
        }
    }
    /// Turns this box info into the corresponding [`NodeList`]
    pub fn open_list(self, start: SourceRef<ET>) -> NodeList<ET> {
        match self {
            HBoxInfo::HBox { .. } => NodeList::Horizontal {
                tp: HorizontalNodeListType::Box(self, start, BoxTarget::none()),
                children: vec![],
            },
            HBoxInfo::ParLine { .. } => NodeList::Horizontal {
                tp: HorizontalNodeListType::Paragraph(start),
                children: vec![],
            },
            HBoxInfo::HAlignRow => NodeList::Horizontal {
                tp: HorizontalNodeListType::HAlignRow(start),
                children: vec![],
            },
            HBoxInfo::HAlignCell { .. } => NodeList::Horizontal {
                tp: HorizontalNodeListType::HAlignCell(start, 0),
                children: vec![],
            },
            HBoxInfo::ParIndent(_) => unreachable!(),
        }
    }

    fn height_inner(v: &[HNode<ET>]) -> ET::Dim {
        v.iter().map(|c| c.height()).max().unwrap_or_default()
    }

    fn depth_inner(v: &[HNode<ET>]) -> ET::Dim {
        v.iter().map(|c| c.depth()).max().unwrap_or_default()
    }

    fn width_inner(v: &[HNode<ET>]) -> ET::Dim {
        v.iter().map(|c| c.width()).sum()
    }

    fn get_height(&self, v: &[HNode<ET>]) -> ET::Dim {
        match self {
            HBoxInfo::HBox {
                assigned_height,
                computed_height,
                ..
            } => assigned_height
                .unwrap_or_else(|| *computed_height.get_or_init(|| Self::height_inner(v))),
            HBoxInfo::ParLine { inner_height, .. } => *inner_height,
            HBoxInfo::HAlignRow => Self::height_inner(v),
            HBoxInfo::HAlignCell { .. } => Self::height_inner(v),
            HBoxInfo::ParIndent(_) => ET::Dim::default(),
        }
    }
    fn get_width(&self, v: &[HNode<ET>]) -> ET::Dim {
        match self {
            HBoxInfo::HBox {
                assigned_width,
                computed_width,
                ..
            } => assigned_width
                .unwrap_or_else(|| *computed_width.get_or_init(|| Self::width_inner(v))),
            HBoxInfo::ParLine { spec, .. } => {
                spec.leftskip.base + spec.rightskip.base + spec.target
            }
            HBoxInfo::HAlignRow => Self::width_inner(v),
            HBoxInfo::HAlignCell {
                to, computed_width, ..
            } => to.unwrap_or_else(|| *computed_width.get_or_init(|| Self::width_inner(v))),
            HBoxInfo::ParIndent(d) => *d,
        }
    }
    fn get_depth(&self, v: &[HNode<ET>]) -> ET::Dim {
        match self {
            HBoxInfo::HBox {
                assigned_depth,
                computed_depth,
                ..
            } => assigned_depth
                .unwrap_or_else(|| *computed_depth.get_or_init(|| Self::depth_inner(v))),
            HBoxInfo::ParLine { inner_depth, .. } => *inner_depth,
            HBoxInfo::HAlignRow => Self::depth_inner(v),
            HBoxInfo::HAlignCell { .. } => Self::depth_inner(v),
            HBoxInfo::ParIndent(_) => ET::Dim::default(),
        }
    }

    /// Raise this box by the given amount (i.e. `\raise` or `\lower`)
    pub fn raise(&mut self, d: ET::Dim) {
        self.to_hbox();
        match self {
            HBoxInfo::HBox { raised, .. } => *raised = Some(d),
            _ => unreachable!(),
        }
    }
    /// Move this box left by the given amount (i.e. `\moveleft` or `\moveright`)
    pub fn move_left(&mut self, d: ET::Dim) {
        self.to_hbox();
        match self {
            HBoxInfo::HBox { moved_left, .. } => *moved_left = Some(d),
            _ => unreachable!(),
        }
    }
}

/// "Metadata" of a vertical box
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VBoxInfo<ET: EngineTypes> {
    /// A "normal" `\vbox`
    VBox {
        /// scaling factor
        scaled: ToOrSpread<ET::Dim>,
        /// width, if assigned via e.g. `\wd0=50pt`
        assigned_width: Option<ET::Dim>,
        /// height, if assigned via e.g. `\ht0=50pt`
        assigned_height: Option<ET::Dim>,
        /// depth, if assigned via e.g. `\dp0=50pt`
        assigned_depth: Option<ET::Dim>,
        /// horizontal movement, if moved via e.g. `\moveleft 50pt` (or `\moveright`)
        moved_left: Option<ET::Dim>,
        /// vertical movement, if raised via e.g. `\raise 50pt` (or `\lower`)
        raised: Option<ET::Dim>,
        /// computed width by taking the maximum width of all children
        computed_width: Once<ET::Dim>,
        /// computed height by summing the heights and depths of all children (except for the last)
        computed_height: Once<ET::Dim>,
        /// computed depth by taking the depth of the last child box
        computed_depth: Once<ET::Dim>,
    },
    /// A `\vtop` box
    VTop {
        /// scaling factor
        scaled: ToOrSpread<ET::Dim>,
        /// width, if assigned via e.g. `\wd0=50pt`
        assigned_width: Option<ET::Dim>,
        /// height, if assigned via e.g. `\ht0=50pt`
        assigned_height: Option<ET::Dim>,
        /// depth, if assigned via e.g. `\dp0=50pt`
        assigned_depth: Option<ET::Dim>,
        /// horizontal movement, if moved via e.g. `\moveleft 50pt` (or `\moveright`)
        moved_left: Option<ET::Dim>,
        /// vertical movement, if raised via e.g. `\raise 50pt` (or `\lower`)
        raised: Option<ET::Dim>,
        /// computed width by taking the maximum width of all children
        computed_width: Once<ET::Dim>,
        /// computed height by taking the height of the first child box
        computed_height: Once<ET::Dim>,
        /// computed depth by taking the height + depth of all children minus the computed height
        computed_depth: Once<ET::Dim>,
    },
    /// A column in a `\valign`; should only contain [VBoxInfo::VAlignCell]s
    VAlignColumn,
    /// A cell in a `\valign`
    VAlignCell {
        /// The height of the cell, as computed by comparing all cells in the same row (not yet implemented)
        to: Option<ET::Dim>,
        /// The number of *additional* rows this cell spans (i.e. by default 0)
        spans: u8,
    },
}
impl<ET: EngineTypes> Display for VBoxInfo<ET> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use VBoxInfo::*;
        match self {
            VBox { .. } => write!(f, "vbox"),
            VTop { .. } => write!(f, "vtop"),
            VAlignColumn => write!(f, "valignrow"),
            VAlignCell { .. } => write!(f, "valigncell"),
        }
    }
}

impl<ET: EngineTypes> VBoxInfo<ET> {
    /// Create a new `\vbox` box info with the given scaling factor
    pub fn new_box(scaled: ToOrSpread<ET::Dim>) -> Self {
        VBoxInfo::VBox {
            scaled,
            assigned_width: None,
            assigned_height: None,
            assigned_depth: None,
            moved_left: None,
            raised: None,
            computed_width: Once::new(),
            computed_height: Once::new(),
            computed_depth: Once::new(),
        }
    }

    /// Convert this box to a `\vbox` (or `\vtop` if it already is a `\vtop`)
    pub fn to_vbox(&mut self) {
        match self {
            VBoxInfo::VTop { .. } | VBoxInfo::VBox { .. } => (),
            VBoxInfo::VAlignColumn => *self = VBoxInfo::new_box(ToOrSpread::None),
            VBoxInfo::VAlignCell { to, .. } => {
                *self = VBoxInfo::new_box(match to {
                    None => ToOrSpread::None,
                    Some(d) => ToOrSpread::To(*d),
                })
            }
        }
    }

    /// Create a new `\vtop` box info with the given scaling factor
    pub fn new_top(scaled: ToOrSpread<ET::Dim>) -> Self {
        VBoxInfo::VTop {
            scaled,
            assigned_width: None,
            assigned_height: None,
            assigned_depth: None,
            moved_left: None,
            raised: None,
            computed_width: Once::new(),
            computed_height: Once::new(),
            computed_depth: Once::new(),
        }
    }
    /// Turns this box info into the corresponding [`NodeList`]
    pub fn open_list(self, start: SourceRef<ET>) -> NodeList<ET> {
        match self {
            VBoxInfo::VBox { .. } => NodeList::Vertical {
                tp: VerticalNodeListType::Box(self, start, BoxTarget::none()),
                children: vec![],
            },
            VBoxInfo::VTop { .. } => NodeList::Vertical {
                tp: VerticalNodeListType::Box(self, start, BoxTarget::none()),
                children: vec![],
            },
            VBoxInfo::VAlignColumn => NodeList::Vertical {
                tp: VerticalNodeListType::VAlignColumn(start),
                children: vec![],
            },
            VBoxInfo::VAlignCell { .. } => NodeList::Vertical {
                tp: VerticalNodeListType::VAlignCell(start, 0),
                children: vec![],
            },
        }
    }
    /// Clone this box info for use in a split box (i.e. `\vsplit`)
    pub fn clone_for_split(&mut self) -> Self {
        match self {
            VBoxInfo::VBox {
                scaled,
                assigned_width,
                assigned_height,
                assigned_depth,
                moved_left,
                raised,
                ..
            } => {
                *assigned_height = None;
                *assigned_depth = None;
                *scaled = ToOrSpread::None;
                VBoxInfo::VBox {
                    scaled: ToOrSpread::None,
                    assigned_width: *assigned_width,
                    assigned_height: None,
                    assigned_depth: None,
                    moved_left: *moved_left,
                    raised: *raised,
                    computed_width: Once::new(),
                    computed_height: Once::new(),
                    computed_depth: Once::new(),
                }
            }
            VBoxInfo::VTop {
                scaled,
                assigned_width,
                assigned_height,
                assigned_depth,
                moved_left,
                raised,
                ..
            } => {
                *assigned_height = None;
                *assigned_depth = None;
                *scaled = ToOrSpread::None;
                VBoxInfo::VBox {
                    scaled: ToOrSpread::None,
                    assigned_width: *assigned_width,
                    assigned_height: None,
                    assigned_depth: None,
                    moved_left: *moved_left,
                    raised: *raised,
                    computed_width: Once::new(),
                    computed_height: Once::new(),
                    computed_depth: Once::new(),
                }
            }
            _ => unreachable!(),
        }
    }

    fn height_inner(v: &[VNode<ET>]) -> ET::Dim {
        v.iter().map(|c| c.height() + c.depth()).sum::<ET::Dim>() + -Self::depth_inner(v)
    }

    fn depth_inner(v: &[VNode<ET>]) -> ET::Dim {
        for c in v.iter().rev() {
            if !c.opaque() {
                return c.depth();
            }
        }
        ET::Dim::default()
    }

    fn width_inner(v: &[VNode<ET>]) -> ET::Dim {
        v.iter().map(|c| c.width()).max().unwrap_or_default()
    }

    fn get_height(&self, v: &[VNode<ET>]) -> ET::Dim {
        match self {
            VBoxInfo::VAlignColumn => Self::height_inner(v),
            VBoxInfo::VAlignCell { to, .. } => to.unwrap_or_else(|| Self::height_inner(v)),
            VBoxInfo::VBox {
                assigned_height,
                computed_height,
                ..
            } => assigned_height
                .unwrap_or_else(|| *computed_height.get_or_init(|| Self::height_inner(v))),
            VBoxInfo::VTop {
                assigned_height,
                computed_height,
                ..
            } => assigned_height.unwrap_or_else(|| {
                *computed_height.get_or_init(|| match v.first() {
                    Some(c @ VNode::Box(..)) => c.height(),
                    _ => ET::Dim::default(),
                })
            }),
        }
    }
    fn get_width(&self, v: &[VNode<ET>]) -> ET::Dim {
        match self {
            VBoxInfo::VAlignColumn => Self::width_inner(v),
            VBoxInfo::VAlignCell { .. } => Self::width_inner(v),
            VBoxInfo::VBox {
                assigned_width,
                computed_width,
                ..
            } => assigned_width
                .unwrap_or_else(|| *computed_width.get_or_init(|| Self::width_inner(v))),
            VBoxInfo::VTop {
                assigned_width,
                computed_width,
                ..
            } => assigned_width
                .unwrap_or_else(|| *computed_width.get_or_init(|| Self::width_inner(v))),
        }
    }
    fn get_depth(&self, v: &[VNode<ET>]) -> ET::Dim {
        match self {
            VBoxInfo::VAlignColumn => Self::depth_inner(v),
            VBoxInfo::VAlignCell { .. } => Self::depth_inner(v),
            VBoxInfo::VBox {
                assigned_depth,
                computed_depth,
                ..
            } => assigned_depth
                .unwrap_or_else(|| *computed_depth.get_or_init(|| Self::depth_inner(v))),
            VBoxInfo::VTop {
                assigned_depth,
                computed_depth,
                ..
            } => assigned_depth.unwrap_or_else(|| {
                *computed_depth.get_or_init(|| {
                    let x = match v.first() {
                        Some(c @ VNode::Box(..)) => c.height(),
                        _ => ET::Dim::default(),
                    };
                    let h = Self::height_inner(v);
                    let d = Self::depth_inner(v);
                    h + d - x
                })
            }),
        }
    }

    /// Raise this box by the given amount (i.e. `\raise` or `\lower`)
    pub fn raise(&mut self, d: ET::Dim) {
        match self {
            VBoxInfo::VBox { raised, .. } => *raised = Some(d),
            VBoxInfo::VTop { raised, .. } => *raised = Some(d),
            _ => {
                self.to_vbox();
                let VBoxInfo::VBox { raised, .. } = self else {
                    unreachable!()
                };
                *raised = Some(d)
            }
        }
    }
    /// Move this box left by the given amount (i.e. `\moveleft` or `\moveright`)
    pub fn move_left(&mut self, d: ET::Dim) {
        match self {
            VBoxInfo::VBox { moved_left, .. } => *moved_left = Some(d),
            VBoxInfo::VTop { moved_left, .. } => *moved_left = Some(d),
            _ => {
                self.to_vbox();
                let VBoxInfo::VBox { moved_left, .. } = self else {
                    unreachable!()
                };
                *moved_left = Some(d)
            }
        }
    }
}

/// "Metadata" of a box
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoxInfo<ET: EngineTypes> {
    H(HBoxInfo<ET>),
    V(VBoxInfo<ET>),
}
impl<ET: EngineTypes> BoxInfo<ET> {
    /// Turns this box info into the corresponding [`NodeList`]
    pub fn open_list(self, start: SourceRef<ET>) -> NodeList<ET> {
        match self {
            BoxInfo::H(h) => h.open_list(start),
            BoxInfo::V(v) => v.open_list(start),
        }
    }
    /// Move this box left by the given amount (i.e. `\moveleft` or `\moveright`)
    pub fn move_left(&mut self, d: ET::Dim) {
        match self {
            BoxInfo::H(h) => h.move_left(d),
            BoxInfo::V(v) => v.move_left(d),
        }
    }
    /// Raise this box by the given amount (i.e. `\raise` or `\lower`)
    pub fn raise(&mut self, d: ET::Dim) {
        match self {
            BoxInfo::H(h) => h.raise(d),
            BoxInfo::V(v) => v.raise(d),
        }
    }

    pub fn assigned_height(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(h) => h.assigned_height(),
            BoxInfo::V(v) => v.assigned_height(),
        }
    }
    pub fn assigned_width(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(h) => h.assigned_width(),
            BoxInfo::V(v) => v.assigned_width(),
        }
    }
    pub fn assigned_depth(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(h) => h.assigned_depth(),
            BoxInfo::V(v) => v.assigned_depth(),
        }
    }
    pub fn computed_height(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(h) => h.computed_height(),
            BoxInfo::V(v) => v.computed_height(),
        }
    }
    pub fn computed_width(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(h) => h.computed_width(),
            BoxInfo::V(v) => v.computed_width(),
        }
    }
    pub fn computed_depth(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(h) => h.computed_depth(),
            BoxInfo::V(v) => v.computed_depth(),
        }
    }
    pub fn to_or_scaled(&self) -> Option<ToOrSpread<ET::Dim>> {
        match self {
            BoxInfo::H(HBoxInfo::HBox { scaled, .. }) => Some(*scaled),
            BoxInfo::V(VBoxInfo::VBox { scaled, .. }) => Some(*scaled),
            BoxInfo::V(VBoxInfo::VTop { scaled, .. }) => Some(*scaled),
            _ => None,
        }
    }
    pub fn raised(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(HBoxInfo::HBox { raised, .. }) => *raised,
            BoxInfo::V(VBoxInfo::VBox { raised, .. }) => *raised,
            BoxInfo::V(VBoxInfo::VTop { raised, .. }) => *raised,
            _ => None,
        }
    }
    pub fn moved_left(&self) -> Option<ET::Dim> {
        match self {
            BoxInfo::H(HBoxInfo::HBox { moved_left, .. }) => *moved_left,
            BoxInfo::V(VBoxInfo::VBox { moved_left, .. }) => *moved_left,
            BoxInfo::V(VBoxInfo::VTop { moved_left, .. }) => *moved_left,
            _ => None,
        }
    }
}
impl<ET: EngineTypes> VBoxInfo<ET> {
    pub fn assigned_height(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox {
                assigned_height, ..
            } => *assigned_height,
            VBoxInfo::VTop {
                assigned_height, ..
            } => *assigned_height,
            _ => None,
        }
    }
    pub fn assigned_width(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox { assigned_width, .. } => *assigned_width,
            VBoxInfo::VTop { assigned_width, .. } => *assigned_width,
            _ => None,
        }
    }
    pub fn assigned_depth(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox { assigned_depth, .. } => *assigned_depth,
            VBoxInfo::VTop { assigned_depth, .. } => *assigned_depth,
            _ => None,
        }
    }
    pub fn computed_height(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox {
                computed_height, ..
            } => computed_height.get().copied(),
            VBoxInfo::VTop {
                computed_height, ..
            } => computed_height.get().copied(),
            _ => None,
        }
    }
    pub fn computed_width(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox { computed_width, .. } => computed_width.get().copied(),
            VBoxInfo::VTop { computed_width, .. } => computed_width.get().copied(),
            _ => None,
        }
    }
    pub fn computed_depth(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox { computed_depth, .. } => computed_depth.get().copied(),
            VBoxInfo::VTop { computed_depth, .. } => computed_depth.get().copied(),
            _ => None,
        }
    }
    pub fn raised(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox { raised, .. } => *raised,
            VBoxInfo::VTop { raised, .. } => *raised,
            _ => None,
        }
    }
    pub fn moved_left(&self) -> Option<ET::Dim> {
        match self {
            VBoxInfo::VBox { moved_left, .. } => *moved_left,
            VBoxInfo::VTop { moved_left, .. } => *moved_left,
            _ => None,
        }
    }
    pub fn to_or_scaled(&self) -> Option<ToOrSpread<ET::Dim>> {
        match self {
            VBoxInfo::VBox { scaled, .. } => Some(*scaled),
            VBoxInfo::VTop { scaled, .. } => Some(*scaled),
            _ => None,
        }
    }
}
impl<ET: EngineTypes> HBoxInfo<ET> {
    pub fn assigned_height(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox {
                assigned_height, ..
            } => *assigned_height,
            _ => None,
        }
    }
    pub fn assigned_width(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox { assigned_width, .. } => *assigned_width,
            _ => None,
        }
    }
    pub fn assigned_depth(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox { assigned_depth, .. } => *assigned_depth,
            _ => None,
        }
    }
    pub fn computed_height(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox {
                computed_height, ..
            } => computed_height.get().copied(),
            _ => None,
        }
    }
    pub fn computed_width(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox { computed_width, .. } => computed_width.get().copied(),
            _ => None,
        }
    }
    pub fn computed_depth(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox { computed_depth, .. } => computed_depth.get().copied(),
            _ => None,
        }
    }
    pub fn raised(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox { raised, .. } => *raised,
            _ => None,
        }
    }
    pub fn moved_left(&self) -> Option<ET::Dim> {
        match self {
            HBoxInfo::HBox { moved_left, .. } => *moved_left,
            _ => None,
        }
    }
    pub fn to_or_scaled(&self) -> Option<ToOrSpread<ET::Dim>> {
        match self {
            HBoxInfo::HBox { scaled, .. } => Some(*scaled),
            _ => None,
        }
    }
}

/// A box, i.e. the result of e.g. `\hbox`, `\vbox`, `\vtop`,
/// a line of a paragraph, a row/cell in an `\halign`...
#[derive(Debug, Clone)]
pub enum TeXBox<ET: EngineTypes> {
    /// A vertial box
    V {
        /// The box info, containing "metadata" about the box
        info: VBoxInfo<ET>,
        /// The nodes in this box
        children: Box<[VNode<ET>]>,
        /// The source reference of the start of this box
        start: SourceRef<ET>,
        /// The source reference of the end of this box
        end: SourceRef<ET>,
    },
    /// A horizontal box
    H {
        /// The box info, containing "metadata" about the box
        info: HBoxInfo<ET>,
        /// The nodes in this box
        children: Box<[HNode<ET>]>,
        /// The source reference of the start of this box
        start: SourceRef<ET>,
        /// The source reference of the end of this box
        end: SourceRef<ET>,
        /// The vertical skip before this box, if any, as computed based on `\prevdepth`
        preskip: Option<Skip<ET::Dim>>,
    },
}

impl<ET: EngineTypes> TeXBox<ET> {
    pub fn to_math(self) -> MathNode<ET, UnresolvedMathFontStyle<ET>> {
        MathNode::Atom(MathAtom {
            nucleus: MathNucleus::Simple {
                kernel: MathKernel::Box(self),
                limits: None,
                cls: MathClass::Ord,
            },
            sub: None,
            sup: None,
        })
    }
    /// Whether this box is empty
    pub fn is_empty(&self) -> bool {
        match self {
            TeXBox::H { children, .. } => children.is_empty(),
            TeXBox::V { children, .. } => children.is_empty(),
        }
    }
    /// Assigns the given height to this box (as e.g. `\ht0=50pt`)
    pub fn assign_height(&mut self, h: ET::Dim) {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox {
                    assigned_height, ..
                },
                ..
            } => *assigned_height = Some(h),
            TeXBox::V {
                info: VBoxInfo::VBox {
                    assigned_height, ..
                },
                ..
            } => *assigned_height = Some(h),
            TeXBox::V {
                info: VBoxInfo::VTop {
                    assigned_height, ..
                },
                ..
            } => *assigned_height = Some(h),
            _ => (),
        }
    }
    /// The assigned height of this box, if any (i.e. the result of `\ht0=...`)
    pub fn assigned_height(&self) -> Option<ET::Dim> {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox {
                    assigned_height, ..
                },
                ..
            } => *assigned_height,
            TeXBox::V {
                info: VBoxInfo::VBox {
                    assigned_height, ..
                },
                ..
            } => *assigned_height,
            TeXBox::V {
                info: VBoxInfo::VTop {
                    assigned_height, ..
                },
                ..
            } => *assigned_height,
            _ => None,
        }
    }
    /// Assigns the given width to this box (as e.g. `\wd0=50pt`)
    pub fn assign_width(&mut self, w: ET::Dim) {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox { assigned_width, .. },
                ..
            } => *assigned_width = Some(w),
            TeXBox::V {
                info: VBoxInfo::VBox { assigned_width, .. },
                ..
            } => *assigned_width = Some(w),
            TeXBox::V {
                info: VBoxInfo::VTop { assigned_width, .. },
                ..
            } => *assigned_width = Some(w),
            _ => (),
        }
    }
    /// The assigned width of this box, if any (i.e. the result of `\wd0=...`)
    pub fn assigned_width(&self) -> Option<ET::Dim> {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox { assigned_width, .. },
                ..
            } => *assigned_width,
            TeXBox::V {
                info: VBoxInfo::VBox { assigned_width, .. },
                ..
            } => *assigned_width,
            TeXBox::V {
                info: VBoxInfo::VTop { assigned_width, .. },
                ..
            } => *assigned_width,
            _ => None,
        }
    }
    /// Assigns the given depth to this box (as e.g. `\dp0=50pt`)
    pub fn assign_depth(&mut self, d: ET::Dim) {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox { assigned_depth, .. },
                ..
            } => *assigned_depth = Some(d),
            TeXBox::V {
                info: VBoxInfo::VBox { assigned_depth, .. },
                ..
            } => *assigned_depth = Some(d),
            TeXBox::V {
                info: VBoxInfo::VTop { assigned_depth, .. },
                ..
            } => *assigned_depth = Some(d),
            _ => (),
        }
    }
    /// The assigned depth of this box, if any (i.e. the result of `\dp0=...`)
    pub fn assigned_depth(&self) -> Option<ET::Dim> {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox { assigned_depth, .. },
                ..
            } => *assigned_depth,
            TeXBox::V {
                info: VBoxInfo::VBox { assigned_depth, .. },
                ..
            } => *assigned_depth,
            TeXBox::V {
                info: VBoxInfo::VTop { assigned_depth, .. },
                ..
            } => *assigned_depth,
            _ => None,
        }
    }
    /// The "scaling factor" of this box, i.e. the `to` or `spread` in e.g. `\hbox to 50pt` or `\hbox spread 10pt`
    pub fn to_or_scaled(&self) -> ToOrSpread<ET::Dim> {
        match self {
            TeXBox::H {
                info: HBoxInfo::HBox { scaled, .. },
                ..
            } => *scaled,
            TeXBox::V {
                info: VBoxInfo::VBox { scaled, .. },
                ..
            } => *scaled,
            TeXBox::V {
                info: VBoxInfo::VTop { scaled, .. },
                ..
            } => *scaled,
            _ => ToOrSpread::None,
        }
    }
}

impl<ET: EngineTypes> NodeTrait<ET> for TeXBox<ET> {
    fn display_fmt(&self, indent: usize, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display_do_indent(indent, f)?;
        match self {
            TeXBox::H { info, children, .. } => {
                write!(f, "<hbox:{}>", info)?;
                for c in children.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                write!(f, "</hbox:{}>", info)
            }
            TeXBox::V { info, children, .. } => {
                write!(f, "<vbox:{}>", info)?;
                for c in children.iter() {
                    c.display_fmt(indent + 2, f)?;
                }
                display_do_indent(indent, f)?;
                write!(f, "</vbox:{}>", info)
            }
        }
    }

    fn height(&self) -> ET::Dim {
        match self {
            TeXBox::H {
                info,
                children,
                preskip,
                ..
            } => info.get_height(children) + preskip.map(|s| s.base).unwrap_or_default(),
            TeXBox::V { info, children, .. } => info.get_height(children),
        }
    }

    fn width(&self) -> ET::Dim {
        match self {
            TeXBox::H { info, children, .. } => info.get_width(children),
            TeXBox::V { info, children, .. } => info.get_width(children),
        }
    }

    fn depth(&self) -> ET::Dim {
        match self {
            TeXBox::H { info, children, .. } => info.get_depth(children),
            TeXBox::V { info, children, .. } => info.get_depth(children),
        }
    }
    fn nodetype(&self) -> NodeType {
        match self {
            TeXBox::H { .. } => NodeType::HList,
            TeXBox::V { .. } => NodeType::VList,
        }
    }
    fn sourceref(&self) -> Option<(&SourceRef<ET>, &SourceRef<ET>)> {
        match self {
            TeXBox::H { start, end, .. } => Some((start, end)),
            TeXBox::V { start, end, .. } => Some((start, end)),
        }
    }
}
