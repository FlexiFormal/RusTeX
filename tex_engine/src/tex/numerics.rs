/*! Numerical values used in TeX, such as [integers](TeXInt) and [dimensions](TeXDimen).*/

use crate::engine::fontsystem::Font;
use crate::engine::state::State;
use crate::engine::{EngineReferences, EngineTypes};
use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::ops::Sub;
use std::ops::{Add, Div, Mul, Neg};

/// Bundles the various numerical types used in some engine, and converts between them.
pub trait NumSet: Clone + Debug {
    /// The integer type (canonically `i32`)
    type Int: TeXInt;
    /// The dimension type (canonically `Dim32`)
    type Dim: TeXDimen + Numeric<Self::Int>;
    /// The mu dimension type (canonically `Mu`)
    type MuDim: MuDim + Numeric<Self::Int>;
    /// Converts a [`MuSkip`] to a [`Skip`], using the current font's `em`.
    fn muskip_to_skip(muskip: MuSkip<Self::MuDim>, em: Self::Dim) -> Skip<Self::Dim> {
        let base = Self::mudim_to_dim(muskip.base, em);
        let stretch = muskip.stretch.map(|s| match s {
            MuStretchShrink::Mu(i) => StretchShrink::Dim(Self::mudim_to_dim(i, em)),
            MuStretchShrink::Fil(i) => StretchShrink::Fil(i),
            MuStretchShrink::Fill(i) => StretchShrink::Fill(i),
            MuStretchShrink::Filll(i) => StretchShrink::Filll(i),
        });
        let shrink = muskip.shrink.map(|s| match s {
            MuStretchShrink::Mu(i) => StretchShrink::Dim(Self::mudim_to_dim(i, em)),
            MuStretchShrink::Fil(i) => StretchShrink::Fil(i),
            MuStretchShrink::Fill(i) => StretchShrink::Fill(i),
            MuStretchShrink::Filll(i) => StretchShrink::Filll(i),
        });
        Skip::new(base, stretch, shrink)
    }
    /// Converts a [`Self::MuDim`] to a [`Self::Dim`], using the current font's `em`.
    fn mudim_to_dim(mudim: Self::MuDim, em: Self::Dim) -> Self::Dim;
    /// Converts a [`Self::Dim`] to a [`Self::Int`].
    fn dim_to_int(dim: Self::Dim) -> Self::Int;
}

/// Generalization of integers, dimensions, skips, etc. In particular, provides a [`scale`](Self::scale) method to scale all of them by their integer type.
pub trait Numeric<I: TeXInt>:
    Eq
    + Ord
    + Neg<Output = Self>
    + Add<Self, Output = Self>
    + Mul<I, Output = Self>
    + Div<I, Output = Self>
    + Copy
    + Default
    + Debug
    + Display
{
    #[must_use]
    fn scale(&self, times: I, div: I) -> Self;
}

/// A TeX integer. By default `i32`.
pub trait TeXInt:
    Numeric<Self>
    + From<i32>
    + TryFrom<i64>
    + Into<i64>
    + TryInto<i32>
    + Debug
    + Display
    + std::str::FromStr
{
    /// The minimum value of this integer type.
    const MIN: Self;
    /// The maximum value of this integer type.
    const MAX: Self;
}
/// A TeX dimension. By default [`Dim32`].
pub trait TeXDimen:
    Copy
    + Eq
    + Ord
    + Default
    + Debug
    + Display
    + Add<Self, Output = Self>
    + Sub<Self, Output = Self>
    + Neg<Output = Self>
    + Into<i64>
    + std::iter::Sum
{
    /// The units used in this dimension. By default [`DEFAULT_UNITS`].
    const UNITS: &'static [&'static [u8]] = DEFAULT_UNITS;
    /// Scales this dimension by a floating-point number.
    fn scale_float(&self, times: f64) -> Self;
    /// Make a new dimension from a value in "scaled points" (`sp` = `1/65536 pt`).
    fn from_sp(sp: i32) -> Self;
    /// Make a new dimension from a floating-point number and a unit. The unit is assumed to be in [`Self::UNITS`].
    fn from_float<ET: EngineTypes<Dim = Self>>(
        engine: &EngineReferences<ET>,
        f: f64,
        dim: &[u8],
    ) -> Self;
}

/// The default units for dimension values used in plain TeX:
/// - sp (scaled points)
/// - pt (points) = 65536sp
/// - pc (picas) = 12pt
/// - in (inches) = 72.27pt
/// - bp (big points) = 1/72 in
/// - cm (centimeters) = 2.54in
/// - mm (millimeters) = 0.1mm
/// - dd (didot points) = 1238/1157 pt
/// - cc (cicero) = 12 dd
/// - em (width of a capital M) = (`\fontdimen6` of the current font)
/// - ex (height of an x) = (`\fontdimen5` of the current font)
///
/// - px: technically only available id pdftex; here: =1bp
pub const DEFAULT_UNITS: &[&[u8]] = &[
    b"pt", b"pc", b"in", b"bp", b"cm", b"mm", b"dd", b"cc", b"sp", b"em", b"ex", b"px",
];

/// The default [`NumSet`] used in plain TeX, using `i32`, [`Dim32`] and [`Mu`] for integers, dimensions and mu
/// dimensions, respectively.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct DefaultNumSet;
impl NumSet for DefaultNumSet {
    type Int = i32;
    type Dim = Dim32;
    type MuDim = Mu;
    fn mudim_to_dim(mudim: Mu, em: Dim32) -> Dim32 {
        Dim32(((mudim.0 as f32 / 65536.0) * (em.0 as f32) / 18.0).round() as i32)
    }
    fn dim_to_int(dim: Dim32) -> i32 {
        dim.0
    }
}

impl Numeric<i32> for i32 {
    fn scale(&self, times: i32, div: i32) -> Self {
        ((*self as f64 * times as f64) / (div as f64)).round() as i32
    }
}
impl TeXInt for i32 {
    const MIN: Self = i32::MIN;
    const MAX: Self = i32::MAX;
}

/// A plain TeX dimension, represented as a 32-bit integer in *scaled points (sp)*, where 65536sp = 1pt.
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Debug, Default)]
pub struct Dim32(pub i32);
impl Numeric<i32> for Dim32 {
    fn scale(&self, times: i32, div: i32) -> Self {
        Self(self.0.scale(times, div))
    }
}
impl Add for Dim32 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Dim32(self.0 + rhs.0)
    }
}
impl Sub for Dim32 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Dim32(self.0 - rhs.0)
    }
}
impl Div<i32> for Dim32 {
    type Output = Self;
    fn div(self, rhs: i32) -> Self::Output {
        Self(self.0 / rhs)
    }
}
impl Mul<i32> for Dim32 {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self::Output {
        Self(self.0 * rhs)
    }
}
impl Dim32 {
    /// Display a number representing 65536 * `unit` (e.g. `pt` in scaled points).
    pub fn display_num(num: i32, unit: &str, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut val = num;
        if val < 0 {
            write!(f, "-")?;
            val = -val;
        }
        write!(f, "{}.", val / 65536)?;
        val = 10 * (val % 65536) + 5;
        let mut delta = 10;
        if val < delta {
            return write!(f, "0{}", unit);
        }
        while val > delta {
            if delta > 65536 {
                val = val + 32768 - 50000;
            }
            write!(f, "{}", val / 65536)?;
            val = 10 * (val % 65536);
            delta *= 10;
        }
        write!(f, "{}", unit)
    }
}
impl Neg for Dim32 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Dim32(-self.0)
    }
}
impl Display for Dim32 {
    // B-Book §103
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Self::display_num(self.0, "pt", f)
    }
}
impl From<Dim32> for i64 {
    fn from(d: Dim32) -> i64 {
        d.0 as i64
    }
}

impl TeXDimen for Dim32 {
    #[inline]
    #[allow(clippy::cast_possible_truncation)]
    fn scale_float(&self, times: f64) -> Self {
        let times = (times * 65536.0).round() as i64;
        Self((i64::from(self.0) * times / 65536) as i32)
    }
    fn from_sp(sp: i32) -> Self {
        Self(sp)
    }
    fn from_float<ET: EngineTypes<Dim = Self>>(
        engine: &EngineReferences<ET>,
        float: f64,
        dim: &[u8],
    ) -> Self {
        match dim {
            b"sp" => Self(1).scale_float(float),
            b"pt" => Self(65536).scale_float(float),
            b"pc" => Self(786_432).scale_float(float),
            b"in" => Self(4_736_286).scale_float(float),
            b"bp" => Self(65_781).scale_float(float),
            b"px" => Self(65_781).scale_float(float),
            b"cm" => Self(1_864_679).scale_float(float),
            b"mm" => Self(186_467).scale_float(float),
            b"dd" => Self(70_124).scale_float(float),
            b"cc" => Self(841_489).scale_float(float),
            b"em" => {
                let f = engine.state.get_current_font();
                let em = f.get_dim(5);
                em.scale_float(float)
            }
            b"ex" => {
                let f = engine.state.get_current_font();
                let ex = f.get_dim(4);
                ex.scale_float(float)
            }
            _ => unreachable!(),
        }
    }
}
impl std::iter::Sum for Dim32 {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::default(), |a, b| a + b)
    }
}

/// The units for strech/shrink values: `fil`, `fill` and `filll`.
pub const STRETCH_SHRINK_UNITS: &[&[u8]] = &[b"fil", b"fill", b"filll"];

/// A skip a.k.a. glue value, i.e. a dimension with optional stretch and shrink components.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub struct Skip<D: TeXDimen> {
    pub base: D,
    pub stretch: Option<StretchShrink<D>>,
    pub shrink: Option<StretchShrink<D>>,
}

/// A stretch/shrink component of a [`Skip`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum StretchShrink<D: TeXDimen> {
    Dim(D),
    Fil(i32),
    Fill(i32),
    Filll(i32),
}
impl<D: TeXDimen> StretchShrink<D> {
    /// Returns a new [`StretchShrink`] from a floating-point number and a unit. The unit is assumed to be
    /// `fil`, `fill`, `filll` or in [`D::UNITS`](TeXDimen::UNITS).
    pub fn from_float<ET: EngineTypes<Dim = D>>(
        engine: &EngineReferences<ET>,
        float: f64,
        dim: &[u8],
    ) -> Self {
        match dim {
            b"fil" => Self::Fil((float * 65536.0).round() as i32),
            b"fill" => Self::Fill((float * 65536.0).round() as i32),
            b"filll" => Self::Filll((float * 65536.0).round() as i32),
            _ => Self::Dim(D::from_float(engine, float, dim)),
        }
    }
}
impl<D: TeXDimen> Add<StretchShrink<D>> for StretchShrink<D> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Dim(d1), Self::Dim(d2)) => Self::Dim(d1 + d2),
            (Self::Fil(i1), Self::Fil(i2)) => Self::Fil(i1 + i2),
            (Self::Fill(i1), Self::Fill(i2)) => Self::Fill(i1 + i2),
            (Self::Filll(i1), Self::Filll(i2)) => Self::Filll(i1 + i2),
            _ => self.max(rhs),
        }
    }
}
impl<D: TeXDimen> PartialOrd for StretchShrink<D> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<D: TeXDimen> Ord for StretchShrink<D> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Dim(d1), Self::Dim(d2)) => d1.cmp(d2),
            (Self::Fil(i1), Self::Fil(i2)) => i1.cmp(i2),
            (Self::Fill(i1), Self::Fill(i2)) => i1.cmp(i2),
            (Self::Filll(i1), Self::Filll(i2)) => i1.cmp(i2),
            (Self::Filll(_), _) => Ordering::Greater,
            (_, Self::Filll(_)) => Ordering::Less,
            (Self::Fill(_), _) => Ordering::Greater,
            (_, Self::Fill(_)) => Ordering::Less,
            (Self::Fil(_), _) => Ordering::Greater,
            (_, Self::Fil(_)) => Ordering::Less,
        }
    }
}

impl<D: TeXDimen> Add<D> for Skip<D> {
    type Output = Self;
    fn add(self, rhs: D) -> Self::Output {
        Self {
            base: self.base + rhs,
            stretch: self.stretch,
            shrink: self.shrink,
        }
    }
}
impl<D: TeXDimen> PartialOrd for Skip<D> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<D: TeXDimen> Ord for Skip<D> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.base.cmp(&other.base)
    }
}
impl<D: TeXDimen> Add<Self> for Skip<D> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            base: self.base + rhs.base,
            stretch: match (self.stretch, rhs.stretch) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                _ => None,
            },
            shrink: match (self.shrink, rhs.shrink) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                _ => None,
            },
        }
    }
}
impl<I: TeXInt, D: TeXDimen + Numeric<I>> Div<I> for Skip<D> {
    type Output = Self;
    fn div(self, rhs: I) -> Self::Output {
        self.scale(1.into(), rhs)
    }
}
impl<I: TeXInt, D: TeXDimen + Numeric<I>> Mul<I> for Skip<D> {
    type Output = Self;
    fn mul(self, rhs: I) -> Self::Output {
        self.scale(rhs, 1.into())
    }
}

impl<D: TeXDimen> Display for Skip<D> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.base)?;
        if let Some(stretch) = &self.stretch {
            write!(f, " plus ")?;
            match stretch {
                StretchShrink::Dim(d) => write!(f, "{}", d)?,
                StretchShrink::Fil(i) => Dim32::display_num(*i, "fil", f)?,
                StretchShrink::Fill(i) => Dim32::display_num(*i, "fill", f)?,
                StretchShrink::Filll(i) => Dim32::display_num(*i, "filll", f)?,
            }
        }
        if let Some(shrink) = &self.shrink {
            write!(f, " minus ")?;
            match shrink {
                StretchShrink::Dim(d) => write!(f, "{}", d)?,
                StretchShrink::Fil(i) => Dim32::display_num(*i, "fil", f)?,
                StretchShrink::Fill(i) => Dim32::display_num(*i, "fill", f)?,
                StretchShrink::Filll(i) => Dim32::display_num(*i, "filll", f)?,
            }
        }
        Ok(())
    }
}
impl<D: TeXDimen> Neg for Skip<D> {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self {
            base: -self.base,
            stretch: self.stretch.map(|s| match s {
                StretchShrink::Dim(d) => StretchShrink::Dim(-d),
                StretchShrink::Fil(i) => StretchShrink::Fil(-i),
                StretchShrink::Fill(i) => StretchShrink::Fill(-i),
                StretchShrink::Filll(i) => StretchShrink::Filll(-i),
            }),
            shrink: self.shrink.map(|s| match s {
                StretchShrink::Dim(d) => StretchShrink::Dim(-d),
                StretchShrink::Fil(i) => StretchShrink::Fil(-i),
                StretchShrink::Fill(i) => StretchShrink::Fill(-i),
                StretchShrink::Filll(i) => StretchShrink::Filll(-i),
            }),
        }
    }
}
impl<D: TeXDimen> Skip<D> {
    pub fn new(
        base: D,
        stretch: Option<StretchShrink<D>>,
        shrink: Option<StretchShrink<D>>,
    ) -> Self {
        Self {
            base,
            stretch: match stretch {
                Some(StretchShrink::Dim(d)) if d == D::default() => None,
                Some(StretchShrink::Fil(0) | StretchShrink::Fill(0) | StretchShrink::Filll(0)) => {
                    None
                }
                _ => stretch,
            },
            shrink: match shrink {
                Some(StretchShrink::Dim(d)) if d == D::default() => None,
                Some(StretchShrink::Fil(0) | StretchShrink::Fill(0) | StretchShrink::Filll(0)) => {
                    None
                }
                _ => shrink,
            },
        }
    }
}

impl<I: TeXInt, D: TeXDimen + Numeric<I>> Numeric<I> for Skip<D> {
    fn scale(&self, times: I, div: I) -> Self {
        Self {
            base: self.base.scale(times, div),
            stretch: self.stretch,
            shrink: self.shrink,
        }
    }
}

/// A math dimension; i.e. the base component of an `\mskip`
pub trait MuDim:
    Display + Debug + Clone + Copy + Neg<Output = Self> + Add<Output = Self> + PartialEq + Ord + Default
{
    /// The set of math units; by default, only `mu`
    const UNITS: &'static [&'static [u8]] = &[b"mu"];
    /// Converts a floating-point number and a unit to a [`Self`]. The unit is assumed to be in [`Self::UNITS`].
    fn from_float<ET: EngineTypes>(engine: &EngineReferences<ET>, float: f64, dim: &[u8]) -> Self;
}

/// A math skip/glue consisting of a [`MuDim`] and optional stretch and shrink components.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub struct MuSkip<M: MuDim> {
    pub base: M,
    pub stretch: Option<MuStretchShrink<M>>,
    pub shrink: Option<MuStretchShrink<M>>,
}
impl<M: MuDim> MuSkip<M> {
    /// Returns a new [`MuSkip`] from a [`MuDim`] and optional stretch and shrink components.
    pub fn new(
        base: M,
        stretch: Option<MuStretchShrink<M>>,
        shrink: Option<MuStretchShrink<M>>,
    ) -> Self {
        Self {
            base,
            stretch: match stretch {
                Some(MuStretchShrink::Mu(d)) if d == M::default() => None,
                Some(
                    MuStretchShrink::Fil(0) | MuStretchShrink::Fill(0) | MuStretchShrink::Filll(0),
                ) => None,
                _ => stretch,
            },
            shrink: match shrink {
                Some(MuStretchShrink::Mu(d)) if d == M::default() => None,
                Some(
                    MuStretchShrink::Fil(0) | MuStretchShrink::Fill(0) | MuStretchShrink::Filll(0),
                ) => None,
                _ => shrink,
            },
        }
    }
}

/// A stretch/shrink component of a [`MuSkip`].
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum MuStretchShrink<M: MuDim> {
    Mu(M),
    Fil(i32),
    Fill(i32),
    Filll(i32),
}
impl<M: MuDim> MuStretchShrink<M> {
    /// Returns a new [`MuStretchShrink`] from a floating-point number and a unit. The unit is assumed to be
    /// `fil`, `fill`, `filll` or in [`M::UNITS`](MuDim::UNITS).
    pub fn from_float<ET: EngineTypes>(
        engine: &EngineReferences<ET>,
        float: f64,
        dim: &[u8],
    ) -> Self {
        match dim {
            b"fil" => Self::Fil((float * 65536.0).round() as i32),
            b"fill" => Self::Fill((float * 65536.0).round() as i32),
            b"filll" => Self::Filll((float * 65536.0).round() as i32),
            _ => Self::Mu(M::from_float(engine, float, dim)),
        }
    }
}
impl<M: MuDim> Add<MuStretchShrink<M>> for MuStretchShrink<M> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Self::Mu(d1), Self::Mu(d2)) => Self::Mu(d1 + d2),
            (Self::Fil(i1), Self::Fil(i2)) => Self::Fil(i1 + i2),
            (Self::Fill(i1), Self::Fill(i2)) => Self::Fill(i1 + i2),
            (Self::Filll(i1), Self::Filll(i2)) => Self::Filll(i1 + i2),
            _ => self.max(rhs),
        }
    }
}
impl<M: MuDim> PartialOrd for MuStretchShrink<M> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<M: MuDim> Ord for MuStretchShrink<M> {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Mu(d1), Self::Mu(d2)) => d1.cmp(d2),
            (Self::Fil(i1), Self::Fil(i2)) => i1.cmp(i2),
            (Self::Fill(i1), Self::Fill(i2)) => i1.cmp(i2),
            (Self::Filll(i1), Self::Filll(i2)) => i1.cmp(i2),
            (Self::Filll(_), _) => Ordering::Greater,
            (_, Self::Filll(_)) => Ordering::Less,
            (Self::Fill(_), _) => Ordering::Greater,
            (_, Self::Fill(_)) => Ordering::Less,
            (Self::Fil(_), _) => Ordering::Greater,
            (_, Self::Fil(_)) => Ordering::Less,
        }
    }
}

/// A plain TeX mu dimension, represented as a 32-bit integer analogously to *scaled points*, i.e. [`Mu`]`(65536) = 1mu`.
/// (where `18mu = 1em`).
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default, PartialOrd, Ord)]
pub struct Mu(pub i32);
impl Numeric<i32> for Mu {
    fn scale(&self, times: i32, div: i32) -> Self {
        Self((self.0 as i64 * (times as i64) / (div as i64)) as i32)
    }
}
impl Div<i32> for Mu {
    type Output = Self;
    fn div(self, rhs: i32) -> Self::Output {
        Self(self.0 / rhs)
    }
}
impl Mul<i32> for Mu {
    type Output = Self;
    fn mul(self, rhs: i32) -> Self::Output {
        Self(self.0 * rhs)
    }
}
impl MuDim for Mu {
    fn from_float<ET: EngineTypes>(_engine: &EngineReferences<ET>, float: f64, dim: &[u8]) -> Self {
        match dim {
            b"mu" => Mu((float * 65536.0).round() as i32),
            _ => unreachable!(),
        }
    }
}
impl Neg for Mu {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Mu(-self.0)
    }
}
impl Add<Mu> for Mu {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Mu(self.0 + rhs.0)
    }
}
impl Display for Mu {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Dim32::display_num(self.0, "mu", f)
    }
}

impl<M: MuDim> PartialOrd for MuSkip<M> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<M: MuDim> Ord for MuSkip<M> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.base.cmp(&other.base)
    }
}
impl<M: MuDim> Display for MuSkip<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.base)?;
        if let Some(stretch) = &self.stretch {
            write!(f, " plus ")?;
            match stretch {
                MuStretchShrink::Mu(d) => write!(f, "{}", d)?,
                MuStretchShrink::Fil(i) => Dim32::display_num(*i, "fil", f)?,
                MuStretchShrink::Fill(i) => Dim32::display_num(*i, "fill", f)?,
                MuStretchShrink::Filll(i) => Dim32::display_num(*i, "filll", f)?,
            }
        }
        if let Some(shrink) = &self.shrink {
            write!(f, " minus ")?;
            match shrink {
                MuStretchShrink::Mu(d) => write!(f, "{}", d)?,
                MuStretchShrink::Fil(i) => Dim32::display_num(*i, "fil", f)?,
                MuStretchShrink::Fill(i) => Dim32::display_num(*i, "fill", f)?,
                MuStretchShrink::Filll(i) => Dim32::display_num(*i, "filll", f)?,
            }
        }
        Ok(())
    }
}
impl<M: MuDim> Neg for MuSkip<M> {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self {
            base: -self.base,
            stretch: self.stretch.map(|s| match s {
                MuStretchShrink::Mu(d) => MuStretchShrink::Mu(-d),
                MuStretchShrink::Fil(i) => MuStretchShrink::Fil(-i),
                MuStretchShrink::Fill(i) => MuStretchShrink::Fill(-i),
                MuStretchShrink::Filll(i) => MuStretchShrink::Filll(-i),
            }),
            shrink: self.shrink.map(|s| match s {
                MuStretchShrink::Mu(d) => MuStretchShrink::Mu(-d),
                MuStretchShrink::Fil(i) => MuStretchShrink::Fil(-i),
                MuStretchShrink::Fill(i) => MuStretchShrink::Fill(-i),
                MuStretchShrink::Filll(i) => MuStretchShrink::Filll(-i),
            }),
        }
    }
}
impl<M: MuDim> Default for MuSkip<M> {
    fn default() -> Self {
        Self {
            base: M::default(),
            stretch: None,
            shrink: None,
        }
    }
}
impl<I: TeXInt, M: MuDim + Numeric<I>> Numeric<I> for MuSkip<M> {
    fn scale(&self, times: I, div: I) -> Self {
        Self {
            base: self.base.scale(times, div),
            stretch: self.stretch,
            shrink: self.shrink,
        }
    }
}

impl<M: MuDim> Add<Self> for MuSkip<M> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            base: self.base + rhs.base,
            stretch: match (self.stretch, rhs.stretch) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                _ => None,
            },
            shrink: match (self.shrink, rhs.shrink) {
                (Some(a), Some(b)) => Some(a + b),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                _ => None,
            },
        }
    }
}
impl<I: TeXInt, M: MuDim + Numeric<I>> Div<I> for MuSkip<M> {
    type Output = Self;
    fn div(self, rhs: I) -> Self::Output {
        self.scale(1.into(), rhs)
    }
}
impl<I: TeXInt, M: MuDim + Numeric<I>> Mul<I> for MuSkip<M> {
    type Output = Self;
    fn mul(self, rhs: I) -> Self::Output {
        self.scale(rhs, 1.into())
    }
}
