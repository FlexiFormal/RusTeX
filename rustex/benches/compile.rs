use criterion::{Criterion, criterion_group, criterion_main};
use rustex_lib::engine::{RusTeXEngine, RusTeXEngineT, Settings};

fn bench_settings() -> Settings {
    Settings {
        sourcerefs: false,
        verbose: false,
        log: false,
        image_options: Default::default(),
        insert_font_info: false,
    }
}

fn compile(path: &str) {
    let ret = RusTeXEngine::do_file(path, bench_settings());
    if let Some((e, _)) = ret.error {
        panic!("benchmark input {path} failed to compile: {e}");
    }
}

fn bench_compile(c: &mut Criterion) {
    let test_tex = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/test.tex");
    let macro_loop_tex = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/bench_macroloop.tex");
    //let thesis = concat!(env!("CARGO_MANIFEST_DIR"), "/../test/thesis/thesis.tex");

    // Force the one-time engine init (loads latex.ltx / rustex_defs.def, builds the font
    // store) outside of the timed loop; DefaultEngine caches this per-thread afterwards.
    compile(test_tex);
    {
        let mut group = c.benchmark_group("simple");
        group.sample_size(20);
        group.noise_threshold(0.05);
        group.significance_level(0.01);
        group.measurement_time(std::time::Duration::from_secs(20));
        group.bench_function("test.tex", |b| b.iter(|| compile(test_tex)));
    }
    {
        let mut group = c.benchmark_group("loop");
        group.sample_size(100);
        group.measurement_time(std::time::Duration::from_secs(20));
        group.noise_threshold(0.05);
        group.significance_level(0.01);
        group.bench_function("macro loop", |b| b.iter(|| compile(macro_loop_tex)));
    }
    /*{
        let mut group = c.benchmark_group("thesis");
        group.sample_size(10);
        group.measurement_time(std::time::Duration::from_secs(400));
        group.bench_function("thesis", |b| b.iter(|| compile(thesis)));
        group.finish();
    }*/
}

criterion_group!(benches, bench_compile);
criterion_main!(benches);
