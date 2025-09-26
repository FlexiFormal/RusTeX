// #![forbid(unsafe_code)]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
use rustex_lib::engine::{RusTeXEngine, RusTeXEngineT, Settings};

use clap::Parser;
use rustex_lib::engine::files::RusTeXFileSystem;
use rustex_lib::engine::output::RusTeXOutput;
use rustex_lib::engine::Types;
use std::path::{Path, PathBuf};
use tex_engine::engine::{DefaultEngine, TeXEngine};
use tex_engine::pdflatex::commands::register_pdftex_primitives;

/*
notes   5:31
profile 1:38 / 1:38     ==> 1:43
thesis  0:15
 */
fn main() {
    //profile()
    //thesis()
    run()
    //test()
    //temp_test()
    //notes()
    //test2()
    //test_all()
}

fn test_all() {
    use std::io::BufRead;
    let files = "/home/jazzpirate/textest.txt";
    let allfiles_reader = std::io::BufReader::new(std::fs::File::open(files).unwrap());
    let mut allfiles = Vec::new();
    for line in allfiles_reader.lines() {
        let line = line.unwrap();
        let (a, b) = line.split_once("|").unwrap();
        if b.trim() == "true" {
            allfiles.push(PathBuf::from(a));
        }
    }

    let files = "/home/jazzpirate/textest_dones.txt";
    let mut dones = Vec::new();
    let donefiles_reader = std::io::BufReader::new(std::fs::File::open(files).unwrap());
    for line in donefiles_reader.lines() {
        let line = line.unwrap();
        dones.push(PathBuf::from(line));
    }
    let len = allfiles.len();
    let allfiles: Vec<_> = allfiles
        .into_iter()
        .enumerate()
        .filter(|(_, x)| !dones.contains(x))
        .collect();

    println!("Testing {} files", allfiles.len());
    //let mut missing_glyphs = HashSet::new();
    //let mut missing_fonts = HashSet::new();
    for (i, f) in allfiles.into_iter() {
        println!("Testing file {} of {}: {}", i + 1, len, f.display());
        let mut ret = RusTeXEngine::do_file(f.to_str().unwrap(), Settings::default());
        /*
        let mut glyphs = ret.missing_glyphs.clone().into_vec();
        glyphs.retain(|g| !missing_glyphs.contains(g));
        let mut fonts = ret.missing_fonts.clone().into_vec();
        fonts.retain(|g| !missing_fonts.contains(g));
        if !fonts.is_empty() {
            println!(
                "Missing fonts: {}",
                fonts
                    .iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for f in fonts {
            missing_fonts.insert(f);
        }
        glyphs.sort_by(|a, b| a.2.cmp(&b.2));
        if !glyphs.is_empty() {
            println!(
                "Missing glyphs: {}",
                glyphs
                    .iter()
                    .map(|(x, c, y)| format!("({},{},{})", x, c, y))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        for g in glyphs {
            missing_glyphs.insert(g);
        }

         */
        match ret.error.take() {
            None => {
                let out =
                    PathBuf::from("/home/jazzpirate/temp/out").join(format!("{}.html", i + 1));
                ret.write_out(&out).unwrap();
                dones.push(f);
                std::fs::write(
                    files,
                    dones
                        .iter()
                        .map(|x| x.to_str().unwrap())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
                .unwrap();
            }
            Some((e, _)) => {
                println!("Errored");
                panic!(
                    "Errored: {}\n{}\n\n", //Missing glyphs: {}\nMissing web fonts: {}",
                    f.display(),
                    e /*,
                      missing_glyphs
                          .into_iter()
                          .map(|(x, c, y)| format!("({},{},{})", x, c, y))
                          .collect::<Vec<_>>()
                          .join(", "),
                      missing_fonts
                          .into_iter()
                          .map(|x| x.to_string())
                          .collect::<Vec<_>>()
                          .join(", ")
                          */
                );
            }
        }
    }
    //let mut missing_glyphs: Vec<_> = missing_glyphs.into_iter().collect();
    //missing_glyphs.sort_by(|a, b| a.2.cmp(&b.2));
    println!(
        "Finished \\o/\n\n", //Missing glyphs: {}\nMissing web fonts: {}",
                             /*missing_glyphs
                                 .into_iter()
                                 .map(|(x, c, y)| format!("({},{},{})", x, c, y))
                                 .collect::<Vec<_>>()
                                 .join(", "),
                             missing_fonts
                                 .into_iter()
                                 .map(|x| x.to_string())
                                 .collect::<Vec<_>>()
                                 .join(", ")*/
    );
}

fn test() {
    //env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    let ret = RusTeXEngine::do_file(
        "/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/test.tex",
        Settings {
            verbose: false,
            log: true,
            sourcerefs: true,
            image_options: Default::default(),
            insert_font_info: true,
        },
    );
    ret.write_out(Path::new(
        "/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/test.html",
    ))
    .unwrap();
}

fn temp_test() {
    //env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    //let ret = RusTeXEngine::do_file("/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/numtest.tex",false,true,true);
    let ret = RusTeXEngine::do_file(
        //"/home/jazzpirate/work/MathHub/courses/UMR/GdMA/course/source/course/sec/Vorwort.de.tex",
        "/home/jazzpirate/work/MathHub/Papers/25-CICM-MathMap/source/paper.tex",
        //"/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/tmptest.tex",
        Settings {
            verbose: true,
            log: true,
            sourcerefs: true,
            image_options: Default::default(),
            insert_font_info: true,
        },
    );
    //let ret = RusTeXEngine::do_file("/home/jazzpirate/work/LaTeX/Papers/17 - Alignment Translation/macros/kwarc/workplan/workplan-template.tex",true,true,true);
    //std::fs::write("/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/numtest.html", &ret.out).unwrap();
    ret.write_out(Path::new(
        //"/home/jazzpirate/rustex.out.html"
        "/home/jazzpirate/work/MathHub/Papers/25-CICM-MathMap/source/paper.html",
        //"/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/tmptest.html", //"/home/jazzpirate/work/MathHub/courses/UMR/GdMA/course/source/course/sec/Vorwort.de.tex.html"
    ))
    .unwrap();
}

fn thesis() {
    //env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    let ret = RusTeXEngine::do_file(
        "/home/jazzpirate/work/LaTeX/Papers/19 - Thesis/thesis.tex",
        Settings {
            verbose: false,
            log: true,
            sourcerefs: true,
            image_options: Default::default(),
            insert_font_info: false,
        },
    );
    ret.write_out(Path::new(
        "/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/thesis.html",
    ))
    .unwrap();
}

fn notes() {
    //env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    // /home/jazzpirate/work/MathHub/MiKoMH/AI/source/search/slides/ts-ex.en.tex
    let ret = RusTeXEngine::do_file(
        "/home/jazzpirate/work/MathHub/MiKoMH/AI/source/course/notes/notes.tex",
        Settings {
            verbose: false,
            log: true,
            sourcerefs: true,
            image_options: Default::default(),
            insert_font_info: false,
        },
    );
    //let ret = RusTeXEngine::do_file("/home/jazzpirate/work/MathHub/MiKoMH/CompLog/source/kr/tikz/axioms2.tex",true,true,true);
    ret.write_out(Path::new(
        "/home/jazzpirate/work/Software/sTeX/RusTeXNew/test/ainotes.html",
    ))
    .unwrap();
}

fn profile() {
    use tex_engine::engine::filesystem::FileSystem;
    println!("Profiling...");
    //env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    let mut engine = DefaultEngine::<Types>::default();
    rustex_lib::engine::commands::register_primitives_preinit(&mut engine);
    engine.aux.outputs = RusTeXOutput::Print(false);
    engine.initialize_etex_primitives();
    register_pdftex_primitives(&mut engine);
    engine.init_file("pdftexconfig.tex").unwrap();
    let state = engine.state.clone();
    for _ in 0..30 {
        let _ = engine.load_latex();
        engine.state = state.clone();
        engine.filesystem = RusTeXFileSystem::new(tex_engine::utils::PWD.to_path_buf());
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Parameters {
    /// Input file (tex)
    #[clap(short, long)]
    input: Option<String>,

    /// Input dir (tex)
    #[clap(short, long)]
    dir: Option<String>,

    /// Input string (tex)
    #[clap(short, long)]
    text: Option<String>,

    /// Output file (html)
    #[clap(short, long)]
    output: Option<String>,
    /*
        /// log file (html)
        #[clap(short, long)]
        log: Option<String>,
    */
    /// insert source references
    #[clap(short, long, default_value_t = true)]
    sourcerefs: bool,

    /// insert glyph attributes (for debugging)
    #[clap(short, long, default_value_t = true)]
    glyph_debug: bool,

    /// console log
    #[clap(short, long, default_value_t = true)]
    console: bool,

    /// verbose
    #[clap(short, long, default_value_t = false)]
    verbose: bool,

    /// profile
    #[clap(short, long, default_value_t = false)]
    profile: bool,

    #[command(subcommand)]
    sub: Option<SubCmd>,
}

#[derive(clap::Subcommand, Debug)]
enum SubCmd {
    /// kpsewhich
    Kpse {
        /// log kpathsea database debug info
        #[clap(long, default_value_t = false)]
        log: bool,
        /// the path to search for in the database
        #[arg(required = true)]
        path: String,
    },
    /// font info
    Font {
        /// name of the font
        #[arg(required = true)]
        name: String,
    },
    /// glyph info
    Glyph {
        #[arg(short, long)]
        font: Option<String>,
        #[arg(short, long)]
        index: Option<u8>,
        #[arg(short, long)]
        glyph_name: Option<String>,
    },
}

fn run() {
    //env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    let params = Parameters::parse();
    if params.profile {
        profile();
        return;
    }
    if let Some(k) = params.sub {
        match k {
            SubCmd::Kpse { log, path } => return kpse(log, path),
            SubCmd::Font { name } => return do_font(name),
            SubCmd::Glyph {
                font,
                index,
                glyph_name,
            } => return do_glyph(font, index, glyph_name),
        }
    }
    match (params.input, params.output) {
        (Some(i), Some(o)) => {
            let ret = RusTeXEngine::do_file(
                i.as_str(),
                Settings {
                    verbose: params.verbose,
                    log: params.console,
                    sourcerefs: params.sourcerefs,
                    image_options: Default::default(),
                    insert_font_info: params.glyph_debug,
                },
            );
            ret.write_out(Path::new(&o)).unwrap();
        }
        _ => {
            println!("No input/output file given. Testing latex.ltx...");
            test_latex_ltx();
            println!("Done");
        }
    }
}

fn test_latex_ltx() {
    let mut engine = DefaultEngine::<Types>::default();
    rustex_lib::engine::commands::register_primitives_preinit(&mut engine);

    //engine.state.set_primitive_int(&mut engine.aux,PRIMITIVES.tracingcommands,1,true);
    //engine.state.set_primitive_int(&mut engine.aux,PRIMITIVES.tracingifs,1,true);

    engine.aux.outputs = RusTeXOutput::Print(true);
    engine.initialize_etex_primitives();
    register_pdftex_primitives(&mut engine);
    engine.init_file("pdftexconfig.tex").unwrap();
    let _ = engine.load_latex();
}

fn kpse(log: bool, path: String) {
    if log {
        tex_engine::engine::filesystem::kpathsea::LOG_KPATHSEA
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let kpse = tex_engine::engine::filesystem::kpathsea::Kpathsea::new(
        std::env::current_dir().expect("Could not find current dir"),
    );
    if log && path == "ALL" {
        for (k, v) in &kpse.global.pre {
            println!("{k}:   {}", v.display());
        }
        for (k, v) in &kpse.local {
            println!("{k}:   {}", v.display());
        }
        for (k, v) in &kpse.global.post {
            println!("{k}:   {}", v.display());
        }
    } else {
        let r = kpse.kpsewhich(path);
        if r.exists {
            println!("{}", r.path.display());
        }
    }
}

fn do_font(s: String) {
    let mut store =
        tex_glyphs::FontInfoStore::new(
            |s| match tex_engine::engine::filesystem::kpathsea::KPATHSEA.which(s) {
                Some(p) => p.display().to_string(),
                None => s.to_string(),
            },
        );
    let Some(d) = store.display_encoding(&s) else {
        println!("Font (encoding) not found!");
        return;
    };
    println!("{d}");
}

fn do_glyph(font: Option<String>, index: Option<u8>, glyph_name: Option<String>) {
    let glyph = match (font, index, glyph_name) {
        (Some(f), Some(i), None) => {
            let mut store = tex_glyphs::FontInfoStore::new(|s| {
                match tex_engine::engine::filesystem::kpathsea::KPATHSEA.which(s) {
                    Some(p) => p.display().to_string(),
                    None => s.to_string(),
                }
            });
            let ls = store.get_glyphlist(&f);
            if !ls.is_defined() {
                println!("Font {f} not found!");
                return;
            }
            ls.get(i)
        }
        (None, None, Some(n)) => tex_glyphs::Glyph::get(n),
        _ => {
            println!(
                "Invalid arguments: either --font and --index, or --glyph_name need to be provided"
            );
            return;
        }
    };
    if !glyph.is_defined() {
        println!("Glyph undefined");
        return;
    }
    if let Some(comb) = glyph.as_combinator() {
        println!("Diacritic {}", comb.apply_char(' '))
    } else {
        println!("Glyph {}: \"{}\"", glyph.name(), glyph);
    }
}
