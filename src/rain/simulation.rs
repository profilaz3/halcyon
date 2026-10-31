//! Bounded large droplets. A fixed collision grid keeps merging local; trails
//! share the same hard pool limit. Time advances only on visible redraws.
use super::Preferences;
pub const LIMIT: usize = 384;
#[derive(Debug, Clone, Copy)]
pub struct Drop {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub speed: f32,
    pub age: f32,
    pub trail: bool,
    // Distance to the next residual bead, independent for every moving drop.
    trail_distance: f32,
    lifetime: f32,
    opacity: f32,
    stretch: f32,
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
impl Simulation {
    fn random(&mut self) -> f32 {
        random(&mut self.seed)
    }
    fn add(&mut self, aspect: f32, p: &Preferences, initial: bool) {
        if self.drops.len() >= LIMIT {
            return;
        }
        let x = self.random() * aspect;
        let y = if initial {
            self.random()
        } else {
            self.random() * 0.35 - 0.05
        };
        let r = (0.004 + self.random() * 0.010) * (0.5 + f32::from(p.size) / 70.);
        let trail_distance = spacing(&mut self.seed, p.trails);
        self.drops.push(Drop {
            x,
            y,
            radius: r,
            speed: 0.,
            age: 0.,
            trail: false,
            trail_distance,
            lifetime: 0.,
            opacity: 1.,
            stretch: 1.,
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
        for d in &mut self.drops {
            d.age += dt;
            if d.trail {
                continue;
            }
            let slip = (d.radius - 0.007).max(0.) * 8.;
            let target = slip * (0.25 + f32::from(p.speed) / 50.);
            d.speed += (target - d.speed) * (dt * 2.).min(1.);
            let old_y = d.y;
            let old_x = d.x;
            d.y += d.speed * dt;
            d.x += f32::from(p.wind) / 100. * d.speed * dt * 0.35;
            d.trail_distance -= (d.x - old_x).hypot(d.y - old_y);
            if d.trail_distance <= 0. && d.speed > 0.02 && p.trails > 0 {
                // Sparse, irregular deposits follow the actual path. No shared
                // vertical grid or identical chains above every large droplet.
                let at = random(&mut self.seed);
                self.trails.push(Drop {
                    x: old_x + (d.x - old_x) * at,
                    y: old_y + (d.y - old_y) * at,
                    radius: d.radius * (0.07 + random(&mut self.seed) * 0.13),
                    speed: 0.,
                    age: 0.,
                    trail: true,
                    trail_distance: 0.,
                    lifetime: 1.4 + random(&mut self.seed) * 3.1,
                    opacity: 0.2 + random(&mut self.seed) * 0.35,
                    stretch: 0.8 + random(&mut self.seed) * 1.6,
                });
                d.trail_distance = spacing(&mut self.seed, p.trails);
                d.radius *= 0.995;
            }
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
            let cell_x = ((self.drops[i].x / aspect * SIDE as f32) as usize).min(SIDE - 1);
            let cell_y = ((self.drops[i].y * SIDE as f32) as usize).min(SIDE - 1);
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
                        if b.radius <= 0. {
                            continue;
                        }
                        if (a.x - b.x).hypot(a.y - b.y) < (a.radius + b.radius) * 0.7 {
                            let aa = a.radius * a.radius;
                            let bb = b.radius * b.radius;
                            self.drops[i].x = (a.x * aa + b.x * bb) / (aa + bb);
                            self.drops[i].y = (a.y * aa + b.y * bb) / (aa + bb);
                            self.drops[i].radius = (aa + bb).sqrt().min(0.022);
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
                && (!d.trail || (p.trails > 0 && d.age < d.lifetime))
        });
        self.drops
            .extend(self.trails.drain(..).take(LIMIT - self.drops.len()));
    }
    pub fn instances(&self, trails: u8) -> Vec<[f32; 5]> {
        self.drops
            .iter()
            .filter(|d| !d.trail || trails > 0)
            .map(|d| {
                let fade = if d.trail {
                    (1. - d.age / d.lifetime).clamp(0., 1.)
                } else {
                    1.
                };
                [
                    d.x,
                    d.y,
                    d.radius * (0.65 + fade * 0.35),
                    d.stretch + d.speed * 1.8,
                    d.opacity
                        * fade
                        * fade
                        * if d.trail {
                            f32::from(trails) / 100.
                        } else {
                            1.
                        },
                ]
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
    (0.012 + r * r * 0.075) / (0.3 + f32::from(amount) * 0.007)
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
            lifetime: 0.,
            opacity: 1.,
            stretch: 1.,
        };
        s.drops = vec![d, Drop { x: 0.501, ..d }];
        s.step(0.01, 1., &Preferences::default());
        assert_eq!(s.drops.len(), 1);
        assert!(s.drops[0].radius > 0.014);
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
