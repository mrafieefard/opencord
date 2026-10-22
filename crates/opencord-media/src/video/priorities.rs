//! What a sender encodes within its uplink (plan §7.10). The bandwidth
//! estimate is the budget; voice comes off the top and is never starved.
//! As the budget shrinks: the camera's top layer goes, then its middle
//! layer, then the screen share's low layer; then the screen's frame rate
//! comes down (to 5 fps), then its resolution. A screen shared at more
//! than 30 fps is "motion" content (games, video) and shrinks first,
//! keeping its frame rate as long as it can (plan §9.2). The camera's low layer and
//! the screen's main layer always stay (their encoders get less instead): a
//! sender that sends nothing never learns that its uplink came back.
//!
//! A plan rides out a dip to 85 % of its cost (an estimate settles not far
//! above what is sent); a cut beyond that takes effect at once. The plan
//! being sent costs what its layers actually send, with room to grow, when
//! that is less than their maxima: encoders often send well under their
//! targets, and the estimate settles near what is sent, so holding on to
//! the maxima would cut layers that fit. Recovery is one step a second,
//! each needing room for the maxima, so the estimator probes before a
//! layer comes back.

use std::time::{Duration, Instant};

use crate::transport::{Layer, TrackKind};

/// The frame rates a screen share steps down through (plan §9.2:
/// "detail" content keeps its resolution and slows down first).
const SCREEN_FPS_STEPS: [u32; 4] = [20, 15, 10, 5];
/// Then the share of its pixels it keeps.
const SIZE_STEPS: [f32; 3] = [0.75, 0.5, 0.25];
/// A screen above this frame rate is "motion" content: size goes first.
const MOTION_ABOVE: u32 = 30;
/// The plan being sent stays until the budget falls this far below its
/// cost.
const TOLERANCE: f64 = 0.15;
/// Undoing a cut needs this much room beyond its cost...
const HEADROOM: f64 = 0.15;
/// ...and at least this long since the last change.
const RAISE_EVERY: Duration = Duration::from_secs(1);
/// The least share of its maximum an encoder is given.
pub const MIN_SHARE: f64 = 0.3;
/// Shares go in steps of this, so a wavering estimate does not keep
/// changing encoders.
const SHARE_STEP: f64 = 0.1;
/// What a layer being sent is allowed beyond what it sends.
const HOLD_MARGIN_PERCENT: u64 = 125;

/// A track being published, as the plan sees it.
#[derive(Debug, Clone, Copy)]
pub struct Sending<'a> {
    pub kind: TrackKind,
    /// Lowest first.
    pub layers: &'a [Layer],
    /// The layers the voice node says anyone needs.
    pub wanted: &'a [bool],
    /// Bits per second each layer sent over the last second; empty when
    /// unknown.
    pub sent: &'a [u64],
}

/// What to encode of one track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackPlan {
    /// By layer, lowest first.
    pub active: Vec<bool>,
    /// A screen share's main layer: share of its frame rate and of its
    /// pixels.
    pub fps_scale: f32,
    pub size_scale: f32,
    /// Bits per second for each layer's encoder, 0 for layers off: their
    /// maxima, all turned down together when even the lowest layers do
    /// not fit, but never below [`MIN_SHARE`] of them.
    pub bitrates: Vec<u32>,
}

impl Sending<'_> {
    /// Bits per second it sends under `plan`, at its layers' maxima.
    pub fn bitrate(&self, plan: &TrackPlan) -> u64 {
        self.layer_bitrates(plan).into_iter().map(u64::from).sum()
    }

    /// What `plan` costs while it is being sent: each active layer at what
    /// it sends, with room to grow, when that is under its share.
    fn held_bitrate(&self, plan: &TrackPlan) -> u64 {
        self.layer_bitrates(plan)
            .into_iter()
            .enumerate()
            .map(|(index, planned)| {
                let planned = u64::from(planned);
                match self.sent.get(index) {
                    Some(&sent) if sent > 0 && planned > 0 => {
                        planned.min(sent.saturating_mul(HOLD_MARGIN_PERCENT) / 100)
                    }
                    _ => planned,
                }
            })
            .sum()
    }

    fn wants(&self, layer: usize) -> bool {
        self.wanted.get(layer) == Some(&true)
    }

    fn full(&self) -> TrackPlan {
        TrackPlan {
            active: (0..self.layers.len())
                .map(|layer| self.wants(layer))
                .collect(),
            fps_scale: 1.0,
            size_scale: 1.0,
            bitrates: Vec::new(),
        }
    }

    /// Each layer's bitrate under `plan` at its maximum, 0 when off.
    fn layer_bitrates(&self, plan: &TrackPlan) -> Vec<u32> {
        let main = self.layers.len().saturating_sub(1);
        self.layers
            .iter()
            .enumerate()
            .map(|(index, layer)| {
                if plan.active.get(index) != Some(&true) {
                    return 0;
                }
                let full = f64::from(layer.max_bitrate);
                let scaled = if self.kind == TrackKind::Screen && index == main {
                    full * f64::from(plan.fps_scale) * f64::from(plan.size_scale)
                } else {
                    full
                };
                scaled as u32
            })
            .collect()
    }
}

/// One step down, in the order the plan gives.
#[derive(Debug, Clone, Copy)]
enum Cut {
    Layer { track: usize, layer: usize },
    Fps { track: usize, scale: f32 },
    Size { track: usize, scale: f32 },
}

/// The tracks and wants a run of plans was made for.
type Signature = Vec<(TrackKind, usize, Vec<bool>)>;

/// Plans one after another, remembering the last.
#[derive(Debug, Default)]
pub struct Planner {
    /// How many cuts the last plan made, for which tracks, and when it
    /// last changed.
    cuts: Option<usize>,
    signature: Signature,
    changed: Option<Instant>,
}

impl Planner {
    pub fn new() -> Self {
        Self::default()
    }

    /// What each track sends within `budget` bits per second, `voice` of
    /// it kept for voice. New tracks or wants start afresh.
    pub fn update(
        &mut self,
        now: Instant,
        budget: u64,
        voice: u64,
        tracks: &[Sending<'_>],
    ) -> Vec<TrackPlan> {
        let signature: Signature = tracks
            .iter()
            .map(|track| (track.kind, track.layers.len(), track.wanted.to_vec()))
            .collect();
        if signature != self.signature {
            self.signature = signature;
            self.cuts = None;
        }
        let steps = cuts(tracks);
        let available = budget.saturating_sub(voice) as f64;
        let cost = |count: usize| -> f64 {
            let plans = apply(tracks, &steps[..count]);
            tracks
                .iter()
                .zip(&plans)
                .map(|(track, plan)| track.bitrate(plan))
                .sum::<u64>() as f64
        };
        let held = |count: usize| -> f64 {
            let plans = apply(tracks, &steps[..count]);
            tracks
                .iter()
                .zip(&plans)
                .map(|(track, plan)| track.held_bitrate(plan))
                .sum::<u64>() as f64
        };
        let fits = (0..=steps.len())
            .find(|count| cost(*count) <= available)
            .unwrap_or(steps.len());
        let count = match self.cuts {
            None => fits,
            // Cut no further than what is sent needs.
            Some(previous) if held(previous) * (1.0 - TOLERANCE) > available => (previous
                ..=steps.len())
                .find(|count| held(*count) <= available)
                .unwrap_or(steps.len()),
            Some(previous) if fits < previous => {
                let rested = self
                    .changed
                    .is_none_or(|at| now.saturating_duration_since(at) >= RAISE_EVERY);
                let roomy = cost(previous - 1) * (1.0 + HEADROOM) <= available;
                if rested && roomy {
                    previous - 1
                } else {
                    previous
                }
            }
            Some(previous) => previous,
        };
        if self.cuts != Some(count) {
            self.cuts = Some(count);
            self.changed = Some(now);
        }
        fit(apply(tracks, &steps[..count]), available, held(count))
    }
}

/// Every step down, mildest first. Layers nobody wants are off already; a
/// camera's low layer and a screen's main layer are never cut.
fn cuts(tracks: &[Sending<'_>]) -> Vec<Cut> {
    let of_kind = |kind: TrackKind| {
        tracks
            .iter()
            .enumerate()
            .filter(move |(_, track)| track.kind == kind)
    };
    let mut steps = Vec::new();
    for (track, sending) in of_kind(TrackKind::Camera) {
        for layer in (1..sending.layers.len()).rev() {
            if sending.wants(layer) {
                steps.push(Cut::Layer { track, layer });
            }
        }
    }
    for (track, sending) in of_kind(TrackKind::Screen) {
        if sending.layers.len() > 1 && sending.wants(0) {
            steps.push(Cut::Layer { track, layer: 0 });
        }
    }
    for (track, sending) in of_kind(TrackKind::Screen) {
        let Some(main) = sending.layers.len().checked_sub(1) else {
            continue;
        };
        if !sending.wants(main) {
            continue;
        }
        let fps = sending.layers[main].fps;
        let slower = SCREEN_FPS_STEPS
            .into_iter()
            .filter(|step| *step < fps)
            .map(|step| Cut::Fps {
                track,
                scale: step as f32 / fps as f32,
            });
        let smaller = SIZE_STEPS
            .into_iter()
            .map(|scale| Cut::Size { track, scale });
        if fps > MOTION_ABOVE {
            steps.extend(smaller.chain(slower));
        } else {
            steps.extend(slower.chain(smaller));
        }
    }
    steps
}

fn apply(tracks: &[Sending<'_>], steps: &[Cut]) -> Vec<TrackPlan> {
    let mut plans: Vec<TrackPlan> = tracks.iter().map(Sending::full).collect();
    for step in steps {
        match *step {
            Cut::Layer { track, layer } => plans[track].active[layer] = false,
            Cut::Fps { track, scale } => plans[track].fps_scale = scale,
            Cut::Size { track, scale } => plans[track].size_scale = scale,
        }
    }
    for (plan, track) in plans.iter_mut().zip(tracks) {
        plan.bitrates = track.layer_bitrates(plan);
    }
    plans
}

/// Turns every encoder down together when the plan still does not fit.
/// `held` is what the plan costs as it is being sent.
fn fit(mut plans: Vec<TrackPlan>, available: f64, held: f64) -> Vec<TrackPlan> {
    let cost: f64 = plans
        .iter()
        .flat_map(|plan| &plan.bitrates)
        .map(|&bitrate| f64::from(bitrate))
        .sum::<f64>()
        .min(held);
    if cost <= available || cost == 0.0 {
        return plans;
    }
    let share = ((available / cost / SHARE_STEP).floor() * SHARE_STEP).clamp(MIN_SHARE, 1.0);
    for bitrate in plans.iter_mut().flat_map(|plan| &mut plan.bitrates) {
        *bitrate = (f64::from(*bitrate) * share) as u32;
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(rid: &str, height: u32, fps: u32, max_bitrate: u32) -> Layer {
        Layer {
            rid: rid.to_owned(),
            width: height * 16 / 9,
            height,
            fps,
            max_bitrate,
        }
    }

    fn camera() -> Vec<Layer> {
        vec![
            layer("l", 180, 15, 150_000),
            layer("m", 360, 30, 500_000),
            layer("h", 720, 30, 1_500_000),
        ]
    }

    fn screen() -> Vec<Layer> {
        vec![
            layer("l", 360, 15, 300_000),
            layer("h", 1080, 30, 2_000_000),
        ]
    }

    const VOICE: u64 = 64_000;

    fn on(layers: &[Layer]) -> Vec<bool> {
        vec![true; layers.len()]
    }

    fn sending<'a>(kind: TrackKind, layers: &'a [Layer], wanted: &'a [bool]) -> Sending<'a> {
        Sending {
            kind,
            layers,
            wanted,
            sent: &[],
        }
    }

    /// A first plan, from nothing.
    fn plan(budget: u64, tracks: &[Sending<'_>]) -> Vec<TrackPlan> {
        Planner::new().update(Instant::now(), budget, VOICE, tracks)
    }

    #[test]
    fn with_room_every_wanted_layer_is_sent_in_full() {
        let layers = camera();
        let wanted = on(&layers);

        let plan = plan(10_000_000, &[sending(TrackKind::Camera, &layers, &wanted)]);

        assert_eq!(plan[0].active, vec![true, true, true]);
        assert_eq!(plan[0].fps_scale, 1.0);
        assert_eq!(plan[0].size_scale, 1.0);
    }

    #[test]
    fn layers_nobody_wants_are_never_sent() {
        let layers = camera();
        let wanted = vec![true, false, true];

        let plan = plan(10_000_000, &[sending(TrackKind::Camera, &layers, &wanted)]);

        assert_eq!(plan[0].active, vec![true, false, true]);
    }

    #[test]
    fn the_camera_loses_its_top_layer_then_its_middle_one_and_keeps_its_low_one() {
        let layers = camera();
        let wanted = on(&layers);
        let camera = [sending(TrackKind::Camera, &layers, &wanted)];

        assert_eq!(
            plan(VOICE + 1_000_000, &camera)[0].active,
            vec![true, true, false]
        );
        assert_eq!(
            plan(VOICE + 400_000, &camera)[0].active,
            vec![true, false, false]
        );
        // Below even that, its encoder gets less, but the track stays.
        assert_eq!(plan(VOICE, &camera)[0].active, vec![true, false, false]);
    }

    #[test]
    fn a_camera_asked_only_for_its_top_layer_can_lose_it() {
        // The node then asks for the layer below (it backs a layer that
        // does not come with the one under it).
        let layers = camera();
        let wanted = vec![false, false, true];

        let plan = plan(
            VOICE + 1_000_000,
            &[sending(TrackKind::Camera, &layers, &wanted)],
        );

        assert_eq!(plan[0].active, vec![false, false, false]);
    }

    #[test]
    fn voice_comes_off_the_top() {
        let layers = camera();
        let wanted = on(&layers);
        let camera = [sending(TrackKind::Camera, &layers, &wanted)];

        assert_eq!(
            plan(VOICE + 650_000, &camera)[0].active,
            vec![true, true, false]
        );
        assert_eq!(plan(650_000, &camera)[0].active, vec![true, false, false]);
    }

    #[test]
    fn a_screen_share_outlasts_the_cameras_upper_layers() {
        let cam = camera();
        let scr = screen();
        let (cam_wanted, scr_wanted) = (on(&cam), on(&scr));
        let both = [
            sending(TrackKind::Camera, &cam, &cam_wanted),
            sending(TrackKind::Screen, &scr, &scr_wanted),
        ];

        // Room for everything but the camera's top layer.
        let first = plan(VOICE + 3_000_000, &both);
        assert_eq!(first[0].active, vec![true, true, false]);
        assert_eq!(first[1].active, vec![true, true]);
        // Then the camera's middle layer, then the screen's low layer.
        let second = plan(VOICE + 2_400_000, &both);
        assert_eq!(second[0].active, vec![true, false, false]);
        assert_eq!(second[1].active, vec![false, true]);
        assert_eq!(second[1].fps_scale, 1.0);
    }

    #[test]
    fn then_the_screen_slows_down_beside_the_cameras_low_layer() {
        let cam = camera();
        let scr = screen();
        let (cam_wanted, scr_wanted) = (on(&cam), on(&scr));
        let both = [
            sending(TrackKind::Camera, &cam, &cam_wanted),
            sending(TrackKind::Screen, &scr, &scr_wanted),
        ];

        let plan = plan(VOICE + 2_050_000, &both);

        assert_eq!(plan[0].active, vec![true, false, false]);
        assert_eq!(plan[1].active, vec![false, true]);
        assert!((plan[1].fps_scale * 30.0 - 20.0).abs() < 0.01);
    }

    #[test]
    fn then_the_screen_slows_down_to_5_fps_and_then_shrinks() {
        let scr = screen();
        let wanted = on(&scr);
        let share = [sending(TrackKind::Screen, &scr, &wanted)];

        let slower = plan(VOICE + 1_000_000, &share);
        assert_eq!(slower[0].fps_scale, 0.5);
        assert_eq!(slower[0].size_scale, 1.0);

        let smaller = plan(VOICE + 250_000, &share);
        assert!((smaller[0].fps_scale * 30.0 - 5.0).abs() < 0.01);
        assert_eq!(smaller[0].size_scale, 0.75);
        assert_eq!(smaller[0].active, vec![false, true]);

        // As small as it gets, but still sent.
        let smallest = plan(VOICE, &share);
        assert_eq!(smallest[0].size_scale, 0.25);
        assert_eq!(smallest[0].active, vec![false, true]);
    }

    #[test]
    fn a_60_fps_screen_keeps_its_frame_rate_and_shrinks_first() {
        let scr = vec![
            layer("l", 360, 15, 300_000),
            layer("h", 1080, 60, 6_500_000),
        ];
        let wanted = on(&scr);
        let share = [sending(TrackKind::Screen, &scr, &wanted)];

        let smaller = plan(VOICE + 3_500_000, &share);
        assert_eq!(smaller[0].active, vec![false, true]);
        assert_eq!(smaller[0].fps_scale, 1.0);
        assert_eq!(smaller[0].size_scale, 0.5);

        // Only at a quarter of its pixels does it slow down.
        let slower = plan(VOICE + 1_000_000, &share);
        assert_eq!(slower[0].size_scale, 0.25);
        assert!(slower[0].fps_scale < 1.0);
    }

    #[test]
    fn a_plan_sending_under_its_maximum_is_held_on_what_it_sends() {
        let layers = camera();
        let wanted = on(&layers);
        let mut planner = Planner::new();
        let now = Instant::now();
        let full = [sending(TrackKind::Camera, &layers, &wanted)];
        planner.update(now, 10_000_000, VOICE, &full);

        // The encoders send about half their maxima; the estimate settles
        // near 1.5 times that, under the declared 2.15 Mbit/s.
        let sent = [80_000, 250_000, 750_000];
        let measured = [Sending {
            sent: &sent,
            ..full[0]
        }];
        let held = planner.update(now, VOICE + 1_600_000, VOICE, &measured);

        assert_eq!(held[0].active, vec![true, true, true]);
        assert_eq!(held[0].bitrates, vec![150_000, 500_000, 1_500_000]);
        // From scratch it starts lower: going up takes the maxima.
        let fresh = Planner::new().update(now, VOICE + 1_600_000, VOICE, &measured);
        assert_eq!(fresh[0].active, vec![true, true, false]);
    }

    #[test]
    fn each_layer_gets_its_bitrate_and_layers_off_get_none() {
        let layers = camera();
        let wanted = vec![true, false, true];

        let plan = plan(10_000_000, &[sending(TrackKind::Camera, &layers, &wanted)]);

        assert_eq!(plan[0].bitrates, vec![150_000, 0, 1_500_000]);
    }

    #[test]
    fn when_even_the_lowest_layers_do_not_fit_their_encoders_are_turned_down() {
        let layers = camera();
        let wanted = on(&layers);
        let camera = [sending(TrackKind::Camera, &layers, &wanted)];

        // 100 of 150 kbit/s: two thirds, rounded down to a 10 % step.
        assert_eq!(
            plan(VOICE + 100_000, &camera)[0].bitrates,
            vec![90_000, 0, 0]
        );
        // Never below 30 %.
        assert_eq!(
            plan(VOICE + 10_000, &camera)[0].bitrates,
            vec![45_000, 0, 0]
        );
    }

    #[test]
    fn a_plan_rides_out_a_dip_to_85_percent_of_its_cost() {
        let layers = camera();
        let wanted = on(&layers);
        let camera = [sending(TrackKind::Camera, &layers, &wanted)];
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let mut planner = Planner::new();
        let mut update = |millis, budget| {
            planner.update(at(millis), budget, VOICE, &camera)[0]
                .active
                .clone()
        };

        assert_eq!(update(0, VOICE + 700_000), vec![true, true, false]);
        assert_eq!(update(100, VOICE + 560_000), vec![true, true, false]);
        assert_eq!(update(200, VOICE + 540_000), vec![true, false, false]);
    }

    #[test]
    fn a_cut_is_immediate_and_recovery_is_one_step_a_second() {
        let layers = camera();
        let wanted = on(&layers);
        let camera = [sending(TrackKind::Camera, &layers, &wanted)];
        let start = Instant::now();
        let at = |millis| start + Duration::from_millis(millis);
        let mut planner = Planner::new();
        let mut update = |millis, budget| {
            planner.update(at(millis), budget, VOICE, &camera)[0]
                .active
                .clone()
        };

        assert_eq!(update(0, VOICE + 1_000_000), vec![true, true, false]);
        assert_eq!(update(100, VOICE + 400_000), vec![true, false, false]);
        assert_eq!(update(500, VOICE + 10_000_000), vec![true, false, false]);
        assert_eq!(update(1_200, VOICE + 10_000_000), vec![true, true, false]);
        assert_eq!(update(1_500, VOICE + 10_000_000), vec![true, true, false]);
        assert_eq!(update(2_300, VOICE + 10_000_000), vec![true, true, true]);
    }

    #[test]
    fn something_cut_comes_back_only_with_room_to_spare() {
        let layers = camera();
        let wanted = on(&layers);
        let camera = [sending(TrackKind::Camera, &layers, &wanted)];
        let start = Instant::now();
        let mut planner = Planner::new();
        planner.update(start, VOICE + 1_000_000, VOICE, &camera);

        // Exactly enough for the top layer again is not enough.
        let exact = planner.update(
            start + Duration::from_secs(2),
            VOICE + 2_150_000,
            VOICE,
            &camera,
        );
        assert_eq!(exact[0].active, vec![true, true, false]);
        let roomy = planner.update(
            start + Duration::from_secs(3),
            VOICE + 2_500_000,
            VOICE,
            &camera,
        );
        assert_eq!(roomy[0].active, vec![true, true, true]);
    }

    #[test]
    fn new_wants_start_afresh() {
        let layers = camera();
        let (all, two) = (on(&layers), vec![true, true, false]);
        let start = Instant::now();
        let mut planner = Planner::new();
        planner.update(
            start,
            VOICE + 400_000,
            VOICE,
            &[sending(TrackKind::Camera, &layers, &two)],
        );

        let plan = planner.update(
            start + Duration::from_millis(100),
            VOICE + 10_000_000,
            VOICE,
            &[sending(TrackKind::Camera, &layers, &all)],
        );

        assert_eq!(plan[0].active, vec![true, true, true]);
    }

    #[test]
    fn the_bitrate_a_plan_sends_counts_scales() {
        let scr = screen();
        let wanted = on(&scr);
        let track = sending(TrackKind::Screen, &scr, &wanted);
        let plan = TrackPlan {
            active: vec![false, true],
            fps_scale: 0.5,
            size_scale: 0.5,
            bitrates: Vec::new(),
        };

        assert_eq!(track.bitrate(&plan), 500_000);
    }
}
