//! When to lower other apps (Phase 2 §12, attenuation): while others speak
//! in the call, or while this device talks, as the user chose, by the
//! share they chose. Other apps come back a second after the last voice,
//! so they do not pump between words.

use std::collections::HashSet;
use std::time::Duration;

use tokio::time::Instant;

use opencord_media::attenuation::Attenuator;

use crate::api::types::AudioSettings;

/// How long after the last voice other apps stay lowered.
const RELEASE: Duration = Duration::from_secs(1);

/// Who is speaking, and whether other apps are lowered for it.
#[derive(Default)]
pub(crate) struct Ducking {
    attenuator: Option<Attenuator>,
    /// Others speaking in the call.
    others: HashSet<i64>,
    talking: bool,
    /// The level other apps were lowered to.
    lowered: Option<f32>,
    release_at: Option<Instant>,
}

impl Ducking {
    pub fn others_speaking(&mut self, user_id: i64, speaking: bool) {
        if speaking {
            self.others.insert(user_id);
        } else {
            self.others.remove(&user_id);
        }
    }

    pub fn talking(&mut self, talking: bool) {
        self.talking = talking;
    }

    /// The call ended: nobody is speaking.
    pub fn clear(&mut self) {
        self.others.clear();
        self.talking = false;
    }

    /// When [`Self::update`] should run again by itself.
    pub fn release_at(&self) -> Option<Instant> {
        self.release_at
    }

    /// Lowers or restores other apps for what is happening at `now`.
    pub fn update(&mut self, settings: &AudioSettings, now: Instant) {
        let level = wanted_level(settings, !self.others.is_empty(), self.talking);
        match level {
            Some(level) => {
                self.release_at = None;
                if self.lowered != Some(level) {
                    if self.attenuator.is_none() {
                        self.attenuator = Attenuator::start();
                    }
                    if let Some(attenuator) = &self.attenuator {
                        attenuator.set(Some(level));
                        self.lowered = Some(level);
                    }
                }
            }
            None if self.lowered.is_some() => {
                let release_at = *self.release_at.get_or_insert(now + RELEASE);
                let off = settings.attenuation_percent == 0;
                if off || now >= release_at {
                    if let Some(attenuator) = &self.attenuator {
                        attenuator.set(None);
                    }
                    self.lowered = None;
                    self.release_at = None;
                }
            }
            None => {}
        }
        if settings.attenuation_percent == 0 {
            // Dropping it puts everything back and closes its connection.
            self.attenuator = None;
        }
    }
}

/// The level other apps play at now (0–1), or `None` for their own.
fn wanted_level(settings: &AudioSettings, others: bool, talking: bool) -> Option<f32> {
    let percent = settings.attenuation_percent.min(100);
    let because = (settings.attenuate_when_others_speak && others)
        || (settings.attenuate_when_i_speak && talking);
    (percent > 0 && because).then(|| 1.0 - percent as f32 / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(percent: u32, others: bool, me: bool) -> AudioSettings {
        AudioSettings {
            attenuation_percent: percent,
            attenuate_when_others_speak: others,
            attenuate_when_i_speak: me,
            ..AudioSettings::default()
        }
    }

    #[test]
    fn other_apps_are_lowered_by_the_share_chosen_for_the_voices_chosen() {
        assert_eq!(
            wanted_level(&settings(0, true, true), true, true),
            None,
            "off"
        );
        assert_eq!(
            wanted_level(&settings(70, true, false), true, false),
            Some(0.3)
        );
        assert_eq!(wanted_level(&settings(70, true, false), false, true), None);
        assert_eq!(
            wanted_level(&settings(70, false, true), false, true),
            Some(0.3)
        );
        assert_eq!(
            wanted_level(&settings(100, true, true), true, false),
            Some(0.0)
        );
        assert_eq!(wanted_level(&settings(70, true, true), false, false), None);
    }

    #[test]
    fn other_apps_come_back_a_second_after_the_last_voice() {
        let start = Instant::now();
        let mut ducking = Ducking {
            lowered: Some(0.3),
            ..Ducking::default()
        };
        let on = settings(70, true, false);

        ducking.update(&on, start);
        assert_eq!(ducking.release_at(), Some(start + RELEASE));
        assert_eq!(ducking.lowered, Some(0.3), "not yet");

        ducking.update(&on, start + RELEASE);
        assert_eq!(ducking.lowered, None);
        assert_eq!(ducking.release_at(), None);
    }

    #[test]
    fn someone_speaking_again_keeps_other_apps_lowered() {
        let start = Instant::now();
        let mut ducking = Ducking {
            lowered: Some(0.3),
            ..Ducking::default()
        };
        let on = settings(70, true, false);
        ducking.update(&on, start);
        assert!(ducking.release_at().is_some());

        ducking.others_speaking(7, true);
        ducking.update(&on, start + Duration::from_millis(500));

        assert_eq!(ducking.release_at(), None);
        assert_eq!(ducking.lowered, Some(0.3));
    }
}
