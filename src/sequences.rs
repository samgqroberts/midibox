use std::ops::{Add, Sub};
use std::sync::Arc;
use crate::{Degree, Interval, Midi, Midibox, Scale, Tone};

// A looping sequence of statically defined notes.
#[derive(Debug, Clone)]
pub struct FixedSequence {
    /// The notes that can be produced by a sequence
    notes: Vec<Midi>,
    /// The index of the play head into notes
    head_position: usize,
}

impl FixedSequence {
    pub fn new(notes: Vec<Midi>) -> Self {
        return FixedSequence {
            notes,
            head_position: 0,
        };
    }

    pub fn empty() -> Self {
        return FixedSequence {
            notes: Vec::new(),
            head_position: 0,
        };
    }

    pub fn midibox(&self) -> Arc<dyn Midibox> {
        Arc::new(self.clone())
    }

    pub fn len(&self) -> usize {
        self.notes.len()
    }

    pub fn len_ticks(&self) -> u32 {
        return self.notes.clone().into_iter().map(|m| m.duration).sum();
    }

    pub fn fast_forward(mut self, ticks: usize) -> Self {
        self.head_position = (self.head_position + ticks) % self.notes.len();
        self
    }

    pub fn duration(mut self, duration: u32) -> Self {
        self.notes = self.notes.into_iter().map(|m| m.set_duration(duration)).collect();
        self
    }

    pub fn velocity(mut self, velocity: u8) -> Self {
        self.notes = self.notes.into_iter().map(|m| m.set_velocity(velocity)).collect();
        self
    }

    pub fn scale_duration(mut self, factor: u32) -> Self {
        self.notes = self.notes.into_iter().map(|m| m * factor).collect();
        self
    }

    pub fn extend(mut self, rhs: &Self) -> Self {
        let mut extend = self.notes;
        extend.append(&mut rhs.notes.clone());
        self.notes = extend;
        self
    }

    pub fn repeat(mut self, times: usize) -> Self {
        self.notes = self.notes.repeat(times);
        self
    }

    pub fn reverse(mut self) -> Self {
        self.notes = self.notes.into_iter().rev().collect();
        self
    }

    pub fn transpose_up(mut self, interval: Interval) -> Self {
        self.notes = self.notes.into_iter().map(|m| m + interval).collect();
        self
    }

    pub fn transpose_down(mut self, interval: Interval) -> Self {
        self.notes = self.notes.into_iter().map(|m| m - interval).collect();
        self
    }

    pub fn harmonize_up(mut self, scale: &Scale, degree: Degree) -> Self {
        self.notes = self.notes.into_iter()
            .map(|m| if m.is_rest() {
                m
            } else {
                scale
                    .harmonize_up(m, degree)
                    .unwrap_or_else(|| m.set_pitch(Tone::Rest, 4))
            })
            .collect();
        self
    }

    pub fn harmonize_down(mut self, scale: &Scale, degree: Degree) -> Self {
        self.notes = self.notes.into_iter()
            .map(|m| if m.is_rest() {
                m
            } else {
                scale
                    .harmonize_down(m, degree)
                    .unwrap_or_else(|| m.set_pitch(Tone::Rest, 4))
            })
            .collect();
        self
    }

    /// Splits each note into a series of metronome ticks adding to the note's duration
    pub fn split_to_ticks(mut self) -> Self {
        self.notes = self.notes.into_iter().flat_map(|m| {
            let old_duration = m.duration as usize;
            return vec![m.set_duration(1)].repeat(old_duration).into_iter();
        }).collect::<Vec<Midi>>();
        return self;
    }

    /// mask is a sequence of bits representing notes to play or mute
    ///
    /// If the bit corresponding to a note in this sequence is false, the note will be muted.
    ///
    /// The mask will be applied starting from the first note of the sequence and will repeat to
    /// match the total number of notes in this sequence.
    pub fn mask(mut self, mask: Vec<bool>) -> Self {
        self.notes = self.notes.into_iter()
            .zip(mask.into_iter().cycle()).map(|(midi, should_play)| {
            return if should_play {
                midi
            } else {
                midi.set_pitch(Tone::Rest, 4)
            };
        }).collect();
        self
    }


    pub fn split_notes(self, mask: Vec<bool>) -> Self {
        self.split_to_ticks().mask(mask)
    }
}

impl Add<FixedSequence> for FixedSequence {
    type Output = FixedSequence;

    fn add(self, rhs: FixedSequence) -> Self::Output {
        return self.clone().extend(&rhs.clone());
    }
}

impl Sub<Interval> for FixedSequence {
    type Output = FixedSequence;

    fn sub(self, rhs: Interval) -> Self::Output {
        return self.transpose_down(rhs);
    }
}

impl Add<Interval> for FixedSequence {
    type Output = FixedSequence;

    fn add(self, rhs: Interval) -> Self::Output {
        return self.transpose_up(rhs);
    }
}

impl Midibox for FixedSequence {
    fn render(&self) -> Vec<Vec<Midi>> {
        let size = self.notes.len();
        return
            self.notes
                .iter()
                .map(|m| vec![*m])
                .cycle()
                .skip(self.head_position)
                .take(size)
                .collect::<Vec<Vec<Midi>>>()
    }
}



