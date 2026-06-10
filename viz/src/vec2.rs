//! Minimal 2D vector math.

use std::ops::{Add, AddAssign, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct V2 {
    pub x: f64,
    pub y: f64,
}

pub const fn v2(x: f64, y: f64) -> V2 {
    V2 { x, y }
}

impl V2 {
    pub fn norm(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn normalized(self) -> V2 {
        let n = self.norm();
        if n < 1e-12 {
            v2(0.0, 0.0)
        } else {
            v2(self.x / n, self.y / n)
        }
    }

    /// Counterclockwise perpendicular.
    pub fn perp(self) -> V2 {
        v2(-self.y, self.x)
    }

    /// Rotates `p` by the rotation taking (1, 0) to `dir` (unit vector).
    pub fn rotate_to(self, dir: V2) -> V2 {
        v2(
            self.x * dir.x - self.y * dir.y,
            self.x * dir.y + self.y * dir.x,
        )
    }

    pub fn dist(self, other: V2) -> f64 {
        (self - other).norm()
    }

    pub fn lerp(self, other: V2, t: f64) -> V2 {
        self + (other - self) * t
    }

    pub fn cross(self, other: V2) -> f64 {
        self.x * other.y - self.y * other.x
    }
}

impl Add for V2 {
    type Output = V2;
    fn add(self, o: V2) -> V2 {
        v2(self.x + o.x, self.y + o.y)
    }
}

impl AddAssign for V2 {
    fn add_assign(&mut self, o: V2) {
        self.x += o.x;
        self.y += o.y;
    }
}

impl Sub for V2 {
    type Output = V2;
    fn sub(self, o: V2) -> V2 {
        v2(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for V2 {
    type Output = V2;
    fn mul(self, s: f64) -> V2 {
        v2(self.x * s, self.y * s)
    }
}

impl Neg for V2 {
    type Output = V2;
    fn neg(self) -> V2 {
        v2(-self.x, -self.y)
    }
}

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy)]
pub struct BBox {
    pub min: V2,
    pub max: V2,
}

impl BBox {
    pub fn empty() -> BBox {
        BBox {
            min: v2(f64::INFINITY, f64::INFINITY),
            max: v2(f64::NEG_INFINITY, f64::NEG_INFINITY),
        }
    }

    pub fn include(&mut self, p: V2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }

    pub fn pad(&self, amount: f64) -> BBox {
        BBox {
            min: self.min - v2(amount, amount),
            max: self.max + v2(amount, amount),
        }
    }

    pub fn width(&self) -> f64 {
        (self.max.x - self.min.x).max(0.0)
    }

    pub fn height(&self) -> f64 {
        (self.max.y - self.min.y).max(0.0)
    }
}
