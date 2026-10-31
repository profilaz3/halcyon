//! Local output audio only. No microphone fallback, files, or network. One bounded
//! stream feeds both presentations; dropping its subscription kills the reader.
use cosmic::iced::{
    Subscription,
    futures::{SinkExt, Stream},
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::io::AsyncReadExt;
pub mod drawing;
#[cfg(test)]
mod tests;
pub const BINS: usize = 48;
const N: usize = 1024;
const RATE: f32 = 22050.;
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Circle {
    #[default]
    Off,
    Bars,
    Dots,
    Rings,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub spectrum: bool,
    pub opacity: u8,
    pub width: u8,
    pub height: u8,
    pub x: u8,
    pub y: u8,
    pub behind_dock: bool,
    pub circle: Circle,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            spectrum: false,
            opacity: 80,
            width: 100,
            height: 18,
            x: 50,
            y: 100,
            behind_dock: true,
            circle: Circle::Off,
        }
    }
}
impl Preferences {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.opacity > 100
            || !(10..=100).contains(&self.width)
            || !(4..=100).contains(&self.height)
            || self.x > 100
            || self.y > 100
        {
            Err("Visualizer size, position and opacity must be within their ranges".into())
        } else {
            Ok(())
        }
    }
    pub fn active(&self) -> bool {
        (self.spectrum && self.opacity > 0) || self.circle != Circle::Off
    }
    /// Percentages describe the travel within the available rectangle: 100% is
    /// exactly the far edge, regardless of size. No normal-widget edge padding.
    pub fn rect(&self, size: [f32; 2]) -> cosmic::iced::Rectangle {
        let [w, h] = size.map(|n| if n.is_finite() { n.max(1.) } else { 1. });
        let width = w * f32::from(self.width.clamp(10, 100)) / 100.;
        let height = h * f32::from(self.height.clamp(4, 100)) / 100.;
        cosmic::iced::Rectangle {
            x: (w - width) * f32::from(self.x.min(100)) / 100.,
            y: (h - height) * f32::from(self.y.min(100)) / 100.,
            width,
            height,
        }
    }
}
#[derive(Debug, Clone)]
pub enum Event {
    Frame([f32; BINS]),
    Unavailable,
}
/// pa_context's special monitor token resolves the default sink's monitor, never
/// the default source. A missing output monitor fails closed.
pub const ARGS: &[&str] = &[
    "--device=@DEFAULT_MONITOR@",
    "--raw",
    "--format=s16le",
    "--rate=22050",
    "--channels=1",
    "--latency-msec=50",
    "--client-name=Halcyon visualizer",
    "--stream-name=Desktop output visualization",
];
fn command() -> tokio::process::Command {
    let mut c = tokio::process::Command::new("/usr/bin/parec");
    c.args(ARGS)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    c
}
pub fn watch() -> Subscription<Event> {
    Subscription::run(feed)
}
fn feed() -> impl Stream<Item = Event> {
    cosmic::iced::stream::channel(1, async move |mut out| {
        loop {
            if let Ok(mut child) = command().spawn() {
                let mut input = child.stdout.take().expect("piped audio");
                let mut bytes = [0u8; N * 2];
                let mut previous = [0.; BINS];
                while matches!(
                    tokio::time::timeout(Duration::from_secs(3), input.read_exact(&mut bytes))
                        .await,
                    Ok(Ok(_))
                ) {
                    let bins = analyze(&bytes, &mut previous);
                    if out.send(Event::Frame(bins)).await.is_err() {
                        return;
                    }
                }
                let _ = child.kill().await;
                let _ = child.wait().await;
            }
            if out.send(Event::Unavailable).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_secs(15)).await;
        }
    })
}
/// In-place radix-2 FFT; fixed input/work/output sizes and bounded frame rate.
fn analyze(bytes: &[u8; N * 2], previous: &mut [f32; BINS]) -> [f32; BINS] {
    let mut re = [0f32; N];
    let mut im = [0f32; N];
    let tau = std::f32::consts::TAU;
    for i in 0..N {
        let sample = f32::from(i16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]])) / 32768.;
        re[i] = sample * (0.5 - 0.5 * (tau * i as f32 / (N - 1) as f32).cos());
    }
    for i in 0..N {
        let j = i.reverse_bits() >> (usize::BITS - N.ilog2());
        if j > i {
            re.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= N {
        let angle = -tau / len as f32;
        for start in (0..N).step_by(len) {
            let (mut wr, mut wi) = (1f32, 0f32);
            let (sr, si) = (angle.cos(), angle.sin());
            for j in 0..len / 2 {
                let (a, b) = (start + j, start + j + len / 2);
                let (tr, ti) = (wr * re[b] - wi * im[b], wr * im[b] + wi * re[b]);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                (wr, wi) = (wr * sr - wi * si, wr * si + wi * sr);
            }
        }
        len *= 2;
    }
    let mut result = [0.; BINS];
    for i in 0..BINS {
        let low = (40. * (10000f32 / 40.).powf(i as f32 / BINS as f32) * N as f32 / RATE) as usize;
        let high =
            (40. * (10000f32 / 40.).powf((i + 1) as f32 / BINS as f32) * N as f32 / RATE) as usize;
        let mut magnitude = 0f32;
        for k in low.max(1)..high.max(low + 1).min(N / 2) {
            magnitude = magnitude.max((re[k] * re[k] + im[k] * im[k]).sqrt() * 4. / N as f32);
        }
        let target = ((20. * magnitude.max(0.00001).log10() + 60.) / 60.).clamp(0., 1.);
        result[i] = target.max(previous[i] * 0.78);
        if result[i] < 0.002 {
            result[i] = 0.;
        }
    }
    *previous = result;
    result
}
