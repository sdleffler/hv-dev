use std::ops::{Add, AddAssign, Mul, MulAssign, Sub, SubAssign};

use na::{Isometry2, RealField, Rotation2, Storage, Vector, Vector2, Vector3, U3};
use serde::*;

/// A velocity structure combining both the linear and angular velocities of a point.
#[repr(C)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Velocity2<N: RealField + Copy> {
    /// The linear velocity.
    pub linear: Vector2<N>,
    /// The angular velocity.
    pub angular: N,
}

impl<N: RealField + Copy> Velocity2<N> {
    /// Create velocity from its linear and angular parts.
    #[inline]
    pub fn new(linear: Vector2<N>, angular: N) -> Self {
        Velocity2 { linear, angular }
    }

    /// Create a purely angular velocity.
    #[inline]
    pub fn angular(w: N) -> Self {
        Velocity2::new(na::zero(), w)
    }

    /// Create a purely linear velocity.
    #[inline]
    pub fn linear(vx: N, vy: N) -> Self {
        Velocity2::new(Vector2::new(vx, vy), N::zero())
    }

    /// Create a zero velocity.
    #[inline]
    pub fn zero() -> Self {
        Self::new(na::zero(), N::zero())
    }

    /// Computes the velocity required to move from `start` to `end` in the given `time`.
    pub fn between_positions(start: &Isometry2<N>, end: &Isometry2<N>, time: N) -> Self {
        let delta = end / start;
        let linear = delta.translation.vector / time;
        let angular = delta.rotation.angle() / time;
        Self::new(linear, angular)
    }

    /// Compute the displacement due to this velocity integrated during the time `dt`.
    pub fn integrate(&self, dt: N) -> Isometry2<N> {
        (*self * dt).to_transform()
    }

    /// Compute the displacement due to this velocity integrated during a time equal to `1.0`.
    ///
    /// This is equivalent to `self.integrate(1.0)`.
    pub fn to_transform(&self) -> Isometry2<N> {
        Isometry2::new(self.linear, self.angular)
    }

    /// This velocity seen as a slice.
    ///
    /// The linear part is stored first.
    #[inline]
    pub fn as_slice(&self) -> &[N] {
        self.as_vector().as_slice()
    }

    /// This velocity seen as a mutable slice.
    ///
    /// The linear part is stored first.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [N] {
        self.as_vector_mut().as_mut_slice()
    }

    /// This velocity seen as a vector.
    ///
    /// The linear part is stored first.
    #[inline]
    pub fn as_vector(&self) -> &Vector3<N> {
        unsafe { std::mem::transmute(self) }
    }

    /// This velocity seen as a mutable vector.
    ///
    /// The linear part is stored first.
    #[inline]
    pub fn as_vector_mut(&mut self) -> &mut Vector3<N> {
        unsafe { std::mem::transmute(self) }
    }

    /// Create a velocity from a vector.
    ///
    /// The linear part of the velocity is expected to be first inside of the input vector.
    #[inline]
    pub fn from_vector<S: Storage<N, U3>>(data: &Vector<N, U3, S>) -> Self {
        Self::new(Vector2::new(data[0], data[1]), data[2])
    }

    /// Create a velocity from a slice.
    ///
    /// The linear part of the velocity is expected to be first inside of the input slice.
    #[inline]
    pub fn from_slice(data: &[N]) -> Self {
        Self::new(Vector2::new(data[0], data[1]), data[2])
    }

    /// Compute the velocity of a point that is located at the coordinates `shift` relative to the point having `self` as velocity.
    #[inline]
    pub fn shift(&self, shift: &Vector2<N>) -> Self {
        Self::new(
            self.linear + Vector2::new(-shift.y, shift.x) * self.angular,
            self.angular,
        )
    }

    /// Rotate each component of `self` by `rot`.
    #[inline]
    pub fn rotated(&self, rot: &Rotation2<N>) -> Self {
        Self::new(rot * self.linear, self.angular)
    }

    /// Transform each component of `self` by `iso`.
    #[inline]
    pub fn transformed(&self, iso: &Isometry2<N>) -> Self {
        Self::new(iso * self.linear, self.angular)
    }
}

impl<N: RealField + Copy> Add<Velocity2<N>> for Velocity2<N> {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self {
        Velocity2::new(self.linear + rhs.linear, self.angular + rhs.angular)
    }
}

impl<N: RealField + Copy> AddAssign<Velocity2<N>> for Velocity2<N> {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        self.linear += rhs.linear;
        self.angular += rhs.angular;
    }
}

impl<N: RealField + Copy> Sub<Velocity2<N>> for Velocity2<N> {
    type Output = Self;

    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Velocity2::new(self.linear - rhs.linear, self.angular - rhs.angular)
    }
}

impl<N: RealField + Copy> SubAssign<Velocity2<N>> for Velocity2<N> {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        self.linear -= rhs.linear;
        self.angular -= rhs.angular;
    }
}

impl<N: RealField + Copy> Mul<N> for Velocity2<N> {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: N) -> Self {
        Velocity2::new(self.linear * rhs, self.angular * rhs)
    }
}

impl<N: RealField + Copy> MulAssign<N> for Velocity2<N> {
    #[inline]
    fn mul_assign(&mut self, rhs: N) {
        *self = Velocity2::new(self.linear * rhs, self.angular * rhs);
    }
}
