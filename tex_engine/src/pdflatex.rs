/*! Adds support for `pdflatex` nodes and primitive commands to the engine. Requires the `pdflatex` feature to be enabled.
*/
pub mod commands;
pub mod nodes;

use crate::engine::filesystem::{File, VirtualFile};
use crate::engine::fontsystem::{Font, TfmFont, TfmFontSystem};
use crate::engine::gullet::DefaultGullet;
use crate::engine::mouth::DefaultMouth;
use crate::engine::stomach::DefaultStomach;
use crate::engine::utils::outputs::LogOutputs;
use crate::engine::{DefaultEngine, EngineReferences, EngineTypes, TeXEngine, filesystem, state};
use crate::pdflatex::nodes::PDFNodeLike;
use crate::prelude::CSName;
use crate::tex;
use crate::tex::characters::Character;
use crate::tex::nodes::vertical::VNode;
use crate::tex::numerics::{Dim32, Mu, Numeric, TeXDimen, TeXInt};
use crate::tex::tokens::control_sequences::InternedCSName;
use crate::utils::errors::{ErrorThrower, TeXResult};
use md5::Digest;
use nodes::{MinimalPDFExtension, PDFExtension, PDFNode};

pub trait PDFTeXEngine: TeXEngine
where
    <Self::Types as EngineTypes>::Extension: PDFExtension<Self::Types>,
    <Self::Types as EngineTypes>::CustomNode: PDFNodeLike<Self::Types>,
    <Self::Types as EngineTypes>::File: FileWithMD5,
    <Self::Types as EngineTypes>::Font: FontWithLpRp,
{
    /// #### Errors
    /// on LaTeX errors
    fn do_file_pdf<
        F: FnMut(&mut EngineReferences<Self::Types>, VNode<Self::Types>) -> TeXResult<(), Self::Types>,
    >(
        &mut self,
        s: &str,
        f: F,
    ) -> TeXResult<(), Self::Types> {
        *self.get_engine_refs().aux.extension.elapsed() = std::time::Instant::now();
        self.do_file_default(s, f)
    }

    /// #### Errors
    /// on LaTeX errors
    fn initialize_pdflatex(&mut self) -> TeXResult<(), Self::Types> {
        self.initialize_etex_primitives();
        commands::register_pdftex_primitives(self);
        self.init_file("pdftexconfig.tex")?;
        self.load_latex()
    }
}

pub trait FileWithMD5: File {
    fn md5(&self) -> [u8; 16];
}
impl<C: Character> FileWithMD5 for VirtualFile<C> {
    fn md5(&self) -> [u8; 16] {
        let mut hasher = md5::Md5::default();
        if let Some(s) = self.source.as_ref() {
            for r in s.iter() {
                for c in r {
                    let c = c.to_char();
                    let mut dst = [0u8; 4];
                    c.encode_utf8(&mut dst);
                    let bts = &dst[..c.len_utf8()];
                    hasher.update(bts);
                }
            }
        } else if let Ok(s) = std::fs::read(&self.path) {
            hasher.update(&s);
        }
        hasher.finalize().into()
    }
}

pub trait FontWithLpRp: Font {
    fn get_lp(&self, c: Self::Char) -> Self::Int;
    fn set_lp(&mut self, c: Self::Char, d: Self::Int);
    fn get_rp(&self, c: Self::Char) -> Self::Int;
    fn set_rp(&mut self, c: Self::Char, d: Self::Int);
}

impl<I: TeXInt, D: TeXDimen + Numeric<I>, CS: CSName<u8>> FontWithLpRp for TfmFont<I, D, CS> {
    fn get_lp(&self, c: Self::Char) -> I {
        let v = &mut self.muts.write().expect("Error mutating font data").lps;
        v.get(&c).copied().unwrap_or_default()
    }
    fn set_lp(&mut self, c: Self::Char, d: I) {
        let v = &mut self.muts.write().expect("Error mutating font data").lps;
        v.insert(c, d);
    }

    fn get_rp(&self, c: Self::Char) -> I {
        let v = &mut self.muts.write().expect("Error mutating font data").rps;
        v.get(&c).copied().unwrap_or_default()
    }
    fn set_rp(&mut self, c: Self::Char, d: I) {
        let v = &mut self.muts.write().expect("Error mutating font data").rps;
        v.insert(c, d);
    }
}

/// Example implementation of [`EngineTypes`] for a plain TeX engine.
#[derive(Copy, Clone, Debug)]
pub struct DefaultPDFTeXEngineTypes;
impl EngineTypes for DefaultPDFTeXEngineTypes {
    type Char = u8;
    type CSName = InternedCSName<u8>; //InternedString;
    type Token = super::tex::tokens::CompactToken; //::StandardToken<u8,Self::CSName>;//
    type Extension = MinimalPDFExtension<Self>;
    type Int = i32;
    type Dim = Dim32;
    type MuDim = Mu;
    type Num = tex::numerics::DefaultNumSet;
    type State = state::tex_state::DefaultState<Self>;
    type File = VirtualFile<u8>;
    type FileSystem = filesystem::NoOutputFileSystem<u8>;
    type Outputs = LogOutputs;
    type Mouth = DefaultMouth<Self>;
    type Gullet = DefaultGullet<Self>;
    type CustomNode = PDFNode<Self>;
    type Stomach = DefaultStomach<Self>;
    type ErrorHandler = ErrorThrower<Self>;
    type Font = TfmFont<i32, Dim32, InternedCSName<u8>>;
    type FontSystem = TfmFontSystem<i32, Dim32, InternedCSName<u8>>; //InternedString>;
}

pub type PlainPDFTeXEngine = DefaultEngine<DefaultPDFTeXEngineTypes>;

//impl PDFTeXEngine for PlainPDFTeXEngine {}
impl<A> PDFTeXEngine for A
where
    A: TeXEngine,
    <A::Types as EngineTypes>::Extension: PDFExtension<A::Types>,
    <A::Types as EngineTypes>::CustomNode: PDFNodeLike<A::Types>,
    <A::Types as EngineTypes>::File: FileWithMD5,
    <A::Types as EngineTypes>::Font: FontWithLpRp,
{
}
