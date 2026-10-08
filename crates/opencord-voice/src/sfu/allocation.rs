//! Which layer of each video a receiver gets (plan §6): no taller than it
//! asked for, only layers the sender is producing, and within its downlink.
//! Everyone gets their lowest layer before anyone gets more; upgrades then
//! go in priority order: screen shares, then the largest tiles.

use std::cmp::Reverse;

/// Going above the layer being sent needs this much room beyond the new
/// layer's cost, and the layer being sent stays until the room falls this
/// far under its cost, so a downlink near a layer's cost does not flap.
pub(crate) const UPGRADE_HEADROOM: f64 = 0.15;
const DOWNGRADE_TOLERANCE: f64 = 0.15;

/// One layer of a video, as a receiver could get it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LayerOption {
    /// 0 for the lowest; taller layers have higher indexes.
    pub index: u8,
    pub height: u32,
    /// Bits per second it takes.
    pub cost: u64,
    /// The sender is producing it.
    pub available: bool,
}

/// A video a receiver wants.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Candidate<'a> {
    pub screen: bool,
    /// The height of its tile, from MediaSinkWants.
    pub want_height: u32,
    pub layers: &'a [LayerOption],
    /// The layer being sent now.
    pub current: Option<u8>,
}

/// The layer a receiver asked for, bandwidth aside: the tallest no taller
/// than its tile, or the smallest when every layer is taller.
pub(crate) fn desired(want_height: u32, layers: &[LayerOption]) -> Option<u8> {
    layers
        .iter()
        .filter(|layer| layer.height <= want_height)
        .max_by_key(|layer| layer.height)
        .or_else(|| layers.iter().min_by_key(|layer| layer.height))
        .map(|layer| layer.index)
}

/// The layer each candidate gets within `budget` bits per second, in the
/// candidates' order; `None` sends that video nothing.
pub(crate) fn allocate(budget: u64, candidates: &[Candidate<'_>]) -> Vec<Option<u8>> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    order.sort_by_key(|&i| {
        let candidate = &candidates[i];
        (Reverse(candidate.screen), Reverse(candidate.want_height), i)
    });
    let usable: Vec<Vec<LayerOption>> = candidates.iter().map(usable_layers).collect();
    let mut chosen: Vec<Option<usize>> = vec![None; candidates.len()];
    let mut remaining = budget;
    for &i in &order {
        let Some(lowest) = usable[i].first() else {
            continue;
        };
        let needed = lowest.cost as i64 + adjustment(&candidates[i], lowest);
        if needed <= remaining as i64 {
            remaining = remaining.saturating_sub(lowest.cost);
            chosen[i] = Some(0);
        }
    }
    for &i in &order {
        let Some(mut at) = chosen[i] else {
            continue;
        };
        while let (Some(from), Some(to)) = (usable[i].get(at), usable[i].get(at + 1)) {
            let extra = to.cost.saturating_sub(from.cost);
            if extra as i64 + adjustment(&candidates[i], to) > remaining as i64 {
                break;
            }
            remaining = remaining.saturating_sub(extra);
            at += 1;
        }
        chosen[i] = Some(at);
    }
    chosen
        .iter()
        .zip(&usable)
        .map(|(at, layers)| at.and_then(|at| layers.get(at)).map(|layer| layer.index))
        .collect()
}

/// Layers the sender produces, no taller than the desired one, lowest
/// first.
fn usable_layers(candidate: &Candidate<'_>) -> Vec<LayerOption> {
    let Some(top) = desired(candidate.want_height, candidate.layers) else {
        return Vec::new();
    };
    let mut layers: Vec<LayerOption> = candidate
        .layers
        .iter()
        .filter(|layer| layer.available && layer.index <= top)
        .copied()
        .collect();
    layers.sort_by_key(|layer| layer.index);
    layers
}

/// How much more (or less) than its cost taking `layer` needs.
fn adjustment(candidate: &Candidate<'_>, layer: &LayerOption) -> i64 {
    let cost = layer.cost as f64;
    match candidate.current {
        Some(current) if layer.index > current => (cost * UPGRADE_HEADROOM) as i64,
        Some(current) if layer.index == current => -((cost * DOWNGRADE_TOLERANCE) as i64),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Vec<LayerOption> {
        vec![
            LayerOption {
                index: 0,
                height: 180,
                cost: 150_000,
                available: true,
            },
            LayerOption {
                index: 1,
                height: 360,
                cost: 500_000,
                available: true,
            },
            LayerOption {
                index: 2,
                height: 720,
                cost: 1_500_000,
                available: true,
            },
        ]
    }

    fn wanting(height: u32, layers: &[LayerOption]) -> Candidate<'_> {
        Candidate {
            screen: false,
            want_height: height,
            layers,
            current: None,
        }
    }

    #[test]
    fn the_desired_layer_is_the_tallest_within_the_want() {
        let layers = camera();

        assert_eq!(desired(720, &layers), Some(2));
        assert_eq!(desired(1080, &layers), Some(2));
        assert_eq!(desired(500, &layers), Some(1));
        assert_eq!(desired(360, &layers), Some(1));
        // Smaller than every layer: the smallest.
        assert_eq!(desired(90, &layers), Some(0));
        assert_eq!(desired(720, &[]), None);
    }

    #[test]
    fn with_room_everyone_gets_what_they_asked_for() {
        let layers = camera();
        let candidates = [
            wanting(720, &layers),
            wanting(360, &layers),
            wanting(100, &layers),
        ];

        assert_eq!(
            allocate(10_000_000, &candidates),
            vec![Some(2), Some(1), Some(0)]
        );
    }

    #[test]
    fn layers_the_sender_is_not_producing_are_skipped() {
        let mut layers = camera();
        layers[2].available = false;

        assert_eq!(
            allocate(10_000_000, &[wanting(720, &layers)]),
            vec![Some(1)]
        );
        layers[0].available = false;
        layers[1].available = false;
        assert_eq!(allocate(10_000_000, &[wanting(720, &layers)]), vec![None]);
    }

    #[test]
    fn a_layer_above_the_want_is_never_sent() {
        let mut layers = camera();
        layers[0].available = false;

        assert_eq!(allocate(10_000_000, &[wanting(180, &layers)]), vec![None]);
    }

    #[test]
    fn a_500_kbps_downlink_gets_the_low_layer() {
        let layers = camera();

        // 500 kbps less headroom and audio.
        assert_eq!(allocate(380_000, &[wanting(720, &layers)]), vec![Some(0)]);
        assert_eq!(allocate(100_000, &[wanting(720, &layers)]), vec![None]);
    }

    #[test]
    fn everyone_gets_the_lowest_layer_before_anyone_gets_more() {
        let layers = camera();
        let candidates = [
            wanting(720, &layers),
            wanting(720, &layers),
            wanting(720, &layers),
        ];

        assert_eq!(
            allocate(800_000, &candidates),
            vec![Some(1), Some(0), Some(0)]
        );
    }

    #[test]
    fn screen_shares_and_large_tiles_come_first() {
        let layers = camera();
        let screen = Candidate {
            screen: true,
            ..wanting(720, &layers)
        };
        let candidates = [wanting(180, &layers), wanting(720, &layers), screen];

        // Room for three low layers and one step up: the screen share's.
        assert_eq!(
            allocate(450_000 + 350_000, &candidates),
            vec![Some(0), Some(0), Some(1)]
        );
        // Without the screen share, the larger tile gets the step up.
        assert_eq!(
            allocate(150_000 + 500_000, &candidates[..2]),
            vec![Some(0), Some(1)]
        );
    }

    #[test]
    fn too_little_for_everyone_leaves_the_last_without_video() {
        let layers = camera();
        let candidates = [wanting(180, &layers), wanting(720, &layers)];

        assert_eq!(allocate(200_000, &candidates), vec![None, Some(0)]);
    }

    #[test]
    fn the_layer_being_sent_rides_out_a_dip_of_15_percent() {
        let layers = camera();
        let staying = Candidate {
            current: Some(1),
            ..wanting(720, &layers)
        };

        // 88 % of what the middle layer and the low one below it cost.
        assert_eq!(allocate(440_000, &[staying]), vec![Some(1)]);
        assert_eq!(allocate(400_000, &[staying]), vec![Some(0)]);
        // Without it being sent, the same downlink gets the low layer.
        assert_eq!(allocate(440_000, &[wanting(720, &layers)]), vec![Some(0)]);
    }

    #[test]
    fn going_up_needs_headroom_but_staying_does_not() {
        let layers = camera();
        let staying = Candidate {
            current: Some(2),
            ..wanting(720, &layers)
        };
        let rising = Candidate {
            current: Some(1),
            ..wanting(720, &layers)
        };

        assert_eq!(allocate(1_500_000, &[staying]), vec![Some(2)]);
        assert_eq!(allocate(1_500_000, &[rising]), vec![Some(1)]);
        assert_eq!(allocate(1_800_000, &[rising]), vec![Some(2)]);
    }
}
