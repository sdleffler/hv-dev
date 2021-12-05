use std::{
    collections::{hash_map::Entry, HashMap},
    ops::{Add, AddAssign},
};

use hv::{
    ecs::{ColumnMut, Entity, PreparedQuery, QueryMarker, SystemContext, With},
    prelude::*,
};
use parry3d::{bounding_volume::AABB, partitioning::QBVH, shape::SharedShape};
use soft_edge::SortedPair;

use crate::{
    lattice::atom_map::AtomMap,
    types::{Dt, Float, Tick},
};

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

#[derive(Debug, Clone, Copy)]
pub struct PhysicsConfig {
    /// Allowed overlap between objects. Default value is `0.1`.
    pub position_slop: f32,
    /// In the approximate range of `[0.1, 0.3]`. Default value is `0.3`.
    pub bias_factor: f32,
    /// Number of iterations to use when correcting velocities/solving velocity constraints. The
    /// higher the number, the more accurate, but more computationally intensive. Default value is
    /// `8`.
    pub velocity_iterations: u32,
    /// Number of iterations to use when correcting positions/solving position constraints. The
    /// higher the number, the more accurate, but more computationally intensive. Velocity
    /// constraints include some position correction bias as well, so we can get away with less
    /// iterations here. Default value is `3`.
    pub position_iterations: u32,
    /// Acceleration due to gravity. Default value is zero.
    pub gravity: Vector3<f32>,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            position_slop: 0.1,
            bias_factor: 0.3,
            velocity_iterations: 8,
            position_iterations: 3,
            gravity: Vector3::zeros(),
        }
    }
}

/// Physics information and solver state.
///
/// Contains mass data as well as intermediate states used by the solver.
///
/// While a `Physics` *does* hold some position/velocity info, this is not actually the current
/// state of the position/velocity/whatnot of an object; rather, it holds state used by the physics
/// constraint solver. This includes intermediate values for the position/velocity of the object as
/// updated by the constraint solver during iteration.
#[derive(Clone)]
pub struct Physics {
    position: CompositePosition3,
    velocity: CompositeVelocity3,
    mass_data: MassData,

    restitution: f32,
    static_friction: f32,
    dynamic_friction: f32,

    collider_tx: Isometry3<f32>,
    collider_shape: SharedShape,
}

impl Physics {
    pub fn new(collider_shape: SharedShape) -> Self {
        Self::with_local_tx(collider_shape, Isometry3::identity())
    }

    pub fn with_local_tx(collider_shape: SharedShape, collider_tx: Isometry3<f32>) -> Self {
        Self {
            position: CompositePosition3::origin(),
            velocity: CompositeVelocity3::zero(),
            mass_data: MassData { inv_mass: 0. },
            restitution: 0.,
            static_friction: 0.,
            dynamic_friction: 0.,
            collider_shape,
            collider_tx,
        }
    }

    pub fn with_density(self, density: f32) -> Self {
        let mass_data = if density == 0. {
            MassData { inv_mass: 0. }
        } else {
            MassData {
                inv_mass: self.collider_shape.mass_properties(density).inv_mass,
            }
        };

        Self { mass_data, ..self }
    }

    pub fn with_friction(self, friction: f32) -> Self {
        Self {
            static_friction: friction,
            dynamic_friction: friction,
            ..self
        }
    }

    pub fn with_static_friction(self, static_friction: f32) -> Self {
        Self {
            static_friction,
            ..self
        }
    }

    pub fn with_dynamic_friction(self, dynamic_friction: f32) -> Self {
        Self {
            dynamic_friction,
            ..self
        }
    }

    pub fn with_restitution(self, restitution: f32) -> Self {
        Self {
            restitution,
            ..self
        }
    }
}

impl LuaUserData for Physics {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component();
    }

    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("static_friction", |_, this| Ok(this.static_friction));
        fields.add_field_method_get("dynamic_friction", |_, this| Ok(this.dynamic_friction));
        fields.add_field_method_set("static_friction", |_, this, static_friction| {
            this.static_friction = static_friction;
            Ok(())
        });
        fields.add_field_method_set("dynamic_friction", |_, this, dynamic_friction| {
            this.dynamic_friction = dynamic_friction;
            Ok(())
        });
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut(
            "set_friction",
            |_, this, (static_friction, dynamic_friction): (f32, Option<f32>)| {
                let dynamic_friction = dynamic_friction.unwrap_or(static_friction);
                this.static_friction = static_friction;
                this.dynamic_friction = dynamic_friction;
                Ok(())
            },
        );
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MassData {
    /// 1/M. If zero, the body has infinite mass.
    pub inv_mass: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum ConstrainedPair {
    Dynamic(SortedPair<Entity>),
    Static(Entity, Vector3<i32>, u32),
}

#[derive(Debug, Clone, Copy)]
pub struct Contact {
    pub normal: UnitVector3<f32>,
    pub tangent1: UnitVector3<f32>,
    pub tangent2: UnitVector3<f32>,
    pub p1: Point3<f32>,
    pub p2: Point3<f32>,
}

impl Contact {
    pub fn new(normal: UnitVector3<f32>, p1: Point3<f32>, p2: Point3<f32>) -> Self {
        // what the fuck Erin Catto?? ok where *is* an explanation for this. i need to find one.
        // this is supposedly a robust method for finding an orthonormal basis for a contact normalo
        // in order to calculate tangent vectors.
        let t1_scaled = if normal.x.abs() >= 0.57735 {
            Vector3::new(normal.y, -normal.x, 0.)
        } else {
            Vector3::new(0., normal.z, -normal.y)
        };

        let tangent1 = UnitVector3::new_normalize(t1_scaled);
        let tangent2 = UnitVector3::new_unchecked(normal.cross(&tangent1));

        Self {
            normal,
            tangent1,
            tangent2,
            p1,
            p2,
        }
    }

    /// Positive if the objects are overlapping.
    pub fn compute_overlap(&self) -> f32 {
        -(self.p2 - self.p1).dot(&self.normal)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContactConstraint {
    pub contact: Contact,
    /// "Lambda" value, used for velocity correction.
    pub normal_impulse: f32,
    pub tangent_impulses: Vector2<f32>,
    /// Pseudo-impulse, used for position correction w/ pseudo-velocities.
    pub pseudo_impulse: f32,
    pub timestamp: u64,
}

impl ContactConstraint {
    pub fn new(contact: Contact) -> Self {
        Self {
            contact,
            normal_impulse: 0.,
            tangent_impulses: Vector2::zeros(),
            pseudo_impulse: 0.,
            timestamp: 0,
        }
    }

    pub fn compute_bias_velocity(&self, config: &PhysicsConfig) -> f32 {
        let overlap = self.contact.compute_overlap();
        (overlap - config.position_slop).max(0.) * config.bias_factor
    }

    /// Compute the required impulse to satisfy the constraint.
    pub fn compute_normal_impulse(
        &self,
        a: &Physics,
        b: Option<&Physics>,
        config: &PhysicsConfig,
    ) -> f32 {
        let delta_v =
            b.map(|b| b.velocity.linear).unwrap_or_else(Vector3::zeros) - a.velocity.linear;
        let k_n = a.mass_data.inv_mass + b.map(|b| b.mass_data.inv_mass).unwrap_or(0.);
        let v_bias = self.compute_bias_velocity(config);
        let e = b.map_or(a.restitution, |b| a.restitution.min(b.restitution));

        (-(1. + e) * delta_v.dot(&self.contact.normal) + v_bias) / k_n
    }

    pub fn compute_tangent_impulses(&self, a: &Physics, b: Option<&Physics>) -> Vector2<f32> {
        let delta_v =
            b.map(|b| b.velocity.linear).unwrap_or_else(Vector3::zeros) - a.velocity.linear;
        let k_n = a.mass_data.inv_mass + b.map(|b| b.mass_data.inv_mass).unwrap_or(0.);

        Vector2::new(
            -delta_v.dot(&self.contact.tangent1) / k_n,
            -delta_v.dot(&self.contact.tangent2) / k_n,
        )
    }

    /// Compute the required pseudo-impulse to satisfy the constraint.
    pub fn compute_pseudo_impulse(
        &self,
        a: &Physics,
        b: Option<&Physics>,
        config: &PhysicsConfig,
    ) -> f32 {
        let k_n = a.mass_data.inv_mass + b.map(|b| b.mass_data.inv_mass).unwrap_or(0.);
        let v_bias = self.compute_bias_velocity(config);

        -v_bias / k_n
    }

    pub fn apply_normal_impulse(&self, delta: f32, a: &mut Physics, b: Option<&mut Physics>) {
        let delta_av = -self.contact.normal.into_inner() * a.mass_data.inv_mass * delta;
        a.velocity.linear += delta_av;

        if let Some(b) = b {
            let delta_bv = self.contact.normal.into_inner() * b.mass_data.inv_mass * delta;
            b.velocity.linear += delta_bv;
        }
    }

    pub fn apply_tangent_impulses(
        &self,
        delta: Vector2<f32>,
        a: &mut Physics,
        b: Option<&mut Physics>,
    ) {
        let delta_av1 = -self.contact.tangent1.into_inner() * a.mass_data.inv_mass * delta.x;
        let delta_av2 = -self.contact.tangent2.into_inner() * a.mass_data.inv_mass * delta.y;
        a.velocity.linear += delta_av1 + delta_av2;

        if let Some(b) = b {
            let delta_bv1 = self.contact.tangent1.into_inner() * b.mass_data.inv_mass * delta.x;
            let delta_bv2 = self.contact.tangent2.into_inner() * b.mass_data.inv_mass * delta.y;
            b.velocity.linear += delta_bv1 + delta_bv2;
        }
    }

    pub fn apply_pseudo_impulse(
        &self,
        delta: f32,
        a: &mut Physics,
        b: Option<&mut Physics>,
        dt: &Dt,
    ) {
        let delta_av = -self.contact.normal.into_inner() * a.mass_data.inv_mass * delta * dt.0;
        a.position.translation += delta_av;

        if let Some(b) = b {
            let delta_bv = self.contact.normal.into_inner() * b.mass_data.inv_mass * delta * dt.0;
            b.position.translation += delta_bv;
        }
    }
}

pub struct PhysicsPipeline {
    qbvh: QBVH<u32>,
    constraints: HashMap<ConstrainedPair, ContactConstraint>,
}

impl Default for PhysicsPipeline {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsPipeline {
    pub fn new() -> Self {
        Self {
            qbvh: QBVH::new(),
            constraints: HashMap::new(),
        }
    }

    pub fn solve_velocities(&mut self, physics: &mut ColumnMut<Physics>, config: &PhysicsConfig) {
        // Warm start.
        for (pair, constraint) in &mut self.constraints {
            let (a, mut b) = match *pair {
                ConstrainedPair::Dynamic(pair) => {
                    let ma_ptr = physics.get(pair.0).unwrap() as *mut _;
                    let mb_ptr = physics.get(pair.1).unwrap() as *mut _;
                    unsafe { (&mut *ma_ptr, Some(&mut *mb_ptr)) }
                }
                ConstrainedPair::Static(a, _, _) => (physics.get(a).unwrap(), None),
            };

            constraint.apply_normal_impulse(constraint.normal_impulse, a, b.as_deref_mut());
            constraint.apply_tangent_impulses(constraint.tangent_impulses, a, b);
        }

        for _ in 0..config.velocity_iterations {
            for (pair, constraint) in &mut self.constraints {
                let (a, mut b) = match *pair {
                    ConstrainedPair::Dynamic(pair) => {
                        let ma_ptr = physics.get(pair.0).unwrap() as *mut _;
                        let mb_ptr = physics.get(pair.1).unwrap() as *mut _;
                        unsafe { (&mut *ma_ptr, Some(&mut *mb_ptr)) }
                    }
                    ConstrainedPair::Static(a, _, _) => (physics.get(a).unwrap(), None),
                };

                let corrective = constraint.compute_normal_impulse(a, b.as_deref(), config);
                let old = constraint.normal_impulse;
                constraint.normal_impulse += corrective;
                constraint.normal_impulse = constraint.normal_impulse.max(0.);
                let delta = constraint.normal_impulse - old;
                constraint.apply_normal_impulse(delta, a, b.as_deref_mut());

                let corrective = constraint.compute_tangent_impulses(a, b.as_deref());
                let old = constraint.tangent_impulses;
                constraint.tangent_impulses += corrective;

                assert!(!constraint.tangent_impulses.norm_squared().is_nan());

                let jt = constraint.tangent_impulses.norm_squared();
                let static_mu = b.as_deref().map_or(a.static_friction, |b| {
                    a.static_friction.hypot(b.static_friction)
                });
                if jt > 0. && jt >= (constraint.normal_impulse * static_mu).powi(2) {
                    let dynamic_mu = b.as_deref().map_or(a.dynamic_friction, |b| {
                        a.dynamic_friction.hypot(b.dynamic_friction)
                    });
                    constraint.tangent_impulses = constraint.tangent_impulses.normalize()
                        * constraint.normal_impulse
                        * dynamic_mu;
                }

                let delta = constraint.tangent_impulses - old;
                constraint.apply_tangent_impulses(delta, a, b);
            }
        }
    }

    pub fn solve_positions(
        &mut self,
        physics: &mut ColumnMut<Physics>,
        config: &PhysicsConfig,
        dt: &Dt,
    ) {
        for constraint in self.constraints.values_mut() {
            // No warm starting for position constraints.
            constraint.pseudo_impulse = 0.;
        }

        for _ in 0..config.position_iterations {
            for (pair, constraint) in &mut self.constraints {
                let (a, b) = match *pair {
                    ConstrainedPair::Dynamic(pair) => {
                        let ma_ptr = physics.get(pair.0).unwrap() as *mut _;
                        let mb_ptr = physics.get(pair.1).unwrap() as *mut _;
                        unsafe { (&mut *ma_ptr, Some(&mut *mb_ptr)) }
                    }
                    ConstrainedPair::Static(a, _, _) => (physics.get(a).unwrap(), None),
                };

                let corrective = constraint.compute_pseudo_impulse(a, b.as_deref(), config);
                let old = constraint.pseudo_impulse;
                constraint.pseudo_impulse += corrective;
                constraint.pseudo_impulse = constraint.pseudo_impulse.max(0.);
                let delta = constraint.pseudo_impulse - old;
                constraint.apply_pseudo_impulse(delta, a, b, dt);
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update(
    context: SystemContext,
    (dt, tick, atom_map, pipeline, physics_config): (
        &Dt,
        &Tick,
        &AtomMap,
        &mut PhysicsPipeline,
        &PhysicsConfig,
    ),
    (
        ref mut physics_query,
        ref mut with_physics_query,
        physics_query_marker,
        ref mut physics_aux_query,
    ): &mut (
        PreparedQuery<&mut Physics>,
        PreparedQuery<With<Physics, ()>>,
        QueryMarker<&mut Physics>,
        PreparedQuery<(&mut Position, &mut Velocity, &mut Physics)>,
    ),
) {
    // Copy physics data from the ECS.
    for (_, (pos, vel, physics)) in context.prepared_query(physics_aux_query).iter() {
        physics.position = pos.current;
        physics.velocity = vel.composite;
    }

    // Rebuild the quadtree.
    pipeline.qbvh.clear_and_rebuild(
        context
            .prepared_query(physics_query)
            .iter()
            .map(|(e, physics)| {
                (
                    e.id(),
                    physics
                        .collider_shape
                        .compute_aabb(&(physics.collider_tx * physics.position.as_isometry3())),
                )
            }),
        0.01,
    );

    // Detect dynamic-dynamic collisions, collecting constraints.
    {
        let physics_column_mut = context.column_mut(*physics_query_marker);
        let mut out = Vec::new();
        for (e1, ()) in context.prepared_query(with_physics_query).iter() {
            let p1 = unsafe { physics_column_mut.get_unchecked(e1).unwrap() };
            let pos1 = p1.collider_tx * p1.position.as_isometry3();
            let aabb = p1.collider_shape.compute_aabb(&pos1);
            pipeline.qbvh.intersect_aabb(&aabb, &mut out);

            for id in out.drain(..) {
                if id == e1.id() {
                    continue;
                }

                let (e2, p2) = unsafe {
                    let e2 = context.find_entity_from_id(id);
                    let p2 = physics_column_mut.get_unchecked(e2).unwrap();
                    (e2, p2)
                };

                let pos2 = p2.collider_tx * p2.position.as_isometry3();
                let s1 = p1.collider_shape.as_ref();
                let s2 = p2.collider_shape.as_ref();

                if let Some(mut c) = parry3d::query::contact(&pos1, s1, &pos2, s2, 0.1).unwrap() {
                    let sorted_pair = SortedPair::new(e1, e2);
                    let pair = ConstrainedPair::Dynamic(sorted_pair);

                    if sorted_pair.0 != e1 {
                        c.flip();
                    }

                    let contact = Contact::new(c.normal1, c.point1, c.point2);
                    let constraint = match pipeline.constraints.entry(pair) {
                        Entry::Vacant(vacant) => vacant.insert(ContactConstraint::new(contact)),
                        Entry::Occupied(occupied) => {
                            let mut_constraint = occupied.into_mut();
                            mut_constraint.contact = contact;
                            mut_constraint
                        }
                    };

                    constraint.timestamp = tick.0;
                }
            }
        }
    }
    // Integrate velocities w/ external forces
    // TODO: add gravity component and integrate here. For now, HAAAAAACK!
    for (_, physics) in context.prepared_query(physics_query).iter() {
        physics.velocity.linear += physics_config.gravity * dt.0;
    }

    // Solve likely-violated velocity constraints
    pipeline.solve_velocities(
        &mut context.column_mut(*physics_query_marker),
        physics_config,
    );

    // Integrate positions w/ newly solved velocities
    for (_, physics) in context.prepared_query(physics_query).iter() {
        physics.position = physics.velocity.integrate(&physics.position, dt.0);
    }

    // Detect dynamic-static collisions, collecting position and velocity constraints
    let mut out = Vec::new();
    for (e, physics) in context.prepared_query(physics_query).iter() {
        let pos_tx = physics.collider_tx * physics.position.as_isometry3();
        let aabb = physics.collider_shape.compute_aabb(&pos_tx);
        for intersection in atom_map.intersect_with(aabb) {
            intersection.shape.contact(
                &intersection.coords,
                physics.collider_shape.as_ref(),
                &pos_tx,
                0.1,
                atom_map.edge_filter(),
                atom_map.vertex_filter(),
                &mut out,
            );

            for (mut c, feature) in out.drain(..) {
                c.flip();
                let pair = ConstrainedPair::Static(e, intersection.coords, feature);
                let contact = Contact::new(c.normal1, c.point1, c.point2);
                let constraint = match pipeline.constraints.entry(pair) {
                    Entry::Vacant(vacant) => vacant.insert(ContactConstraint::new(contact)),
                    Entry::Occupied(occupied) => {
                        let mut_constraint = occupied.into_mut();
                        mut_constraint.contact = contact;
                        mut_constraint
                    }
                };

                constraint.timestamp = tick.0;
            }
        }
    }

    // Solve position constraints
    pipeline.solve_positions(
        &mut context.column_mut(*physics_query_marker),
        physics_config,
        dt,
    );

    pipeline
        .constraints
        .retain(|_, constraint| constraint.timestamp == tick.0);

    // Copy physics data back to the ECS.
    for (_, (pos, vel, physics)) in context.prepared_query(physics_aux_query).iter() {
        pos.current = physics.position;
        vel.composite = physics.velocity;
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

    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, this| Ok(this.linear.x));
        fields.add_field_method_get("y", |_, this| Ok(this.linear.y));
        fields.add_field_method_get("z", |_, this| Ok(this.linear.z));
        fields.add_field_method_get("linear", |_, this| Ok(this.linear));
        fields.add_field_method_get("angular", |_, this| Ok(this.angular));
        fields.add_field_method_set("x", |_, this, x| Ok(this.linear.x = x));
        fields.add_field_method_set("y", |_, this, y| Ok(this.linear.y = y));
        fields.add_field_method_set("z", |_, this, z| Ok(this.linear.z = z));
        fields.add_field_method_set("linear", |_, this, linear| Ok(this.linear = linear));
        fields.add_field_method_set("angular", |_, this, angular| Ok(this.angular = angular));
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
        methods.add_method_mut("get", |_, this, out: Option<LuaAnyUserData>| match out {
            Some(ud) => {
                *ud.borrow_mut::<CompositeVelocity3>()? = this.composite;
                Ok(None)
            }
            None => Ok(Some(this.composite)),
        });

        methods.add_method_mut("set", |_, this, composite: CompositeVelocity3| {
            this.composite = composite;
            Ok(())
        });

        methods.add_method_mut("add", |_, this, composite: CompositeVelocity3| {
            this.composite += composite;
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

impl Add for CompositeVelocity3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            linear: self.linear + rhs.linear,
            angular: self.angular + rhs.angular,
        }
    }
}

impl AddAssign for CompositeVelocity3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
