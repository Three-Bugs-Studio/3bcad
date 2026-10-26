//! Forward-mode automatic differentiation. Constraint residuals are written once over [`Scalar`]:
//! evaluated with `f64` they give values, with [`Dual`] they give exact partial derivatives,
//! which is the solver's Jacobian.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// Most parameters a single constraint reads (two lines = 8 coordinates).
pub const MAX_LOCAL: usize = 8;

pub trait Scalar:
    Copy + Add<Output = Self> + Sub<Output = Self> + Mul<Output = Self> + Div<Output = Self> + Neg<Output = Self>
{
    fn cst(v: f64) -> Self;
    fn value(self) -> f64;
    fn sqrt(self) -> Self;
    /// `self.atan2(x)`, like `f64::atan2`.
    fn atan2(self, x: Self) -> Self;

    fn abs(self) -> Self {
        if self.value() < 0.0 {
            -self
        } else {
            self
        }
    }

    fn hypot(self, other: Self) -> Self {
        (self * self + other * other).sqrt()
    }
}

impl Scalar for f64 {
    fn cst(v: f64) -> Self {
        v
    }
    fn value(self) -> f64 {
        self
    }
    fn sqrt(self) -> Self {
        f64::sqrt(self)
    }
    fn atan2(self, x: Self) -> Self {
        f64::atan2(self, x)
    }
}

/// A value with its gradient with respect to up to [`MAX_LOCAL`] variables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dual {
    pub v: f64,
    pub d: [f64; MAX_LOCAL],
}

impl Dual {
    /// The `i`-th independent variable.
    pub fn var(v: f64, i: usize) -> Self {
        let mut d = [0.0; MAX_LOCAL];
        d[i] = 1.0;
        Self { v, d }
    }

    fn map(self, v: f64, scale: f64) -> Self {
        Self {
            v,
            d: self.d.map(|x| x * scale),
        }
    }
}

impl Scalar for Dual {
    fn cst(v: f64) -> Self {
        Self { v, d: [0.0; MAX_LOCAL] }
    }
    fn value(self) -> f64 {
        self.v
    }
    fn sqrt(self) -> Self {
        let s = self.v.sqrt();
        // The derivative is unbounded at 0; zero keeps the solver finite (e.g. a zero-length line).
        self.map(s, if s > 0.0 { 0.5 / s } else { 0.0 })
    }
    fn atan2(self, x: Self) -> Self {
        let r2 = x.v * x.v + self.v * self.v;
        let mut d = [0.0; MAX_LOCAL];
        if r2 > 0.0 {
            for (i, d) in d.iter_mut().enumerate() {
                *d = (x.v * self.d[i] - self.v * x.d[i]) / r2;
            }
        }
        Self {
            v: self.v.atan2(x.v),
            d,
        }
    }
}

impl Add for Dual {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        let mut d = self.d;
        d.iter_mut().zip(o.d).for_each(|(a, b)| *a += b);
        Self { v: self.v + o.v, d }
    }
}

impl Sub for Dual {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        self + -o
    }
}

impl Neg for Dual {
    type Output = Self;
    fn neg(self) -> Self {
        self.map(-self.v, -1.0)
    }
}

impl Mul for Dual {
    type Output = Self;
    fn mul(self, o: Self) -> Self {
        let mut d = [0.0; MAX_LOCAL];
        for (i, d) in d.iter_mut().enumerate() {
            *d = self.d[i] * o.v + self.v * o.d[i];
        }
        Self { v: self.v * o.v, d }
    }
}

impl Div for Dual {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        let mut d = [0.0; MAX_LOCAL];
        for (i, d) in d.iter_mut().enumerate() {
            *d = (self.d[i] * o.v - self.v * o.d[i]) / (o.v * o.v);
        }
        Self { v: self.v / o.v, d }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// d/dx and d/dy of f(x, y) = atan2(y, x) * sqrt(x^2 + y^2) / (x - y), against central differences.
    #[test]
    fn dual_matches_finite_differences() {
        fn f<T: Scalar>(x: T, y: T) -> T {
            y.atan2(x) * x.hypot(y) / (x - y) + (-x).abs() * T::cst(2.0)
        }
        let (x, y) = (1.3, -0.7);
        let d = f(Dual::var(x, 0), Dual::var(y, 1));
        let h = 1e-6;
        let dx = (f(x + h, y) - f(x - h, y)) / (2.0 * h);
        let dy = (f(x, y + h) - f(x, y - h)) / (2.0 * h);
        assert!((d.v - f(x, y)).abs() < 1e-12);
        assert!(
            (d.d[0] - dx).abs() < 1e-6 && (d.d[1] - dy).abs() < 1e-6,
            "{:?} vs {dx} {dy}",
            d.d
        );
    }
}
