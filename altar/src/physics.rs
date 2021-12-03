use hv::prelude::*;
use parry3d::{bounding_volume::AABB, shape::SharedShape};

use crate::types::Float;

#[derive(Debug, Clone, Copy)]
pub struct CompositePosition3 {
    /// Translational component.
    pub translation: Vector3<Float>,
    /// Rotation around the Z axis.
    pub rotation: UnitComplex<Float>,
}

impl CompositePosition3 {
    pub fn origin() -> Self {
        Self::new(Vector3::zeros(), 0.)
    }

    pub fn new(translation: Vector3<Float>, rotation: Float) -> Self {
        Self {
            translation,
            rotation: UnitComplex::new(rotation),
        }
    }

    pub fn translation(x: Float, y: Float, z: Float) -> Self {
        Self {
            translation: Vector3::new(x, y, z),
            rotation: UnitComplex::identity(),
        }
    }

    pub fn as_isometry3(&self) -> Isometry3<Float> {
        // A quaternion from axis/angle will use `w = cos(theta/2)` and `sin(theta/2)` for the ijk
        // components. So, we sqrt the rotation to sqrt the internal complex, then extract the
        // resulting cos/sin.
        let sqrt_rot2 = self.rotation.powf(0.5);
        let quat = Quaternion::new(sqrt_rot2.cos_angle(), 0., 0., sqrt_rot2.sin_angle());

        Isometry3::from_parts(
            Translation3::from(self.translation),
            UnitQuaternion::new_unchecked(quat),
        )
    }

    pub fn transform_point(&self, pt: &Point3<Float>) -> Point3<Float> {
        let coords = self.translation + self.rotation.transform_point(&pt.xy()).coords.push(0.);
        Point3::from(coords)
    }

    pub fn transform_vector(&self, v: &Vector3<Float>) -> Vector3<Float> {
        self.rotation.transform_vector(&v.xy()).push(v.z)
    }

    pub fn lerp_slerp(&self, b: &Self, t: Float) -> Self {
        Self {
            translation: self.translation.lerp(&b.translation, t),
            rotation: self.rotation.slerp(&b.rotation, t),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CompositeVelocity3 {
    pub linear: Vector3<Float>,
    pub angular: Float,
}

impl CompositeVelocity3 {
    pub fn zero() -> Self {
        Self::new(Vector3::zeros(), 0.)
    }

    pub fn new(linear: Vector3<Float>, angular: Float) -> Self {
        Self { linear, angular }
    }

    pub fn integrate(&self, composite: &CompositePosition3, dt: Float) -> CompositePosition3 {
        CompositePosition3 {
            translation: composite.translation + self.linear * dt,
            rotation: UnitComplex::new(composite.rotation.angle() + self.angular * dt),
        }
    }
}

/// A position component.
///
/// Contains the current position/orientation and previous position/orientation of the object.
///
/// This current/previous setup allows us to interpolate between positions during rendering, which
/// in turn allows us to get smooth movement when locking our physics/mechanics updates to a
/// specific rate but rendering as fast as possible.
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
        dt: Float,
    ) -> Self {
        let current = position;
        let previous = velocity.integrate(&current, -dt);
        Self { current, previous }
    }

    /// Interpolate between the previous and current positions. This is useful when rendering w/ a
    /// fixed timestep and uncapped/higher than timestep render rate.
    pub fn lerp_slerp(&self, t: Float) -> CompositePosition3 {
        self.previous.lerp_slerp(&self.current, t)
    }

    /// Perform an integration step.
    pub fn integrate(&mut self, velocity: &CompositeVelocity3, dt: Float) {
        self.previous = self.current;
        self.current = velocity.integrate(&self.current, dt);
    }
}

/// A velocity component.
///
/// Stores the "composite" velocity of an object (full 2D linear/angular velocity, plus a linear Z
/// velocity.)
#[derive(Debug, Clone, Copy)]
pub struct Velocity {
    pub composite: CompositeVelocity3,
}

/// A collider component.
///
/// Stores an offset and a shape for collision testing. Since it's a component, you can only have
/// one per entity; so if you want multiple shapes attached, you'll need to use a [`Compound`]
/// shape.
///
/// [`Compound`]: parry3d::shape::Compound
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

impl LuaUserData for CompositePosition3 {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().add_send().add_sync();
    }
}

impl LuaUserData for CompositeVelocity3 {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().add_send().add_sync();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, (linear, angular)| Ok(Self::new(linear, angular)));
        methods.add_function("zero", |_, ()| Ok(Self::zero()));
    }
}

impl LuaUserData for Position {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }
}

impl LuaUserData for Velocity {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("set", |_, this, composite: CompositeVelocity3| {
            this.composite = composite;
            Ok(())
        });
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
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
