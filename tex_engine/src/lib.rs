/*! This crate provides the necessary data structures and algorithms for
processing TeX documents fully. Since there are many different TeX engines
(e.g. `pdfTeX`, `XeTeX`, `LuaTeX`, etc.), this crate largely follows an object-oriented design
for modularity and adaptability, with functionality largely implemented in generic traits.
Ideally, this lets the compiler optimize the code for the specific types used,
while still allowing for "easy" customization.

See [doc] for more information.

## Current state

- Runs without errors over all `.tex` files that succeed with `pdflatex` that I have tried so far
    -- which is every `.tex` file I have on my hard drive going back >10 years, which use everything
    from `tikz` to `biblatex` to `lstlisting`.
- Performance is roughly equivalent to `pdflatex`.
- Many primitive commands are missing, but those that are seem to be barely used (or at least
    not used in the `.tex` files I have tried).
- Many algorithms are approximate; in particular: computations of dimensions of boxes/nodes,
  page and line breaking, when exactly to enter output routines, etc. Also, various
  details of automatically inserted glue or kerning are not implemented.
- Some apects are only implemented to the extent that they do not throw errors, e.g. hyphenation,
  badness.
- Some mechanisms have grown organically and should be overhauled, e.g. the way the stomach handles
  node lists.
- error messages for content errors - a lot of them are TODO placeholders, mostly because
  they should be handled by an [`ErrorHandler`](crate::utils::errors::ErrorHandler) that may recover
  from them (as TeX does it - e.g. inserting 0 when a number is expected). This is implemented for
  *some* errors, but not many.

It should be noted that this crate primarily exists as the basis of (an upcoming reimplementation
of) [RusTeX](https://github.com/slatex/RusTeX), a TeX-to-HTML converter, the current (old) implementation
of which is ineffecient, difficult to extend and generally a mess. As such, the features this crate offers
are guided by my own needs and by what I have learned in my previous attempt. Ideally it should
be possible to extend it to support other use cases as well -- at least that is the intention.

I will happily accept pull requests for additional features or suggestions on how to
make the algorithms here more accurate. I am also very open to redesigning interfaces, if someone
has ideas on how to improve them - the ones I used are the best compromise I could come up with
between usability, efficiency and modularity. I will also happily help anyone
willing to contribute to this crate, but bear in mind that I am ridiculously overconstrained
already, so as much as I wish I could, I am in no position to dig into the various rabbit holes
that would be required to get this crate more in line with "actual TeX".

Feel free to contact me [on github](https://github.com/Jazzpirate) if you have questions.
 */
//#![warn(missing_docs)]
#![doc(html_root_url = "https://docs.rs")]
#![allow(clippy::type_complexity)]

pub mod commands;
pub mod engine;
pub mod tex;
pub mod utils;

#[cfg(doc)]
pub mod doc;

#[cfg(feature = "pdflatex")]
pub mod pdflatex;

#[cfg(test)]
#[doc(hidden)]
pub mod tests;

/// Default `pub use` prelude for the crate
pub mod prelude {
    pub use crate::engine::mouth::Mouth;
    pub use crate::engine::stomach::TeXMode;
    pub use crate::engine::{DefaultEngine, EngineTypes, PlainTeXEngine, TeXEngine};
    pub use crate::tex::catcodes::{CategoryCode, CategoryCodeScheme, CommandCode};
    pub use crate::tex::characters::{Character, CharacterMap};
    pub use crate::tex::nodes::{horizontal::HNode, math::MathNode, vertical::VNode, NodeTrait};
    pub use crate::tex::tokens::control_sequences::{
        CSHandler, CSName, InternedCSName, ResolvedCSName,
    };
    pub use crate::tex::tokens::token_lists::{CharWrite, TokenList};
    pub use crate::tex::tokens::Token;
    pub use crate::utils::errors::ErrorHandler;
}
