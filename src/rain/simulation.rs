//! Bounded water on glass. Raindrops land anywhere and sit until merging makes
//! them heavy enough to slide; a sliding drop sticks and surges, wanders
//! sideways, picks up the beads in its path and leaves a few behind. A fixed
//! collision grid keeps merging local, and residue shares the hard pool limit.
//! Time advances only on visible redraws.
use super::Preferences;
pub const LIMIT: usize = 384;
/// Floats per drop instance: position, radius, stretch, opacity, kind, shape.
pub const INSTANCE: usize = 7;
#[derive(Debug, Clone, Copy)]
pub struct Drop {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub speed: f32,
    /// Residue: time since it was left. Water: time since it last moved.
    pub age: f32,
    pub trail: bool,
    // Distance to the next residual bead, independent for every moving drop.
    trail_distance: f32,
    /// Residue fades over its lifetime; resting water dries up after it.
    lifetime: f32,
    opacity: f32,
    stretch: f32,
    /// Seconds a sliding drop stays caught on the glass.
    hold: f32,
    /// Sideways heading, a slow random walk between -1 and 1.
    lean: f32,
    /// Outline variation, fixed for the drop's life.
    shape: f32,
}
#[derive(Debug)]
pub struct Simulation {
    pub drops: Vec<Drop>,
    seed: u32,
    spawn: f32,
    trails: Vec<Drop>,
}
impl Default for Simulation {
    fn default() -> Self {
        Self {
            drops: Vec::with_capacity(LIMIT),
            seed: 0x73a9f21,
            spawn: 0.,
            trails: Vec::new(),
        }
    }
}
/// Drop sizes follow the Drop size control.
fn scale(p: &Preferences) -> f32 {
    0.5 + f32::from(p.size) / 70.
}
/// Water heavier than this slides; lighter water waits to merge.
fn critical(p: &Preferences) -> f32 {
    0.0095 * scale(p)
}
/// Resting water fades out over this many seconds at the end of its life.
const DRY: f32 = 2.;
impl Simulation {
    fn random(&mut self) -> f32 {
        random(&mut self.seed)
    }
    fn add(&mut self, aspect: f32, p: &Preferences, initial: bool) {
        if self.drops.len() >= LIMIT {
            return;
        }
        let x = self.random() * aspect;
        let y = self.random() * 1.02 - 0.02;
        // Mostly small drops, now and then a heavy one.
        let size = self.random();
        let radius = (0.0035 + size * size * 0.012) * scale(p);
        let trail_distance = spacing(&mut self.seed, p.trails);
        let lifetime = 6. + self.random() * 14.;
        let age = if initial {
            self.random() * lifetime * 0.8
        } else {
            0.
        };
        let lean = self.random() * 2. - 1.;
        let shape = self.random();
        self.drops.push(Drop {
            x,
            y,
            radius,
            speed: 0.,
            age,
            trail: false,
            trail_distance,
            lifetime,
            opacity: if initial { 1. } else { 0. },
            stretch: 1.,
            hold: 0.,
            lean,
            shape,
        });
    }
    pub fn seed(&mut self, aspect: f32, p: &Preferences) {
        if p.intensity == 0 {
            return;
        }
        for _ in 0..(12 + usize::from(p.intensity) / 2) {
            self.add(aspect, p, true);
        }
    }
    pub fn step(&mut self, dt: f32, aspect: f32, p: &Preferences) {
        if p.intensity == 0 {
            self.drops.clear();
            self.spawn = 0.;
            return;
        }
        if !dt.is_finite() || !aspect.is_finite() || aspect <= 0. {
            return;
        }
        let dt = dt.clamp(0., 0.05);
        self.spawn += dt * (1. + f32::from(p.intensity) * 0.12);
        while self.spawn >= 1. {
            self.spawn -= 1.;
            self.add(aspect, p, false);
        }
        self.trails.clear();
        if self.trails.capacity() == 0 {
            self.trails.reserve_exact(LIMIT);
        }
        let critical = critical(p);
        let pace = 0.25 + f32::from(p.speed) / 50.;
        let wind = f32::from(p.wind) / 100.;
        for i in 0..self.drops.len() {
            let mut d = self.drops[i];
            d.age += dt;
            if d.trail {
                continue;
            }
            d.opacity = (d.opacity + dt * 8.).min(1.);
            let excess = (d.radius - critical) / critical;
            if excess <= 0. {
                // Too light to overcome the glass: it rests and slows to a stop.
                d.speed *= (1. - dt * 6.).max(0.);
            } else if d.hold > 0. {
                d.hold -= dt;
                d.speed *= (1. - dt * 12.).max(0.);
            } else {
                let target = (0.05 + excess.min(2.) * 0.22) * pace;
                d.speed += (target - d.speed) * (dt * 3.).min(1.);
                // Dry patches and dirt catch a sliding drop now and then;
                // heavier drops break free sooner.
                if random(&mut self.seed) < dt * 1.4 / (1. + excess * 2.) {
                    d.hold = (0.15 + random(&mut self.seed) * 0.9) / (1. + excess);
                    // It breaks free at an angle, so its path kinks.
                    d.lean = random(&mut self.seed) * 2. - 1.;
                }
            }
            let old_y = d.y;
            let old_x = d.x;
            if d.speed > 0.004 {
                d.age = 0.;
                d.lean = (d.lean + (random(&mut self.seed) - 0.5) * dt * 18.).clamp(-1., 1.);
                d.y += d.speed * dt;
                d.x += (d.lean * 0.45 + wind * 0.6) * d.speed * dt;
            }
            // A sliding drop draws out behind its leading edge.
            d.stretch = 1. + (d.speed * 3.).min(0.7);
            d.trail_distance -= (d.x - old_x).hypot(d.y - old_y);
            if d.trail_distance <= 0. && d.speed > 0.02 && p.trails > 0 {
                // Sparse, irregular deposits follow the actual path. No shared
                // vertical grid or identical chains above every large droplet.
                let at = random(&mut self.seed);
                self.trails.push(Drop {
                    x: old_x + (d.x - old_x) * at + (random(&mut self.seed) - 0.5) * d.radius,
                    y: old_y + (d.y - old_y) * at,
                    radius: d.radius * (0.1 + random(&mut self.seed).powi(2) * 0.24),
                    speed: 0.,
                    age: 0.,
                    trail: true,
                    trail_distance: 0.,
                    lifetime: (1.5 + random(&mut self.seed) * 3.)
                        * (0.5 + f32::from(p.trails) / 50.),
                    opacity: 0.45 + random(&mut self.seed) * 0.45,
                    stretch: 0.8 + random(&mut self.seed) * 0.5,
                    hold: 0.,
                    lean: 0.,
                    shape: random(&mut self.seed),
                });
                d.trail_distance = spacing(&mut self.seed, p.trails);
                // The water it leaves behind is water it no longer has.
                d.radius *= 0.988;
            }
            self.drops[i] = d;
        }
        // Grid cells are larger than the maximum merge diameter. No all-pairs
        // scan and no unbounded allocation from an output's pixel dimensions.
        const SIDE: usize = 32;
        const NONE: usize = usize::MAX;
        let mut heads = [NONE; SIDE * SIDE];
        let mut tails = [NONE; SIDE * SIDE];
        let mut next = [NONE; LIMIT];
        for i in 0..self.drops.len() {
            if self.drops[i].radius <= 0. || self.drops[i].trail {
                continue;
            }
            let cell_x = ((self.drops[i].x / aspect * SIDE as f32).max(0.) as usize).min(SIDE - 1);
            let cell_y = ((self.drops[i].y * SIDE as f32).max(0.) as usize).min(SIDE - 1);
            // Radius is capped below; expand neighborhoods on narrow portraits.
            let reach = ((0.045 / aspect.min(1.)) * SIDE as f32).ceil() as usize;
            for y in cell_y.saturating_sub(reach)..=(cell_y + reach).min(SIDE - 1) {
                for x in cell_x.saturating_sub(reach)..=(cell_x + reach).min(SIDE - 1) {
                    let mut cursor = heads[y * SIDE + x];
                    while cursor != NONE {
                        let j = cursor;
                        cursor = next[j];
                        let a = self.drops[i];
                        let b = self.drops[j];
                        if b.radius <= 0. || a.radius <= 0. {
                            continue;
                        }
                        if (a.x - b.x).hypot(a.y - b.y) < (a.radius + b.radius) * 0.8 {
                            // Water flows into the heavier drop, which keeps
                            // its course and most of its momentum.
                            let aa = a.radius * a.radius;
                            let bb = b.radius * b.radius;
                            let (big, small) = if aa >= bb { (a, b) } else { (b, a) };
                            let pull = 0.5 * (1. - big.radius * big.radius / (aa + bb));
                            let merged = &mut self.drops[i];
                            merged.x = big.x + (small.x - big.x) * pull;
                            merged.y = big.y + (small.y - big.y) * pull;
                            merged.radius = (aa + bb).sqrt().min(0.022);
                            merged.speed = a.speed.max(b.speed) * 0.85;
                            merged.lean = big.lean;
                            merged.shape = big.shape;
                            merged.trail_distance = big.trail_distance;
                            merged.lifetime = big.lifetime;
                            merged.age = 0.;
                            merged.hold = 0.;
                            merged.opacity = a.opacity.max(b.opacity);
                            self.drops[j].radius = 0.;
                        }
                    }
                }
            }
            let cell = cell_y * SIDE + cell_x;
            if tails[cell] == NONE {
                heads[cell] = i;
            } else {
                next[tails[cell]] = i;
            }
            tails[cell] = i;
        }
        self.drops.retain(|d| {
            d.radius > 0.0008
                && d.y < 1.05
                && d.x > -0.05
                && d.x < aspect + 0.05
                && if d.trail {
                    p.trails > 0 && d.age < d.lifetime
                } else {
                    d.age < d.lifetime + DRY
                }
        });
        // Residue never takes more than half the pool, so new drops always
        // find room to land.
        let residue = self.drops.iter().filter(|d| d.trail).count();
        let room = (LIMIT / 2)
            .saturating_sub(residue)
            .min(LIMIT - self.drops.len());
        self.drops.extend(self.trails.drain(..).take(room));
    }
    /// One instance per drop: x, y, radius, stretch, opacity, kind and shape.
    /// Kind is the teardrop taper of water that wipes the glass as it moves,
    /// or -1 for water that rests on it.
    /// Residue comes first so the water that left it is drawn on top.
    pub fn instances(&self, trails: u8) -> Vec<[f32; INSTANCE]> {
        let residue = self.drops.iter().filter(|d| d.trail && trails > 0);
        residue
            .chain(self.drops.iter().filter(|d| !d.trail))
            .map(|d| {
                let fade = if d.trail {
                    (1. - d.age / d.lifetime).clamp(0., 1.)
                } else {
                    (1. - (d.age - d.lifetime).max(0.) / DRY).clamp(0., 1.)
                };
                let (radius, opacity) = if d.trail {
                    (
                        d.radius * (0.65 + fade * 0.35),
                        d.opacity * fade * fade * (0.5 + f32::from(trails) / 200.),
                    )
                } else {
                    (d.radius * (0.8 + fade * 0.2), d.opacity * fade)
                };
                let kind = if d.trail || d.speed <= 0.01 {
                    -1.
                } else {
                    (d.speed * 4.).min(0.32)
                };
                [d.x, d.y, radius, d.stretch, opacity, kind, d.shape]
            })
            .collect()
    }
}
fn random(seed: &mut u32) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    (*seed >> 8) as f32 / 16777216.
}
fn spacing(seed: &mut u32, amount: u8) -> f32 {
    let r = random(seed);
    (0.01 + r * r * 0.06) / (0.3 + f32::from(amount) * 0.007)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rain_hybrid_pool_stays_bounded_across_long_runs_and_resume() {
        let p = Preferences {
            intensity: 100,
            size: 100,
            ..Default::default()
        };
        let mut s = Simulation::default();
        s.seed(1.6, &p);
        for _ in 0..3000 {
            s.step(0.05, 1.6, &p);
            assert!(s.drops.len() <= LIMIT);
        }
        s.step(86400., 1.6, &p);
        assert!(
            s.drops
                .iter()
                .all(|d| d.x.is_finite() && d.y.is_finite() && d.radius <= 0.03)
        );
    }
    #[test]
    fn rain_hybrid_drops_merge_and_grow() {
        let mut s = Simulation::default();
        let d = Drop {
            x: 0.5,
            y: 0.5,
            radius: 0.01,
            speed: 0.,
            age: 0.,
            trail: false,
            trail_distance: 0.05,
            lifetime: 10.,
            opacity: 1.,
            stretch: 1.,
            hold: 0.,
            lean: 0.,
            shape: 0.5,
        };
        s.drops = vec![d, Drop { x: 0.501, ..d }];
        s.step(0.01, 1., &Preferences::default());
        assert_eq!(s.drops.len(), 1);
        assert!(s.drops[0].radius > 0.014);
    }
    fn water(x: f32, y: f32, radius: f32) -> Drop {
        Drop {
            x,
            y,
            radius,
            speed: 0.,
            age: 0.,
            trail: false,
            trail_distance: 1.,
            lifetime: 30.,
            opacity: 1.,
            stretch: 1.,
            hold: 0.,
            lean: 0.,
            shape: 0.5,
        }
    }
    #[test]
    fn rain_heavy_drops_stick_slip_and_wander_while_light_ones_rest() {
        let p = Preferences {
            intensity: 1,
            trails: 0,
            speed: 0,
            wind: 0,
            ..Default::default()
        };
        let mut s = Simulation::default();
        s.drops = vec![water(0.8, 0.02, 0.02), water(0.3, 0.3, 0.004)];
        let (mut speeds, mut xs) = (Vec::new(), Vec::new());
        for _ in 0..600 {
            s.step(1. / 60., 1.6, &p);
            let heavy = s
                .drops
                .iter()
                .filter(|d| !d.trail)
                .max_by(|a, b| a.radius.total_cmp(&b.radius))
                .unwrap();
            speeds.push(heavy.speed);
            xs.push(heavy.x);
        }
        let top = speeds.iter().copied().fold(0., f32::max);
        assert!(top > 0.05, "heavy water slides: {top}");
        assert!(
            speeds[60..].iter().any(|v| *v < top * 0.3),
            "a sliding drop gets caught and surges again"
        );
        let (left, right) = xs
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), x| {
                (a.min(*x), b.max(*x))
            });
        assert!(right - left > 0.002, "it wanders without wind");
        assert!(
            s.drops
                .iter()
                .any(|d| d.x == 0.3 && d.y == 0.3 && d.speed == 0.),
            "light water rests where it landed"
        );
    }
    #[test]
    fn rain_resting_water_dries_up_after_its_lifetime() {
        let p = Preferences {
            intensity: 1,
            ..Default::default()
        };
        let mut s = Simulation::default();
        s.drops = vec![Drop {
            lifetime: 1.,
            ..water(0.5, 0.5, 0.004)
        }];
        let resting = |s: &Simulation| s.drops.iter().find(|d| d.x == 0.5 && d.y == 0.5).copied();
        for _ in 0..60 {
            s.step(1. / 30., 1.6, &p);
        }
        let drying = resting(&s).expect("still drying");
        assert!(drying.age > drying.lifetime);
        let fade = s.instances(0)[0][4];
        assert!(
            fade > 0. && fade < 1.,
            "it fades instead of popping: {fade}"
        );
        for _ in 0..60 {
            s.step(1. / 30., 1.6, &p);
        }
        assert!(resting(&s).is_none());
    }
    #[test]
    fn rain_instances_mark_wiping_water_and_draw_residue_first() {
        let mut s = Simulation::default();
        let sliding = Drop {
            speed: 0.2,
            stretch: 1.4,
            ..water(0.2, 0.4, 0.015)
        };
        let residue = Drop {
            trail: true,
            lifetime: 4.,
            opacity: 0.8,
            ..water(0.2, 0.3, 0.003)
        };
        s.drops = vec![sliding, water(0.6, 0.5, 0.005), residue];
        let instances = s.instances(100);
        assert_eq!(instances.len(), 3);
        assert_eq!((instances[0][0], instances[0][1]), (0.2, 0.3));
        assert_eq!(instances[0][5], -1., "residue never wipes");
        assert!(
            instances[1][5] > 0. && instances[1][5] <= 0.32,
            "moving water tapers and wipes"
        );
        assert_eq!(instances[1][3], 1.4);
        assert_eq!(instances[2][5], -1., "resting water never wipes");
        assert!(instances.iter().all(|i| i.len() == INSTANCE));
    }
    #[test]
    fn rain_residuals_are_irregular_fade_and_can_be_disabled() {
        let mut p = Preferences {
            intensity: 80,
            trails: 100,
            ..Default::default()
        };
        let mut s = Simulation::default();
        s.seed(1.6, &p);
        for _ in 0..150 {
            s.step(1. / 30., 1.6, &p);
        }
        let residuals: Vec<_> = s.drops.iter().filter(|d| d.trail).collect();
        assert!(residuals.len() > 5);
        let range = |values: Vec<f32>| {
            values.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                - values.iter().copied().fold(f32::INFINITY, f32::min)
        };
        assert!(range(residuals.iter().map(|d| d.radius).collect()) > 0.001);
        assert!(range(residuals.iter().map(|d| d.lifetime).collect()) > 1.);
        assert!(
            range(
                s.drops
                    .iter()
                    .filter(|d| !d.trail)
                    .map(|d| d.trail_distance)
                    .collect()
            ) > 0.03
        );
        let residue = *residuals[0];
        s.drops = vec![Drop { age: 0., ..residue }];
        let fresh = s.instances(100)[0];
        s.drops[0].age = residue.lifetime * 0.5;
        let fading = s.instances(100)[0];
        assert!(fading[2] < fresh[2]);
        assert!(fading[4] < fresh[4] * 0.3);
        assert!(
            s.instances(0).is_empty(),
            "turning trails off is immediate even before another step"
        );
        p.trails = 0;
        for _ in 0..150 {
            s.step(1. / 30., 1.6, &p);
        }
        assert!(s.drops.iter().all(|d| !d.trail));
    }
    #[test]
    fn rain_density_and_trails_are_independent_bounded_controls() {
        let sample = |intensity, trails| {
            let p = Preferences {
                intensity,
                trails,
                ..Default::default()
            };
            let mut s = Simulation::default();
            s.seed(1.6, &p);
            for _ in 0..300 {
                s.step(1. / 30., 1.6, &p);
            }
            assert!(s.drops.len() <= LIMIT);
            (
                s.drops.iter().filter(|d| !d.trail).count(),
                s.drops.iter().filter(|d| d.trail).count(),
            )
        };
        assert_eq!(sample(0, 100), (0, 0));
        assert!(sample(100, 0).0 > sample(10, 0).0 * 2);
        assert_eq!(sample(70, 0).1, 0);
        assert!(sample(70, 100).1 > sample(70, 20).1);
        assert_eq!(
            sample(70, 35),
            sample(70, 35),
            "visual captures are reproducible"
        );
    }
}
