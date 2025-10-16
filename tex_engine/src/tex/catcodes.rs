/*!
Category codes
 */

use crate::tex::characters::Character;
use crate::tex::tokens::control_sequences::CSName;
use crate::tex::tokens::token_lists::CharWrite;
use const_for::const_for;
use std::fmt::Formatter;

/** The category code of a character.

To convert between [`CategoryCode`]s and their numerical values (as [`u8`]), use [`CategoryCode::try_from`]
and [`u8::from`], respectively.

# Example
```rust
use tex_engine::tex::catcodes::CategoryCode;

let cat = CategoryCode::BeginGroup;
let num : u8 = cat.into();
assert_eq!(num,1);
let cat2 = CategoryCode::try_from(1).unwrap();
assert_eq!(cat2,cat);
```
 */
#[derive(Copy, PartialEq, Eq, Clone, Default)]
pub enum CategoryCode {
    /// Escape character (0); usually `\`
    Escape = 0,
    /// Begin group character (1); usually `{`
    BeginGroup = 1,
    /// End group character (2); usually `}`
    EndGroup = 2,
    /// Math shift character (3); usually `$`
    MathShift = 3,
    /// Alignment tab character (4); usually `&`
    AlignmentTab = 4,
    /// End of line character (5); usually `\n`
    EOL = 5,
    /// Parameter character (6); usually `#`
    Parameter = 6,
    /// Superscript character (7); usually `^`
    Superscript = 7,
    /// Subscript character (8); usually `_`
    Subscript = 8,
    /// Ignored character (9)
    Ignored = 9,
    /// Space character (10); usually ` `
    Space = 10,
    /// Letter character (11), usually a-z and A-Z
    Letter = 11,
    /// Other character (12), usually e.g. `@`, `!`, `?`, etc.
    #[default]
    Other = 12,
    /// Active character (13); usually `~`
    Active = 13,
    /// Comment character (14); usually `%`
    Comment = 14,
    /// Invalid character (15)
    Invalid = 15,
}

impl std::fmt::Debug for CategoryCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
impl std::fmt::Display for CategoryCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use CategoryCode as CC;
        write!(
            f,
            "{}",
            match self {
                CC::Escape => "escape",
                CC::BeginGroup => "begin group",
                CC::EndGroup => "end group",
                CC::MathShift => "math shift",
                CC::AlignmentTab => "alignment",
                CC::EOL => "end of line",
                CC::Parameter => "parameter",
                CC::Superscript => "superscript",
                CC::Subscript => "subscript",
                CC::Ignored => "ignored",
                CC::Space => "space",
                CC::Letter => "letter",
                CC::Other => "other",
                CC::Active => "active",
                CC::Comment => "comment",
                CC::Invalid => "invalid",
            }
        )
    }
}

impl From<CategoryCode> for u8 {
    fn from(cc: CategoryCode) -> Self {
        use CategoryCode as CC;
        match cc {
            CC::Escape => 0,
            CC::BeginGroup => 1,
            CC::EndGroup => 2,
            CC::MathShift => 3,
            CC::AlignmentTab => 4,
            CC::EOL => 5,
            CC::Parameter => 6,
            CC::Superscript => 7,
            CC::Subscript => 8,
            CC::Ignored => 9,
            CC::Space => 10,
            CC::Letter => 11,
            CC::Other => 12,
            CC::Active => 13,
            CC::Comment => 14,
            CC::Invalid => 15,
        }
    }
}

impl TryFrom<u8> for CategoryCode {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use CategoryCode as CC;
        Ok(match value {
            0 => CC::Escape,
            1 => CC::BeginGroup,
            2 => CC::EndGroup,
            3 => CC::MathShift,
            4 => CC::AlignmentTab,
            5 => CC::EOL,
            6 => CC::Parameter,
            7 => CC::Superscript,
            8 => CC::Subscript,
            9 => CC::Ignored,
            10 => CC::Space,
            11 => CC::Letter,
            12 => CC::Other,
            13 => CC::Active,
            14 => CC::Comment,
            15 => CC::Invalid,
            _ => return Err(()),
        })
    }
}

/// A [`CategoryCodeScheme`] assigns a [`CategoryCode`] to each [`Character`].
pub type CategoryCodeScheme<Char> = <Char as Character>::CharMap<CategoryCode>;

/** The [`CategoryCodeScheme`] where all characters have [`CategoryCode::Other`] (12) except for
         the space character, which has [`CategoryCode::Space`] (10).
*/
pub static OTHER_SCHEME_U8: CategoryCodeScheme<u8> = {
    let mut catcodes = [CategoryCode::Other; 256];
    catcodes[32] = CategoryCode::Space;
    catcodes
};

/**
The default [`CategoryCodeScheme`] for TeX, pre latex.ltx.

All characters have [`CategoryCode::Other`] (12) except for:

| Character    | Category Code   |
|--------------|-----------------
 | ` `          | [`Space`](CategoryCode::Space)|
| a-z, A-Z     | [`Letter`](CategoryCode::Letter) |
| `\`          | [`Escape`](CategoryCode::Other)  |
| `\r`         | [`EOL`](CategoryCode::EOL)       |
| `%`          | [`Comment`](CategoryCode::Comment)|
 */
pub static STARTING_SCHEME_U8: CategoryCodeScheme<u8> = {
    let mut catcodes = [CategoryCode::Other; 256];
    catcodes[92] = CategoryCode::Escape;
    catcodes[32] = CategoryCode::Space;
    catcodes[13] = CategoryCode::EOL;
    catcodes[37] = CategoryCode::Comment;
    const_for!(i in 65..91 => catcodes[i] = CategoryCode::Letter);
    const_for!(i in 97..123 => catcodes[i] = CategoryCode::Letter);
    catcodes[126] = CategoryCode::Active;
    catcodes
};

/**
The default [`CategoryCodeScheme`] used almost everywhere in LaTeX.

All characters have [`CategoryCode::Other`] (12) except for:

| Character    | Category Code   |
|--------------|-----------------
 | ` `          | [`Space`](CategoryCode::Space)|
| a-z, A-Z     | [`Letter`](CategoryCode::Letter) |
| `\`          | [`Escape`](CategoryCode::Other)  |
| `\r`         | [`EOL`](CategoryCode::EOL)       |
| `%`          | [`Comment`](CategoryCode::Comment)|
| `~`          | [`Active`](CategoryCode::Active)  |
| `#`          | [`Parameter`](CategoryCode::Parameter)|
| `^`          | [`Superscript`](CategoryCode::Superscript)|
| `_`          | [`Subscript`](CategoryCode::Subscript)|
| `{`          | [`BeginGroup`](CategoryCode::BeginGroup)|
| `}`          | [`EndGroup`](CategoryCode::EndGroup)|
| `$`          | [`MathShift`](CategoryCode::MathShift)|
| `&`          | [`AlignmentTab`](CategoryCode::AlignmentTab)|
 */
pub static DEFAULT_SCHEME_U8: CategoryCodeScheme<u8> = {
    let mut catcodes = [CategoryCode::Other; 256];
    catcodes[123] = CategoryCode::BeginGroup;
    catcodes[125] = CategoryCode::EndGroup;
    catcodes[36] = CategoryCode::MathShift;
    catcodes[38] = CategoryCode::AlignmentTab;
    catcodes[35] = CategoryCode::Parameter;
    catcodes[94] = CategoryCode::Superscript;
    catcodes[95] = CategoryCode::Subscript;
    catcodes[126] = CategoryCode::Active;
    catcodes[92] = CategoryCode::Escape;
    catcodes[32] = CategoryCode::Space;
    catcodes[13] = CategoryCode::EOL;
    catcodes[37] = CategoryCode::Comment;
    const_for!(i in 65..91 => catcodes[i] = CategoryCode::Letter);
    const_for!(i in 97..123 => catcodes[i] = CategoryCode::Letter);
    catcodes
};

/// Like [`DEFAULT_SCHEME_U8`](static@DEFAULT_SCHEME_U8), but with `@` as a letter.
/// (i.e. as after `\makeatletter`)
pub static AT_LETTER_SCHEME: CategoryCodeScheme<u8> = {
    let mut catcodes = [CategoryCode::Other; 256];
    catcodes[123] = CategoryCode::BeginGroup;
    catcodes[125] = CategoryCode::EndGroup;
    catcodes[36] = CategoryCode::MathShift;
    catcodes[38] = CategoryCode::AlignmentTab;
    catcodes[35] = CategoryCode::Parameter;
    catcodes[94] = CategoryCode::Superscript;
    catcodes[95] = CategoryCode::Subscript;
    catcodes[126] = CategoryCode::Active;
    catcodes[92] = CategoryCode::Escape;
    catcodes[32] = CategoryCode::Space;
    catcodes[13] = CategoryCode::EOL;
    catcodes[37] = CategoryCode::Comment;
    const_for!(i in 64..91 => catcodes[i] = CategoryCode::Letter);
    const_for!(i in 97..123 => catcodes[i] = CategoryCode::Letter);
    catcodes
};

/// After scanning a file, [`CategoryCode`]s such as [`EOL`](CategoryCode::EOL),
/// [`Comment`](CategoryCode::Comment) or [`Invalid`](CategoryCode::Invalid)
/// can not occur anymore. Instead, a [`Token`](super::tokens::Token) can represent e.g. a
/// *numbered parameter* (e.g. `#1` in a macro expansion), or an end-of-file, or
/// a `\noexpand` marker, or a marker for the end of an alignment cell, etc.
#[derive(Copy, PartialEq, Eq, Clone, Debug)]
pub enum CommandCode {
    /// Escape character (0); usually `\`
    Escape = 0,
    /// Begin group character (1); usually `{`
    BeginGroup = 1,
    /// End group character (2); usually `}`
    EndGroup = 2,
    /// Math shift character (3); usually `$`
    MathShift = 3,
    /// Alignment tab character (4); usually `&`
    AlignmentTab = 4,
    /// End of file marker`
    EOF = 5,
    /// Parameter character (6); usually `#`
    Parameter = 6,
    /// Superscript character (7); usually `^`
    Superscript = 7,
    /// Subscript character (8); usually `_`
    Subscript = 8,
    /// marker for a primitive command
    Primitive = 9,
    /// Space character (10); usually ` `
    Space = 10,
    /// Letter character (11), usually a-z and A-Z
    Letter = 11,
    /// Other character (12), usually e.g. `@`, `!`, `?`, etc.
    Other = 12,
    /// Active character (13); usually `~`
    Active = 13,
    /// Argument Marker
    Argument = 14,
}
impl CommandCode {
    pub fn meaning<C: Character, CS: CSName<C>, W: CharWrite<C, CS>>(
        &self,
        c: C,
        mut f: W,
    ) -> std::fmt::Result {
        match self {
            CommandCode::BeginGroup => write!(f, "begin-group character "),
            CommandCode::EndGroup => write!(f, "end-group character "),
            CommandCode::MathShift => write!(f, "math shift character "),
            CommandCode::Parameter => write!(f, "macro parameter character "),
            CommandCode::Superscript => write!(f, "superscript character "),
            CommandCode::Subscript => write!(f, "subscript character "),
            CommandCode::Space => {
                write!(f, "blank space  ")?;
                return Ok(());
            }
            CommandCode::Letter => write!(f, "the letter "),
            _ => write!(f, "the character "),
        }?;
        f.push_char(c);
        Ok(())
    }
    pub const fn as_byte(self) -> u8 {
        use CommandCode::*;
        match self {
            Escape => 0,
            BeginGroup => 1,
            EndGroup => 2,
            MathShift => 3,
            AlignmentTab => 4,
            EOF => 5,
            Parameter => 6,
            Superscript => 7,
            Subscript => 8,
            Primitive => 9,
            Space => 10,
            Letter => 11,
            Other => 12,
            Active => 13,
            Argument => 14,
        }
    }
}

impl From<CategoryCode> for CommandCode {
    fn from(value: CategoryCode) -> Self {
        match value {
            CategoryCode::Escape => CommandCode::Escape,
            CategoryCode::BeginGroup => CommandCode::BeginGroup,
            CategoryCode::EndGroup => CommandCode::EndGroup,
            CategoryCode::MathShift => CommandCode::MathShift,
            CategoryCode::AlignmentTab => CommandCode::AlignmentTab,
            CategoryCode::EOL => CommandCode::EOF,
            CategoryCode::Parameter => CommandCode::Parameter,
            CategoryCode::Superscript => CommandCode::Superscript,
            CategoryCode::Subscript => CommandCode::Subscript,
            CategoryCode::Space => CommandCode::Space,
            CategoryCode::Letter => CommandCode::Letter,
            CategoryCode::Other => CommandCode::Other,
            CategoryCode::Active => CommandCode::Active,
            _ => panic!("Invalid category code for command code: {:?}\n This is an implementation error and should not happen",value)
        }
    }
}

impl TryFrom<u8> for CommandCode {
    type Error = ();
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        use CommandCode::*;
        Ok(match value {
            0 => Escape,
            1 => BeginGroup,
            2 => EndGroup,
            3 => MathShift,
            4 => AlignmentTab,
            5 => EOF,
            6 => Parameter,
            7 => Superscript,
            8 => Subscript,
            9 => Primitive,
            10 => Space,
            11 => Letter,
            12 => Other,
            13 => Active,
            14 => Argument,
            _ => return Err(()),
        })
    }
}
