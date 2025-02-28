use crate::engine::extension::CSS;
use crate::engine::{Font, Types};
use crate::shipout::state::{
    Alignment, CharOrStr, Common, FontData, ShipoutNodeH, ShipoutNodeHRow, ShipoutNodeM,
    ShipoutNodeSVG, ShipoutNodeTable, ShipoutNodeV, SourceRef,
};
use crate::utils::{Flex, VecMap, VecSet};
use crate::RUSTEX_CSS_URL;
use std::borrow::Cow;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::path::Path;
use tex_engine::engine::fontsystem::Font as FontT;
use tex_engine::pdflatex::nodes::{NumOrName, PDFColor, PDFImage};
use tex_engine::tex::nodes::boxes::{HBoxInfo, ToOrSpread, VBoxInfo};
use tex_engine::tex::nodes::math::MathClass;
use tex_engine::tex::numerics::TeXDimen;
use tex_engine::utils::HMap;
use tex_glyphs::fontstyles::FontModifier;

#[derive(Default)]
pub enum ImageOptions {
    #[default]
    AsIs,
    ModifyURL(Box<dyn Fn(&Path) -> String>),
    Embed,
}

pub(crate) struct CompilationDisplay<'a, 'b> {
    pub(crate) width: i32,
    pub(crate) indent: u8,
    pub(crate) color: PDFColor,
    pub(crate) font: Font,
    pub(crate) in_link:bool,
    pub(crate) font_data: &'a HMap<Box<str>, FontData>,
    pub(crate) attrs: VecMap<Cow<'static, str>, Cow<'static, str>>,
    pub(crate) styles: VecMap<Cow<'static, str>, Cow<'static, str>>,
    pub(crate) sourcerefs: bool,
    pub(crate) image: &'a ImageOptions,
    pub(crate) f: &'a mut Formatter<'b>,
}

macro_rules! node {
    ($self:ident !$($tk:tt)* ) => {{
        $self.do_indent()?;$self.indent +=1;
        node!(@START $self;$($tk)* IND );
    }};
    ($self:ident $($tk:tt)* ) => {
        node!(@START $self;$($tk)*)
    };
    (@START $self:ident;<<$tag:expr; $($tk:tt)*) => {
        write!($self.f,"<{}",$tag)?;
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@START $self:ident;<$tag:ident $($tk:tt)*) => {{
        write!($self.f,"<{}",stringify!($tag))?;
        node!(@ATTRS $self;stringify!($tag); $($tk)*);
    }};
    (@ATTRS $self:ident;$tag:expr; class=$cls:literal?($b:expr) $($tk:tt)*) => {
        if $b {write!($self.f," class=\"{}\"",$cls)?;}
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; class=$cls:literal $($tk:tt)*) => {
        write!($self.f," class=\"{}\"",$cls)?;
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; class=$cls:expr;? $($tk:tt)*) => {
        if (!$cls.is_empty()) {write!($self.f," class=\"{}\"",$cls)?;}
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; class=$cls:expr; $($tk:tt)*) => {
        write!($self.f," class=\"{}\"",$cls)?;
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; ref=$r:ident $($tk:tt)*) => {
        if $self.sourcerefs { write!($self.f," data-rustex-sourceref=\"{}\"",$r)? }
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; $a:literal=$v:expr; $($tk:tt)*) => {
        write!($self.f," {}=\"{}\"",$a,$v)?;
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; ?($v:expr) $($tk:tt)*) => {
        if let Some((k,v)) = $v {write!($self.f," {}=\"{}\"",k,v)?;}
        node!(@ATTRS $self;$tag; $($tk)*);
    };
    (@ATTRS $self:ident;$tag:expr; $($tk:tt)*) => {
        for (k,v) in std::mem::take(&mut $self.attrs) {
            write!($self.f," {}=\"{}\"",k,v)?
        }
        node!(@STYLES? $self;$tag; $($tk)*);
    };
    (@STYLES? $self:ident;$tag:expr; style:$style:block $($tk:tt)*) => {
        let mut in_style = false;
        #[allow(unused_mut)]
        let mut needs_span:Option<i32> = None;
        macro_rules! style {
            (!$s:ident; ($a:expr)=$v:expr) => {{
                if !in_style {
                    write!($s.f," style=\"")?;
                    in_style = true;
                }
                write!($s.f,"{}:{};",$a,$v)?;
            }};
            ($a:literal=$v:expr) => {{
                if !in_style {
                    write!($self.f," style=\"")?;
                    in_style = true;
                }
                write!($self.f,"{}:{};",$a,$v)?;
            }};
        }
        #[allow(unused_macros)]
        macro_rules! width {
            ($v:expr) => {
                if $v == 0 {style!("width"="0");} else if $v != $self.width {
                    needs_span = Some($self.width);
                    let pctg = $v as f32 / ($self.width as f32);
                    $self.width = $v;
                    style!("--rustex-scale-width"=format_args!("{:.2}",pctg));
                }
            }
        }
        $style
        if in_style {
            if let Some(w) = needs_span {
                node!(@STYLES! $self;$tag; {$($tk)*} WIDTH=w;);
            } else {
                node!(@STYLES! $self;$tag; {$($tk)*});
            }
        }
        else { node!(@STYLES? $self;$tag; $($tk)*); }
    };
    (@STYLES? $self:ident;$tag:expr; style:$a:literal=$v:expr; $($tk:tt)*) => {
        write!($self.f," style=\"{}:{};",$a,$v)?;
        node!(@STYLES! $self;$tag; {$($tk)*});
    };
    (@STYLES? $self:ident;$tag:expr; $($tk:tt)*) => {
        if !$self.styles.is_empty() {
            write!($self.f," style=\"")?;
            $self.do_styles()?;
            $self.f.write_char('"')?;
        }
        node!(@BODY $self;$tag; $($tk)*);
    };
    (@STYLES! $self:ident;$tag:expr; {style:$a:literal=$v:expr; $($tk:tt)*} $(WIDTH=$w:expr;)?) => {
        write!($self.f,"{}:{};",$a,$v)?;
        node!(@STYLES! $self;$tag; {$($tk)*} $(WIDTH=$w;)?);
    };
    (@STYLES! $self:ident;$tag:expr; {$($tk:tt)*} $(WIDTH=$w:expr;)?) => {
        $self.do_styles()?;
        $self.f.write_char('"')?;
        node!(@BODY $self;$tag; $($tk)* $(WIDTH=$w;)?);
    };
    (@BODY $self:ident;$tag:expr; />>) => {
        $self.f.write_str("/>")?
    };
    (@BODY $self:ident;$tag:expr; /> $(WIDTH=$w:expr;)?) => {
        write!($self.f,"></{}>",$tag)?;
        $( $self.width = $w; )?
    };
    (@BODY $self:ident;$tag:expr; /> IND $(WIDTH=$w:expr;)?) => {
        write!($self.f,"></{}>",$tag)?;
        $self.indent -= 1;
        $( $self.width = $w; )?
    };
    (@BODY $self:ident;$tag:expr; $b:block/>) => {
        $self.f.write_char('>')?;
        $b
        write!($self.f,"</{}>",$tag)?;
    };
    (@BODY $self:ident;$tag:expr; $b:block/> IND) => {
        $self.f.write_char('>')?;
        $b
        $self.indent -= 1;
        $self.do_indent()?;
        write!($self.f,"</{}>",$tag)?;
    };
    (@BODY $self:ident;$tag:expr; $b:block/> WIDTH=$w:expr;) => {
        $self.f.write_str("><span>")?;
        $b
        $self.width = $w;
        write!($self.f,"</span></{}>",$tag)?;
    };
    (@BODY $self:ident;$tag:expr; $b:block/> IND WIDTH=$w:expr;) => {
        $self.f.write_str("><span>")?;
        $b
        $self.indent -= 1;
        $self.do_indent()?;
        $self.width = $w;
        write!($self.f,"</span></{}>",$tag)?;
    };
}

impl CompilationDisplay<'_, '_> {
    pub fn display(
        &mut self,
        metas: &[VecMap<String, String>],
        top: &VecMap<String, String>,
        css: &[CSS],
        page_width: i32,
        out: &[ShipoutNodeV],
    ) -> std::fmt::Result {
        self.f.write_str("<!DOCTYPE html>\n<html lang=\"en\"")?;
        for (k, v) in top.iter() {
            write!(self.f, " {}=\"{}\"", k, v)?;
        }
        self.f.write_str(">\n<head>\t<meta charset=\"UTF-8\">\n")?;
        for m in metas {
            write!(self.f, "\t<meta")?;
            for (k, v) in m.iter() {
                write!(self.f, " {}=\"{}\"", k, v)?;
            }
            self.f.write_str(">\n")?;
        }
        writeln!(
            self.f,
            "\t<link rel=\"stylesheet\" type=\"text/css\" href=\"{}\">",
            RUSTEX_CSS_URL
        )?;
        for c in css.iter() {
            match c {
                CSS::File(s) => writeln!(
                    self.f,
                    "\t<link rel=\"stylesheet\" type=\"text/css\" href=\"{}\">",
                    s
                )?,
                CSS::Literal(s) => writeln!(self.f, "\t<style>\n{s}</style>")?,
            }
        }
        let mut fonts = VecSet::default();
        for (name, d) in self.font_data.iter() {
            //.filter_map(|d| d.1.web.as_ref().map(|s| s.as_ref().ok()).flatten()) {
            match &d.web {
                Some((l, _)) => fonts.insert(l),
                None => writeln!(self.f, "\t<!-- Missing web font for {name} -->")?,
            }
        }
        for font in fonts {
            //.filter_map(|d| d.1.web.as_ref().map(|s| s.as_ref().ok()).flatten()) {
            writeln!(self.f, "\t<link rel=\"stylesheet\" href=\"{font}\">")?
        }
        self.f.write_str("</head>")?;
        write!(
            self.f,
            "<body class=\"rustex-body\" style=\"--rustex-text-width:{};--rustex-page-width:{};",
            Self::dim_to_num(self.width),
            Self::dim_to_num(page_width)
        )?;
        if let Some(font) = self.font_data.get(self.font.filename()) {
            if let Some((_, css)) = font.web.as_ref() {
                write!(self.f, "font-family:{css};")?;
            }
            write!(
                self.f,
                "font-size:{};",
                Self::dim_to_string(self.font.get_at().0)
            )?;
        }
        self.f.write_str("\">")?;
        for c in out.iter() {
            self.do_v(c, true)?;
        }
        self.f.write_str("\n</body></html>")
    }

    #[inline(always)]
    fn dim_to_px(d: i32) -> f32 {
        d as f32 / 65536.0 * 1.5
    }
    #[inline(always)]
    fn dim_to_num(d: i32) -> String {
        format!("{:.5}", Self::dim_to_px(d))
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
    #[inline(always)]
    fn dim_to_string(d: i32) -> String {
        Self::dim_to_num(d) + "px"
    }
    #[inline(always)]
    fn mu_to_string(d: i32) -> String {
        format!("{:.5}", (d as f32) / 18.0 / 65536.0)
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
            + "em"
    }
    #[inline(always)]
    fn do_indent(&mut self) -> std::fmt::Result {
        self.f.write_char('\n')?;
        for _ in 0..(self.indent + 1) {
            self.f.write_str("\t")?;
        }
        Ok(())
    }
    fn do_styles(&mut self) -> std::fmt::Result {
        for (k, v) in std::mem::take(&mut self.styles) {
            write!(self.f, "{}:{};", k, v)?
        }
        Ok(())
    }

    fn font_attrs(
        &mut self,
        old: &Font,
        mut style: impl FnMut(&mut Self, &'static str, Cow<'static, str>) -> std::fmt::Result,
    ) -> std::fmt::Result {
        let oldd = self.font_data.get(old.filename()).unwrap();
        let newd = self.font_data.get(self.font.filename()).unwrap();
        let oldcss = oldd.web.as_ref().map(|(_, c)| c.as_str());
        let newcss = newd.web.as_ref().map(|(_, c)| c.as_str());
        if oldcss != newcss {
            if let Some(c) = newcss {
                style(self, "font-family", c.to_string().into())?;
            }
        }
        let size = ((self.font.get_at().0 as f32 / (old.get_at().0 as f32)) * 100.0).round();
        if size != 100.0 {
            style(self, "font-size", format!("{}%", size).into())?;
        }
        let old = oldd.modifiers.unwrap_or_default();
        let new = newd.modifiers.unwrap_or_default();
        if new.has(FontModifier::Capitals) && !old.has(FontModifier::Capitals) {
            style(self, "font-variant", "small-caps".into())?;
        } else if old.has(FontModifier::Capitals) && !new.has(FontModifier::Capitals) {
            style(self, "font-variant", "normal".into())?;
        }
        if new.has(FontModifier::Bold) && !old.has(FontModifier::Bold) {
            style(self, "font-weight", "bold".into())?;
        } else if old.has(FontModifier::Bold) && !new.has(FontModifier::Bold) {
            style(self, "font-weight", "normal".into())?;
        }
        if new.has(FontModifier::Italic) && !old.has(FontModifier::Italic) {
            style(self, "font-style", "italic".into())?;
        } else if new.has(FontModifier::Oblique) && !old.has(FontModifier::Oblique) {
            style(self, "font-style", "oblique".into())?;
        } else if (old.has(FontModifier::Italic) || old.has(FontModifier::Oblique))
            && !(new.has(FontModifier::Italic) || new.has(FontModifier::Oblique))
        {
            style(self, "font-style", "normal".into())?;
        }
        Ok(())
    }

    fn do_color<N>(
        &mut self,
        node: &'static str,
        color: &PDFColor,
        children: &Vec<N>,
        mut f: impl FnMut(&mut Self, &N) -> std::fmt::Result,
    ) -> std::fmt::Result {
        if *color == self.color {
            for c in children {
                f(self, c)?
            }
            Ok(())
        } else {
            let old = self.color;
            self.color = *color;
            if children.len() == 1 {
                self.styles
                    .insert("color".into(), format!("{}", color).into());
                for c in children {
                    f(self, c)?
                }
            } else {
                node!(self <<node; class="rustex-contents"?(node!="mrow" && node !="g") style:"color"=color; {
                    for c in children { f(self,c)? }
                }/>);
            }
            self.color = old;
            Ok(())
        }
    }

    fn do_font<N>(
        &mut self,
        node: &'static str,
        font: &Font,
        children: &Vec<N>,
        mut f: impl FnMut(&mut Self, &N) -> std::fmt::Result,
    ) -> std::fmt::Result {
        if *font == self.font {
            for c in children {
                f(self, c)?
            }
            Ok(())
        } else {
            let old = std::mem::replace(&mut self.font, font.clone());
            if children.len() == 1 {
                self.font_attrs(&old, |s, a, b| {
                    s.styles.insert(a.into(), b);
                    Ok(())
                })?;
                for c in children {
                    f(self, c)?
                }
            } else {
                node!(self <<node; class="rustex-contents"?(node!="mrow" && node !="g") style:{
                    self.font_attrs(&old,|s,k,v| Ok(style!(!s; (k)=v)))?;
                } {
                    for c in children { f(self,c)? }
                }/>);
            }
            self.font = old;
            Ok(())
        }
    }
    fn do_annotations<N>(
        &mut self,
        node: &str,
        attrs: &VecMap<Cow<'static, str>, Cow<'static, str>>,
        styles: &VecMap<Cow<'static, str>, Cow<'static, str>>,
        classes: &[Cow<'static, str>],
        children: &Vec<N>,
        mut f: impl FnMut(&mut Self, &N) -> std::fmt::Result,
    ) -> std::fmt::Result {
        let class_str = || {
            if classes.is_empty() {
                if node == "mrow" || node == "g" {
                    Cow::Borrowed("")
                } else {
                    Cow::Borrowed("rustex_contents")
                }
            } else {
                Cow::Owned(classes.join(" "))
            }
        };
        if !attrs.iter().any(|(k, _)| self.attrs.get(k).is_some()) {
            for (k, v) in attrs.iter() {
                self.attrs.insert(k.clone(), v.clone());
            }
            for (k, v) in styles.iter() {
                self.styles.insert(k.clone(), v.clone());
            }
            node!(self <<node; class=class_str();? {
                for c in children { f(self,c)? }
            }/>);
        } else {
            node!(self <<node; class=class_str();? {
                for (k,v) in attrs.iter() {
                    self.attrs.insert(k.clone(),v.clone());
                }
                for (k,v) in styles.iter() {
                    self.styles.insert(k.clone(),v.clone());
                }
                node!(self <<node; class=class_str();? {
                    for c in children { f(self,c)? }
                }/>);
            }/>);
        }
        Ok(())
    }

    fn do_v(&mut self, c: &ShipoutNodeV, top: bool) -> std::fmt::Result {
        match c {
            ShipoutNodeV::Common(Common::WithColor {
                color, children, ..
            }) => self.do_color("div", color, children, |s, n| s.do_v(n, top)),
            ShipoutNodeV::Common(Common::WithFont { font, children, .. }) => {
                self.do_font("div", font, children, |s, n| s.do_v(n, top))
            }
            ShipoutNodeV::Common(Common::WithAnnotation {
                attrs,
                styles,
                classes,
                children,
                tag,
                ..
            }) => self.do_annotations(
                tag.as_ref().map_or("div", |s| s.as_str()),
                attrs,
                styles,
                &classes.inner,
                children,
                |s, n| s.do_v(n, top),
            ),
            ShipoutNodeV::Common(Common::Literal(s)) => self.f.write_str(s),
            ShipoutNodeV::Common(Common::WithLink { children, .. }) if self.in_link => {
                for c in children { self.do_v(c,top)? }
                Ok(())
            }
            ShipoutNodeV::Common(Common::WithLink { href, children, .. }) => {
                node!(self <a "href"=href;{
                    self.in_link = true;
                    for c in children { self.do_v(c,top)? }
                    self.in_link = false;
                }/>);
                Ok(())
            }
            ShipoutNodeV::Common(Common::WithMatrix {
                scale,
                rotate,
                skewx,
                skewy,
                children,
                ..
            }) => {
                node!(self <span class="rustex-pdfmatrix" style:"transform"=format_args!("matrix({scale},{rotate},{skewx},{skewy},0,0)"); {
                for c in children { self.do_v(c,top)? }
            }/>);
                Ok(())
            }
            ShipoutNodeV::Common(Common::PDFDest(n)) => {
                match n {
                    NumOrName::Name(s) => node!(self !<a "id"=s;/>),
                    NumOrName::Num(n) => {
                        node!(self !<a "id"=format_args!("NUM_{}",n); "name"=format_args!("NUM_{}",n);/>)
                    }
                }
                Ok(())
            }
            ShipoutNodeV::KernSkip(m) => {
                node!(self !<div class="rustex-vskip" style:{
                if m.base.is_positive() {
                    style!("height"=Self::dim_to_string(m.base))
                } else {
                    style!("margin-bottom"=Self::dim_to_string(m.base))
                }
                match m.stretch {
                    Flex::Fil(_) | Flex::Fill(_) | Flex::Filll(_) =>
                        style!("margin-top"="auto"),
                    _ => ()
                }
            }/>);
                match m.stretch {
                    Flex::Fill(_) => {
                        node!(self !<div class="rustex-vskip" style:"margin-top"="auto";/>);
                    }
                    Flex::Filll(_) => {
                        node!(self !<div class="rustex-vskip" style:"margin-top"="auto";/>);
                        node!(self !<div class="rustex-vskip" style:"margin-top"="auto";/>);
                    }
                    _ => (),
                }
                Ok(())
            }
            ShipoutNodeV::Common(Common::VBox {
                sref,
                info: info @ VBoxInfo::VBox { .. },
                children,
                ..
            }) => {
                self.do_indent()?;
                self.do_vbox(sref, info, children, top)
            }
            ShipoutNodeV::Common(Common::VBox {
                sref,
                info: info @ VBoxInfo::VTop { .. },
                children,
                ..
            }) => self.do_vtop(sref, info, children, false),
            ShipoutNodeV::Common(Common::HBox {
                sref,
                info: info @ HBoxInfo::HBox { .. },
                children,
                ..
            }) => {
                self.do_indent()?;
                self.do_hbox(sref, info, children)
            }
            ShipoutNodeV::HRule {
                width,
                height,
                depth,
            } => {
                let ht = height.map(|h| h.0).unwrap_or(26214) + depth.map(|d| d.0).unwrap_or(0);
                if ht <= 0 {
                    return Ok(());
                }
                let bottom = match depth {
                    None => None,
                    Some(d) if d.0 == 0 => None,
                    Some(d) => Some(-d.0),
                };
                let color = self.color;

                node!(self !<div class="rustex-hrule" style:{
                style!("height"=Self::dim_to_string(ht));
                match width {
                    None => style!("min-width"="100%"),
                    Some(w) => style!("--rustex-scale-width"=(w.0 as f32) / (self.width as f32))
                }
            }{node!(self !<div style:{
                style!("background"=color);
                style!("height"=Self::dim_to_string(ht));
                if let Some(b) = bottom {
                    style!("margin-bottom"=Self::dim_to_string(b));
                }
            }/>)}/>);
                Ok(())
            }
            ShipoutNodeV::Paragraph {
                children,
                alignment,
                parskip: _,
                line_skip: _,
                left_skip,
                right_skip,
                width,
                sref,
                ..
            } => {
                self.do_indent()?;
                self.indent += 1;
                let cls = match width {
                    i if *i != 0 && *i != self.width => "rustex-paragraph rustex-scalewidth",
                    0 => todo!(),
                    _ => "rustex-paragraph rustex-withwidth",
                };
                node!(self <div class=cls;ref=sref style:{
                if !left_skip.is_zero() {
                    style!("margin-left"=Self::dim_to_string(left_skip.base))
                }
                if !right_skip.is_zero() {
                    style!("margin-right"=Self::dim_to_string(right_skip.base))
                }
                match alignment {
                    Alignment::L => style!("text-align"="left"),
                    Alignment::C => style!("text-align"="center"),
                    Alignment::R => style!("text-align"="right"),
                    _ => ()
                }
                width!(*width);
            }{for c in children {
                self.do_h(c,false)?
            }}/>);
                self.indent -= 1;
                Ok(())
            }
            ShipoutNodeV::HAlign {
                children, num_cols, ..
            } => {
                node!(self !<table class="rustex-halign" style:"--rustex-align-num"=num_cols;{
                node!(self <tbody {
                    for c in children {
                        self.do_row(c)?
                    }
                }/>);
            }/>);
                Ok(())
            }
            _ => todo!("{c:?}"),
        }
    }

    fn do_h(&mut self, c: &ShipoutNodeH, escape: bool) -> std::fmt::Result {
        match c {
            ShipoutNodeH::Common(Common::WithColor {
                color, children, ..
            }) => self.do_color("span", color, children, |s, n| s.do_h(n, escape)),
            ShipoutNodeH::Common(Common::WithFont { font, children, .. }) => {
                self.do_font("span", font, children, |s, n| s.do_h(n, escape))
            }
            ShipoutNodeH::Common(Common::WithAnnotation {
                attrs,
                styles,
                classes,
                children,
                tag,
                ..
            }) => self.do_annotations(
                tag.as_ref().map_or("span", |s| s.as_str()),
                attrs,
                styles,
                &classes.inner,
                children,
                |s, n| s.do_h(n, escape),
            ),
            ShipoutNodeH::Common(Common::Literal(s)) => self.f.write_str(s),
            ShipoutNodeH::Common(Common::WithLink { children, .. }) if self.in_link => {
                for c in children { self.do_h(c,escape)? }
                Ok(())
            }
            ShipoutNodeH::Common(Common::WithLink { href, children, .. }) => {
                node!(self <a "href"=href;{
                    self.in_link = true;
                    for c in children { self.do_h(c,escape)? }
                    self.in_link = false;
                }/>);
                Ok(())
            }
            ShipoutNodeH::Common(Common::WithMatrix {
                scale,
                rotate,
                skewx,
                skewy,
                children,
                ..
            }) => {
                node!(self <span class="rustex-pdfmatrix" style:"transform"=format_args!("matrix({scale},{rotate},{skewx},{skewy},0,0)"); {
                for c in children { self.do_h(c,escape)? }
            }/>);
                Ok(())
            }
            ShipoutNodeH::Common(Common::PDFDest(n)) => {
                match n {
                    NumOrName::Name(s) => node!(self <a "id"=s; "name"=s;/>),
                    NumOrName::Num(n) => {
                        node!(self <a "id"=format_args!("NUM_{}",n); "name"=format_args!("NUM_{}",n);/>)
                    }
                }
                Ok(())
            }
            ShipoutNodeH::KernSkip(m) => {
                node!(self <div class="rustex-hskip" style:{
                style!("margin-left"=Self::dim_to_string(m.base));
                match m.stretch {
                    Flex::Fil(_) | Flex::Fill(_) | Flex::Filll(_) =>
                        style!("margin-right"="auto"),
                    _ => ()
                }
            }/>);
                match m.stretch {
                    Flex::Fill(_) => {
                        node!(self <div class="rustex-hskip" style:"margin-right"="auto";/>);
                    }
                    Flex::Filll(_) => {
                        node!(self <div class="rustex-hskip" style:"margin-right"="auto";/>);
                        node!(self <div class="rustex-hskip" style:"margin-right"="auto";/>);
                    }
                    _ => (),
                }
                Ok(())
            }
            ShipoutNodeH::Common(Common::SVG {
                sref,
                minx,
                maxx,
                miny,
                maxy,
                children,
                ..
            }) => self.do_svg(sref, *minx, *maxx, *miny, *maxy, children),
            ShipoutNodeH::Common(Common::HBox {
                sref,
                info: info @ HBoxInfo::HBox { .. },
                children,
                ..
            }) => self.do_hbox(sref, info, children),
            ShipoutNodeH::Common(Common::VBox {
                sref,
                info: info @ VBoxInfo::VBox { .. },
                children,
                ..
            }) => self.do_vbox(sref, info, children, false),
            ShipoutNodeH::Common(Common::VBox {
                sref,
                info: info @ VBoxInfo::VTop { .. },
                children,
                ..
            }) => self.do_vtop(sref, info, children, false),
            ShipoutNodeH::VRule {
                width,
                height,
                depth,
            } => {
                let wd = match width {
                    Some(w) if w.0 <= 0 => return Ok(()),
                    Some(w) => w.0,
                    None => 26214,
                };
                let color = self.color;
                if height.is_none() && depth.is_none() {
                    node!(self <div class="rustex-vrule" style:{
                    style!("--rustex-this-width"=Self::dim_to_string(wd));
                    style!("background"=color);
                    if escape {
                        style!("align-self"="stretch");
                    } else {
                        let font_size = self.font.get_at().0;
                        style!("min-height"=Self::dim_to_string(font_size));
                    }
                }/>);
                } else {
                    let ht = height.map(|h| h.0).unwrap_or(0) + depth.map(|d| d.0).unwrap_or(0);
                    let dp = if depth.is_none() && !escape {
                        None
                    } else {
                        Some(depth.map(|d| d.0).unwrap_or(0))
                    };
                    node!(self <div class="rustex-vrule-container"
                    style:"height"=Self::dim_to_string(ht);
                    style:"--rustex-this-width"=Self::dim_to_string(wd);
                    {node!(self <div style:{
                        style!("background"=color);
                        match dp {
                            Some(dp) => style!("margin-bottom"=Self::dim_to_string(-dp)),
                            None => {
                                style!("height"=format_args!("calc(0.5ex + {})",Self::dim_to_string(ht)));
                                style!("margin-bottom"="-0.5ex");
                            }
                        }
                    }/>)}
                />);
                }
                Ok(())
            }
            ShipoutNodeH::Char(c) => {
                if self.attrs.is_empty() && self.styles.is_empty() {
                    Display::fmt(&Escaped(c), self.f)?
                } else {
                    node!(self <span class="rustex-contents" {Display::fmt(&Escaped(c), self.f)?}/>)
                }
                Ok(())
            }
            ShipoutNodeH::Space if !escape => {
                if self.attrs.is_empty() && self.styles.is_empty() {
                    self.f.write_str(" ")?
                } else {
                    node!(self <span class="rustex-contents" {self.f.write_char(' ')?}/>)
                }
                Ok(())
            }
            ShipoutNodeH::Space => {
                node!(self <div class="rustex-space-in-hbox" />);
                Ok(())
            }
            ShipoutNodeH::Math {
                display,
                sref,
                children,
                ..
            } => {
                let inner = move |s: &mut Self| {
                    node!(s <math class="rustex-math" ref=sref {
                    node!(s !<mrow {
                        for c in children {
                           s.do_indent()?;s.do_math(c,None/*,false */)?
                        }
                    }/>);
                }/>);
                    Ok(())
                };

                match display {
                    Some((above, below)) => {
                        node!(self <div class="rustex-display"
                        style:"margin-top"=Self::dim_to_string(above.base);
                        style:"margin-bottom"=Self::dim_to_string(below.base);{
                        inner(self)?
                    }/>);
                        Ok(())
                    }
                    None => inner(self),
                }
            }
            ShipoutNodeH::Img(img) => match (&self.image, &img.img) {
                (ImageOptions::AsIs, PDFImage::PDF(imgfile)) => {
                    let width = img.width().0;
                    let height = img.height().0;
                    let path = format!("{}-rustex.png", img.filepath.display());

                    node!(self <img "src"=path;
                        "width"=Self::dim_to_string(width);
                        "height"=Self::dim_to_string(height);
                    />>);
                    if !std::path::Path::new(&path).exists() {
                        let _ = imgfile.save_with_format(path, image::ImageFormat::Png);
                    }
                    Ok(())
                }
                (ImageOptions::AsIs, _) => {
                    let width = img.width().0;
                    let height = img.height().0;
                    node!(self <img "src"=img.filepath.display();
                        "width"=Self::dim_to_string(width);
                        "height"=Self::dim_to_string(height);
                    />>);
                    Ok(())
                }
                (ImageOptions::ModifyURL(f), _) => {
                    let width = img.width().0;
                    let height = img.height().0;
                    node!(self <img "src"=f(&img.filepath);
                        "width"=Self::dim_to_string(width);
                        "height"=Self::dim_to_string(height);
                    />>);
                    Ok(())
                }
                _ => todo!(),
            },
            ShipoutNodeH::Indent(i) => {
                if *i != 0 {
                    node!(self <div class="rustex-parindent" style:"margin-left"=Self::dim_to_string(*i);/>);
                }
                Ok(())
            }
            ShipoutNodeH::LineBreak => self.f.write_str("<br/>"),
            ShipoutNodeH::MissingGlyph {
                char, font_name, ..
            } => {
                node!(self <span class="rustex-missing-glyph" "title"=format_args!("Missing Glyph {char} in {font_name}");/>);
                Ok(())
            }
            _ => todo!("{c:?}"),
        }
    }

    fn cls(cls: MathClass) -> &'static str {
        match cls {
            MathClass::Ord => "rustex-math-ord",
            MathClass::Op => "rustex-math-op",
            MathClass::Bin => "rustex-math-bin",
            MathClass::Rel => "rustex-math-rel",
            MathClass::Open => "rustex-math-open",
            MathClass::Close => "rustex-math-close",
            MathClass::Punct => "rustex-math-punct",
        }
    }

    fn do_math(
        &mut self,
        c: &ShipoutNodeM,
        cls: Option<MathClass>, /*,cramped:bool */
    ) -> std::fmt::Result {
        match c {
            ShipoutNodeM::Common(Common::Literal(s)) => self.f.write_str(s),
            ShipoutNodeM::Common(Common::WithLink { href, children, .. }) => {
                node!(self !<mrow "href"=href; {
                for c in children { self.do_math(c,cls/*,cramped*/)? }
            }/>);
                Ok(())
            }
            ShipoutNodeM::Common(Common::WithColor {
                color, children, ..
            }) => self.do_color("mrow", color, children, |s, n| {
                s.do_math(n, cls /*,cramped*/)
            }),
            ShipoutNodeM::Common(Common::WithAnnotation {
                attrs,
                styles,
                classes,
                children,
                tag,
                ..
            }) => {
                self.do_annotations(
                    tag.as_ref().map_or("mrow", |s| s.as_str()),
                    attrs,
                    styles,
                    &classes.inner,
                    children,
                    |s, n| s.do_math(n, cls /*,cramped*/),
                )
            }
            ShipoutNodeM::Common(Common::WithFont { font, children, .. }) => {
                // TODO check if this is right - maybe move it down to the next non-math-node...?
                self.do_font("mrow", font, children, |s, n| {
                    s.do_math(n, cls /*,cramped*/)
                })
            }
            ShipoutNodeM::Common(Common::PDFDest(n)) => {
                match n {
                    NumOrName::Name(s) => node!(self !<mspace "id"=s; "name"=s;/>),
                    NumOrName::Num(n) => {
                        node!(self !<mspace "id"=format_args!("NUM_{}",n); "name"=format_args!("NUM_{}",n);/>)
                    }
                }
                Ok(())
            }
            /*ShipoutNodeM::MSkip{base,mu} if !*mu => {
                self.do_indent()?;
                node!(self !<mspace class="rustex-mkern" "width"=Self::dim_to_string(*base);/>);
                Ok(())
            }*/
            ShipoutNodeM::MSkip { base, .. } => {
                self.do_indent()?;
                if *base > 0 {
                    node!(self !<mspace class="rustex-mkern" "width"=Self::mu_to_string(*base);/>);
                } else {
                    let s = Self::mu_to_string(*base);
                    node!(self !<mspace class="rustex-mkern" "width"="0"; style:"margin-right"=s;/>);
                }
                Ok(())
            }
            ShipoutNodeM::WithClass {
                class, children, ..
            } => {
                if children.len() == 1 {
                    self.do_math(children.first().unwrap(), Some(*class) /*,cramped*/)
                } else {
                    let cls = Self::cls(*class);
                    node!(self !<mrow class=cls; {
                    for c in children {
                        self.do_math(c, Some(*class)/*,cramped*/)?
                    }
                }/>);
                    Ok(())
                }
            }
            ShipoutNodeM::Common(Common::HBox {
                sref,
                info: info @ HBoxInfo::HBox { .. },
                children,
                ..
            }) => {
               /* let oldwd = self.width;
                let wd = info
                    .assigned_width()
                    .unwrap_or_else(|| info.computed_width().unwrap_or_default())
                    .0; */
                node!(self !<mtext class="rustex-math-escape" ref=sref style:{
               /* match wd {
                    0 => style!("width"="0"),
                    x if x > 0 => {
                        self.width = wd;
                        let wd = Self::dim_to_string(wd);
                        style!("min-width"=wd);
                        style!("--rustex-curr-width"=wd);
                    }
                    _ => ()
                } */
            } {
                self.do_hbox(sref,info,children)?
            }/>);
                //self.width = oldwd;
                Ok(())
            }
            ShipoutNodeM::Common(Common::VBox {
                sref,
                info: info @ VBoxInfo::VTop { .. },
                children,
                ..
            }) => {
                /*let oldwd = self.width;
                let wd = info
                    .assigned_width()
                    .unwrap_or_else(|| info.computed_width().unwrap_or_default())
                    .0;*/
                node!(self !<mtext class="rustex-math-escape" ref=sref style:{
                /*match wd {
                    0 => style!("width"="0"),
                    x if x > 0 => {
                        self.width = wd;
                        let wd = Self::dim_to_string(wd);
                        style!("min-width"=wd);
                        style!("--rustex-curr-width"=wd);
                    }
                    _ => ()
                }*/
            } {
                self.do_vtop(sref,info,children,false)?;
            }/>);
                //self.width = oldwd;
                Ok(())
            }
            ShipoutNodeM::Common(Common::VBox {
                sref,
                info,
                children,
                ..
            }) => {
                /*let oldwd = self.width;
                let wd = info
                    .assigned_width()
                    .unwrap_or_else(|| info.computed_width().unwrap_or_default())
                    .0;*/
                node!(self !<mtext class="rustex-math-escape" ref=sref style:{
                /*match wd {
                    0 => style!("width"="0"),
                    x if x > 0 => {
                        self.width = wd;
                        let wd = Self::dim_to_string(wd);
                        style!("min-width"=wd);
                        style!("--rustex-curr-width"=wd);
                    }
                    _ => ()
                }*/
            } {
                self.do_vbox(sref,info,children,false)?;
            }/>);
                //self.width = oldwd;
                Ok(())
            }
            ShipoutNodeM::VCenter {
                sref,
                width,
                children,
                ..
            } => {
                let oldwd = self.width;
                node!(self !<mtext class="rustex-math-escape" ref=sref style:{
                match *width {
                    0 => style!("width"="0"),
                    x if x > 0 => {
                        self.width = *width;
                        let wd = Self::dim_to_string(*width);
                        style!("width"=wd);
                        style!("--rustex-curr-width"=wd);
                    }
                    _ => ()
                }
            } {
                node!(self <div class="rustex-vcenter-container" {
                    node!(self <div {
                        for c in children {
                            self.do_v(c,false)?;
                        }
                    } />)
                }/>)
            }/>);
                self.width = oldwd;
                Ok(())
            }
            ShipoutNodeM::VRule {
                width,
                height,
                depth,
            } => {
                let wd = match width {
                    Some(w) if w.0 <= 0 => return Ok(()),
                    Some(w) => w.0,
                    None => 26214,
                };
                let color = self.color;
                self.do_indent()?;
                node!(self <mspace "background"=color;
                "width"=Self::dim_to_string(wd);
                "height"=Self::dim_to_string(height.unwrap_or_default().0);
                "depth"=Self::dim_to_string(depth.unwrap_or_default().0);
            />);
                Ok(())
            }
            ShipoutNodeM::Glyph {
                char,
                cramped,
                display,
            } => {
                match cls {
                    Some(MathClass::Ord) | None => {
                        node!(self <mi class=Self::cls(MathClass::Ord); {Display::fmt(&Escaped(char), self.f)?}/>);
                    }
                    _ if *cramped => {
                        node!(self <mo "lspace"="0"; "rspace"="0"; class=Self::cls(MathClass::Ord); {Display::fmt(&Escaped(char), self.f)?}/>);
                    }
                    Some(MathClass::Op) if *display => {
                        node!(self <mo "lspace"="0"; "rspace"="0"; class=Self::cls(MathClass::Op); "stretchy"="true"; {Display::fmt(&Escaped(char), self.f)?}/>);
                    }
                    Some(MathClass::Op) => {
                        node!(self <mo "lspace"="0"; "rspace"="0"; class=Self::cls(MathClass::Op); "stretchy"="false"; {Display::fmt(&Escaped(char), self.f)?}/>);
                    }
                    Some(o) => {
                        node!(self <mo "lspace"="0"; "rspace"="0"; class=Self::cls(o); "stretchy"="false"; {Display::fmt(&Escaped(char), self.f)?}/>);
                    }
                };
                Ok(())
            }
            ShipoutNodeM::Space => {
                node!(self !<mspace class="rustex-mkern" "width"="0.25em";/>);
                Ok(())
            }
            ShipoutNodeM::Sup { base, sup, limits } => {
                node!(self !<<if *limits {"mover"} else {"msup"}; 
            ?(if *limits {Some(("displaystyle","true"))} else {None})
            {
                self.do_math(base,None/*,cramped*/)?;
                let at = self.font.get_at();
                self.font.set_at(at.scale_float(0.7));
                if sup.len() == 1 {
                    self.do_math(sup.first().unwrap(),None/*,cramped*/)?;
                } else {
                    node!(self !<mrow {
                        for c in sup.iter() {
                            self.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                }
                self.font.set_at(at);
            } />);
                Ok(())
            }
            ShipoutNodeM::Sub { base, sub, limits } => {
                node!(self !<<if *limits {"munder"} else {"msub"}; 
            ?(if *limits {Some(("displaystyle","true"))} else {None})
            {
                self.do_math(base,None/*,cramped*/)?;
                let at = self.font.get_at();
                self.font.set_at(at.scale_float(0.7));
                if sub.len() == 1 {
                    self.do_math(sub.first().unwrap(),None/*,cramped*/)?;
                } else {
                    node!(self !<mrow {
                        for c in sub.iter() {
                            self.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                }
                self.font.set_at(at);
            } />);
                Ok(())
            }
            ShipoutNodeM::LeftRight {
                sref,
                left,
                right,
                children,
                ..
            } => {
                node!(self !<mrow ref=sref {
                if let Some(Ok(c)) = left {
                    node!(self <mo "lspace"="0"; "rspace"="0";  class="rustex-math-open" "stretchy"="true"; {Display::fmt(&Escaped(c),self.f)?} />);
                }
                for c in children {
                    self.do_math(c,None/*,cramped*/)?
                }
                if let Some(Ok(c)) = right {
                    node!(self <mo "lspace"="0"; "rspace"="0";  class="rustex-math-close" "stretchy"="true"; {Display::fmt(&Escaped(c),self.f)?} />);
                }
            }/>);
                Ok(())
            }
            ShipoutNodeM::Middle(r) => {
                match r {
                    Ok(c) => {
                        node!(self <mo "lspace"="0"; "rspace"="0";  "stretchy"="true"; {Display::fmt(&Escaped(c),self.f)?}/>)
                    }
                    Err((_, char, font_name)) => {
                        node!(self <mtext class="rustex-missing" "title"=format_args!("Missing Glyph {char} in {font_name}");/>)
                    }
                }
                Ok(())
            }
            ShipoutNodeM::SubSup {
                base,
                sub,
                sup,
                limits,
            } => {
                node!(self !<<if *limits {"munderover"} else {"msubsup"}; 
                ?(if *limits {Some(("displaystyle","true"))} else {None})
            {
                self.do_math(base,None/*,cramped*/)?;
                let at = self.font.get_at();
                self.font.set_at(at.scale_float(0.7));
                if sub.len() == 1 {
                    self.do_math(sub.first().unwrap(),None/*,cramped*/)?;
                } else {
                    node!(self !<mrow {
                        for c in sub.iter() {
                            self.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                }
                if sup.len() == 1 {
                    self.do_math(sup.first().unwrap(),None/*,cramped*/)?;
                } else {
                    node!(self !<mrow {
                        for c in sup.iter() {
                            self.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                }
                self.font.set_at(at);
            } />);
                Ok(())
            }
            ShipoutNodeM::Phantom {
                width,
                height,
                depth,
            } => {
                self.do_indent()?;
                node!(self !<mspace
                "width"=Self::dim_to_string(*width);
                "height"=Self::dim_to_string(*height);
                "depth"=Self::dim_to_string(*depth);
            />);
                Ok(())
            }
            ShipoutNodeM::Over {
                sref,
                sep,
                left,
                right,
                top,
                bottom,
                ..
            } => {
                let inner = move |s: &mut Self| {
                    node!(s !<mfrac ref=sref "linethickness"=sep.map(Self::dim_to_string).unwrap_or_default(); {
                    node!(s !<mrow {
                        for c in top.iter() {
                            s.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                    node!(s !<mrow {
                        for c in bottom.iter() {
                            s.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                }/>);
                    Ok::<_, std::fmt::Error>(())
                };
                match (left, right) {
                    (None, None) => inner(self)?,
                    _ => node!(self !<mrow {
                    if let Some(Ok(c)) = left {
                        node!(self !<mo "lspace"="0"; "rspace"="0"; class="rustex-math-open" "stretchy"="true"; {Display::fmt(&Escaped(c),self.f)?} />);
                    }
                    inner(self)?;
                    if let Some(Ok(c)) = right {
                        node!(self !<mo "lspace"="0"; "rspace"="0"; class="rustex-math-close" "stretchy"="true"; {Display::fmt(&Escaped(c),self.f)?} />);
                    }
                }/>),
                }
                Ok(())
            }
            ShipoutNodeM::Underline { children, .. } => {
                if children.len() == 1 {
                    self.styles
                        .insert("text-decoration".into(), "underline".into());
                    self.do_math(children.first().unwrap(), cls /*,cramped*/)
                } else {
                    node!(self !<mrow style:"text-decoration"="underline"; {
                    for c in children {
                        self.do_math(c,cls/*,cramped*/)?;
                    }
                }/>);
                    Ok(())
                }
            }
            ShipoutNodeM::Overline { children, .. } => {
                if children.len() == 1 {
                    self.styles
                        .insert("text-decoration".into(), "overline".into());
                    self.do_math(children.first().unwrap(), cls /*,cramped*/)
                } else {
                    node!(self !<mrow style:"text-decoration"="overline"; {
                    for c in children {
                        self.do_math(c,cls/*,cramped*/)?;
                    }
                }/>);
                    Ok(())
                }
            }
            ShipoutNodeM::Accent {
                accent, children, ..
            } => {
                node!(self !<mover {
                if children.len() == 1 {
                    self.do_math(children.first().unwrap(),None/*,cramped*/)?
                } else {
                    node!(self !<mrow {
                        for c in children {
                            self.do_math(c,None/*,cramped*/)?;
                        }
                    }/>);
                }
                match accent {
                    Ok(c) => node!(self !<mo "lspace"="0"; "rspace"="0"; "stretchy"="false"; {Display::fmt(&Escaped(c),self.f)?}/>),
                    Err((_,c,fnt)) =>
                        node!(self !<mtext class="rustex-missing" "title"=format_args!("Missing Glyph {c} in {fnt}");/>)
                }
            }/>);
                Ok(())
            }
            ShipoutNodeM::MissingGlyph {
                char, font_name, ..
            } => {
                node!(self !<mtext class="rustex-missing" "title"=format_args!("Missing Glyph {char} in {font_name}");/>);
                Ok(())
            }
            ShipoutNodeM::Radical { children, .. } => {
                node!(self <msqrt {
                for c in children {
                    self.do_math(c,None/*,cramped*/)?;
                }
            }/>);
                Ok(())
            }
            ShipoutNodeM::Img(img) => Ok(node!(self <mtext {match (&self.image, &img.img) {
                (ImageOptions::AsIs, PDFImage::PDF(imgfile)) => {
                    let width = img.width().0;
                    let height = img.height().0;
                    let path = format!("{}-rustex.png", img.filepath.display());

                    node!(self <img "src"=path;
                        "width"=Self::dim_to_string(width);
                        "height"=Self::dim_to_string(height);
                    />>);
                    if !std::path::Path::new(&path).exists() {
                        let _ = imgfile.save_with_format(path, image::ImageFormat::Png);
                    }
                }
                (ImageOptions::AsIs, _) => {
                    let width = img.width().0;
                    let height = img.height().0;
                    node!(self <img "src"=img.filepath.display();
                    "width"=Self::dim_to_string(width);
                    "height"=Self::dim_to_string(height);
                />>);
                }
                (ImageOptions::ModifyURL(f), _) => {
                    let width = img.width().0;
                    let height = img.height().0;
                    node!(self <img "src"=f(&img.filepath);
                    "width"=Self::dim_to_string(width);
                    "height"=Self::dim_to_string(height);
                />>);
                }
                _ => todo!(),
            }}/>)),
            _ => todo!("{c:?}"),
        }
    }

    fn do_row(&mut self, c: &ShipoutNodeTable) -> std::fmt::Result {
        match c {
            ShipoutNodeTable::Common(Common::Literal(s)) => self.f.write_str(s),
            ShipoutNodeTable::Common(Common::WithAnnotation {
                children,
                attrs,
                styles,
                ..
            }) => {
                for c in children {
                    for (k, v) in attrs.iter() {
                        self.attrs.insert(k.clone(), v.clone())
                    }
                    for (k, v) in styles.iter() {
                        self.styles.insert(k.clone(), v.clone())
                    }
                    self.do_row(c)?;
                }
                Ok(())
            }
            ShipoutNodeTable::Common(Common::WithFont {
                children, /*font,*/
                ..
            }) => {
                for c in children {
                    // TODO insert font
                    self.do_row(c)?;
                }
                Ok(())
            }
            ShipoutNodeTable::Row { children, .. } => {
                node!(self !<tr {
                for c in children {
                    self.do_cell(c)?;
                }
            }/>);
                Ok(())
            }
            ShipoutNodeTable::NoAlign { children, .. } => {
                node!(self !<tr {node!(self <td class="rustex-noalign" {
                for c in children {
                    self.do_v(c,false)?;
                }
            }/>);}/>);
                Ok(())
            }
            _ => todo!("{c:?}"),
        }
    }

    fn do_cell(&mut self, c: &ShipoutNodeHRow) -> std::fmt::Result {
        match c {
            ShipoutNodeHRow::Common(Common::Literal(s)) => self.f.write_str(s),
            ShipoutNodeHRow::Common(Common::WithAnnotation {
                children,
                attrs,
                styles,
                ..
            }) => {
                for c in children {
                    for (k, v) in attrs.iter() {
                        self.attrs.insert(k.clone(), v.clone())
                    }
                    for (k, v) in styles.iter() {
                        self.styles.insert(k.clone(), v.clone())
                    }
                    self.do_cell(c)?;
                }
                Ok(())
            }
            ShipoutNodeHRow::Cell {
                children, spans, ..
            } => {
                node!(self !<td class="rustex-halign-cell" style:{
                if *spans > 1 {
                    style!("grid-column"=format_args!("span {}",spans))
                }
            }{
                for c in children {
                    self.do_h(c,true)?;
                }
            }/>);
                Ok(())
            }
            _ => todo!("{c:?}"),
        }
    }

    fn do_svg(
        &mut self,
        sref: &SourceRef,
        minx: i32,
        maxx: i32,
        miny: i32,
        maxy: i32,
        children: &Vec<ShipoutNodeSVG>,
    ) -> std::fmt::Result {
        node!(self <div class="rustex-svg" {node!(self <svg ref=sref
            "width"=Self::dim_to_string(maxx - minx);
            "height"=Self::dim_to_string(maxy - miny);
            "viewBox"=format_args!("{} {} {} {}",
                Self::dim_to_num(minx),
                Self::dim_to_num(miny),
                Self::dim_to_num(maxx - minx),
                Self::dim_to_num(maxy - miny)
            );{node!(self !<g "transform"=format_args!("translate(0,{})",Self::dim_to_num(maxy + miny)); {
                for c in children {
                    self.do_svg_node(c)?
                }
            }/>);}
        />)}/>);
        Ok(())
    }

    fn do_svg_node(&mut self, c: &ShipoutNodeSVG) -> std::fmt::Result {
        match c {
            ShipoutNodeSVG::Common(Common::Literal(s)) => self.f.write_str(s),
            ShipoutNodeSVG::SVGNode {
                tag,
                attrs,
                children,
                ..
            } => {
                for (k, v) in attrs.iter() {
                    self.attrs.insert((*k).into(), v.clone().into())
                }
                node!(self !<<tag; {for c in children {
                self.do_svg_node(c)?
            }} />);
                Ok(())
            }
            ShipoutNodeSVG::Common(Common::WithFont { font, children, .. }) => {
                // that should be okay...
                self.do_font("g", font, children, |s, n| s.do_svg_node(n))
            }
            ShipoutNodeSVG::Common(Common::WithColor {
                color, children, ..
            }) => self.do_color("g", color, children, |s, n| s.do_svg_node(n)),
            ShipoutNodeSVG::Common(Common::WithAnnotation {
                attrs,
                styles,
                classes,
                children,
                ..
            }) => self.do_annotations("g", attrs, styles, &classes.inner, children, |s, n| {
                s.do_svg_node(n)
            }),
            ShipoutNodeSVG::Common(Common::HBox {
                sref,
                info: info @ HBoxInfo::HBox { .. },
                children,
                ..
            }) => {
                let wd = info.computed_width().map(|d| d.0).unwrap_or_default();
                let ht = info.computed_height().map(|d| d.0).unwrap_or_default()
                    + info.assigned_depth().map(|d| d.0).unwrap_or_default();
                self.do_indent()?;
                node!(self !<foreignObject class="rustex-foreign"
                style:"width"=Self::dim_to_string(wd);
                style:"height"=Self::dim_to_string(ht);
                style:"translate"=format_args!("0 {}",Self::dim_to_string(-ht));
                {node!(self <div
                    {self.do_hbox(sref,info,children)?;}
                />)}
            />);
                Ok(())
            }
            _ => todo!("{c:?}"),
        }
    }

    fn do_hbox(
        &mut self,
        sref: &SourceRef,
        info: &HBoxInfo<Types>,
        children: &Vec<ShipoutNodeH>,
    ) -> std::fmt::Result {
        let (wd, ht, bottom, to) = get_box_dims_h(info);
        let cls = match wd {
            Some(i) if i != 0 && i != self.width => "rustex-hbox-container rustex-scalewidth",
            //Some(0) => todo!(),
            Some(i) if i == self.width => "rustex-hbox-container rustex-withwidth",
            _ => "rustex-hbox-container",
        };
        let inner = move |s: &mut Self| {
            node!(s <div class=cls; ref=sref style:{
                if let Some(bottom) = bottom {
                    style!("margin-bottom"=Self::dim_to_string(bottom))
                }
                if let Some(ht) = ht {
                    style!("height"=Self::dim_to_string(ht))
                }
                if let Some(wd) = wd { width!(wd) }
            } {
                let cls = match to {
                    Some(i) if i != 0 && i != s.width =>
                        "rustex-hbox rustex-scalewidth",
                    Some(i) if i == s.width =>
                        "rustex-hbox rustex-withwidth",
                    _ => "rustex-hbox"
                };
                node!(s <div class=cls; style:{if to.is_some() {style!("justify-content"="space-between")}; if let Some(w) = to {width!(w)}} {
                    for c in children {
                        s.do_h(c,true)?;
                    }
                }/>)
            } />);
            Ok::<_, std::fmt::Error>(())
        };
        match (info.raised(), info.moved_left()) {
            (Some(r), None) => {
                node!(self <div class="rustex-raise" style:"--rustex-raise"=Self::dim_to_string(r.0);{
                    inner(self)?
                }/>);
            }
            (None, Some(ml)) => {
                node!(self <div class="rustex-moveleft" style:"--rustex-moveleft"=Self::dim_to_string(ml.0);{
                    inner(self)?
                }/>);
            }
            _ => inner(self)?, // both are impossible anyway
        }
        Ok(())
    }

    fn do_vbox(
        &mut self,
        sref: &SourceRef,
        info: &VBoxInfo<Types>,
        children: &Vec<ShipoutNodeV>,
        top: bool,
    ) -> std::fmt::Result {
        let (wd, ht, bottom, to) = get_box_dims_v(top, info);
        let cls = match wd {
            Some(i) if i != 0 && i != self.width => "rustex-vbox-container rustex-scalewidth",
            //Some(0) => todo!(),
            Some(i) if i == self.width => "rustex-vbox-container rustex-withwidth",
            _ => "rustex-vbox-container",
        };
        let inner = move |slf: &mut Self| {
            node!(slf <div class=cls; ref=sref style:{
                if let Some(bottom) = bottom {
                    style!("margin-bottom"=Self::dim_to_string(bottom))
                }
                if let Some(wd) = wd { width!(wd) }
            } {
                let inner = move |slf:&mut Self| {
                    node!(slf <div class="rustex-vbox" style:{
                        match to {
                            Some(i) if i < 0 => {
                                style!("height"="0");
                                style!("margin-bottom"=Self::dim_to_string(i))
                            }
                            Some(i) => style!("height"=Self::dim_to_string(i)),
                            _ => ()
                        }
                    } {
                        for c in children {
                            slf.do_v(c,false)?;
                        }
                    }/>);
                    Ok::<_,std::fmt::Error>(())
                };
                if let Some(h) = ht {
                    node!(slf <div class="rustex-vbox-height-container" style:"height"=Self::dim_to_string(h);{
                        inner(slf)?
                    }/>);
                } else { inner(slf)?}
            }/>);
            Ok::<_, std::fmt::Error>(())
        };
        match (info.raised(), info.moved_left()) {
            (Some(r), None) => {
                node!(self <div class="rustex-raise" style:"--rustex-raise"=Self::dim_to_string(r.0);{
                    inner(self)?
                }/>);
            }
            (None, Some(ml)) => {
                node!(self <div class="rustex-moveleft" style:"--rustex-moveleft"=Self::dim_to_string(ml.0);{
                    inner(self)?
                }/>);
            }
            _ => inner(self)?, // both are impossible anyway
        }
        Ok(())
    }

    fn do_vtop(
        &mut self,
        sref: &SourceRef,
        info: &VBoxInfo<Types>,
        children: &Vec<ShipoutNodeV>,
        top: bool,
    ) -> std::fmt::Result {
        //let (wd,ht,bottom,to) = get_box_dims_v(top,info);
        let (wd, ht, bottom, to) = get_box_dims_vtop(&info);
        let cls = match wd {
            Some(i) if i != 0 && i != self.width => "rustex-vtop-container rustex-scalewidth",
            //Some(0) => todo!(),
            Some(i) if i == self.width => "rustex-vtop-container rustex-withwidth",
            _ => "rustex-vtop-container",
        };
        let inner = move |slf: &mut Self| {
            node!(slf <div class=cls; ref=sref style:{
                if let Some(ht) = ht {
                    style!("height"=Self::dim_to_string(ht))
                }
                if let Some(wd) = wd { width!(wd) }
            } {
                let inner = move |slf:&mut Self| {
                    node!(slf <div class="rustex-vtop" style:{
                        match to {
                            Some(i) if i < 0 => {
                                style!("height"=0);
                                style!("margin-bottom"=Self::dim_to_string(i));
                            }
                            Some(i) => style!("height"=Self::dim_to_string(i)),
                            _ => ()
                        }
                    } {
                        for c in children {
                            slf.do_v(c,false)?;
                        }
                    }/>);
                    Ok::<_,std::fmt::Error>(())
                };
                if let Some(bottom) = bottom {
                    node!(slf <div class="rustex-vtop-height-container" style:{
                        style!("bottom"=Self::dim_to_string(bottom));
                        style!("margin-top"=Self::dim_to_string(bottom));
                    } {
                        inner(slf)?
                    }/>);
                } else { inner(slf)?}
            }/>);
            Ok::<_, std::fmt::Error>(())
        };

        match (info.raised(), info.moved_left()) {
            (Some(r), None) => {
                node!(self <div class="rustex-raise" style:"--rustex-raise"=Self::dim_to_string(r.0);{
                    inner(self)?
                }/>);
            }
            (None, Some(ml)) => {
                node!(self <div class="rustex-moveleft" style:"--rustex-moveleft"=Self::dim_to_string(ml.0);{
                    inner(self)?
                }/>);
            }
            _ => inner(self)?, // both are impossible anyway
        }
        Ok(())
    }
}

fn get_box_dims_vtop(
    info: &VBoxInfo<Types>,
) -> (Option<i32>, Option<i32>, Option<i32>, Option<i32>) {
    let wd = match info.assigned_width() {
        Some(w) => Some(w.0),
        /*_ if top => match info.computed_width() {
            Some(w) if w.0 < 0 => Some(0),
            _ => None
        }*/
        _ => None,
    };
    let to = match info.to_or_scaled() {
        Some(ToOrSpread::To(d)) => Some(d.0),
        Some(ToOrSpread::Spread(s)) => Some(s.0 + info.computed_height().map(|d| d.0).unwrap_or(0)),
        _ => None,
    };
    if info.assigned_height().is_none() && info.assigned_depth().is_none() {
        return (wd, None, None, to);
    }
    let full_height = info
        .assigned_height()
        .or_else(|| info.computed_height())
        .unwrap_or_default()
        .0
        + info
            .assigned_depth()
            .or_else(|| info.computed_depth())
            .unwrap_or_default()
            .0;
    let bottom = if let Some(ht) = info.assigned_height() {
        Some(ht.0 - info.computed_height().map_or(0, |d| d.0))
    } else {
        None
    }; //info.assigned_height().or_else(|| info.computed_height()).unwrap_or_default().0 - info.computed_height().map_or(0, |d| d.0);
    (wd, Some(full_height), bottom, to)
}

fn get_box_dims_v(
    top: bool,
    info: &VBoxInfo<Types>,
) -> (Option<i32>, Option<i32>, Option<i32>, Option<i32>) {
    let wd = match info.assigned_width() {
        Some(w) => Some(w.0),
        _ if top => match info.computed_width() {
            Some(w) if w.0 < 0 => Some(0),
            _ => None,
        },
        _ => None,
    };
    let (ht, mut bottom) = match info.assigned_height() {
        Some(h) if h.0 < 0 => (Some(0), Some(h.0)),
        Some(h) => (Some(h.0), None),
        _ if top && info.computed_height().map(|d| d.0 < 0).unwrap_or(false) => (Some(0), None),
        _ => (None, None),
    };
    match (bottom, info.assigned_depth()) {
        (Some(b), Some(d)) => {
            let s = b + d.0;
            if s != 0 {
                bottom = Some(b);
            } else {
                bottom = None;
            }
        }
        (_, Some(d)) if d.0 != 0 => bottom = Some(d.0),
        _ => (),
    }
    let to = match info.to_or_scaled() {
        Some(ToOrSpread::To(d)) => Some(d.0),
        Some(ToOrSpread::Spread(s)) => Some(s.0 + info.computed_height().map(|d| d.0).unwrap_or(0)),
        _ => None,
    };
    (wd, ht, bottom, to)
}

fn get_box_dims_h(info: &HBoxInfo<Types>) -> (Option<i32>, Option<i32>, Option<i32>, Option<i32>) {
    let wd = match info.assigned_width() {
        Some(w) => Some(w.0),
        _ => match info.computed_width() {
            Some(w) if w.0 < 0 => Some(0),
            _ => None,
        },
    };
    let (ht, mut bottom) = match info.assigned_height() {
        Some(h) if h.0 < 0 => (Some(0), Some(h.0)),
        Some(h) => (Some(h.0), None),
        _ => (None, None),
    };
    match (bottom, info.assigned_depth()) {
        (Some(b), Some(d)) => {
            let s = b + d.0;
            if s != 0 {
                bottom = Some(b);
            } else {
                bottom = None;
            }
        }
        (_, Some(d)) if d.0 != 0 => bottom = Some(d.0),
        _ => (),
    }
    let to = match info.to_or_scaled() {
        Some(ToOrSpread::To(d)) => Some(d.0),
        Some(ToOrSpread::Spread(s)) => Some(s.0 + info.computed_width().map(|d| d.0).unwrap_or(0)),
        _ => None,
    };
    (wd, ht, bottom, to)
}

struct Escaped<'a>(&'a CharOrStr);
impl Display for Escaped<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        use CharOrStr::*;
        const TRIGGER: [char; 3] = ['<', '>', '&'];
        match self.0 {
            Char('<') => f.write_str("&lt;"),
            Char('>') => f.write_str("&gt;"),
            Char('&') => f.write_str("&amp;"),
            //'"' => self.f.write_str("&quot;"),
            //'\'' => self.f.write_str("&apos;"),
            Char(c) => f.write_char(*c),
            Str(s) if s.contains(TRIGGER) => f.write_str(
                &s.replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .replace('&', "&amp;"),
            ),
            Str(s) => f.write_str(s),
        }
    }
}
