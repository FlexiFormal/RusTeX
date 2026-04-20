/*! This module contains additional documentation, use cases, etc.

# Why is this so complicated?

Unlike most programming languages, which broadly adhere to a conceptual split between
*tokenization*, *parsing* and *interpreting* as separate, largely independent steps,
TeX combines all of these into a single process -- meaning that tokenization and parsing
are stateful processes that depend on the current state of the interpreter.
For example, the existence of the `\catcode` command means that whether the string
`\catcode` is a single token (i.e. a control sequence) or a sequence of 7 characters (or fewer)
depends on the current category codes of the characters in the string, which can be changed at any time.
Hence, without *interpreting* the document, and updating the state in the process, it is impossible
to even *parse* the document correctly into an abstract syntax tree.

Moreover, different engines treat the same document potentially very differently. For example,
plain TeX processes a file as a stream of individual *bytes*, whereas XeTeX interprets the file
as a stream of *Unicode codepoints*. While this may seem like a minor difference, it is a difference
that carries through the entire codebase. Plain TeX treats *fonts* by loading a `.tfm` file,
which provides metrics for up to at most 256 glyphs, whereas XeTeX uses system fonts with
potentially thousands of glyphs. Plain TeX offers a mere 256 registers for integer values,
dimensions, glue, etc., whereas e-TeX offers 32768 registers for each of these.
Plain TeX implements dimensions as integers of "scaled points", with corresponding arithmetic
operations and hence fixed-point precision, but an implementation taking advantage
of floating-point arithmetic would be possible without changing any of the core mechanisms of
macro expansion or (ultimately) producing nodes to be output. Having said that, the original
TeX engine produces dvips output, which was changed by pdfTeX to produce pdf output, adding
additional primitives for pdf-specific features.

But apart of those differences, the core primitive commands, macro expansion etc. are the same
for all engines.

This crate therefore tries to abstract away as much of the differences between engines as possible,
while still providing efficient implementations of the core algorithms and data structures.


# Okay, okay, so, how do I use this?

Well, it depends on what you want to do. In the simplest case, you want to use an already existing
engine to process a `.tex` file. In the simplest (but probably not too realistic) case, a plain
TeX engine without any of the extensions provided by e-TeX, pdfTeX, XeTeX, LuaTeX, etc., e.g.
maybe you want to process David Carlisle's famous [xii.tex](https://github.com/davidcarlisle/dpctex/blob/main/xii/xii.tex).
Then:
```no_run
use tex_engine::prelude::*;

// instantiate a new plain TeX engine:
let mut engine = PlainTeXEngine::default();
// register the default primitive commands and process `plain.tex`:
engine.initialize_plain_tex().unwrap();
// the engine is now ready to process a document:
engine.do_file_default("/path/to/your/xii.tex",|_,node| {
    // do something with the nodes produced, e.g. print them to stdout:
    println!("{}",node.display());Ok(())
}).unwrap()
```

This will produce something along the lines of:
```text
<vbox:vbox>
  <hbox:parline>
    <hbox:parindent></hbox:parindent>On<space>the<space>§rst<space>day<space>of<space>Christmas
    <space>my<space>true<space>love<space>gave<space>to<space>me
  </hbox:parline>
  <vskip:0.0pt plus 1.0pt>
  <hbox:parline>
    <hbox:parindent></hbox:parindent>a<space>partridge<space>in<space>a<space>pear<space>tree.
  </hbox:parline>
  <vskip:12.0pt plus 4.0pt minus 4.0pt>
  <vskip:0.0pt plus 1.0pt>
  ...
```
...representing the nodes produced by the engine, which can then be used to produce e.g. a `.dvi`
or `.pdf`file.

Now, chances are, if you have a `.tex` file, it will requite at least LaTeX (which is technically
just a set of macros on top of plain TeX). You can load the LaTeX macros by calling
```no_run
# use tex_engine::prelude::*;
# let mut engine = PlainTeXEngine::default();
engine.initialize_etex_primitives(); // registers the plain TeX + e-TeX primitives
engine.load_latex().unwrap(); // processes `latex.ltx`
```
...but this will throw an error, because LaTeX requires the primitives of one of several modern TeX engines,
such as pdfTeX, XeTeX or LuaTeX.

This crate implements (most of) the primitives of pdfTeX, when the `pdflatex` feature is enabled.
Assuming the feature is enabled, you can load the pdfTeX primitives by calling
```no_run
# #[cfg(feature="pdflatex")]
# {
# use tex_engine::prelude::*;
use tex_engine::pdflatex::{PDFTeXEngine,PlainPDFTeXEngine};
let mut engine = PlainPDFTeXEngine::default();
// registers the plain TeX + e-TeX + pdfTeX primitives,
// and processes `pdftexconfig.tex` and `latex.ltx`
engine.initialize_pdflatex().unwrap();
# }
```
This will (on `--release`) process `latex.ltx` in about 3.5 seconds on my machine.

You can then process a `.tex` file by calling
```no_run
# #[cfg(feature="pdflatex")]
# {
# use tex_engine::prelude::*;
# use tex_engine::pdflatex::{PDFTeXEngine,PlainPDFTeXEngine};
# let mut engine = PlainPDFTeXEngine::default();
engine.do_file_pdf("/path/to/your/tex/file.tex",|e,node| {
    // do something with the nodes produced, e.g. print them to stdout:
    println!("{}",node.display());Ok(())
}).unwrap()
# }
```

# What else can I do?
...todo

*/
