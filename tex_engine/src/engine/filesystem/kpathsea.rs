/*! An implementation of (the central part of) kpathsea, the path searching library used by TeX.
Used by instantiating a [Kpathsea] instance with the working directory.


**Example:**
```rust
use tex_engine::engine::filesystem::kpathsea::Kpathsea;
let kpse = Kpathsea::new(std::env::current_dir().unwrap());
assert!(kpse.kpsewhich("latex.ltx").path.to_str().unwrap().replace('\\' ,"/").ends_with("tex/latex/base/latex.ltx"));
assert!(kpse.kpsewhich("article.cls").path.to_str().unwrap().replace('\\' ,"/").ends_with("tex/latex/base/article.cls"));
// as expected, the `.tex` file extension is optional:
assert!(kpse.kpsewhich("expl3-code").path.to_str().unwrap().replace('\\' ,"/").ends_with("tex/latex/l3kernel/expl3-code.tex"));
```
*/

use crate::utils::HMap;
use lazy_static::lazy_static;
use std::collections::hash_map::Entry;
use std::fmt::Debug;
use std::fs::File;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The result of a [`Kpathsea`] search.
/// TODO capitalization might be an issue. TeX is more permissive wrt case distinctions
/// than the filesystem, (apparently) iff either not covered by an ls-R file or
/// directly in a TEXINPUTS path...?
pub struct KpseResult {
    /// The path to the file.
    pub path: PathBuf,
    /// Whether the file exists.
    pub exists: bool,
}

lazy_static! {
    pub static ref KPATHSEA: Arc<KpathseaBase> = Arc::new(KpathseaBase::new());
}

/** A "database" of paths to search for files. Notably, the "global" part (e.g. the system-wide
`TEXINPUTS`, `TEXMF`, etc.) is shared between all instances of [`Kpathsea`].
and lazily computed on first use.
 **/
#[derive(Clone)]
pub struct Kpathsea {
    pub pwd: PathBuf,
    pub local: HMap<String, PathBuf>,
    pub global: Arc<KpathseaBase>,
}
impl Kpathsea {
    /// Create a new [`Kpathsea`] instance with the given working directory.
    pub fn new(pwd: PathBuf) -> Kpathsea {
        let global = KPATHSEA.clone();
        let local = get_dot(global.recdot, &pwd);
        Self { pwd, local, global }
    }

    /// Search for a file in the database.
    pub fn kpsewhich<S: AsRef<str>>(&self, filestr: S) -> KpseResult {
        use path_dedot::*;
        let filestr = filestr.as_ref();
        if filestr.starts_with("|kpsewhich") {
            return KpseResult {
                path: self.pwd.join(filestr),
                exists: true,
            };
        }
        if filestr.starts_with("nul:") && cfg!(target_os = "windows") {
            return KpseResult {
                path: PathBuf::from(filestr),
                exists: true,
            };
        } else if filestr.starts_with("nul:") {
            return KpseResult {
                path: self.pwd.join(filestr),
                exists: false,
            };
        } else if filestr.is_empty() {
            panic!("Empty string in kpsewhich")
        }
        if filestr.contains("./") {
            let p1 = self.pwd.join(Path::new(filestr));
            let p = p1.parse_dot().unwrap();
            if p.is_file() {
                return KpseResult {
                    exists: true,
                    path: p.to_path_buf(),
                };
            }
            let q = PathBuf::from(p.display().to_string() + ".tex");
            if q.is_file() {
                return KpseResult {
                    exists: true,
                    path: q,
                };
            }
            return KpseResult {
                exists: false,
                path: p.to_path_buf(),
            };
        }
        if Path::new(filestr).is_absolute() {
            if Path::new(filestr).is_file() {
                return KpseResult {
                    path: PathBuf::from(&filestr),
                    exists: true,
                };
            }
            let pb = PathBuf::from(filestr.to_string() + ".tex");
            if pb.is_file() {
                return KpseResult {
                    path: pb,
                    exists: true,
                };
            }
            return KpseResult {
                path: PathBuf::from(&filestr),
                exists: false,
            };
            //else {return KpseResult{path:PathBuf::from(format!("{}.tex",filestr)),exists:false} }
        }
        if let Some(p) = self.global.pre.get(filestr) {
            return KpseResult {
                path: p.clone(),
                exists: true,
            };
        }
        if let Some(p) = self.local.get(filestr) {
            return KpseResult {
                path: p.clone(),
                exists: true,
            };
        }
        if let Some(p) = self.global.post.get(filestr) {
            return KpseResult {
                path: p.clone(),
                exists: true,
            };
        }
        let p = self.pwd.join(Path::new(filestr));
        KpseResult {
            exists: p.exists(),
            path: p,
        }
    }
}

/// The "base" part of [`Kpathsea`] holding information about the "global" parts of the file database, which is
/// (or should be) shared between all instances. Never needs to be instantiated directly;
/// use the canoncial [`static@KPATHSEA`] instance instead.
#[derive(Clone, Debug)]
pub struct KpathseaBase {
    /// The paths to search before the working directory.
    pub pre: HMap<String, PathBuf>,
    /// Whether to search recursively in the working directory.
    pub recdot: bool,
    /// The paths to search after the working directory.
    pub post: HMap<String, PathBuf>,
}

pub static LOG_KPATHSEA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl KpathseaBase {
    fn new() -> KpathseaBase {
        let log = LOG_KPATHSEA.load(std::sync::atomic::Ordering::Relaxed);
        let locout = std::process::Command::new("kpsewhich")
            .args(vec!["-var-value", "SELFAUTOLOC"])
            .output()
            .expect("kpsewhich not found!")
            .stdout;
        let loc = std::str::from_utf8(locout.as_slice()).unwrap().trim();
        let selfautoloc = Path::new(loc);

        let (pre, dot, post) = if loc.contains("miktex") {
            todo!()
        } else {
            let mut vars = Self::get_vars(selfautoloc);
            let home = if cfg!(target_os = "windows") {
                std::env::vars().find(|x| x.0 == "HOMEDRIVE").unwrap().1
                    + &std::env::vars().find(|x| x.0 == "HOMEPATH").unwrap().1
            } else {
                std::env::vars().find(|x| x.0 == "HOME").unwrap().1
            };
            if log {
                println!("Variables:\n-------------------------");
                for (k, v) in &vars {
                    println!("{k}:   {v}");
                }
            }

            let paths = Self::paths_to_scan(&mut vars);
            if log {
                println!("-------------------------\nScan paths:\n-------------------------");
                for p in &paths {
                    println!("{}", p);
                }
            }
            let mut parser = PathParser {
                vars,
                diddot: false,
                recdot: false,
                predot: vec![],
                postdot: vec![],
                home: PathBuf::from(home),
                resolved_vars: HMap::default(),
            };

            for s in paths {
                parser.do_dir(&s);
            }

            if log {
                println!(
                    "-------------------------\nResolved variables:\n-------------------------"
                );
                for (k, v) in &parser.resolved_vars {
                    let val = v
                        .iter()
                        .map(|v| String::from_utf8_lossy(v))
                        .collect::<Vec<_>>()
                        .join("; ");
                    println!("{k}:   {val}");
                }
                println!("-------------------------\nResolved paths:\n-------------------------");
                for (p, b) in &parser.predot {
                    println!("{} ({b})", p.display());
                }
                for (p, b) in &parser.postdot {
                    println!("{} ({b})", p.display());
                }
                println!("-------------------------\n");
            }

            let r = if log {
                parser.close::<true>()
            } else {
                parser.close::<false>()
            };
            if log {
                println!("-------------------------\n");
            }
            r
        };
        KpathseaBase {
            pre,
            recdot: dot,
            post,
        }
    }

    /// Search for a file in the database; basically `kpsewhich <file>`, but without the working directory.
    pub fn which<S: AsRef<str>>(&self, filestr: S) -> Option<PathBuf> {
        let filestr = filestr.as_ref();
        if Path::new(filestr).is_absolute() {
            return Some(PathBuf::from(filestr));
        }
        if let Some(p) = self.pre.get(filestr) {
            return Some(p.clone());
        }
        if let Some(p) = self.post.get(filestr) {
            return Some(p.clone());
        }
        None
    }

    fn get_vars(selfautoloc: &Path) -> HMap<String, String> {
        let mut vars = HMap::<String, String>::default();
        vars.insert(
            "SELFAUTOLOC".to_string(),
            selfautoloc.to_str().unwrap().to_string(),
        );
        if let Some(autodir) = selfautoloc.parent() {
            vars.insert(
                "SELFAUTODIR".to_string(),
                autodir.to_str().unwrap().to_string(),
            );
            if let Some(autoparent) = autodir.parent() {
                vars.insert(
                    "SELFAUTOPARENT".to_string(),
                    autoparent.to_str().unwrap().to_string(),
                );
                if let Some(autograndparent) = autoparent.parent() {
                    vars.insert(
                        "SELFAUTOGRANDPARENT".to_string(),
                        autograndparent.to_str().unwrap().to_string(),
                    );
                } else {
                    vars.insert(
                        "SELFAUTOGRANDPARENT".to_string(),
                        autoparent.to_str().unwrap().to_string(),
                    );
                }
            } else {
                vars.insert(
                    "SELFAUTOPARENT".to_string(),
                    autodir.to_str().unwrap().to_string(),
                );
                vars.insert(
                    "SELFAUTOGRANDPARENT".to_string(),
                    autodir.to_str().unwrap().to_string(),
                );
            }
        } else {
            vars.insert(
                "SELFAUTODIR".to_string(),
                selfautoloc.to_str().unwrap().to_string(),
            );
            vars.insert(
                "SELFAUTOPARENT".to_string(),
                selfautoloc.to_str().unwrap().to_string(),
            );
            vars.insert(
                "SELFAUTOGRANDPARENT".to_string(),
                selfautoloc.to_str().unwrap().to_string(),
            );
        }

        let out = std::process::Command::new("kpsewhich")
            .args(vec!["-a", "texmf.cnf"])
            .output()
            .expect("kpsewhich not found!")
            .stdout;
        let outstr = std::str::from_utf8(out.as_slice()).unwrap();
        let rs = outstr
            .split('\n')
            .map(|x| x.trim())
            .filter(|s| !s.is_empty());
        for r in rs {
            let p = Path::new(r);
            if let Ok(f) = File::open(p) {
                let lines = std::io::BufReader::new(f).lines();
                for l in lines.map_while(Result::ok) {
                    if !l.starts_with('%') && !l.is_empty() {
                        let mut kb = l.split('=').map(|x| x.trim()).collect::<Vec<_>>();
                        if kb.len() == 2 {
                            let v = kb.pop().unwrap();
                            let mut k = kb.pop().unwrap();
                            if let Some(i) = k.find('.') {
                                let pre = &k[..i];
                                let post = &k[i + 1..];
                                if post != "pdftex" {
                                    continue;
                                }
                                k = pre;
                            }
                            if k.chars().any(|c| c.is_lowercase()) {
                                continue;
                            }
                            match vars.entry(k.to_string()) {
                                Entry::Occupied(_) => (),
                                Entry::Vacant(e) => {
                                    e.insert(v.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        vars.insert("progname".to_string(), "pdftex".to_string());
        vars
    }

    fn paths_to_scan(vars: &mut HMap<String, String>) -> Vec<String> {
        let mut todo = [
            NamedVar("TEXINPUTS", false),
            NamedVar("VARTEXFONTS", false),
            NamedVar("VFFONTS", false),
            NamedVar("TFMFONTS", false),
            NamedVar("T1FONTS", false),
            NamedVar("ENCFONTS", false),
        ];

        let mut ret = vec![];
        for (k, mut v) in std::env::vars() {
            if let Some(td) = todo.iter_mut().find(|x| x.0 == k) {
                td.1 = true;
                if v.ends_with(';') || v.ends_with(':') {
                    if let Some(oldv) = vars.get(td.0) {
                        v.push_str(oldv);
                    }
                }
                ret.push(v);
            } else {
                match vars.entry(k) {
                    Entry::Occupied(_) => (),
                    Entry::Vacant(e) => {
                        e.insert(v);
                    }
                }
            }
        }
        for td in todo.iter().filter(|x| !x.1) {
            if let Some(v) = vars.get(td.0) {
                ret.push(v.clone());
            }
        }
        ret
    }
}

struct NamedVar(&'static str, bool);
struct PathParser {
    vars: HMap<String, String>,
    diddot: bool,
    recdot: bool,
    predot: Vec<(PathBuf, bool)>,
    postdot: Vec<(PathBuf, bool)>,
    home: PathBuf,
    resolved_vars: HMap<String, Vec<Vec<u8>>>,
}
struct StringSet(Vec<Vec<u8>>);
impl StringSet {
    fn push_string(&mut self, s: &[u8]) {
        if s.is_empty() {
            return;
        }
        if self.0.is_empty() {
            self.0.push(s.to_vec())
        } else {
            for r in &mut self.0 {
                r.extend(s)
            }
        }
    }
    fn new() -> StringSet {
        StringSet(vec![])
    }
    fn split(&mut self, strs: &Vec<Vec<u8>>) {
        if strs.len() == 1 {
            self.push_string(&strs[0]);
            return;
        }
        if strs.is_empty() {
            return;
        }
        if self.0.is_empty() {
            self.0 = strs.clone();
            return;
        }
        let mut ret = vec![];
        for s in &self.0 {
            for r in strs {
                let mut s = s.clone();
                s.extend(r);
                ret.push(s)
            }
        }
        self.0 = ret;
    }
    fn clear(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.0)
    }
}
impl PathParser {
    fn do_dir(&mut self, s: &str) {
        for mut s in self
            .parse_string(s)
            .into_iter()
            .map(|v| String::from_utf8(v).unwrap())
        {
            let mut recurse = false;
            #[cfg(target_os = "windows")]
            {
                if s.ends_with("//") || s.ends_with("\\\\") {
                    recurse = true;
                    s.pop();
                    s.pop();
                } else if s.ends_with('/') || s.ends_with('\\') {
                    s.pop();
                }
            }
            #[cfg(not(target_os = "windows"))]
            {
                if s.ends_with("//") {
                    recurse = true;
                    s.pop();
                    s.pop();
                } else if s.ends_with('/') {
                    s.pop();
                }
            }
            if s == "." {
                if self.diddot {
                    continue;
                }
                self.diddot = true;
                self.recdot = recurse;
                continue;
            }
            self.push_path(Path::new(&s), recurse);
        }
    }
    fn push_path(&mut self, p: &Path, rec: bool) {
        let map = if self.diddot {
            &mut self.postdot
        } else {
            &mut self.predot
        };
        for (ip, m) in map.iter_mut() {
            if ip == p {
                *m = *m || rec;
                return;
            }
        }
        map.push((p.to_path_buf(), rec));
    }

    fn parse_string(&mut self, s: &str) -> Vec<Vec<u8>> {
        self.parse_bytes(s.as_bytes())
    }
    fn parse_bytes(&mut self, mut s: &[u8]) -> Vec<Vec<u8>> {
        let mut ret: Vec<Vec<u8>> = vec![];
        let mut currs = StringSet::new();
        let breaks = if cfg!(target_os = "windows") {
            [b';', b';', b'$', b'!', b'{', b'~']
        } else {
            [b';', b':', b'$', b'!', b'{', b'~']
        };
        while !s.is_empty() {
            if let Some((i, b)) = s.iter().enumerate().find(|(_, c)| breaks.contains(*c)) {
                let first = &s[..i];
                currs.push_string(first);
                s = &s[i + 1..];
                match b {
                    b'!' => (),
                    b'~' => currs.push_string(self.home.to_str().unwrap().as_bytes()),
                    b';' | b':' => ret.extend(currs.clear().into_iter()),
                    b'$' => {
                        let name = if let Some((i, _)) =
                            s.iter().enumerate().find(|(_, c)| !c.is_ascii_alphabetic())
                        {
                            let r = &s[..i];
                            s = &s[i..];
                            r
                        } else {
                            let r = s;
                            s = b"";
                            r
                        };
                        let name = std::str::from_utf8(name).unwrap();
                        let resolved = self.get_resolved_var(name);
                        currs.split(resolved);
                    }
                    b'{' => {
                        let mut inbracks = 0;
                        let mut ret = vec![vec![]];
                        while !s.is_empty() {
                            let b = s[0];
                            s = &s[1..];
                            if b == b'{' {
                                inbracks += 1
                            } else if b == b'}' && inbracks == 0 {
                                break;
                            } else if b == b'}' {
                                inbracks -= 1
                            } else if b == b',' && inbracks == 0 {
                                ret.push(vec![]);
                                continue;
                            }
                            ret.last_mut().unwrap().push(b)
                        }
                        let v = ret
                            .into_iter()
                            .flat_map(|v| self.parse_bytes(v.as_slice()))
                            .collect::<Vec<_>>();
                        currs.split(&v);
                    }
                    _ => unreachable!(),
                }
            } else {
                currs.push_string(s);
                break;
            }
        }
        ret.extend(currs.clear());
        ret
    }
    fn get_resolved_var(&mut self, key: &str) -> &Vec<Vec<u8>> {
        if !self.resolved_vars.contains_key(key) {
            let val = self.vars.get(key).unwrap().clone();
            let resolved = self.parse_string(&val);
            self.resolved_vars.insert(key.to_string(), resolved);
        }
        self.resolved_vars.get(key).unwrap()
    }
    fn close<const LOG: bool>(self) -> (HMap<String, PathBuf>, bool, HMap<String, PathBuf>) {
        (
            Self::close_i::<LOG>(self.predot),
            self.recdot,
            Self::close_i::<LOG>(self.postdot),
        )
    }
    fn close_i<const LOG: bool>(v: Vec<(PathBuf, bool)>) -> HMap<String, PathBuf> {
        let mut ret = HMap::default();
        for (p, rec) in v.into_iter().rev() {
            if LOG {
                println!("Checking {} ({rec})", p.display());
            }
            let len = p.to_str().unwrap().len() + 1;
            for e in walkdir::WalkDir::new(&p)
                .follow_links(true)
                .min_depth(1)
                .into_iter()
                .filter_map(|e| match e {
                    Err(e) if LOG => {
                        println!("ERROR: {e}");
                        None
                    }
                    Err(_) => None,
                    Ok(s)
                        if s.path()
                            .components()
                            .any(|c| c.as_os_str().to_str() == Some(".git")) =>
                    {
                        None
                    }
                    Ok(e) => Some(e),
                })
            {
                let sub = &e.path().to_str().unwrap()[len..];
                if sub.contains('.') {
                    if LOG {
                        println!("Adding {} ({rec})", e.path().display());
                    }
                    let sub = sub.to_string();
                    let pb = e.path().to_path_buf();
                    if sub.ends_with(".tex") {
                        let sub = sub[..sub.len() - 4].to_string();
                        ret.insert(sub, pb.clone());
                    }
                    if rec {
                        let filename = pb.file_name().unwrap().to_str().unwrap();
                        ret.insert(filename.to_string(), pb.clone());
                        if sub.ends_with(".tex") {
                            ret.insert(filename[..filename.len() - 4].to_string(), pb.clone());
                        }
                    }
                    ret.insert(sub, pb);
                }
            }
        }
        ret
    }
}

fn get_dot(recdot: bool, pwd: &Path) -> HMap<String, PathBuf> {
    let mut ret = HMap::default();
    let len = pwd.to_str().unwrap().len() + 1;
    for e in walkdir::WalkDir::new(pwd)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| match e.ok() {
            None => None,
            Some(s) if s.path().as_os_str().to_str().unwrap().contains(".git") => None,
            Some(e) => Some(e),
        })
    {
        let sub = &e.path().to_str().unwrap()[len..];
        let sub = sub.to_string();
        let pb = e.path().to_path_buf();
        if e.file_type().is_file() {
            let filename = pb.file_name().unwrap().to_str().unwrap();
            if sub.ends_with(".tex") {
                let sub = sub[..sub.len() - 4].to_string();
                ret.insert(sub, pb.clone());
            }
            if recdot {
                ret.insert(filename.to_string(), pb.clone());
                //ret.insert(filename.to_uppercase(), pb.clone());
                if sub.ends_with(".tex") {
                    ret.insert(filename[..filename.len() - 4].to_string(), pb.clone());
                    //ret.insert(filename.to_uppercase()[..filename.len() - 4].to_string(), pb.clone());
                }
            }
            //let sub2 = sub[..sub.len() - filename.len()].to_string() + filename.to_uppercase().as_str();
            //ret.insert(sub2, pb.clone());
            ret.insert(sub, pb);
        }
    }
    ret
}
