/*! Accessing files on `\input`, `\open`/`\write` etc. */

use crate::engine::filesystem::kpathsea::Kpathsea;
use crate::engine::mouth::strings::InputTokenizer;
use crate::engine::state::State;
use crate::engine::utils::outputs::Outputs;
use crate::engine::{EngineAux, EngineTypes};
use crate::tex::characters::{Character, StringLineSource, TextLine, TextLineSource};
use crate::tex::tokens::control_sequences::CSName;
use crate::utils::errors::{TeXError, TeXResult};
use crate::utils::{HMap, Ptr};
use std::path::{Path, PathBuf};

pub mod kpathsea;

/// A [`FileSystem`] provides access to files.
pub trait FileSystem: Clone {
    /// The type of files provided by this [`FileSystem`].
    type File: File;
    /// Creates a new [`FileSystem`] with the given working directory.
    fn new(pwd: PathBuf) -> Self;
    /// Returns the file with the given name in the file database.
    /// May return nonexistent files in the CWD
    fn get<S: AsRef<str>>(&mut self, path: S) -> Self::File;
    /// Sets the working directory of this [`FileSystem`], returning the old working directory
    /// and updating the file database.
    fn set_pwd(&mut self, pwd: PathBuf) -> PathBuf;

    /// Opens the file with the given index for writing (`\openout`).
    fn open_out(&mut self, idx: u8, file: Self::File);
    /// Opens the file with the given index for reading (`\openin`).
    fn open_in(&mut self, idx: u8, file: Self::File);
    /// Closes the file with the given index (`\closein`).
    fn close_in(&mut self, idx: u8);
    /// Closes the file with the given index (`\closeout`).
    fn close_out(&mut self, idx: u8);
    /// Ehether the file with the given index is at its end (`\ifeof`).
    fn eof(&self, idx: u8) -> bool;
    /// Writes the given string to the file with the given index (`\write`).
    fn write<ET: EngineTypes, D: std::fmt::Display>(
        &mut self,
        idx: i64,
        string: D,
        newlinechar: Option<ET::Char>,
        aux: &mut EngineAux<ET>,
    );
    /// Reads a line from the file with the given index and current [`CategoryCodeScheme`](crate::tex::catcodes::CategoryCodeScheme) (`\read`),
    /// respecting groups (i.e. will continue reading at the end of a line until all open groups are closed).
    fn read<ET: EngineTypes<Char = <Self::File as File>::Char>, F: FnMut(ET::Token)>(
        &mut self,
        idx: u8,
        handler: &mut <ET::CSName as CSName<ET::Char>>::Handler,
        state: &ET::State,
        cont: F,
    ) -> TeXResult<(), ET>;
    /// Reads a line from the file with the given index using [`CategoryCode::Other`](crate::tex::catcodes::CategoryCode::Other)
    /// expect for space characters (`\readline`).

    fn readline<ET: EngineTypes<Char = <Self::File as File>::Char>, F: FnMut(ET::Token)>(
        &mut self,
        idx: u8,
        state: &ET::State,
        cont: F,
    ) -> TeXResult<(), ET>;

    /// Returns a human-readable representation of a [`SourceRefID`](File::SourceRefID); e.g. the file name/path.
    fn ref_str(&self, id: <Self::File as File>::SourceRefID) -> &str;
}

/// A (virtual or physical) file.
pub trait File: std::fmt::Display + Clone + std::fmt::Debug + 'static {
    /// The type of characters to be read from the file.
    type Char: Character;
    /// A `Copy`able identifier for this file to be used in [`SourceReference`]s.
    type SourceRefID: Copy + std::fmt::Debug + Default;
    /// The type of line sources to be read from the file.
    type LineSource: FileLineSource<Self::Char>;
    /// Returns the path of this file.
    fn path(&self) -> &Path;
    /// Returns a line source for this file. Used by a [`Mouth`](crate::engine::mouth::Mouth) to read from this file.
    fn line_source(self) -> Result<Self::LineSource, PathBuf>;
    /// Returns whether this file exists.
    fn exists(&self) -> bool {
        self.path().exists()
    }
    /// Returns the size of this file in bytes.
    fn size(&self) -> usize;
    /// Returns a [`SourceRefID`](File::SourceRefID) for this file.
    fn sourceref(&self) -> Self::SourceRefID;
}

/// An abstraction over a [`TextLineSource`] that has a `Path` - i.e. represents the contents of a file.
pub trait FileLineSource<C: Character>: TextLineSource<C> {
    fn path(&self) -> &Path;
}

/// A [`FileSystem`] that does not write to the local physical file system.
/// If a file is modified, its contents are kept in memory.
///
pub struct NoOutputFileSystem<C: Character> {
    pub kpse: Kpathsea,
    files: HMap<PathBuf, VirtualFile<C>>,
    pub envs: HMap<String, String>,
    write_files: Vec<Option<WritableVirtualFile<C>>>,
    read_files: Vec<Option<InputTokenizer<C, VirtualFileLineSource<C>>>>,
    pub interner: string_interner::StringInterner<
        string_interner::backend::StringBackend<string_interner::symbol::SymbolU32>,
        rustc_hash::FxBuildHasher,
    >,
}
impl<C: Character> Clone for NoOutputFileSystem<C> {
    fn clone(&self) -> Self {
        Self {
            kpse: self.kpse.clone(),
            files: self.files.clone(),
            write_files: self.write_files.clone(),
            envs: self.envs.clone(),
            read_files: Vec::new(),
            interner: self.interner.clone(),
        }
    }
}
impl<C: Character> NoOutputFileSystem<C> {
    pub fn add_file(&mut self, path: PathBuf, file_content: &str) {
        let string = if path.starts_with(&self.kpse.pwd) {
            format!("./{}", path.strip_prefix(&self.kpse.pwd).unwrap().display())
        } else {
            path.display().to_string()
        };
        let source = StringLineSource::make_lines(file_content.bytes());
        let f = VirtualFile {
            path,
            source: Some(source.into()),
            pipe: false,
            exists: true,
            id: Some(self.interner.get_or_intern(string)),
        };
        self.files.insert(f.path.clone(), f.clone());
    }
}
impl<C: Character> FileSystem for NoOutputFileSystem<C> {
    type File = VirtualFile<C>;

    fn new(pwd: PathBuf) -> Self {
        let mut envs = HMap::default();
        envs.insert("PWD".to_string(), pwd.display().to_string());
        envs.insert("CD".to_string(), pwd.display().to_string());
        Self {
            envs,
            kpse: Kpathsea::new(pwd),
            files: HMap::default(),
            write_files: Vec::new(),
            read_files: Vec::new(),
            interner: string_interner::StringInterner::new(),
        }
    }
    fn ref_str(&self, id: <Self::File as File>::SourceRefID) -> &str {
        match id {
            Some(id) => self.interner.resolve(id).unwrap(),
            None => "(NONE)",
        }
    }
    fn get<S: AsRef<str>>(&mut self, path: S) -> Self::File {
        let path = path.as_ref();
        if path.is_empty() {
            return VirtualFile {
                path: self.kpse.pwd.clone(),
                source: None,
                pipe: false,
                exists: false,
                id: Some(self.interner.get_or_intern("")),
            };
        }
        let kpath = self.kpse.kpsewhich(path);
        match self.files.get(&kpath.path) {
            Some(f) => f.clone(),
            None => {
                if path.starts_with("|kpsewhich ") {
                    let s = &path[1..];
                    let out = if cfg!(target_os = "windows") {
                        std::process::Command::new("cmd")
                            .current_dir(&self.kpse.pwd)
                            .envs(self.envs.iter())
                            .args(["/C", s]) //args.collect::<Vec<&str>>())
                            .output()
                            .expect("kpsewhich not found!")
                            .stdout
                    } else {
                        let args = s[10..].split(' ');
                        std::process::Command::new("kpsewhich")
                            .current_dir(&self.kpse.pwd)
                            .envs(self.envs.iter())
                            .args(args.collect::<Vec<&str>>())
                            .output()
                            .expect("kpsewhich not found!")
                            .stdout
                    };
                    let source = Some(StringLineSource::make_lines(out.into_iter()).into());
                    let f = VirtualFile {
                        path: kpath.path,
                        source,
                        pipe: true,
                        exists: kpath.exists,
                        id: Some(self.interner.get_or_intern(path)),
                    };
                    self.files.insert(f.path.clone(), f.clone());
                    return f;
                }
                let string = if kpath.path.starts_with(&self.kpse.pwd) {
                    format!(
                        "./{}",
                        kpath.path.strip_prefix(&self.kpse.pwd).unwrap().display()
                    )
                } else {
                    kpath.path.display().to_string()
                };
                let f = VirtualFile {
                    path: kpath.path,
                    source: None,
                    pipe: false,
                    exists: kpath.exists,
                    id: Some(self.interner.get_or_intern(string)),
                };
                self.files.insert(f.path.clone(), f.clone());
                f
            }
        }
    }
    fn set_pwd(&mut self, pwd: PathBuf) -> PathBuf {
        self.envs
            .insert("PWD".to_string(), pwd.display().to_string());
        self.envs
            .insert("CD".to_string(), pwd.display().to_string());
        let old = std::mem::replace(&mut self.kpse, Kpathsea::new(pwd));
        old.pwd
    }
    fn open_in(&mut self, idx: u8, file: Self::File) {
        while self.read_files.len() <= idx as usize {
            self.read_files.push(None);
        }
        match self.read_files.get_mut(idx as usize) {
            Some(n) => *n = file.line_source().ok().map(InputTokenizer::new),
            _ => unreachable!(),
        }
    }
    fn read<ET: EngineTypes<Char = <Self::File as File>::Char>, F: FnMut(ET::Token)>(
        &mut self,
        idx: u8,
        handler: &mut <ET::CSName as CSName<ET::Char>>::Handler,
        state: &ET::State,
        cont: F,
    ) -> TeXResult<(), ET> {
        match self.read_files.get_mut(idx as usize) {
            Some(Some(f)) => {
                let par = state.get_par_token();
                match f.read(
                    handler,
                    state.get_catcode_scheme(),
                    state.get_endline_char(),
                    &par,
                    cont,
                ) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e.into()),
                }
            }
            _ => Err(TeXError::EmergencyStop),
        }
    }
    fn readline<ET: EngineTypes<Char = <Self::File as File>::Char>, F: FnMut(ET::Token)>(
        &mut self,
        idx: u8,
        _state: &ET::State,
        cont: F,
    ) -> TeXResult<(), ET> {
        match self.read_files.get_mut(idx as usize) {
            Some(Some(f)) => {
                //debug_log!(debug => "readline: {}",f.source.path.display());
                f.readline(cont);
                Ok(())
            }
            _ => Err(TeXError::EmergencyStop),
        }
    }
    fn eof(&self, idx: u8) -> bool {
        match self.read_files.get(idx as usize) {
            Some(Some(f)) => f.eof(),
            _ => true,
        }
    }

    fn close_in(&mut self, idx: u8) {
        if let Some(f) = self.read_files.get_mut(idx as usize) {
            *f = None;
        }
    }
    fn open_out(&mut self, idx: u8, file: Self::File) {
        if idx as usize >= self.write_files.len() {
            self.write_files.resize((idx + 1) as usize, None);
        }
        self.write_files[idx as usize] = Some(WritableVirtualFile::new(file.path, file.id.unwrap()))
    }
    fn close_out(&mut self, idx: u8) {
        if let Some(o) = self.write_files.get_mut(idx as usize) {
            if let Some(f) = std::mem::take(o) {
                let vf = VirtualFile {
                    path: f.1,
                    exists: true,
                    source: Some(f.0.into()),
                    id: Some(f.2),
                    pipe: false,
                };
                self.files.insert(vf.path.clone(), vf);
            }
        }
    }
    fn write<ET: EngineTypes, D: std::fmt::Display>(
        &mut self,
        idx: i64,
        string: D,
        newlinechar: Option<ET::Char>,
        aux: &mut EngineAux<ET>,
    ) {
        if idx < 0 {
            aux.outputs.write_neg1(string)
        } else if idx == 16 {
            aux.outputs.write_16(string)
        } else if idx == 17 {
            aux.outputs.write_17(string)
        } else if idx == 18 {
            aux.outputs.write_18(string)
        } else {
            match self.write_files.get_mut(idx as usize) {
                Some(Some(f)) => {
                    let s = string.to_string().into_bytes();
                    if let Some(c) = newlinechar {
                        if let Ok(u) = c.try_into() {
                            for l in s.split(|b| *b == u) {
                                f.0.push(C::convert(l.to_vec()));
                            }
                            return;
                        }
                    }
                    let tl = C::convert(s);
                    f.0.push(tl);
                }
                _ => aux.outputs.write_other(string),
            }
        }
    }
}

/// A [`File`] that may live in memory or on the physical file system.
#[derive(Clone, Debug)]
pub struct VirtualFile<C: Character> {
    pub path: PathBuf,
    pub pipe: bool,
    pub id: Option<string_interner::symbol::SymbolU32>,
    pub source: Option<VirtualFileContents<C>>,
    pub exists: bool,
}
impl<C: Character> std::fmt::Display for VirtualFile<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.path.display())
    }
}

impl<C: Character> File for VirtualFile<C> {
    type Char = C;
    type LineSource = VirtualFileLineSource<C>;
    type SourceRefID = Option<string_interner::symbol::SymbolU32>;

    fn exists(&self) -> bool {
        self.exists
    }

    fn sourceref(&self) -> Self::SourceRefID {
        self.id
    }

    fn path(&self) -> &Path {
        &self.path
    }
    fn line_source(self) -> Result<Self::LineSource, PathBuf> {
        use std::io::BufRead;
        match self.source {
            Some(src) => Ok(VirtualFileLineSource {
                path: self.path,
                source: VirtualOrPhysicalFile::Virtual(src, 0),
            }),
            None => {
                let Some(f) = std::fs::File::open(&self.path).ok() else {
                    return Err(self.path);
                };
                let f = std::io::BufReader::new(f);
                let f = f.split(b'\n');
                Ok(VirtualFileLineSource {
                    path: self.path,
                    source: VirtualOrPhysicalFile::Physical(f),
                })
            }
        }
    }
    fn size(&self) -> usize {
        match &self.source {
            Some(src) => {
                let cnt = src.iter().map(|s| s.len()).sum::<usize>() + src.len();
                if cnt == 0 {
                    0
                } else {
                    cnt - 1
                }
            }
            None => match std::fs::metadata(&self.path) {
                Ok(md) => md.len() as usize,
                _ => 0,
            },
        }
    }
}

/// A [`TextLineSource`] that lives in memory.
#[derive(Debug)]
pub struct VirtualFileLineSource<C: Character> {
    path: PathBuf,
    source: VirtualOrPhysicalFile<C>,
}
impl<C: Character> FileLineSource<C> for VirtualFileLineSource<C> {
    fn path(&self) -> &Path {
        &self.path
    }
}
impl<C: Character> TextLineSource<C> for VirtualFileLineSource<C> {
    fn get_line(&mut self) -> Option<TextLine<C>> {
        match &mut self.source {
            VirtualOrPhysicalFile::Virtual(v, i) => {
                if *i >= v.len() {
                    None
                } else {
                    let ret = v[*i].clone();
                    *i += 1;
                    Some(ret)
                }
            }
            VirtualOrPhysicalFile::Physical(f) => match f.next() {
                Some(Ok(mut s)) => {
                    if let Some(b'\r') = s.last() {
                        s.pop();
                    }
                    while let Some(b' ') = s.last() {
                        s.pop();
                        if s.last() == Some(&b'\\') {
                            s.push(b' ');
                            break;
                        }
                    }
                    Some(C::convert(s))
                }
                _ => None,
            },
        }
    }
}

/// A [`SourceReference`] is a reference to a location in a file.
#[derive(Debug, Copy, Clone)]
pub struct SourceReference<FileId: Copy + Default> {
    /// The file this [`SourceReference`] refers to.
    pub file: FileId,
    /// The line number of this [`SourceReference`].
    pub line: usize,
    /// The column number of this [`SourceReference`].
    pub column: usize,
}
impl<FileID: Copy + Default> SourceReference<FileID> {
    /// Yields a [`Display`](std::fmt::Display)able version of this [`SourceReference`].
    pub fn display<'a, F: File<SourceRefID = FileID>, FS: FileSystem<File = F>>(
        &'a self,
        fs: &'a FS,
    ) -> impl std::fmt::Display + 'a {
        DisplaySourceReference { rf: self, fs }
    }
}
impl<FileID: Copy + Default> Default for SourceReference<FileID> {
    fn default() -> Self {
        Self {
            file: Default::default(),
            line: 0,
            column: 0,
        }
    }
}
pub type SourceRef<ET> = SourceReference<<<ET as EngineTypes>::File as File>::SourceRefID>;

struct DisplaySourceReference<'a, FS: FileSystem> {
    rf: &'a SourceReference<<<FS as FileSystem>::File as File>::SourceRefID>,
    fs: &'a FS,
}
impl<'a, FS: FileSystem> std::fmt::Display for DisplaySourceReference<'a, FS> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} l. {} c. {}",
            self.fs.ref_str(self.rf.file),
            self.rf.line,
            self.rf.column
        )
    }
}

#[derive(Clone)]
struct WritableVirtualFile<C: Character>(
    Vec<Box<[C]>>,
    PathBuf,
    string_interner::symbol::SymbolU32,
);
impl<C: Character> WritableVirtualFile<C> {
    fn new(p: PathBuf, id: string_interner::symbol::SymbolU32) -> Self {
        Self(Vec::new(), p, id)
    }
}

type VirtualFileContents<C> = Ptr<[TextLine<C>]>;

#[derive(Debug)]
enum VirtualOrPhysicalFile<C: Character> {
    Virtual(VirtualFileContents<C>, usize),
    Physical(std::io::Split<std::io::BufReader<std::fs::File>>),
}
