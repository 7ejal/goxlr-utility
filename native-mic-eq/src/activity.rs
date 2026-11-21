use crate::{daemon, model::ProfileKey};
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const MAX_AGE: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct Activity {
    pub enabled: bool,
    pending: Option<(ProfileKey, Receiver<anyhow::Result<f64>>)>,
    last_poll: Option<Instant>,
    sample: Option<(ProfileKey, Instant, f64)>,
    pub error: Option<String>,
}

impl Activity {
    pub fn value(&self, key: &ProfileKey, busy: bool, now: Instant) -> Option<f64> {
        let (sample_key, at, value) = self.sample.as_ref()?;
        (self.enabled && !busy && sample_key == key && now.duration_since(*at) <= MAX_AGE)
            .then_some(*value)
    }

    fn can_poll(&self, key: Option<&ProfileKey>, busy: bool, now: Instant) -> bool {
        self.enabled
            && !busy
            && key.is_some()
            && self.pending.is_none()
            && self
                .last_poll
                .is_none_or(|last| now.duration_since(last) >= POLL_INTERVAL)
    }

    pub fn tick(&mut self, ctx: &eframe::egui::Context, key: Option<&ProfileKey>, busy: bool) {
        let completed =
            self.pending
                .as_ref()
                .and_then(|(expected, receiver)| match receiver.try_recv() {
                    Ok(result) => Some((expected.clone(), result)),
                    Err(TryRecvError::Disconnected) => Some((
                        expected.clone(),
                        Err(anyhow::anyhow!("Device reading did not complete")),
                    )),
                    Err(TryRecvError::Empty) => None,
                });
        if let Some((expected, result)) = completed {
            self.pending = None;
            if key == Some(&expected) {
                match result {
                    Ok(value) if value.is_finite() && (-72.2..=0.).contains(&value) => {
                        self.sample = Some((expected, Instant::now(), value));
                        self.error = None;
                    }
                    result => {
                        self.sample = None;
                        self.error = Some(match result {
                            Err(error) => format!("{error:#}"),
                            Ok(_) => "Device returned an invalid activity value".into(),
                        });
                    }
                }
            }
        }
        if !self.enabled
            || busy
            || self
                .sample
                .as_ref()
                .is_some_and(|(sample_key, _, _)| Some(sample_key) != key)
        {
            self.sample = None;
        }
        if !self.enabled || key.is_none() {
            return;
        }
        ctx.request_repaint_after(POLL_INTERVAL);
        if !self.can_poll(key, busy, Instant::now()) {
            return;
        }
        let expected = key.expect("Checked profile").clone();
        let (sender, receiver) = mpsc::channel();
        self.pending = Some((expected.clone(), receiver));
        self.last_poll = Some(Instant::now());
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(anyhow::Error::from)
                .and_then(|runtime| runtime.block_on(daemon::read_mic_activity(expected)));
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> ProfileKey {
        ProfileKey {
            serial: "device".into(),
            name: name.into(),
        }
    }

    #[test]
    fn polling_is_rate_limited_single_flight_and_paused_during_other_operations() {
        let now = Instant::now();
        let profile = key("one");
        let mut activity = Activity {
            enabled: true,
            last_poll: Some(now),
            ..Default::default()
        };
        assert!(!activity.can_poll(Some(&profile), false, now + Duration::from_millis(99)));
        assert!(activity.can_poll(Some(&profile), false, now + POLL_INTERVAL));
        assert!(!activity.can_poll(Some(&profile), true, now + POLL_INTERVAL));
        assert!(!activity.can_poll(None, false, now + POLL_INTERVAL));
        let (_sender, receiver) = mpsc::channel();
        activity.pending = Some((profile.clone(), receiver));
        assert!(!activity.can_poll(Some(&profile), false, now + Duration::from_secs(2)));
    }

    #[test]
    fn stale_wrong_profile_paused_and_failed_readings_are_unavailable() {
        let now = Instant::now();
        let profile = key("one");
        let mut activity = Activity {
            enabled: true,
            sample: Some((profile.clone(), now, -24.)),
            ..Default::default()
        };
        assert_eq!(activity.value(&profile, false, now), Some(-24.));
        assert_eq!(activity.value(&profile, true, now), None);
        assert_eq!(activity.value(&key("two"), false, now), None);
        assert_eq!(
            activity.value(&profile, false, now + MAX_AGE + Duration::from_millis(1)),
            None
        );
        let (sender, receiver) = mpsc::channel();
        sender.send(Err(anyhow::anyhow!("Disconnected"))).unwrap();
        activity.pending = Some((profile.clone(), receiver));
        activity.tick(&eframe::egui::Context::default(), Some(&profile), true);
        assert!(activity.sample.is_none());
        assert!(activity.error.as_ref().unwrap().contains("Disconnected"));
    }
}
