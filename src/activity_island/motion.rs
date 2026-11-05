//! A critically damped transition with continuous position and velocity on retarget.
use std::time::Instant;
#[derive(Debug)]
pub struct Motion {
    pub value: f32,
    velocity: f32,
    target: f32,
    last: Instant,
}
impl Motion {
    pub fn new(now: Instant) -> Self {
        Self {
            value: 0.,
            velocity: 0.,
            target: 0.,
            last: now,
        }
    }
    pub fn active(&self) -> bool {
        self.value != self.target || self.velocity != 0.
    }
    pub fn target(&self) -> f32 {
        self.target
    }
    pub fn retarget(&mut self, target: f32, animate: bool, now: Instant) {
        self.advance(now);
        self.target = target.clamp(0., 1.);
        if !animate {
            self.value = self.target;
            self.velocity = 0.
        }
        self.last = now;
    }
    pub fn advance(&mut self, now: Instant) {
        if now < self.last {
            return;
        }
        let dt = now.saturating_duration_since(self.last).as_secs_f32();
        self.last = now;
        let omega = 30.;
        let delta = self.value - self.target;
        let c = self.velocity + omega * delta;
        let decay = (-omega * dt).exp();
        self.value = self.target + (delta + c * dt) * decay;
        self.velocity = (self.velocity - omega * c * dt) * decay;
        if (self.value - self.target).abs() < 0.001 && self.velocity.abs() < 0.015 {
            self.value = self.target;
            self.velocity = 0.;
        }
        if self.value < 0. || self.value > 1. {
            self.value = self.value.clamp(0., 1.);
            self.velocity = 0.;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn reversal_preserves_position_and_velocity() {
        let t = Instant::now();
        let mut m = Motion::new(t);
        m.retarget(1., true, t);
        m.advance(t + Duration::from_millis(90));
        let before = (m.value, m.velocity);
        m.retarget(0., true, t + Duration::from_millis(90));
        assert_eq!(before, (m.value, m.velocity));
        m.advance(t + Duration::from_secs(1));
        assert!(!m.active());
        assert_eq!(m.value, 0.);
    }
    #[test]
    fn settles_independently_of_refresh_rate() {
        for hz in [30, 60, 120, 144, 240] {
            let t = Instant::now();
            let mut m = Motion::new(t);
            m.retarget(1., true, t);
            let mut previous = 0.;
            for frame in 1..=hz {
                m.advance(t + Duration::from_secs_f32(frame as f32 / hz as f32));
                assert!(m.value >= previous && m.value <= 1.);
                previous = m.value;
            }
            assert!(!m.active());
            assert_eq!(m.value, 1.);
        }
    }
    #[test]
    fn reduced_motion_and_resume_do_not_keep_frames_alive() {
        let t = Instant::now();
        let mut m = Motion::new(t);
        m.retarget(1., false, t);
        assert_eq!(m.value, 1.);
        assert!(!m.active());
        m.retarget(0., true, t);
        m.advance(t + Duration::from_secs(7200));
        assert_eq!(m.value, 0.);
        assert!(!m.active());
    }
}
