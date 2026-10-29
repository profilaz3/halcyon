//! Bounded, process-local weather work. Persistent widget identity is unchanged.
use crate::{config::Location, data::WeatherSnapshot, widget_presentation::Context};
use cosmic::iced::futures::future::{AbortHandle, AbortRegistration, Abortable};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

pub const MAX_KEYS: usize = 32;
pub const MAX_JOBS: usize = 4;
pub const DEADLINE: Duration = Duration::from_secs(15);
const RETRY: Duration = Duration::from_secs(120);

/// All variable request inputs. Provider, metric units, fields, auto timezone
/// and three forecast days are fixed by data::fetch_weather. A new query
/// variant must extend this key before sharing its work.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    name: String,
    latitude: u64,
    longitude: u64,
}
impl Key {
    pub fn new(location: &Location) -> Option<Self> {
        if !location.latitude.is_finite()
            || !location.longitude.is_finite()
            || !(-90. ..=90.).contains(&location.latitude)
            || !(-180. ..=180.).contains(&location.longitude)
        {
            return None;
        }
        let bits = |v: f64| if v == 0. { 0f64.to_bits() } else { v.to_bits() };
        Some(Self {
            name: location.name.clone(),
            latitude: bits(location.latitude),
            longitude: bits(location.longitude),
        })
    }
    pub fn location(&self) -> Location {
        Location {
            name: self.name.clone(),
            latitude: f64::from_bits(self.latitude),
            longitude: f64::from_bits(self.longitude),
            timezone: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Demand {
    pub consumer: u64,
    pub key: Key,
    pub interval: Duration,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Loading,
    Ready,
    Stale,
    Unavailable,
    Disconnected,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub key: Key,
    pub revision: u64,
    pub data: Option<Arc<WeatherSnapshot>>,
    pub state: State,
    pub failed: bool,
    pub refreshing: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    key: Key,
    serial: u64,
}
#[derive(Debug, Clone)]
pub struct Completion {
    pub token: Token,
    pub result: Option<Result<WeatherSnapshot, String>>,
}
pub struct Request {
    pub token: Token,
    registration: AbortRegistration,
}
impl Request {
    pub async fn run(self) -> Completion {
        let location = self.token.key.location();
        self.execute(crate::data::fetch_weather(&location), DEADLINE)
            .await
    }
    async fn execute(
        self,
        fetch: impl Future<Output = Result<WeatherSnapshot, String>>,
        deadline: Duration,
    ) -> Completion {
        let result = Abortable::new(
            async {
                tokio::time::timeout(deadline, fetch)
                    .await
                    .unwrap_or_else(|_| Err("Weather request timed out".into()))
            },
            self.registration,
        )
        .await
        .ok();
        Completion {
            token: self.token,
            result,
        }
    }
}
struct Flight {
    token: Token,
    abort: AbortHandle,
    valid: bool,
}
struct Entry {
    consumers: BTreeMap<u64, Duration>,
    interval: Duration,
    due: Instant,
    revision: u64,
    data: Option<Arc<WeatherSnapshot>>,
    failed: bool,
    flight: Option<Flight>,
}
pub struct Hub {
    context: Context,
    entries: BTreeMap<Key, Entry>,
    serial: u64,
}
impl Hub {
    pub fn new(context: Context) -> Self {
        Self {
            context,
            entries: BTreeMap::new(),
            serial: 0,
        }
    }
    /// Replace the complete demand set atomically. Limits reject before changing
    /// existing subscriptions. Multiple consumers of a key use the fastest cadence.
    pub fn reconcile(&mut self, demands: &[Demand], now: Instant) -> Result<(), &'static str> {
        let mut wanted: BTreeMap<Key, BTreeMap<u64, Duration>> = BTreeMap::new();
        if self.context == Context::Live {
            if demands.len() > 64 {
                return Err("Too many weather consumers");
            }
            for d in demands {
                wanted.entry(d.key.clone()).or_default().insert(
                    d.consumer,
                    d.interval
                        .clamp(Duration::from_secs(1), Duration::from_secs(86400)),
                );
            }
        }
        // Cancelling jobs keep their slots until their future is actually dropped
        // and acknowledges completion. Rebinding A -> B -> A cannot overlap A.
        let retained = self
            .entries
            .iter()
            .filter(|(key, e)| !wanted.contains_key(*key) && e.flight.is_some())
            .count();
        if wanted.len() + retained > MAX_KEYS {
            return Err("Too many weather bindings");
        }
        for (key, e) in &mut self.entries {
            if let Some(consumers) = wanted.remove(key) {
                let interval = *consumers.values().min().unwrap();
                if e.consumers.is_empty() || interval != e.interval {
                    e.due = now;
                }
                e.consumers = consumers;
                e.interval = interval;
            } else {
                e.consumers.clear();
                if let Some(f) = &mut e.flight {
                    f.valid = false;
                    f.abort.abort();
                }
            }
        }
        self.entries
            .retain(|_, e| !e.consumers.is_empty() || e.flight.is_some());
        for (key, consumers) in wanted {
            let interval = *consumers.values().min().unwrap();
            self.entries.insert(
                key,
                Entry {
                    consumers,
                    interval,
                    due: now,
                    revision: 0,
                    data: None,
                    failed: false,
                    flight: None,
                },
            );
        }
        Ok(())
    }
    pub fn seed(&mut self, snapshot: WeatherSnapshot) {
        let Some(key) = Key::new(&snapshot.location) else {
            return;
        };
        if chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at).is_err() {
            return;
        }
        if let Some(e) = self.entries.get_mut(&key)
            && e.data.is_none()
        {
            e.data = Some(Arc::new(snapshot));
        }
    }
    #[cfg(test)]
    pub fn token_for_test(&self, key: &Key) -> Option<Token> {
        self.entries
            .get(key)?
            .flight
            .as_ref()
            .map(|f| f.token.clone())
    }
    pub fn due_now(&mut self, now: Instant) {
        for e in self.entries.values_mut() {
            if !e.consumers.is_empty() {
                e.due = now;
            }
        }
    }
    pub fn resumed(&mut self, lost: Duration, now: Instant) {
        for e in self.entries.values_mut() {
            e.due = e.due.checked_sub(lost).unwrap_or(now);
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        if self.entries.values().filter(|e| e.flight.is_some()).count() >= MAX_JOBS {
            return None;
        }
        self.entries
            .values()
            .filter(|e| !e.consumers.is_empty() && e.flight.is_none())
            .map(|e| e.due)
            .min()
    }
    pub fn start_due(&mut self, now: Instant) -> Vec<Request> {
        let mut slots = MAX_JOBS - self.entries.values().filter(|e| e.flight.is_some()).count();
        let mut result = Vec::new();
        // Earliest due first prevents a frequent binding starving queued ones.
        let mut keys: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, e)| !e.consumers.is_empty() && e.flight.is_none() && e.due <= now)
            .map(|(k, e)| (e.due, k.clone()))
            .collect();
        keys.sort();
        for (_, key) in keys {
            if slots == 0 {
                break;
            }
            self.serial = self
                .serial
                .checked_add(1)
                .expect("weather request serial exhausted");
            let token = Token {
                key: key.clone(),
                serial: self.serial,
            };
            let (abort, registration) = AbortHandle::new_pair();
            self.entries.get_mut(&key).unwrap().flight = Some(Flight {
                token: token.clone(),
                abort,
                valid: true,
            });
            result.push(Request {
                token,
                registration,
            });
            slots -= 1;
        }
        result
    }
    /// Retires exactly this request, never a newer same-key flight. Cancellation
    /// and late delivery do not change readings, errors, retry times or caches.
    pub fn complete(
        &mut self,
        completion: Completion,
        now: Instant,
        wall: chrono::DateTime<chrono::Local>,
    ) -> Option<Snapshot> {
        let key = &completion.token.key;
        let e = self.entries.get_mut(key)?;
        let f = e.flight.as_ref()?;
        if f.token != completion.token {
            return None;
        }
        let valid = f.valid && !e.consumers.is_empty();
        e.flight = None;
        if e.consumers.is_empty() {
            self.entries.remove(key);
            return None;
        }
        if !valid {
            return None;
        }
        let result = completion.result?;
        let result = result.and_then(|snapshot| {
            if Key::new(&snapshot.location).as_ref() != Some(key)
                || chrono::DateTime::parse_from_rfc3339(&snapshot.updated_at).is_err()
            {
                Err("Weather response has an invalid binding or observation time".into())
            } else {
                Ok(snapshot)
            }
        });
        e.revision = completion.token.serial;
        match result {
            Ok(snapshot) => {
                e.data = Some(Arc::new(snapshot));
                e.failed = false;
                e.due = now + e.interval;
            }
            Err(_) => {
                e.failed = true;
                e.due = now + RETRY;
            }
        }
        Some(self.snapshot(key, wall))
    }
    pub fn snapshot(&self, key: &Key, wall: chrono::DateTime<chrono::Local>) -> Snapshot {
        let e = self.entries.get(key).filter(|e| !e.consumers.is_empty());
        let state = match e {
            None => State::Disconnected,
            Some(e) => match &e.data {
                None if e.failed => State::Unavailable,
                None => State::Loading,
                Some(data) => {
                    let stale =
                        chrono::DateTime::parse_from_rfc3339(&data.updated_at).map_or(true, |at| {
                            let age = wall.signed_duration_since(at);
                            age < chrono::Duration::zero()
                                || age.num_seconds() as u64 >= e.interval.as_secs()
                        });
                    if e.failed || stale {
                        State::Stale
                    } else {
                        State::Ready
                    }
                }
            },
        };
        Snapshot {
            key: key.clone(),
            revision: e.map_or(0, |e| e.revision),
            data: e.and_then(|e| e.data.clone()),
            state,
            failed: e.is_some_and(|e| e.failed),
            refreshing: e.is_some_and(|e| e.flight.as_ref().is_some_and(|f| f.valid)),
        }
    }
}
impl Drop for Hub {
    fn drop(&mut self) {
        for e in self.entries.values() {
            if let Some(f) = &e.flight {
                f.abort.abort();
            }
        }
    }
}

/// One disk writer and one replaceable pending snapshot. The mutex protects
/// only the queue; disk I/O never holds it or blocks the application thread.
#[derive(Clone, Default)]
pub struct Cache {
    state: Arc<std::sync::Mutex<CacheState>>,
}
#[derive(Default)]
struct CacheState {
    next_worker: u64,
    active: Option<u64>,
    pending: Option<(Arc<WeatherSnapshot>, Option<std::path::PathBuf>)>,
}
pub struct CacheWrite {
    cache: Cache,
    worker: u64,
}
impl Cache {
    pub fn prepare(&self, data: Arc<WeatherSnapshot>) -> Option<CacheWrite> {
        self.prepare_at(data, WeatherSnapshot::path())
    }
    fn prepare_at(
        &self,
        data: Arc<WeatherSnapshot>,
        path: Option<std::path::PathBuf>,
    ) -> Option<CacheWrite> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.pending = Some((data, path));
        if state.active.is_some() {
            return None;
        }
        state.next_worker = state
            .next_worker
            .checked_add(1)
            .expect("weather cache serial exhausted");
        let worker = state.next_worker;
        state.active = Some(worker);
        Some(CacheWrite {
            cache: self.clone(),
            worker,
        })
    }
}
impl CacheWrite {
    pub fn save(self) -> Result<(), String> {
        self.save_with(|data, path| data.save_at(path))
    }
    fn save_with(
        self,
        mut write: impl FnMut(&WeatherSnapshot, &std::path::Path) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut error = None;
        loop {
            let pending = {
                let mut state = self
                    .cache
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                let pending = state.pending.take();
                if pending.is_none() {
                    state.active = None;
                }
                pending
            };
            let Some((data, path)) = pending else {
                return error.map_or(Ok(()), Err);
            };
            if let Some(path) = path
                && let Err(problem) = write(&data, &path)
            {
                error = Some(problem);
            }
        }
    }
}
impl Drop for CacheWrite {
    fn drop(&mut self) {
        let mut state = self
            .cache
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A newly started worker may already own the queue after our final
        // drain. Dropping this old lease must not release that newer worker.
        if state.active == Some(self.worker) {
            state.active = None;
        }
    }
}

#[cfg(test)]
mod tests;
