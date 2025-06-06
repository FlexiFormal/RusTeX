# RusTeX
A modular implementation of a TeX engine in rust, used to convert LaTeX documents to html5.

### Usage
```
rustex -i <path-to-input-file>.tex -o <path-to-output-file>.html
```

### Requirements
RusTeX implements (primarily) the primitives of (plain) TeX, eTeX and pdfTeX -- besides that, it will delegate to your local TeX system. This means that you need to have TeX installed on your system. RusTeX will then process your latex.ltx first, before processing your input file. It will also use the same TEXINPUTS settings as your TeX configuration. Consequently, RusTeX should behave exactly as your local TeX system does, except for producing html rather than pdf.
