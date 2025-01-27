use crate::Midibox;
use rand::Rng;
use crate::midi::Midi;

pub struct RandomVelocity {
    factor: f64,
    midibox: Box<dyn Midibox>,
}

impl RandomVelocity {
    pub fn wrap(midibox: Box<dyn Midibox>) -> Box<dyn Midibox> {
        Box::new(RandomVelocity {
            factor: 1_f64,
            midibox
        })
    }
}

impl Midibox for RandomVelocity {
    fn next(&mut self) -> Option<Vec<Midi>> {
        let v = rand::thread_rng().gen_range(0..99);
        self.factor = (v as f64) / (100_f64);
        self.midibox.next()
            .map(|it|
                it.into_iter()
                    .map(|note| {
                        note.set_velocity((note.velocity as f64 * self.factor) as u8)
                    }).collect::<Vec<Midi>>()
            )
    }
}
