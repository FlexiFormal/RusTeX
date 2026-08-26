pub mod engine;
pub mod shipout;

pub mod utils;
pub use shipout::html::ImageOptions;

pub const RUSTEX_CSS_URL: &str =
    "https://raw.githack.com/FlexiFormal/RusTeX/main/rustex/src/resources/rustex.css";

/*
#[cfg(test)]
mod tests {
    use crate::engine::RusTeXEngineT;
    use crate::engine::{RusTeXEngine, Settings};
    use path_dedot::*;
    use std::path::PathBuf;
    use tex_engine::utils::PWD;

    #[test]
    fn initialize() {
        RusTeXEngine::initialize(true);
    }
    #[test]
    fn test_tex() {
        let testpath: PathBuf = PWD
            .join("../test/test.tex")
            .parse_dot()
            .unwrap()
            .to_path_buf();
        let ret = RusTeXEngine::do_file(testpath.to_str().unwrap(), Settings::default());
        let out = testpath.with_extension("html");
        ret.write_out(&out).unwrap();
    }
}
*/
