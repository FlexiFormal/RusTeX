#![allow(clippy::unwrap_used)]
#![allow(unused_must_use)]
#[doc(hidden)]
#[cfg(any(test, doctest))]
#[allow(dead_code)]
pub mod test_utils {
    #[macro_export]
    macro_rules! measure {
        ($key:ident:$x:expr) => {{
            let measure_start = std::time::Instant::now();
            let ret = $x;
            log::warn!(target:stringify!($key),"Finished after {:?}",measure_start.elapsed());
            ret
        }};
    }

    pub fn trace() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Trace)
            .try_init();
    }

    pub fn debug() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Debug)
            .try_init();
    }

    pub fn info() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .try_init();
    }

    pub fn warn() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Warn)
            .try_init();
    }

    pub fn error() {
        env_logger::builder()
            .filter_level(log::LevelFilter::Error)
            .try_init();
    }
}

use crate::measure;
use crate::prelude::*;
use crate::tests::test_utils::*;
use log::*;

#[test]
fn kpsewhich() {
    measure!(kpsewhich: {
        use crate::engine::filesystem::kpathsea::*;
        debug();
        let kpse = Kpathsea::new(std::env::current_dir().unwrap());
        info!(target:"kpsewhich", "latex.ltx: {:?}", kpse.kpsewhich("latex.ltx").path);
        info!(target:"kpsewhich", "article.cls: {:?}",kpse.kpsewhich("article.cls").path);
        info!(target:"kpsewhich", "expl3-code: {:?}",kpse.kpsewhich("expl3-code").path);
        info!(target:"kpsewhich", "-var-value MATHHUB: {:?}",kpse.kpsewhich("|kpsewhich -var-value MATHHUB").path);
    });
}

#[test]
fn tokenizer() {
    use crate::engine::mouth::strings::InputTokenizer;
    use crate::tex::catcodes::DEFAULT_SCHEME_U8;
    use crate::tex::characters::StringLineSource;
    use crate::tex::tokens::StandardToken;
    use crate::utils::Ptr;

    type T = StandardToken<u8, Ptr<str>>;
    debug();
    let mut cs_handler = ();
    let cc = &DEFAULT_SCHEME_U8;

    let string = "\\foo   \n  \n   {a}{!}";
    let input: StringLineSource<u8> = string.into();
    let mut tokenizer = InputTokenizer::new(input);
    let eol = Some(b'\r');
    let par = CSHandler::<u8, Ptr<str>>::par(&cs_handler);
    let next = tokenizer.get_next(&mut cs_handler, cc, None, &par); // \foo
    assert!(matches!(next,Ok(Some(T::ControlSequence(s))) if &*s == "foo"));
    let next = tokenizer.get_next(&mut cs_handler, cc, eol, &par); // \par
    assert!(matches!(next,Ok(Some(T::ControlSequence(s))) if &*s == "par"));
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // {
    assert_eq!(next.command_code(), CommandCode::BeginGroup);
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // a
    assert_eq!(next.command_code(), CommandCode::Letter);
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // }
    assert_eq!(next.command_code(), CommandCode::EndGroup);
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // {
    assert_eq!(next.command_code(), CommandCode::BeginGroup);
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // !
    assert_eq!(next.command_code(), CommandCode::Other);
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // }
    assert_eq!(next.command_code(), CommandCode::EndGroup);
    let next: T = tokenizer
        .get_next(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .unwrap(); // end of line => space
    assert_eq!(next.command_code(), CommandCode::Space);
    assert!(tokenizer
        .get_next::<T>(&mut cs_handler, cc, eol, &par)
        .unwrap()
        .is_none()); // EOF
}

#[test]
fn carlisle() {
    const CARLISLE: &str = r"\let~\catcode~`76~`A13~`F1~`j00~`P2jdefA71F~`7113jdefPALLF
    PA''FwPA;;FPAZZFLaLPA//71F71iPAHHFLPAzzFenPASSFthP;A$$FevP
    A@@FfPARR717273F737271P;ADDFRgniPAWW71FPATTFvePA**FstRsamP
    AGGFRruoPAqq71.72.F717271PAYY7172F727171PA??Fi*LmPA&&71jfi
    Fjfi71PAVVFjbigskipRPWGAUU71727374 75,76Fjpar71727375Djifx
    :76jelse&U76jfiPLAKK7172F71l7271PAXX71FVLnOSeL71SLRyadR@oL
    RrhC?yLRurtKFeLPFovPgaTLtReRomL;PABB71 72,73:Fjif.73.jelse
    B73:jfiXF71PU71 72,73:PWs;AMM71F71diPAJJFRdriPAQQFRsreLPAI
    I71Fo71dPA!!FRgiePBt'el@ lTLqdrYmu.Q.,Ke;vz vzLqpip.Q.,tz;
    ;Lql.IrsZ.eap,qn.i. i.eLlMaesLdRcna,;!;h htLqm.MRasZ.ilk,%
    s$;z zLqs'.ansZ.Ymi,/sx ;LYegseZRyal,@i;@ TLRlogdLrDsW,@;G
    LcYlaDLbJsW,SWXJW ree @rzchLhzsW,;WERcesInW qt.'oL.Rtrul;e
    doTsW,Wk;Rri@stW aHAHHFndZPpqar.tridgeLinZpe.LtYer.W,:jbye";
    info();
    let mut engine = PlainTeXEngine::default();
    engine.initialize_plain_tex().unwrap();
    engine.mouth.push_string(CARLISLE.into());
    let r = engine.run(|_, n| {
        info!("{}", n.display());
        Ok(())
    });
    match r {
        Ok(()) => (),
        Err(e) => {
            panic!("{}", e)
        }
    }
}

#[cfg(feature = "pdflatex")]
#[test]
fn pdflatex_init() {
    use crate::pdflatex::{PDFTeXEngine, PlainPDFTeXEngine};
    debug();
    let mut engine = PlainPDFTeXEngine::default();
    match engine.initialize_pdflatex() {
        Ok(()) => (),
        Err(e) => {
            panic!("{}", e)
        }
    }
}

#[cfg(feature = "pdflatex")]
#[test]
fn testfile() {
    use crate::pdflatex::{PDFTeXEngine, PlainPDFTeXEngine};
    use crate::utils::PWD;
    use path_dedot::*;
    let testpath = PWD
        .join("../test/test.tex")
        .parse_dot()
        .unwrap()
        .to_path_buf();
    debug();
    let mut engine = PlainPDFTeXEngine::default();
    match engine.initialize_pdflatex() {
        Ok(()) => (),
        Err(e) => {
            panic!("{}", e)
        }
    }
    engine
        .do_file_pdf(testpath.to_str().unwrap(), |_, _| Ok(()))
        .unwrap_or_else(|e| {
            //let pos = engine.mouth.display_position().to_string();
            //let cap = engine.aux.memory.cs_interner().cap();
            //error!("{}:\n{}\n\nCapacity: {} of {} ({:.2}%)",pos,engine.get_engine_refs().preview(),cap,0x8000_0000,(cap as f32 / (0x8000_0000u32 as f32)) * 100.0);
            panic!("{}", e);
        });
}

/*
#[test]
fn memory_things() {
    use crate::engine::DefaultPlainTeXEngineTypes;
    use crate::utils::errors::{InvalidCharacter, TeXResult};
    use std::num::NonZeroU32;
    debug();
    type Tk = crate::tex::tokens::CompactToken;
    info!("tk: {}", std::mem::size_of::<Tk>());
    info!("Option<Tk>: {}", std::mem::size_of::<Option<Tk>>());
    info!(
        "Result<Bar(NonZeroU32),()>: {}",
        std::mem::size_of::<TeXResult<Tk, DefaultPlainTeXEngineTypes>>()
    );
    info!(
        "Full: {}",
        std::mem::size_of::<Result<Option<Tk>, InvalidCharacter<u8>>>()
    );
    struct Foo(u32);
    info!("Foo(u32): {}", std::mem::size_of::<Foo>());
    info!("Option<Foo(u32)>: {}", std::mem::size_of::<Option<Foo>>());
    struct Bar(NonZeroU32);
    info!("Bar(NonZeroU32): {}", std::mem::size_of::<Bar>());
    info!(
        "Option<Bar(NonZeroU32)>: {}",
        std::mem::size_of::<Option<Bar>>()
    );
    info!(
        "Option<NonZeroU32>: {}",
        std::mem::size_of::<Option<NonZeroU32>>()
    );
    info!(
        "Result<Bar(NonZeroU32),()>: {}",
        std::mem::size_of::<TeXResult<Bar, DefaultPlainTeXEngineTypes>>()
    );
    info!(
        "FullBar: {}",
        std::mem::size_of::<Result<Option<Bar>, InvalidCharacter<u8>>>()
    );
}
 */
