use std::{
    collections::{hash_map::Entry, HashMap},
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

use hv::{
    ecs::{ColumnMut, Entity, PreparedQuery, QueryMarker, Satisfies, SystemContext, With, Without},
    elastic::{ElasticMut, ElasticRef},
    prelude::*,
    resources::Resources,
};
use parry3d::{
    bounding_volume::{BoundingVolume, AABB},
    partitioning::QBVH,
    query::TOI,
    shape::SharedShape,
};
use shrev::{EventChannel, ReaderId};
use soft_edge::SortedPair;
use thunderdome::{Arena, Index};

use crate::{
    lattice::atom_map::AtomMap,
    types::{Float, UpdateDt, UpdateTick},
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
    /// Allowed overlap between objects. Default value is `0.01`.
    pub position_slop: f32,
    /// In the approximate range of `[0.1, 0.3]`. Default value is `0.1`.
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
    /// Acceleration due to gravity. Default value is the zero vector.
    pub gravity: Vector3<f32>,
    /// The continuous collision detection velocity threshold. Bodies with a velocity lower than
    /// this will not have CCD performed, even if their CCD flags are set. Default value is `20.0`.
    pub continuous_collision_velocity_threshold: f32,
    /// TOI bias is a tiny value added to the calculated time-of-impact for bodies w/ CCD enabled.
    /// It ensures that the newly motion-clamped location is (likely) slightly impacting its
    /// destination, so that it will be noticed by the subsequent discrete collision detection pass.
    ///
    /// Default: `0.1`.
    pub continuous_collision_toi_bias: f32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            position_slop: 0.01,
            bias_factor: 0.1,
            velocity_iterations: 8,
            position_iterations: 3,
            gravity: Vector3::zeros(),
            continuous_collision_velocity_threshold: 20.0,
            continuous_collision_toi_bias: 0.1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContactIdEntry {
    pub flipped: bool,
    pub id: ContactId,
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
    target: CompositePosition3,
    velocity: CompositeVelocity3,
    mass_data: MassData,

    restitution: f32,
    static_friction: f32,
    dynamic_friction: f32,
    gravity_k: f32,

    collider_tx: Isometry3<f32>,
    collider_shape: SharedShape,

    contacts: Vec<ContactIdEntry>,
}

impl Physics {
    pub fn new(collider_shape: SharedShape) -> Self {
        Self::with_local_tx(collider_shape, Isometry3::identity())
    }

    pub fn with_local_tx(collider_shape: SharedShape, collider_tx: Isometry3<f32>) -> Self {
        Self {
            position: CompositePosition3::origin(),
            target: CompositePosition3::origin(),
            velocity: CompositeVelocity3::zero(),
            mass_data: MassData { inv_mass: 0. },
            restitution: 0.,
            static_friction: 0.,
            dynamic_friction: 0.,
            gravity_k: 1.,
            collider_shape,
            collider_tx,
            contacts: Vec::new(),
        }
        .with_density(1.)
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

    pub fn with_mass(self, mass: f32) -> Self {
        let mass_data = if mass == 0. {
            MassData { inv_mass: 0. }
        } else {
            MassData {
                inv_mass: mass.recip(),
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

    fn remove_contact(&mut self, contact_id: ContactId) {
        let i = self
            .contacts
            .iter()
            .position(|&entry| entry.id == contact_id)
            .unwrap();
        self.contacts.swap_remove(i);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MassData {
    /// 1/M. If zero, the body has infinite mass.
    pub inv_mass: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CcdEnabled;

#[derive(Debug, Clone, Copy)]
pub struct KinematicMarker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum ConstrainedPair {
    Dynamic(SortedPair<Entity>),
    Static(Entity, Vector3<i32>, u32),
}

impl ConstrainedPair {
    pub fn get_participant(self, flipped: bool) -> Entity {
        match self {
            Self::Dynamic(pair) if !flipped => pair.0,
            Self::Dynamic(pair) => pair.1,
            Self::Static(e, _, _) => e,
        }
    }
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
    pub participants: ConstrainedPair,
    pub contact: Contact,
    /// "Lambda" value, used for velocity correction.
    pub normal_impulse: f32,
    pub tangent_impulses: Vector2<f32>,
    /// Pseudo-impulse, used for position correction w/ pseudo-velocities.
    pub pseudo_impulse: f32,
    pub timestamp: u64,
}

impl ContactConstraint {
    pub fn new(participants: ConstrainedPair, contact: Contact) -> Self {
        Self {
            participants,
            contact,
            normal_impulse: 0.,
            tangent_impulses: Vector2::zeros(),
            pseudo_impulse: 0.,
            timestamp: 0,
        }
    }

    pub fn compute_bias_velocity(&self, config: &PhysicsConfig, dt: &UpdateDt) -> f32 {
        let overlap = self.contact.compute_overlap();
        (overlap - config.position_slop).max(0.) * config.bias_factor / dt.0
    }

    /// Compute the required impulse to satisfy the constraint.
    pub fn compute_normal_impulse(
        &self,
        a: &Physics,
        b: Option<&Physics>,
        config: &PhysicsConfig,
        dt: &UpdateDt,
    ) -> f32 {
        let delta_v =
            b.map(|b| b.velocity.linear).unwrap_or_else(Vector3::zeros) - a.velocity.linear;
        let k_n = a.mass_data.inv_mass + b.map(|b| b.mass_data.inv_mass).unwrap_or(0.);
        let v_bias = self.compute_bias_velocity(config, dt);
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
        dt: &UpdateDt,
    ) -> f32 {
        let k_n = a.mass_data.inv_mass + b.map(|b| b.mass_data.inv_mass).unwrap_or(0.);
        let v_bias = self.compute_bias_velocity(config, dt);

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

    pub fn apply_pseudo_impulse(&self, delta: f32, a: &mut Physics, b: Option<&mut Physics>) {
        let delta_av = -self.contact.normal.into_inner() * a.mass_data.inv_mass * delta;
        a.position.translation += delta_av;

        if let Some(b) = b {
            let delta_bv = self.contact.normal.into_inner() * b.mass_data.inv_mass * delta;
            b.position.translation += delta_bv;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactId(Index);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum PhysicsEvent {
    BeginContact(ContactId),
    EndContact(ContactId, ConstrainedPair),
}

pub struct PhysicsPipeline {
    qbvh: QBVH<u32>,
    constraints: Arena<ContactConstraint>,
    contacts: HashMap<ConstrainedPair, ContactId>,
    events: EventChannel<PhysicsEvent>,

    pub config: PhysicsConfig,
}

impl Default for PhysicsPipeline {
    fn default() -> Self {
        Self::new(PhysicsConfig::default())
    }
}

impl PhysicsPipeline {
    pub fn new(config: PhysicsConfig) -> Self {
        Self {
            qbvh: QBVH::new(),
            constraints: Arena::new(),
            contacts: HashMap::new(),
            config,
            events: EventChannel::new(),
        }
    }

    pub fn solve_velocities(&mut self, physics: &mut ColumnMut<Physics>, dt: &UpdateDt) {
        // Warm start.
        for (_, constraint) in &mut self.constraints {
            let (a, mut b) = match constraint.participants {
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

        for _ in 0..self.config.velocity_iterations {
            for (_, constraint) in &mut self.constraints {
                let (a, mut b) = match constraint.participants {
                    ConstrainedPair::Dynamic(pair) => {
                        let ma_ptr = physics.get(pair.0).unwrap() as *mut _;
                        let mb_ptr = physics.get(pair.1).unwrap() as *mut _;
                        unsafe { (&mut *ma_ptr, Some(&mut *mb_ptr)) }
                    }
                    ConstrainedPair::Static(a, _, _) => (physics.get(a).unwrap(), None),
                };

                let corrective =
                    constraint.compute_normal_impulse(a, b.as_deref(), &self.config, dt);
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

    pub fn solve_positions(&mut self, physics: &mut ColumnMut<Physics>, dt: &UpdateDt) {
        for (_, constraint) in &mut self.constraints {
            // No warm starting for position constraints.
            constraint.pseudo_impulse = 0.;
        }

        for _ in 0..self.config.position_iterations {
            for (_, constraint) in &mut self.constraints {
                let (a, b) = match constraint.participants {
                    ConstrainedPair::Dynamic(pair) => {
                        let ma_ptr = physics.get(pair.0).unwrap() as *mut _;
                        let mb_ptr = physics.get(pair.1).unwrap() as *mut _;
                        unsafe { (&mut *ma_ptr, Some(&mut *mb_ptr)) }
                    }
                    ConstrainedPair::Static(a, _, _) => (physics.get(a).unwrap(), None),
                };

                let corrective =
                    constraint.compute_pseudo_impulse(a, b.as_deref(), &self.config, dt);
                let old = constraint.pseudo_impulse;
                constraint.pseudo_impulse += corrective;
                constraint.pseudo_impulse = constraint.pseudo_impulse.max(0.);
                let delta = constraint.pseudo_impulse - old;
                constraint.apply_pseudo_impulse(delta, a, b);
            }
        }
    }

    pub fn contact(&self, contact_id: ContactId) -> Option<&ContactConstraint> {
        self.constraints.get(contact_id.0)
    }

    pub fn register_reader(&mut self) -> ReaderId<PhysicsEvent> {
        self.events.register_reader()
    }

    pub fn events(&self) -> &EventChannel<PhysicsEvent> {
        &self.events
    }
}

pub type DynamicBody<Q> = With<Velocity, Without<KinematicMarker, Q>>;

#[allow(clippy::type_complexity)]
pub fn update(
    context: SystemContext,
    (dt, tick, atom_map, pipeline): (&UpdateDt, &UpdateTick, &AtomMap, &mut PhysicsPipeline),
    (
        ref mut all_colliders_query,
        ref mut dynamic_objects_query,
        ref mut with_physics_and_dynamic_query,
        physics_query_marker,
        ref mut update_target_pos_query,
        ref mut motion_clamping_query,
        ref mut all_physics_objects_query,
    ): &mut (
        PreparedQuery<&mut Physics>,
        PreparedQuery<DynamicBody<&mut Physics>>,
        PreparedQuery<DynamicBody<With<Physics, ()>>>,
        QueryMarker<&mut Physics>,
        PreparedQuery<With<Velocity, (&mut Physics, Satisfies<&CcdEnabled>)>>,
        PreparedQuery<DynamicBody<With<CcdEnabled, With<Physics, ()>>>>,
        PreparedQuery<(&mut Position, Option<&mut Velocity>, &mut Physics)>,
    ),
) {
    // Copy physics data from the ECS.
    for (_, (pos, maybe_vel, physics)) in context.prepared_query(all_physics_objects_query).iter() {
        physics.position = pos.current;

        if let Some(vel) = maybe_vel {
            physics.velocity = vel.composite;
        } else {
            physics.velocity = CompositeVelocity3::zero();
        }
    }

    // Rebuild the quadtree.
    pipeline.qbvh.clear_and_rebuild(
        context
            .prepared_query(all_colliders_query)
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
        for (e1, ()) in context
            .prepared_query(with_physics_and_dynamic_query)
            .iter()
        {
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
                    let constraint = match pipeline.contacts.entry(pair) {
                        Entry::Vacant(vacant) => {
                            let contact_id = ContactId(
                                pipeline
                                    .constraints
                                    .insert(ContactConstraint::new(pair, contact)),
                            );
                            vacant.insert(contact_id);
                            p1.contacts.push(ContactIdEntry {
                                flipped: false,
                                id: contact_id,
                            });
                            p2.contacts.push(ContactIdEntry {
                                flipped: true,
                                id: contact_id,
                            });
                            pipeline
                                .events
                                .single_write(PhysicsEvent::BeginContact(contact_id));
                            &mut pipeline.constraints[contact_id.0]
                        }
                        Entry::Occupied(occupied) => {
                            let contact_id = *occupied.get();
                            let mut_constraint = &mut pipeline.constraints[contact_id.0];
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
    for (_, physics) in context.prepared_query(dynamic_objects_query).iter() {
        physics.velocity.linear += physics.gravity_k * pipeline.config.gravity * dt.0;
    }

    // Solve likely-violated velocity constraints
    pipeline.solve_velocities(&mut context.column_mut(*physics_query_marker), dt);

    // Integrate positions w/ newly solved velocities, for bodies w/o CCD
    for (_, (physics, ccd_enabled)) in context.prepared_query(update_target_pos_query).iter() {
        physics.target = physics.velocity.integrate(&physics.position, dt.0);

        if !ccd_enabled {
            physics.position = physics.target;
        }
    }

    // Integrate positions w/ newly solved velocities, for bodies w/ CCD, performing motion clamping
    // where applicable.
    //
    // At current, we only perform CCD against static bodies, and we only perform CCD for bodies
    // which are above the threshold.
    {
        let physics_column_mut = context.column_mut(*physics_query_marker);
        for (e1, ()) in context.prepared_query(motion_clamping_query).iter() {
            let mut min_toi = None::<TOI>;

            let p1 = unsafe { physics_column_mut.get_unchecked(e1).unwrap() };

            if p1.velocity.linear.norm_squared()
                >= pipeline
                    .config
                    .continuous_collision_velocity_threshold
                    .powi(2)
            {
                let pos_start_tx = p1.collider_tx * p1.position.as_isometry3();
                let pos_end_tx = p1.collider_tx * p1.target.as_isometry3();
                let aabb = p1
                    .collider_shape
                    .compute_swept_aabb(&pos_start_tx, &pos_end_tx)
                    .loosened(0.1);

                for intersection in atom_map.intersect_with(aabb) {
                    let maybe_new_toi = intersection.shape.time_of_impact(
                        &intersection.coords,
                        &pos_start_tx,
                        &p1.velocity.linear,
                        p1.collider_shape.as_ref(),
                        dt.0,
                    );

                    if let Some(new_toi) = maybe_new_toi {
                        let do_replace =
                            min_toi.map_or(true, |prev_toi| new_toi.toi < prev_toi.toi);
                        if do_replace {
                            min_toi = Some(new_toi);
                        }
                    }
                }

                // TODO: When/if we do CCD on dynamic-dynamic bodies, it should probably go here.
                // We're borrowing the ECS world via the column API specifically in order to make
                // this easy for if/when we do this.
            }

            if let Some(toi) = min_toi {
                let extra =
                    pipeline.config.continuous_collision_toi_bias / p1.velocity.linear.norm();
                p1.position = p1
                    .position
                    .lerp_slerp(&p1.target, (toi.toi / dt.0 + extra).min(1.));
            } else {
                p1.position = p1.target;
            }
        }
    }

    // Detect dynamic-static collisions, collecting position and velocity constraints
    let mut out = Vec::new();
    for (e, physics) in context.prepared_query(dynamic_objects_query).iter() {
        let pos_tx = physics.collider_tx * physics.position.as_isometry3();
        let aabb = physics.collider_shape.compute_aabb(&pos_tx);
        for intersection in atom_map.intersect_with(aabb) {
            intersection.shape.contact(
                &intersection.coords,
                physics.collider_shape.as_ref(),
                &pos_tx,
                0.0,
                atom_map.edge_filter(),
                atom_map.vertex_filter(),
                &mut out,
            );

            for (mut c, feature) in out.drain(..) {
                c.flip();
                let pair = ConstrainedPair::Static(e, intersection.coords, feature);
                let contact = Contact::new(c.normal1, c.point1, c.point2);
                let constraint = match pipeline.contacts.entry(pair) {
                    Entry::Vacant(vacant) => {
                        let contact_id = ContactId(
                            pipeline
                                .constraints
                                .insert(ContactConstraint::new(pair, contact)),
                        );
                        vacant.insert(contact_id);
                        physics.contacts.push(ContactIdEntry {
                            flipped: false,
                            id: contact_id,
                        });
                        pipeline
                            .events
                            .single_write(PhysicsEvent::BeginContact(contact_id));
                        &mut pipeline.constraints[contact_id.0]
                    }
                    Entry::Occupied(occupied) => {
                        let contact_id = *occupied.get();
                        let mut_constraint = &mut pipeline.constraints[contact_id.0];
                        mut_constraint.contact = contact;
                        mut_constraint
                    }
                };

                constraint.timestamp = tick.0;
            }
        }
    }

    {
        let mut physics = context.column_mut(*physics_query_marker);
        // Solve position constraints
        pipeline.solve_positions(&mut physics, dt);

        pipeline.constraints.retain(|id, constraint| {
            if constraint.timestamp != tick.0 {
                pipeline.contacts.remove(&constraint.participants);

                let contact_id = ContactId(id);
                match constraint.participants {
                    ConstrainedPair::Dynamic(pair) => {
                        physics.get(pair.0).unwrap().remove_contact(contact_id);
                        physics.get(pair.1).unwrap().remove_contact(contact_id);
                    }
                    ConstrainedPair::Static(e1, _, _) => {
                        physics.get(e1).unwrap().remove_contact(contact_id);
                    }
                }

                pipeline.events.single_write(PhysicsEvent::EndContact(
                    contact_id,
                    constraint.participants,
                ));

                false
            } else {
                true
            }
        });
    }

    // Copy physics data back to the ECS.
    for (_, (pos, maybe_vel, physics)) in context.prepared_query(all_physics_objects_query).iter() {
        if let Some(vel) = maybe_vel {
            pos.current = physics.position;
            vel.composite = physics.velocity;
        }
    }
}

impl LuaUserData for CompositePosition3 {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().add_copy().add_send().add_sync();
    }

    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("x", |_, this| Ok(this.translation.x));
        fields.add_field_method_get("y", |_, this| Ok(this.translation.y));
        fields.add_field_method_get("z", |_, this| Ok(this.translation.z));
        fields.add_field_method_get("translation", |_, this| Ok(this.translation));
        fields.add_field_method_get("rotation", |_, this| Ok(this.rotation.angle()));
        fields.add_field_method_set("x", |_, this, x| Ok(this.translation.x = x));
        fields.add_field_method_set("y", |_, this, y| Ok(this.translation.y = y));
        fields.add_field_method_set("z", |_, this, z| Ok(this.translation.z = z));
        fields.add_field_method_set("translation", |_, this, translation| {
            Ok(this.translation = translation)
        });
        fields.add_field_method_set("rotation", |_, this, rotation| {
            Ok(this.rotation = UnitComplex::new(rotation))
        });
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, (translation, rotation): (_, Option<_>)| {
            Ok(Self::new(translation, rotation.unwrap_or(0.)))
        });
        methods.add_function("origin", |_, ()| Ok(Self::origin()));
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

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get", |_, this, out: Option<LuaAnyUserData>| match out {
            Some(ud) => {
                *ud.borrow_mut::<CompositePosition3>()? = this.current;
                Ok(None)
            }
            None => Ok(Some(this.current)),
        });

        methods.add_method_mut("set", |_, this, composite: CompositePosition3| {
            this.current = composite;
            Ok(())
        });
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, composite| Ok(Self::new(composite)));
        methods.add_function("new_out_of_sync", |_, (position, velocity, dt)| {
            Ok(Self::new_out_of_sync(position, &velocity, dt))
        });
        methods.add_function("origin", |_, ()| {
            Ok(Self::new(CompositePosition3::origin()))
        });
    }
}

impl LuaUserData for Velocity {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("get", |_, this, out: Option<LuaAnyUserData>| match out {
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

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, composite| Ok(Self { composite }));
        methods.add_function("zero", |_, ()| {
            Ok(Self {
                composite: CompositeVelocity3::zero(),
            })
        });
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

impl LuaUserData for Physics {
    fn on_metatable_init(table: Type<Self>) {
        table.add_clone().mark_component();
    }

    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("static_friction", |_, this| Ok(this.static_friction));
        fields.add_field_method_get("dynamic_friction", |_, this| Ok(this.dynamic_friction));
        fields.add_field_method_get("gravity_k", |_, this| Ok(this.gravity_k));

        fields.add_field_method_set("static_friction", |_, this, static_friction| {
            this.static_friction = static_friction;
            Ok(())
        });
        fields.add_field_method_set("dynamic_friction", |_, this, dynamic_friction| {
            this.dynamic_friction = dynamic_friction;
            Ok(())
        });
        fields.add_field_method_set("gravity_k", |_, this, gravity_k| {
            this.gravity_k = gravity_k;
            Ok(())
        });
    }

    fn add_methods<'lua, M: LuaUserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_function_mut("with_density", |_, (ud, density): (LuaAnyUserData, _)| {
            let mut this = ud.borrow_mut::<Self>()?;
            *this = this.clone().with_density(density);
            drop(this);
            Ok(ud)
        });

        methods.add_function_mut("with_friction", |_, (ud, friction): (LuaAnyUserData, _)| {
            let mut this = ud.borrow_mut::<Self>()?;
            *this = this.clone().with_friction(friction);
            drop(this);
            Ok(ud)
        });

        methods.add_function_mut(
            "with_static_friction",
            |_, (ud, friction): (LuaAnyUserData, _)| {
                let mut this = ud.borrow_mut::<Self>()?;
                *this = this.clone().with_static_friction(friction);
                drop(this);
                Ok(ud)
            },
        );

        methods.add_function_mut(
            "with_dynamic_friction",
            |_, (ud, friction): (LuaAnyUserData, _)| {
                let mut this = ud.borrow_mut::<Self>()?;
                *this = this.clone().with_dynamic_friction(friction);
                drop(this);
                Ok(ud)
            },
        );

        methods.add_function_mut(
            "with_restitution",
            |_, (ud, restitution): (LuaAnyUserData, _)| {
                let mut this = ud.borrow_mut::<Self>()?;
                *this = this.clone().with_restitution(restitution);
                drop(this);
                Ok(ud)
            },
        );

        methods.add_method_mut(
            "set_friction",
            |_, this, (static_friction, dynamic_friction): (f32, Option<f32>)| {
                let dynamic_friction = dynamic_friction.unwrap_or(static_friction);
                this.static_friction = static_friction;
                this.dynamic_friction = dynamic_friction;
                Ok(())
            },
        );

        methods.add_method(
            "max_projected_contact_normal",
            |lua, this, (normal, out): (Vector3<f32>, Option<LuaAnyUserData>)| {
                let pp_elastic;
                let res_elastic;
                let pp_borrow;
                let res_borrow;
                let pp_ref;
                let pp;

                if let Some(pp_ref) = lua.app_data_ref::<ElasticMut<PhysicsPipeline>>() {
                    pp_elastic = pp_ref;
                    pp_borrow = pp_elastic.borrow();
                    pp = &*pp_borrow;
                } else if let Some(resources) = lua.app_data_ref::<ElasticRef<Resources>>() {
                    res_elastic = resources;
                    res_borrow = res_elastic.borrow();
                    pp_ref = res_borrow.get().to_lua_err()?;
                    pp = &*pp_ref;
                } else {
                    return Err(anyhow!("no physics pipeline loaned to Lua state!")).to_lua_err();
                }

                let query_normal = UnitVector3::new_normalize(normal);
                let mut max_projected = None::<f32>;
                let mut max_normal = None::<Vector3<f32>>;

                for &contact_id_entry in &this.contacts {
                    let constraint = pp.contact(contact_id_entry.id).expect("invalid contact");
                    let contact_normal = if contact_id_entry.flipped {
                        -constraint.contact.normal
                    } else {
                        constraint.contact.normal
                    };

                    let projected = query_normal.dot(&contact_normal);
                    let replace =
                        max_projected.map_or(true, |max_projected| projected > max_projected);

                    if replace {
                        max_projected = Some(projected);
                        max_normal = Some(contact_normal.into_inner());
                    }
                }

                if let (Some(contact_normal), Some(out)) = (max_normal, out) {
                    let mut out = out.borrow_mut::<Vector3<f32>>()?;
                    *out = contact_normal;
                }

                Ok(max_projected)
            },
        );
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, collider_shape| Ok(Self::new(collider_shape)));

        methods.add_function("with_local_tx", |_, (collider_shape, collider_tx)| {
            Ok(Self::with_local_tx(collider_shape, collider_tx))
        });
    }
}

impl LuaUserData for CcdEnabled {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self));
    }
}

impl LuaUserData for KinematicMarker {
    fn on_metatable_init(table: Type<Self>) {
        table.mark_component().add_clone().add_copy();
    }

    fn on_type_metatable_init(table: Type<Type<Self>>) {
        table.mark_component_type();
    }

    fn add_type_methods<'lua, M: LuaUserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self));
    }
}

impl LuaUserData for PhysicsPipeline {
    fn on_metatable_init(table: Type<Self>) {
        table.add_send().add_sync();
    }

    #[allow(clippy::unit_arg)]
    fn add_fields<'lua, F: LuaUserDataFields<'lua, Self>>(fields: &mut F) {
        fields.add_field_method_get("bias_factor", |_, this| Ok(this.config.bias_factor));
        fields.add_field_method_set("bias_factor", |_, this, bias| {
            Ok(this.config.bias_factor = bias)
        });
    }
}

impl<'lua> ToLua<'lua> for ContactId {
    fn to_lua(self, lua: &'lua Lua) -> LuaResult<LuaValue<'lua>> {
        LuaLightUserData(self.0.to_bits() as *mut _).to_lua(lua)
    }
}

impl<'lua> FromLua<'lua> for ContactId {
    fn from_lua(lua_value: LuaValue<'lua>, lua: &'lua Lua) -> LuaResult<Self> {
        LuaLightUserData::from_lua(lua_value, lua).and_then(|lud| {
            Index::from_bits(lud.0 as _)
                .map(ContactId)
                .ok_or_else(|| anyhow!("invalid index!").to_lua_err())
        })
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

impl Sub for CompositeVelocity3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            linear: self.linear - rhs.linear,
            angular: self.angular - rhs.angular,
        }
    }
}

impl SubAssign for CompositeVelocity3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f32> for CompositeVelocity3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self {
            linear: self.linear * rhs,
            angular: self.angular * rhs,
        }
    }
}

impl MulAssign<f32> for CompositeVelocity3 {
    fn mul_assign(&mut self, rhs: f32) {
        *self = *self * rhs;
    }
}

impl Div<f32> for CompositeVelocity3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self::Output {
        Self {
            linear: self.linear / rhs,
            angular: self.angular / rhs,
        }
    }
}

impl DivAssign<f32> for CompositeVelocity3 {
    fn div_assign(&mut self, rhs: f32) {
        *self = *self / rhs;
    }
}

impl Sub for CompositePosition3 {
    type Output = CompositeVelocity3;
    fn sub(self, rhs: Self) -> Self::Output {
        CompositeVelocity3 {
            linear: self.translation - rhs.translation,
            angular: rhs.rotation.angle_to(&self.rotation),
        }
    }
}
