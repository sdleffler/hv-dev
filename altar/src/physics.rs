use hv::prelude::*;
use parry3d::{bounding_volume::AABB, shape::SharedShape};

use crate::types::Float;

#[derive(Debug, Clone, Copy)]
pub struct CompositePosition3 {
    pub xy: Isometry2<Float>,
    pub z: Float,
}

impl CompositePosition3 {
    pub fn new(translation: Vector3<Float>, rotation: Float) -> Self {
        Self {
            xy: Isometry2::new(translation.xy(), rotation),
            z: translation.z,
        }
    }

    pub fn as_isometry3(&self) -> Isometry3<Float> {
        let translation = self.xy.translation.vector.push(self.z);

        // A quaternion from axis/angle will use `w = cos(theta/2)` and `sin(theta/2)` for the ijk
        // components. So, we sqrt the rotation to sqrt the internal complex, then extract the
        // resulting cos/sin.
        let sqrt_rot2 = self.xy.rotation.powf(0.5);
        let quat = Quaternion::new(sqrt_rot2.cos_angle(), 0., 0., sqrt_rot2.sin_angle());

        Isometry3::from_parts(
            Translation3::from(translation),
            UnitQuaternion::new_unchecked(quat),
        )
    }

    pub fn transform_point(&self, pt: &Point3<Float>) -> Point3<Float> {
        Point3::from(self.xy.transform_point(&pt.xy()).coords.push(self.z + pt.z))
    }

    pub fn transform_vector(&self, v: &Vector3<Float>) -> Vector3<Float> {
        self.xy.transform_vector(&v.xy()).push(v.z)
    }

    pub fn lerp_slerp(&self, b: &Self, t: f32) -> Self {
        Self {
            xy: self.xy.lerp_slerp(&b.xy, t),
            z: self.z + t * (b.z - self.z),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompositeVelocity3 {
    pub xy: Velocity2<Float>,
    pub z: Float,
}

impl CompositeVelocity3 {
    pub fn new(linear: Vector3<Float>, angular: Float) -> Self {
        Self {
            xy: Velocity2::new(linear.xy(), angular),
            z: linear.z,
        }
    }

    pub fn integrate(&self, position: &CompositePosition3, dt: Float) -> CompositePosition3 {
        let xy = self.xy.integrate(dt);
        let z = self.z * dt;
        CompositePosition3 {
            xy: Isometry2::from_parts(
                position.xy.translation * xy.translation,
                position.xy.rotation * xy.rotation,
            ),
            z: position.z + z,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Position {
    /// The current position of the entity.
    pub current: CompositePosition3,
    /// The last position of the entity.
    pub previous: CompositePosition3,
}

impl Position {
    pub fn new(position: CompositePosition3) -> Self {
        Self {
            current: position,
            previous: position,
        }
    }

    /// Create a new position "out of sync" with the current frame. Useful when responding to
    /// input/performing spawning actions at a different rate/with nonzero remaining dt with respect
    /// to the physics timestep. This will spawn the entity at the position specified but with the
    /// *previous* position set to the current integration integrated by `velocity * -dt` (back by
    /// one step), so that it ends up being rendered at the correct spot.
    pub fn new_out_of_sync(
        position: CompositePosition3,
        velocity: &CompositeVelocity3,
        dt: f32,
    ) -> Self {
        let current = position;
        let previous = velocity.integrate(&current, -dt);
        Self { current, previous }
    }

    /// Interpolate between the previous and current positions. This is useful when rendering w/ a
    /// fixed timestep and uncapped/higher than timestep render rate.
    pub fn lerp_slerp(&self, t: f32) -> CompositePosition3 {
        self.previous.lerp_slerp(&self.current, t)
    }

    /// Perform an integration step.
    pub fn integrate(&mut self, velocity: &CompositeVelocity3, dt: f32) {
        self.previous = self.current;
        self.current = velocity.integrate(&self.current, dt);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Velocity {
    pub composite: CompositeVelocity3,
}

#[derive(Clone)]
pub struct Collider {
    pub local_tx: Isometry3<Float>,
    pub shape: SharedShape,
}

impl Collider {
    pub fn new(local_tx: Isometry3<Float>, shape: SharedShape) -> Self {
        Self { local_tx, shape }
    }

    pub fn compute_aabb(&self, tx: &Isometry3<Float>) -> AABB {
        self.shape.compute_aabb(&(tx * self.local_tx))
    }
}

impl LuaUserData for Collider {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component();
    }

    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("local_tx", |_, this| Ok(this.local_tx));
        fields.add_field_method_set("local_tx", |_, this, local_tx| Ok(this.local_tx = local_tx));
        fields.add_field_method_get("shape", |_, this| Ok(this.shape.clone()));
        fields.add_field_method_set("shape", |_, this, shape| Ok(this.shape = shape));
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, (local_tx, shape)| Ok(Self::new(local_tx, shape)));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::f32;

    #[test]
    fn composite_position_as_isometry3() {
        let composite = CompositePosition3 {
            xy: Isometry2::new(Vector2::new(5., -2.3), f32::consts::FRAC_2_PI),
            z: 4.,
        };

        let axisangle = Vector3::z() * f32::consts::FRAC_2_PI;
        let translation = Vector3::new(5., -2.3, 4.);
        let isometry3 = Isometry3::new(translation, axisangle);

        let pt = Point3::new(-23.6, 13., -42.);
        let d = na::distance(
            &composite.as_isometry3().transform_point(&pt),
            &isometry3.transform_point(&pt),
        );
        assert!(d < f32::EPSILON, "bad!");
    }
}
