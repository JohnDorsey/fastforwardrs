//! Joining a new stretch of sound onto the sound that is waiting at the speakers.
//!
//! When something that is heard is edited while the video plays (a volume is changed, say), the
//! sound is made again from the playhead on. The sound that was already waiting at the speakers is
//! still right for the next moment or two, but not after that, so the new sound takes over from the
//! old one a little way into the queue. To keep the join from being heard as a click the old
//! sound fades out while the new one fades in.

use std::collections::VecDeque;

/// Replaces the end of the sound waiting at the speakers, `waiting` (moments of `channel_count`
/// samples, one after another): the first `keep_moments` stay, and everything after them is
/// replaced by `new_sound`, which starts where they end.
///
/// Over the first `fade_moments` of `new_sound` the sound that it replaces fades out, in a straight
/// line, as the new sound fades in. Where there was nothing waiting that far on, the new sound is
/// heard at full strength at once.
pub fn splice_waiting_sound(
    waiting: &mut VecDeque<f32>,
    channel_count: usize,
    keep_moments: u64,
    fade_moments: u64,
    new_sound: &[f32],
) {
    let channel_count = channel_count.max(1);
    // Whole moments only: a moment is kept entirely or not at all.
    let keep = usize::try_from(keep_moments)
        .unwrap_or(usize::MAX)
        .saturating_mul(channel_count)
        .min(waiting.len());
    let keep = keep - keep % channel_count;
    let fade_moments = usize::try_from(fade_moments).unwrap_or(usize::MAX);

    let joined: Vec<f32> = new_sound
        .iter()
        .enumerate()
        .map(|(index, &new)| {
            let moment = index / channel_count;
            match waiting.get(keep + index) {
                Some(&old) if moment < fade_moments => {
                    // How far into the fade this moment is, strictly between 0 and 1.
                    let new_part = (moment as f32 + 0.5) / fade_moments as f32;
                    old * (1.0 - new_part) + new * new_part
                }
                _ => new,
            }
        })
        .collect();
    waiting.truncate(keep);
    waiting.extend(joined);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue_of(samples: &[f32]) -> VecDeque<f32> {
        samples.iter().copied().collect()
    }

    fn contents(queue: &VecDeque<f32>) -> Vec<f32> {
        queue.iter().copied().collect()
    }

    #[test]
    fn the_first_moments_stay_and_the_new_sound_goes_on_where_they_end() {
        let mut waiting = queue_of(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
        splice_waiting_sound(&mut waiting, 1, 3, 0, &[100.0, 101.0, 102.0]);
        assert_eq!(contents(&waiting), vec![0.0, 1.0, 2.0, 100.0, 101.0, 102.0]);
    }

    #[test]
    fn the_new_sound_may_be_longer_or_shorter_than_the_sound_it_replaces() {
        let mut waiting = queue_of(&[1.0; 4]);
        splice_waiting_sound(&mut waiting, 1, 1, 0, &[9.0; 10]);
        assert_eq!(waiting.len(), 11);
        let mut waiting = queue_of(&[1.0; 10]);
        splice_waiting_sound(&mut waiting, 1, 2, 0, &[9.0; 3]);
        assert_eq!(waiting.len(), 5, "what is waiting is cut off where the new sound ends");
        let mut waiting = queue_of(&[1.0; 10]);
        splice_waiting_sound(&mut waiting, 1, 2, 0, &[]);
        assert_eq!(waiting.len(), 2, "no new sound is the same as cutting the rest off");
    }

    #[test]
    fn keeping_more_than_is_waiting_keeps_all_of_it_and_adds_the_new_sound_to_the_end() {
        let mut waiting = queue_of(&[1.0, 2.0]);
        splice_waiting_sound(&mut waiting, 1, 50, 10, &[7.0, 8.0]);
        assert_eq!(contents(&waiting), vec![1.0, 2.0, 7.0, 8.0]);
        let mut nothing_waiting = VecDeque::new();
        splice_waiting_sound(&mut nothing_waiting, 1, 5, 10, &[7.0, 8.0]);
        assert_eq!(
            contents(&nothing_waiting),
            vec![7.0, 8.0],
            "straight away at full strength"
        );
    }

    #[test]
    fn the_old_sound_fades_out_as_the_new_fades_in_in_a_straight_line() {
        let mut waiting = queue_of(&[1.0; 40]);
        splice_waiting_sound(&mut waiting, 1, 5, 10, &[3.0; 30]);
        let joined = contents(&waiting);
        assert_eq!(joined.len(), 35);
        assert!(
            joined[..5].iter().all(|sample| *sample == 1.0),
            "what is kept is untouched"
        );
        let fade = &joined[5..15];
        assert!(fade[0] > 1.0 && fade[0] < 1.2, "starts as the old sound: {}", fade[0]);
        assert!(fade[9] > 2.8 && fade[9] < 3.0, "ends as the new sound: {}", fade[9]);
        assert!(
            fade.windows(2).all(|pair| pair[1] > pair[0]),
            "steadily from one to the other: {fade:?}"
        );
        let steps: Vec<f32> = fade.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            steps.iter().all(|step| (step - steps[0]).abs() < 1e-5),
            "in a straight line: {steps:?}"
        );
        assert!(
            joined[15..].iter().all(|sample| *sample == 3.0),
            "then only the new sound"
        );
    }

    #[test]
    fn where_the_old_sound_runs_out_during_the_fade_the_new_sound_is_at_full_strength() {
        let mut waiting = queue_of(&[1.0; 8]);
        splice_waiting_sound(&mut waiting, 1, 5, 10, &[3.0; 20]);
        let joined = contents(&waiting);
        // Three moments of old sound are faded with; after them nothing is left to fade out.
        assert!(joined[5] < 3.0 && joined[6] < 3.0 && joined[7] < 3.0);
        assert!(joined[8..].iter().all(|sample| *sample == 3.0), "{joined:?}");
    }

    #[test]
    fn the_channels_of_a_moment_are_kept_together_and_faded_together() {
        // Left is the number, right is minus the number, as in the tests of the speakers.
        let old: Vec<f32> = (0..30)
            .flat_map(|moment| [10.0 + moment as f32, -(10.0 + moment as f32)])
            .collect();
        let new: Vec<f32> = (0..20)
            .flat_map(|moment| [50.0 + moment as f32, -(50.0 + moment as f32)])
            .collect();
        let mut waiting = queue_of(&old);
        splice_waiting_sound(&mut waiting, 2, 4, 6, &new);
        let joined = contents(&waiting);
        assert_eq!(joined.len(), 2 * (4 + 20));
        for moment in joined.as_chunks::<2>().0 {
            assert_eq!(moment[0], -moment[1], "{moment:?}");
        }
        assert_eq!(&joined[..8], &old[..8], "four whole moments were kept");
    }

    #[test]
    fn a_keep_that_would_split_a_moment_is_made_whole() {
        // Asking to keep three moments of a stereo queue of three moments: all six samples.
        let mut waiting = queue_of(&[1.0; 6]);
        splice_waiting_sound(&mut waiting, 2, 3, 0, &[9.0, 9.0]);
        assert_eq!(waiting.len(), 8);
        // A queue that has an odd number of samples (it should never) does not make a half moment of the keep.
        let mut waiting = queue_of(&[1.0; 5]);
        splice_waiting_sound(&mut waiting, 2, 3, 0, &[9.0, 9.0]);
        assert_eq!(waiting.len() % 2, 0, "{waiting:?}");
    }
}
