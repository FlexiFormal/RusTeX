use std::collections::HashMap;
use std::fs::File;
use std::io::BufRead;
use std::io::Write;
use std::path::Path;

static GLYPH_MAP: &[u8] = include_bytes!("src/resources/glyphs.map");
static PATCHES: &str = include_str!("src/resources/patches.md");
//static MISSING : &str = include_str!("src/resources/missing.txt");

struct State {
    glyphlist: String,
    glyphnames: String,
    glyphmap: phf_codegen::Map<String>,
    glyphlookup: phf_codegen::Map<String>,
    done_glyphs: Vec<String>,
    build_glyphlist: Vec<String>,
    idx: u16,
    build_map: HashMap<String, u16>,
    reader: std::io::Lines<std::io::BufReader<&'static [u8]>>,
}

impl State {
    fn go() -> Self {
        let mut st = Self {
            glyphlist: "[\"???\"".to_string(),
            glyphnames: "[\".notdef\"".to_string(),
            glyphmap: phf_codegen::Map::new(),
            glyphlookup: phf_codegen::Map::new(),
            done_glyphs: Vec::new(),
            idx: 0u16,
            build_map: HashMap::new(),
            reader: std::io::BufReader::new(GLYPH_MAP).lines(),
            build_glyphlist: vec![".notdef".to_string()],
        };
        st.build_map.insert(".notdef".to_string(), 0);
        // known unknowns
        /*for m in MISSING.split(' ') {
            st.glyphmap.entry(m.to_string(),"0");
        }*/

        while let Some(Ok(line)) = st.reader.next() {
            st.do_line(&line);
        }

        st
    }

    fn do_line(&mut self, line: &str) {
        self.idx += 1;
        let split = line.find(' ').unwrap_or_else(|| unreachable!());
        let glyphname = line[..split].to_string();
        let mut glyphcode = line[split + 1..].to_string();
        if glyphcode.is_empty() {
            match self.reader.next() {
                Some(Ok(s)) if s.is_empty() => glyphcode = "\\n".to_string(),
                Some(Ok(s)) => {
                    self.do_name_code(glyphname, glyphcode);
                    return self.do_line(&s);
                }
                _ => unreachable!(),
            }
        }
        self.do_name_code(glyphname, glyphcode);
    }

    fn do_name_code(&mut self, glyphname: String, glyphcode: String) {
        use std::fmt::Write;
        let _ = write!(&mut self.glyphlist, ",{glyphcode:?}");
        let _ = write!(&mut self.glyphnames, ",{glyphname:?}");
        if !self.done_glyphs.contains(&glyphcode) {
            self.done_glyphs.push(glyphcode.clone());
            self.glyphlookup.entry(glyphcode, &format!("{}", self.idx));
        }
        self.build_map.insert(glyphname.clone(), self.idx);
        self.build_glyphlist.push(glyphname.clone());
        self.glyphmap.entry(glyphname, &format!("{}", self.idx));
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ModifierSeq {
    blackboard: bool,
    fraktur: bool,
    script: bool,
    bold: bool,
    capitals: bool,
    monospaced: bool,
    italic: bool,
    oblique: bool,
    sans_serif: bool,
}

#[derive(Clone, Debug)]
struct FontMap {
    modifiers: ModifierSeq,
    table: Option<String>,
    fontmap: Option<(String, String)>,
}
struct TableParser {
    s: &'static str,
    map: HashMap<String, FontMap>,
    tables: Vec<(String, [String; 256])>,
    glyphmap: HashMap<String, u16>,
    glyphlist: Vec<String>,
}
impl TableParser {
    fn get(glyphmap: HashMap<String, u16>, glyphlist: Vec<String>) -> Self {
        let mut patch = PATCHES.trim_start();
        patch = patch[patch
            .find("<!--- START -->")
            .unwrap_or_else(|| unreachable!())
            + 15..]
            .trim_start();
        patch = patch[patch.find('\n').unwrap_or_else(|| unreachable!())..].trim_start();
        patch = patch[patch.find('\n').unwrap_or_else(|| unreachable!())..].trim_start();
        assert!(patch.starts_with('|'), "???: {patch}");
        let mut ret = Self {
            s: patch,
            map: HashMap::default(),
            tables: Vec::new(),
            glyphmap,
            glyphlist,
        };
        loop {
            if !ret.read_line() {
                break;
            }
        }
        assert!(ret.s.starts_with("<!--- END -->"));

        ret.s = ret.s[ret
            .s
            .find("<!--- START -->")
            .unwrap_or_else(|| unreachable!())
            + 15..]
            .trim_start();
        loop {
            if !ret.read_table() {
                break;
            }
        }
        assert!(ret.s.starts_with("<!--- END -->"));
        ret
    }
    fn read_table(&mut self) -> bool {
        if !self.s.starts_with('-') {
            return false;
        }
        self.s = self.s[1..].trim_start();
        let idx = self.s.find('\n').unwrap_or_else(|| unreachable!());
        let (mut name, rest) = self.s.split_at(idx);
        if name.ends_with('\r') {
            name = &name[..name.len() - 1];
        }
        self.s = rest[1..].trim_start();
        assert!(self.s.starts_with('|'));
        self.s = self.s[self.s.find('\n').unwrap_or_else(|| unreachable!())..].trim_start();
        self.s = self.s[self.s.find('\n').unwrap_or_else(|| unreachable!())..].trim_start();
        assert!(self.s.starts_with('|'));
        let mut table = array_init::array_init::<_, String, 256>(|_| String::new());
        for i in 0usize..16 {
            self.skip_field();
            for j in 0usize..16 {
                table[i * 16 + j] = self.read_glyph();
            }
            self.s = self.s[1..].trim_start();
        }
        self.tables.push((name.to_string(), table));
        true
    }
    fn read_glyph(&mut self) -> String {
        assert!(self.s.starts_with('|'));
        self.s = self.s[1..].trim_start();

        if self.s.starts_with('|') {
            return "UNDEFINED".to_string();
        }
        if self.s.starts_with("\\u") {
            let u = u32::from_str_radix(&self.s[2..6], 16).unwrap_or_else(|_| panic!("1"));
            self.s = self.s[6..].trim_start();
            assert!(self.s.starts_with('|'));
            return format!(
                "Glyph(GlyphI::Unicode({:?}))",
                char::from_u32(u).unwrap_or_else(|| panic!("2"))
            );
        }
        if self.s.starts_with('/') {
            let idx = self.s.find('|').unwrap_or_else(|| panic!("3"));
            let (glyph, rest) = self.s.split_at(idx);
            let glyph = glyph[1..].trim();
            self.s = rest;
            let i = self
                .glyphmap
                .get(glyph)
                .unwrap_or_else(|| panic!("Unknown glyph: {glyph}\n{:?}", self.glyphmap));
            return format!("Glyph(GlyphI::S({i}))");
        }
        if self.s.starts_with('`') {
            self.s = &self.s[1..];
            let mut ret = String::new();
            loop {
                let n = self.s.chars().next().unwrap_or_else(|| panic!("5"));
                self.s = &self.s[n.len_utf8()..];
                if n == '`' {
                    self.s = self.s.trim_start();
                    assert!(self.s.starts_with('|'));
                    if ret.chars().count() == 1 {
                        return format!(
                            "Glyph(GlyphI::Unicode({:?}))",
                            ret.chars().next().unwrap_or_else(|| panic!("6"))
                        );
                    }
                    let i = self
                        .glyphlist
                        .iter()
                        .enumerate()
                        .find(|p| p.1 == &ret)
                        .unwrap_or_else(|| panic!("7"))
                        .0;
                    return format!("Glyph(GlyphI::S({i}))");
                }
                if n == '\\' {
                    let n = self.s.chars().next().unwrap_or_else(|| panic!("8"));
                    ret.push(n);
                    self.s = &self.s[n.len_utf8()..];
                } else {
                    ret.push(n);
                }
            }
        }
        panic!("Unexpected glyph in table: {}", self.s);
    }
    fn skip_field(&mut self) {
        assert!(self.s.starts_with('|'));
        self.s = &self.s[1..];
        let idx = self.s.find('|').unwrap_or_else(|| unreachable!());
        self.s = &self.s[idx..];
    }
    fn read_line(&mut self) -> bool {
        if !self.s.starts_with('|') {
            return false;
        }
        self.s = &self.s[1..];
        let idx = self.s.find('|').unwrap_or_else(|| unreachable!());
        let (names, rest) = self.s.split_at(idx);
        self.s = rest[1..].trim_start();
        let names = names
            .split(',')
            .map(|d| d.trim().to_string())
            .collect::<Vec<_>>();

        let mut modifiers = ModifierSeq::default();
        let idx = self.s.find('|').unwrap_or_else(|| unreachable!());
        let (mods, rest) = self.s.split_at(idx);
        self.s = rest[1..].trim_start();
        for b in mods.trim().as_bytes() {
            match b {
                b'B' => modifiers.blackboard = true,
                b'f' => modifiers.fraktur = true,
                b'S' => modifiers.script = true,
                b'b' => modifiers.bold = true,
                b'c' => modifiers.capitals = true,
                b'm' => modifiers.monospaced = true,
                b'i' => modifiers.italic = true,
                b'o' => modifiers.oblique = true,
                b's' => modifiers.sans_serif = true,
                _ => panic!("Unexpected modifier in patches.md: {}, {}", b, self.s),
            }
        }

        let idx = self.s.find('|').unwrap_or_else(|| unreachable!());
        let (table, rest) = self.s.split_at(idx);
        self.s = rest[1..].trim_start();
        let table = table.trim();
        let table = if table.is_empty() {
            None
        } else {
            Some(table.to_string())
        };

        let idx = self.s.find('|').unwrap_or_else(|| unreachable!());
        let (font, rest) = self.s.split_at(idx);
        self.s = rest[1..].trim_start();
        let font = font.trim();
        let fontmap = if font.is_empty() {
            None
        } else {
            let idx = font
                .rfind(char::is_whitespace)
                .unwrap_or_else(|| unreachable!());
            let (name, link) = font.split_at(idx);
            Some((name.trim().to_string(), link.trim().to_string()))
        };

        let map = FontMap {
            modifiers,
            table,
            fontmap,
        };
        for n in names {
            self.map.insert(n, map.clone());
        }

        true
    }
}

fn main() {
    println!("cargo::rerun-if-changed=src/resources/glyphs.map");
    println!("cargo::rerun-if-changed=src/resources/patches.md");
    let st = State::go();

    let path =
        Path::new(&std::env::var("OUT_DIR").unwrap_or_else(|_| unreachable!())).join("codegen.rs");
    let mut file = std::io::BufWriter::new(File::create(path).unwrap_or_else(|_| unreachable!()));
    writeln!(file, "use crate::glyphs::{{GlyphI,UNDEFINED}};").unwrap_or_else(|_| unreachable!());
    writeln!(
        &mut file,
        "static GLYPH_LIST: [&str;{}] = {}];",
        st.idx + 1,
        st.glyphlist
    )
    .expect("infallible");
    writeln!(
        &mut file,
        "static GLYPH_NAMES: [&str;{}] = {}];",
        st.idx + 1,
        st.glyphnames
    )
    .expect("infallible");
    writeln!(
        &mut file,
        "static GLYPH_MAP: phf::Map<&'static str, u16> = {};",
        st.glyphmap.build()
    )
    .expect("infallible");
    writeln!(
        &mut file,
        "static GLYPH_LOOKUP: phf::Map<&'static str, u16> = {};",
        st.glyphlookup.build()
    )
    .expect("infallible");

    let parser = TableParser::get(st.build_map, st.build_glyphlist);

    let path = Path::new(&std::env::var("OUT_DIR").unwrap_or_else(|_| unreachable!()))
        .join("codegen_patch.rs");
    let mut patchfile =
        std::io::BufWriter::new(File::create(path).unwrap_or_else(|_| unreachable!()));
    writeln!(&mut patchfile, "{{").unwrap_or_else(|_| unreachable!());
    for (name, val) in parser.map {
        writeln!(
            &mut patchfile,
            "patch(&mut map,\"{}\",{:?},{:?},{:?});",
            name,
            val.modifiers,
            val.table.map(|n| parser
                .tables
                .iter()
                .enumerate()
                .find(|p| p.1 .0 == n)
                .unwrap_or_else(|| unreachable!())
                .0),
            val.fontmap
        )
        .expect("infallible");
    }
    writeln!(&mut patchfile, "}}").unwrap_or_else(|_| unreachable!());

    let tbl = &parser
        .tables
        .iter()
        .find(|p| p.0 == "PostScript Standard Encoding")
        .unwrap_or_else(|| unreachable!())
        .1;
    writeln!(
        &mut file,
        "const STANDARD_ENCODING: GlyphList = GlyphList(["
    )
    .unwrap_or_else(|_| unreachable!());
    for t in tbl {
        writeln!(&mut file, "    {t},").unwrap_or_else(|_| unreachable!());
    }
    writeln!(&mut file, "]);").unwrap_or_else(|_| unreachable!());

    writeln!(
        &mut file,
        "const PATCHED_TABLES: [GlyphList;{}] = [",
        parser.tables.len()
    )
    .unwrap_or_else(|_| unreachable!());
    for (_, table) in parser.tables {
        writeln!(&mut file, "    GlyphList([").unwrap_or_else(|_| unreachable!());
        for t in table {
            writeln!(&mut file, "        {t},").unwrap_or_else(|_| unreachable!());
        }
        writeln!(&mut file, "    ]),").unwrap_or_else(|_| unreachable!());
    }
    writeln!(&mut file, "];").unwrap_or_else(|_| unreachable!());

    //panic!("Here: Maps: {:?}\n\nTables: {:?}",parser.map,parser.tables);
    //panic!("Here: {}", glyphmap.display());
}
