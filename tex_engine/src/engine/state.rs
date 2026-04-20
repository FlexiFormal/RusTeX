/*! The [`State`] of a TeX engine keeps track of scoped (by TeX groups)
values. */
pub mod tex_state;

use crate::commands::primitives::{PrimitiveCommands, PrimitiveIdentifier, PRIMITIVES};
use crate::commands::{PrimitiveCommand, TeXCommand};
use crate::engine::gullet::methods::CSOrActiveChar;
use crate::engine::{EngineAux, EngineReferences, EngineTypes};
use crate::tex::catcodes::{CategoryCode, CategoryCodeScheme};
use crate::tex::nodes::boxes::TeXBox;
use crate::tex::numerics::{MuSkip, Skip};
use crate::tex::tokens::control_sequences::CSName;
use crate::tex::tokens::token_lists::TokenList;
use std::fmt::Formatter;

/// The type of a group, e.g. `{...}`, `\begingroup...\endgroup`, `$...$`.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum GroupType {
    /// A group delimited by `{` and `}`.
    Simple,
    /// `\hbox`
    HBox,
    /// `\vadjust`
    VAdjust,
    /// `\vbox`
    VBox,
    /// `\vtop`
    VTop,
    /// `\halign` or `\valign`
    Align,
    /// `\noalign`
    Noalign,
    /// Output routine
    Output,
    /// `{...}` in math mode
    Math,
    /// Discretionary
    Disc,
    /// Insert
    Insert,
    /// `\vcenter`
    VCenter,
    /// `\mathchoice`
    MathChoice,
    /// A group delimited by `\begingroup` and `\endgroup`.
    SemiSimple,
    /// A math group delimited by `$` (inline) or `$$` (display).
    MathShift { display: bool },
    /// An inner math group delimited by `\left` and `\right`.
    LeftRight,
}
impl std::fmt::Display for GroupType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupType::Simple => write!(f, "simple"),
            GroupType::HBox => write!(f, "hbox"),
            GroupType::VAdjust => write!(f, "adjusted hbox"),
            GroupType::VBox => write!(f, "vbox"),
            GroupType::VTop => write!(f, "vtop"),
            GroupType::Align => write!(f, "align"),
            GroupType::Noalign => write!(f, "no align"),
            GroupType::Output => write!(f, "output"),
            GroupType::Math => write!(f, "math"),
            GroupType::Disc => write!(f, "disc"),
            GroupType::Insert => write!(f, "insert"),
            GroupType::VCenter => write!(f, "vcenter"),
            GroupType::MathChoice => write!(f, "math choice"),
            GroupType::SemiSimple => write!(f, "semi simple"),
            GroupType::MathShift { .. } => write!(f, "math shift"),
            GroupType::LeftRight { .. } => write!(f, "math left"),
        }
    }
}
impl GroupType {
    pub fn to_byte(&self) -> u8 {
        match self {
            // 0: bottom level (no group)
            GroupType::Simple => 1,
            GroupType::HBox => 2,
            GroupType::VAdjust => 3,
            GroupType::VBox => 4,
            GroupType::VTop => 5,
            GroupType::Align => 6,
            GroupType::Noalign => 7,
            GroupType::Output => 8,
            GroupType::Math => 9,
            GroupType::Disc => 10,
            GroupType::Insert => 11,
            GroupType::VCenter => 12,
            GroupType::MathChoice => 13,
            GroupType::SemiSimple => 14,
            GroupType::MathShift { .. } => 15,
            GroupType::LeftRight => 16,
        }
    }
}

/// A TeX state; holds all the different parameters, equivalents, registers etc.
/// The canoncial implementation is [`TeXState`](tex_state::DefaultState).
///
/// Note that we do not require `ET:`[`EngineTypes`]`<`[`State`](EngineTypes::State)`=Self>` - this allows for
/// implementing your own State by just wrapping an existing implementation in a new wrapper struct and pass on functionality
/// to the inner State, which would otherwise
/// fail since `ET::State` would be the outer wrapper struct, not the inner one.
pub trait State<ET: EngineTypes>: Sized + Clone {
    /// Instantiate a new [`State`]
    fn new(nullfont: ET::Font, aux: &mut EngineAux<ET>) -> Self;
    /// convert the provided integer value into a legal index for a register (or None if out of range)
    fn register_index(i: ET::Int) -> Option<usize> {
        let idx: i64 = i.into();
        if idx < 0 || idx > u16::MAX.into() {
            None
        } else {
            Some(idx as usize)
        }
    }

    /// Append a [`Token`](crate::tex::tokens::Token) to be inserted when the current group ends (i.e. `\aftergroup`)
    fn aftergroup(&mut self, token: ET::Token);

    /// Get the current set of mathfonts (text, script, scriptscript)
    /// for the given family number, where `fam` is 0..15
    fn get_mathfonts(&self, fam: u8) -> (ET::Font, ET::Font, ET::Font) {
        (
            self.get_textfont(fam).clone(),
            self.get_scriptfont(fam).clone(),
            self.get_scriptscriptfont(fam).clone(),
        )
    }
    /// register a new [`PrimitiveCommand`]. Should only be called during engine initialization.
    fn register_primitive(
        &mut self,
        aux: &mut EngineAux<ET>,
        name: &'static str,
        cmd: PrimitiveCommand<ET>,
    );
    /// return the set of all registered [`PrimitiveCommands`].
    fn primitives(&self) -> &PrimitiveCommands<ET>;
    /// push a new group level to the scoping stack; `line_number` is used for `\tracinggroups`
    fn push(&mut self, aux: &mut EngineAux<ET>, group_type: GroupType, line_number: usize);
    /// pop a group level from the scoping stack. Needs the mouth to insert the `\aftergroup` [`Token`](crate::tex::tokens::Token)s (if set)
    fn pop(&mut self, aux: &mut EngineAux<ET>, mouth: &mut ET::Mouth);
    /// The current [`GroupType] (i.e. `\currentgrouptype`).
    fn get_group_type(&self) -> Option<GroupType>;
    /// The current group level/depth (0 being the top level; i.e. `\currentgrouplevel`)
    fn get_group_level(&self) -> usize;
    /// Get the current [`Font`](crate::engine::fontsystem::Font)
    fn get_current_font(&self) -> &ET::Font;
    /// Set the current [`Font`](crate::engine::fontsystem::Font)
    fn set_current_font(&mut self, aux: &mut EngineAux<ET>, fnt: ET::Font, globally: bool);
    /// Get the current `\textfont` for the given family number, where `fam` is 0..15
    fn get_textfont(&self, fam: u8) -> &ET::Font;
    /// Set the current `\textfont` for the given family number, where `fam` is 0..15
    fn set_textfont(&mut self, aux: &mut EngineAux<ET>, fam: u8, fnt: ET::Font, globally: bool);
    /// Get the current `\scriptfont` for the given family number, where `fam` is 0..15
    fn get_scriptfont(&self, fam: u8) -> &ET::Font;
    /// Set the current `\scriptfont` for the given family number, where `fam` is 0..15
    fn set_scriptfont(&mut self, aux: &mut EngineAux<ET>, fam: u8, fnt: ET::Font, globally: bool);
    /// Get the current `\scriptscriptfont` for the given family number, where `fam` is 0..15
    fn get_scriptscriptfont(&self, fam: u8) -> &ET::Font;
    /// Set the current `\scriptscriptfont` for the given family number, where `fam` is 0..15
    fn set_scriptscriptfont(
        &mut self,
        aux: &mut EngineAux<ET>,
        fam: u8,
        fnt: ET::Font,
        globally: bool,
    );

    /// Get the current `\par` token
    fn get_par_token(&self) -> ET::CSName;
    /// Set the current `\par` token
    fn set_par_token(&mut self, par: ET::CSName);
    /// Get the current [`CategoryCodeScheme`]
    fn get_catcode_scheme(&self) -> &CategoryCodeScheme<ET::Char>;
    /// Set the current [`CategoryCode`] for a character
    fn set_catcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, cc: CategoryCode, globally: bool);
    /// Get the current space factor code `\sfcode` for a character
    fn get_sfcode(&self, c: ET::Char) -> u16;
    /// Set the current space factor code `\sfcode` for a character
    fn set_sfcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, sfcode: u16, globally: bool);
    /// Get the current lower case code `\lccode` for a character
    fn get_lccode(&self, c: ET::Char) -> ET::Char;
    /// Set the current lower case code `\lccode` for a character
    fn set_lccode(&mut self, aux: &EngineAux<ET>, c: ET::Char, lccode: ET::Char, globally: bool);
    /// Get the current upper case code `\uccode` for a character
    fn get_uccode(&self, c: ET::Char) -> ET::Char;
    /// Set the current upper case code `\uccode` for a character
    fn set_uccode(&mut self, aux: &EngineAux<ET>, c: ET::Char, uccode: ET::Char, globally: bool);
    /// Get the current delimiter code `\delcode` for a character
    fn get_delcode(&self, c: ET::Char) -> ET::Int;
    /// Set the current delimiter code `\delcode` for a character
    fn set_delcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, delcode: ET::Int, globally: bool);
    /// Get the current math code `\mathcode` for a character
    fn get_mathcode(&self, c: ET::Char) -> u32;
    /// Set the current math code `\mathcode` for a character
    fn set_mathcode(&mut self, aux: &EngineAux<ET>, c: ET::Char, mathcode: u32, globally: bool);
    /// Get the current endline character
    fn get_endline_char(&self) -> Option<ET::Char>;
    /// Set the current endline character
    fn set_endline_char(&mut self, aux: &EngineAux<ET>, c: Option<ET::Char>, globally: bool);
    /// Get the current escape character
    fn get_escape_char(&self) -> Option<ET::Char>;
    /// Set the current escape character
    fn set_escape_char(&mut self, aux: &EngineAux<ET>, c: Option<ET::Char>, globally: bool);
    /// Get the current newline character
    fn get_newline_char(&self) -> Option<ET::Char>;
    /// Set the current newline character
    fn set_newline_char(&mut self, aux: &EngineAux<ET>, c: Option<ET::Char>, globally: bool);
    /// Get the current `\parshape`
    fn get_parshape(&self) -> &Vec<(ET::Dim, ET::Dim)>;
    /// Get the current `\parshape` and set it to its default value
    fn take_parshape(&mut self) -> Vec<(ET::Dim, ET::Dim)>;
    /// Set the current `\parshape`
    fn set_parshape(
        &mut self,
        aux: &EngineAux<ET>,
        parshape: Vec<(ET::Dim, ET::Dim)>,
        globally: bool,
    );
    /// Get an integer register value
    fn get_int_register(&self, idx: usize) -> ET::Int;
    /// Set an integer register value
    fn set_int_register(&mut self, aux: &EngineAux<ET>, idx: usize, v: ET::Int, globally: bool);
    /// Get a primitive integer value
    fn get_primitive_int(&self, name: PrimitiveIdentifier) -> ET::Int;
    /// Set a primitive integer value
    fn set_primitive_int(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: ET::Int,
        globally: bool,
    );
    /// Get a dimen register value
    fn get_dim_register(&self, idx: usize) -> ET::Dim;
    /// Set a dimen register value
    fn set_dim_register(&mut self, aux: &EngineAux<ET>, idx: usize, v: ET::Dim, globally: bool);
    /// Get a skip register value
    fn get_skip_register(&self, idx: usize) -> Skip<ET::Dim>;
    /// Set a skip register value
    fn set_skip_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: Skip<ET::Dim>,
        globally: bool,
    );
    /// Get a muskip register value
    fn get_muskip_register(&self, idx: usize) -> MuSkip<ET::MuDim>;
    /// Set a muskip register value
    fn set_muskip_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: MuSkip<ET::MuDim>,
        globally: bool,
    );
    /// Get a token register value
    fn get_toks_register(&self, idx: usize) -> &TokenList<ET::Token>;
    /// Set a token register value
    fn set_toks_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: TokenList<ET::Token>,
        globally: bool,
    );
    /// Get a box register value
    fn get_box_register(&self, idx: usize) -> Option<&TeXBox<ET>>;
    /// Get a box register value mutably (to e.g. change `\ht`, `\wd`, `\dp`, etc.)
    fn get_box_register_mut(&mut self, idx: usize) -> Option<&mut TeXBox<ET>>;
    /// Take a box register value; replacing it with `None` (i.e. void box)
    fn take_box_register(&mut self, idx: usize) -> Option<TeXBox<ET>>;
    /// Set a box register value
    fn set_box_register(
        &mut self,
        aux: &EngineAux<ET>,
        idx: usize,
        v: Option<TeXBox<ET>>,
        globally: bool,
    );
    /// Get a primitive dimension value
    fn get_primitive_dim(&self, name: PrimitiveIdentifier) -> ET::Dim;
    /// Set a primitive dimension value
    fn set_primitive_dim(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: ET::Dim,
        globally: bool,
    );
    /// Get a primitive skip value
    fn get_primitive_skip(&self, name: PrimitiveIdentifier) -> Skip<ET::Dim>;
    /// Set a primitive skip value
    fn set_primitive_skip(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: Skip<ET::Dim>,
        globally: bool,
    );
    /// Get a primitive muskip value
    fn get_primitive_muskip(&self, name: PrimitiveIdentifier) -> MuSkip<ET::MuDim>;
    /// Set a primitive muskip value
    fn set_primitive_muskip(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: MuSkip<ET::MuDim>,
        globally: bool,
    );
    /// Get a primitive token list
    fn get_primitive_tokens(&self, name: PrimitiveIdentifier) -> &TokenList<ET::Token>;
    /// Set a primitive token list
    fn set_primitive_tokens(
        &mut self,
        aux: &EngineAux<ET>,
        name: PrimitiveIdentifier,
        v: TokenList<ET::Token>,
        globally: bool,
    );
    /// Get the current definition for the control sequence name
    fn get_command(&self, name: &ET::CSName) -> Option<&TeXCommand<ET>>;
    /// Set the current definition for the control sequence name
    fn set_command(
        &mut self,
        aux: &EngineAux<ET>,
        name: ET::CSName,
        cmd: Option<TeXCommand<ET>>,
        globally: bool,
    );
    /// Get the current definition for the active character
    fn get_ac_command(&self, c: ET::Char) -> Option<&TeXCommand<ET>>;
    /// Set the current definition for the active character
    fn set_ac_command(
        &mut self,
        aux: &EngineAux<ET>,
        c: ET::Char,
        cmd: Option<TeXCommand<ET>>,
        globally: bool,
    );
}

/// Convenience trait for tracking state changes in a [`StateStack`] and rolling them back
/// if necessary at the end of a group.
pub trait StateChangeTracker<ET: EngineTypes>: State<ET> {
    /// Get the current [`StateStack`]
    fn stack(&mut self) -> &mut StateStack<ET>;
    /// Change a field of the state, and add the change to the [`StateStack`]. Also takes care of inspecting
    /// and considering the current `\globaldefs` value and passes on the *actual* computed `globally` value
    /// to the continuation function, which should return the [`StateChange`] containg the *old* value.
    ///
    /// For example, on `\count5=4`, you could call this function like this:
    /// `self.change_field(false,|slf,g| `[`StateChange::IntRegister`]`(5,std::mem::replace(&mut slf.int_registers[5],4)))`
    fn change_field<F: FnOnce(&mut Self, bool) -> StateChange<ET>>(
        &mut self,
        globally: bool,
        f: F,
    ) {
        let globaldefs = self.get_primitive_int(PRIMITIVES.globaldefs);
        let zero = ET::Int::default();
        let global = if globaldefs == zero {
            globally
        } else {
            globaldefs > zero
        };
        let change = f(self, global);
        if global {
            self.stack().add_change_globally(change)
        } else {
            self.stack().add_change_locally(change)
        }
    }
}

/// A change to a [`State`], to be potentially rolled back when a group ends.
#[derive(Clone, Debug)]
pub enum StateChange<ET: EngineTypes> {
    Catcode {
        char: ET::Char,
        old: CategoryCode,
    },
    SfCode {
        char: ET::Char,
        old: u16,
    },
    DelCode {
        char: ET::Char,
        old: ET::Int,
    },
    LcCode {
        char: ET::Char,
        old: ET::Char,
    },
    UcCode {
        char: ET::Char,
        old: ET::Char,
    },
    MathCode {
        char: ET::Char,
        old: u32,
    },
    ParShape {
        old: Vec<(ET::Dim, ET::Dim)>,
    },
    CurrentFont(ET::Font),
    TextFont {
        idx: u8,
        old: ET::Font,
    },
    ScriptFont {
        idx: u8,
        old: ET::Font,
    },
    ScriptScriptFont {
        idx: u8,
        old: ET::Font,
    },
    EndlineChar {
        old: Option<ET::Char>,
    },
    EscapeChar {
        old: Option<ET::Char>,
    },
    NewlineChar {
        old: Option<ET::Char>,
    },
    IntRegister {
        idx: usize,
        old: ET::Int,
    },
    DimRegister {
        idx: usize,
        old: ET::Dim,
    },
    SkipRegister {
        idx: usize,
        old: Skip<ET::Dim>,
    },
    MuSkipRegister {
        idx: usize,
        old: MuSkip<ET::MuDim>,
    },
    BoxRegister {
        idx: usize,
        old: Option<TeXBox<ET>>,
    },
    ToksRegister {
        idx: usize,
        old: TokenList<ET::Token>,
    },
    PrimitiveInt {
        name: PrimitiveIdentifier,
        old: ET::Int,
    },
    PrimitiveDim {
        name: PrimitiveIdentifier,
        old: ET::Dim,
    },
    PrimitiveToks {
        name: PrimitiveIdentifier,
        old: TokenList<ET::Token>,
    },
    PrimitiveSkip {
        name: PrimitiveIdentifier,
        old: Skip<ET::Dim>,
    },
    PrimitiveMuSkip {
        name: PrimitiveIdentifier,
        old: MuSkip<ET::MuDim>,
    },
    Command {
        name: ET::CSName,
        old: Option<TeXCommand<ET>>,
    },
    AcCommand {
        char: ET::Char,
        old: Option<TeXCommand<ET>>,
    },
    // /// A custom state change, to be implemented by the engine, if additional state change types are needed
    //Custom{ change:Ptr<Mutex<Option<Box<dyn CustomStateChange<ET>>>>>},
}
/*
/// A custom state change, to be implemented by the engine, if additional state change types are needed.
pub trait CustomStateChange<ET:EngineTypes> {
    /// Check if this state change is equivalent to another one, i.e. if it needs to be rolled back
    /// when a group ends, or is superseded by a previous change
    fn equiv(&self,other:&dyn CustomStateChange<ET>) -> bool;
    fn restore(&mut self,aux:&EngineAux<ET>,state:&mut ET::State,trace:bool);
} */

/// A level of the [`StateStack`], to be rolled back when a group ends
#[derive(Clone)]
pub struct StackLevel<ET: EngineTypes> {
    /// The type of the group
    pub group_type: GroupType,
    /// The `\aftergroup` [`Token`](crate::tex::tokens::Token)s to be inserted when the group ends
    pub aftergroup: Vec<ET::Token>,
    changes: Vec<StateChangeI<ET>>,
}

/// A stack of [`StateChange`]s, to be rolled back when a group ends
pub struct StateStack<ET: EngineTypes> {
    /// The stack of [`StackLevel`]s
    pub stack: Vec<StackLevel<ET>>,
    vecs: Vec<Vec<StateChangeI<ET>>>,
}
impl<ET: EngineTypes> Clone for StateStack<ET> {
    fn clone(&self) -> Self {
        Self {
            stack: self.stack.clone(),
            vecs: vec![],
        }
    }
}
impl<ET: EngineTypes> Default for StateStack<ET> {
    fn default() -> Self {
        Self {
            stack: vec![],
            vecs: vec![],
        }
    }
}
impl<ET: EngineTypes> StateStack<ET> {
    /// Push a new stack level onto the stack with the given [`GroupType`], as a new group begins
    pub fn push(&mut self, group_type: GroupType) {
        let lvl = StackLevel {
            group_type,
            aftergroup: vec![],
            changes: self.vecs.pop().unwrap_or_default(),
        };
        self.stack.push(lvl);
    }
    /// Pop a stack level from the stack, as a group ends. Returns the [`GroupType`], the `\aftergroup` [`Token`](crate::tex::tokens::Token)s,
    /// and an iterator over the [`StateChange`]s to be rolled back.
    pub fn pop(&mut self) -> (GroupType, Vec<ET::Token>, ChangeIter<'_, ET>) {
        let lvl = self.stack.pop().unwrap();
        (
            lvl.group_type,
            lvl.aftergroup,
            ChangeIter {
                stack: self,
                chs: lvl.changes,
            },
        )
    }
    /// Register a global state change, never to be rolled back;
    /// if there is a stack level, remove any equivalent previous changes
    pub fn add_change_globally(&mut self, change: StateChange<ET>) {
        let change = change.into();
        for lvl in &mut self.stack {
            for c in lvl.changes.iter_mut() {
                if c.equiv(&change) {
                    c.active = false
                }
            }
        }
    }
    /// Register a local state change, to be rolled back when the current group ends
    pub fn add_change_locally(&mut self, change: StateChange<ET>) {
        if let Some(lvl) = self.stack.last_mut() {
            let change = change.into();
            if lvl.changes.iter().all(|c| !c.equiv(&change)) {
                lvl.changes.push(change);
            }
        }
    }
}

/// An iterator over [`StateChange`]s to be rolled back when a group ends. Iterated over using [`ChangeIter::close`].
pub struct ChangeIter<'a, ET: EngineTypes> {
    stack: &'a mut StateStack<ET>,
    chs: Vec<StateChangeI<ET>>,
}
impl<'a, ET: EngineTypes> ChangeIter<'a, ET> {
    /// Close the iterator, rolling back any changes
    pub fn close<F: FnMut(StateChange<ET>)>(mut self, mut f: F) {
        for ch in self.chs.drain(..) {
            if ch.active {
                f(ch.ch);
            }
        }
        self.stack.vecs.push(self.chs);
    }
}

#[derive(Clone)]
struct StateChangeI<ET: EngineTypes> {
    active: bool,
    id: (std::mem::Discriminant<StateChange<ET>>, usize),
    ch: StateChange<ET>,
}
impl<ET: EngineTypes> StateChangeI<ET> {
    fn equiv(&self, other: &StateChangeI<ET>) -> bool {
        self.active && self.id == other.id
    }
}
impl<ET: EngineTypes> From<StateChange<ET>> for StateChangeI<ET> {
    fn from(value: StateChange<ET>) -> Self {
        let u = match &value {
            StateChange::Catcode { char, .. } => (*char).into() as usize,
            StateChange::SfCode { char, .. } => (*char).into() as usize,
            StateChange::LcCode { char, .. } => (*char).into() as usize,
            StateChange::UcCode { char, .. } => (*char).into() as usize,
            StateChange::MathCode { char, .. } => (*char).into() as usize,
            StateChange::DelCode { char, .. } => (*char).into() as usize,
            StateChange::AcCommand { char, .. } => (*char).into() as usize,
            StateChange::CurrentFont(_) => 0,
            StateChange::EndlineChar { .. } => 0,
            StateChange::EscapeChar { .. } => 0,
            StateChange::NewlineChar { .. } => 0,
            StateChange::ParShape { .. } => 0,
            StateChange::TextFont { idx, .. } => *idx as usize,
            StateChange::ScriptFont { idx, .. } => *idx as usize,
            StateChange::ScriptScriptFont { idx, .. } => *idx as usize,
            StateChange::IntRegister { idx, .. } => *idx,
            StateChange::DimRegister { idx, .. } => *idx,
            StateChange::SkipRegister { idx, .. } => *idx,
            StateChange::MuSkipRegister { idx, .. } => *idx,
            StateChange::ToksRegister { idx, .. } => *idx,
            StateChange::BoxRegister { idx, .. } => *idx,
            StateChange::PrimitiveInt { name, .. } => name.as_u16() as usize,
            StateChange::PrimitiveDim { name, .. } => name.as_u16() as usize,
            StateChange::PrimitiveSkip { name, .. } => name.as_u16() as usize,
            StateChange::PrimitiveMuSkip { name, .. } => name.as_u16() as usize,
            StateChange::PrimitiveToks { name, .. } => name.as_u16() as usize,
            StateChange::Command { name, .. } => name.id(),
        };
        StateChangeI {
            active: true,
            id: (std::mem::discriminant(&value), u),
            ch: value,
        }
    }
}

impl<ET: EngineTypes> EngineReferences<'_, ET> {
    /// Set the current definition of the provided control sequence name or active character
    pub fn set_command(
        &mut self,
        name: &CSOrActiveChar<ET::Token>,
        cmd: Option<TeXCommand<ET>>,
        globally: bool,
    ) {
        match name {
            CSOrActiveChar::Active(c) => self.state.set_ac_command(self.aux, *c, cmd, globally),
            CSOrActiveChar::Name(cs) => self.state.set_command(self.aux, cs.clone(), cmd, globally),
        }
    }
}
