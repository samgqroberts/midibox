use log::{debug, error, info};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread::sleep;

use crossbeam::atomic::AtomicCell;
use ctrlc;
use midir::{MidiOutput, MidiOutputConnection};
use crate::Midibox;
use crate::meter::Meter;
use crate::midi::{Midi, NOTE_OFF_MSG, NOTE_ON_MSG};
use crate::router::{Router, StaticRouter};


pub struct Player {
    /// Describes the time spent playing in ticks.
    tick_id: u64,
    /// A unique identifier for notes generated by the player.
    note_id: u64,
    /// A map from a sounding note's ID to the note, decorated with metadata about how the note was
    /// generated.
    playing_notes: HashMap<u64, PlayingNote>,
}

#[derive(Debug, Clone, Copy)]
pub struct PlayingNote {
    pub channel_id: usize,
    pub start_tick_id: u64,
    pub note: Midi,
}

impl Player {
    pub fn new() -> Self {
        Player {
            tick_id: 0,
            note_id: 0,
            playing_notes: HashMap::new(),
        }
    }

    /// Increment and return the tick_id, after sleeping for the required duration.
    /// Meter describes the tempo that the player should use during playback.
    pub fn do_tick(&mut self, meter: &dyn Meter) -> u64 {
        self.tick_id += 1;
        sleep(meter.tick_duration());
        self.tick_id
    }

    /// Gets the current time in ticks since start
    pub fn time(&self) -> u64 {
        self.tick_id
    }

    /// Determines whether we need to poll the channel for new notes in the sequence
    /// Each channel may send a set of notes to the player -- but cannot send any more notes until
    /// those are done playing. So check that there are no active notes for the channel.
    fn should_poll_channel(&self, channel_id: usize) -> bool {
        self.playing_notes.values()
            .filter(|v| v.channel_id == channel_id)
            .count() == 0
    }

    /// TODO: Testing for multiple notes of different durations.
    /// TODO: Sparse channel representations since snapshots of Player should be immutable.
    pub fn poll_channels(
        &mut self,
        channels: &mut [Box<dyn Midibox>]
    ) -> Vec<PlayingNote> {
        for (channel_id, channel) in channels.iter_mut().enumerate() {
            if !self.should_poll_channel(channel_id) {
                continue;
            }

            match channel.next() {
                Some(notes) => {
                    debug!("Channel {} sent notes {:?}", channel_id, notes);
                    for note in notes {
                        self.note_id += 1;
                        let note_id = self.note_id;
                        if note.duration == 0 {
                            continue; // ignore zero-duration notes
                        }
                        // track the note we're about to play so that we can stop it after the
                        // number of ticks equaling the note's duration have elapsed.
                        self.playing_notes.insert(note_id, PlayingNote {
                            channel_id,
                            start_tick_id: self.tick_id,
                            note,
                        });
                    }
                }
                None => {
                    error!("No input from channel {}", channel_id);
                }
            }
        }

        let mut notes: Vec<PlayingNote> = Vec::new();
        notes.extend(
            self.playing_notes
                .values()
                .filter(|note| note.start_tick_id == self.tick_id)
        );
        notes
    }

    pub fn clear_elapsed_notes(&mut self) -> Vec<PlayingNote> {
        let current_tick = self.tick_id;
        self.clear_notes(|note| {
            note.start_tick_id + (note.note.duration as u64) == current_tick
        })
    }

    pub fn clear_all_notes(&mut self) -> Vec<PlayingNote> {
        self.clear_notes(|_| true)
    }

    fn clear_notes<F>(&mut self, should_clear: F) -> Vec<PlayingNote> where
        F: Fn(&PlayingNote) -> bool
    {
        let mut notes: Vec<PlayingNote> = Vec::new();
        for (note_id, playing) in self.playing_notes.clone() {
            if should_clear(&playing) {
                self.playing_notes.remove(&note_id);
                notes.push(playing);
            }
        }

        notes
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PlayerConfig {
    router: Box<dyn Router>
}

impl PlayerConfig {
    pub fn empty() -> Self {
        PlayerConfig {
            router: Box::new(StaticRouter::new(0))
        }
    }

    pub fn for_port(port_id: usize) -> Self {
        PlayerConfig {
            router: Box::new(StaticRouter::new(port_id))
        }
    }

    pub fn from_router(router: Box<dyn Router>) -> Self {
        PlayerConfig {
            router
        }
    }
}

impl Router for PlayerConfig {
    fn route(&self, channel_id: usize) -> Option<&usize> {
        self.router.route(channel_id)
    }

    fn required_ports(&self) -> HashSet<usize> {
        self.router.required_ports()
    }
}

pub fn try_run(
    player_config: PlayerConfig,
    bpm: &dyn Meter,
    channels: &mut Vec<Box<dyn Midibox>>
) -> Result<(), Box<dyn Error>> {
    let name = "Midibox";
    let mut map : HashMap<String, bool> = HashMap::new();
    map.insert(name.to_string(), true);
    let running = Arc::new(Mutex::new(map));
    // Set up listener for ctrl-C command
    let ctrlc_running = Arc::clone(&running);
    ctrlc::set_handler(move || {
        ctrlc_running.lock().unwrap().insert(name.to_string(), false);
    })?;

    return try_run_ext(name, player_config, bpm, channels, &running);
}

pub fn try_run_ext(
    name: &str,
    player_config: PlayerConfig,
    bpm: &dyn Meter,
    channels: &mut Vec<Box<dyn Midibox>>,
    running: &Arc<Mutex<HashMap<String, bool>>>
) -> Result<(), Box<dyn Error>> {
    let midi_out = MidiOutput::new("Midi Outputs")?;
    let out_ports = midi_out.ports();

    for (i, p) in out_ports.iter().enumerate() {
        info!("{}: {}", i, midi_out.port_name(p).unwrap());
    }

    let required_ports = player_config.required_ports();
    let mut port_id_to_conn: HashMap<usize, MidiOutputConnection> =
        HashMap::with_capacity(required_ports.len());

    for i in 0..out_ports.len() {
        let port = out_ports.get(i).expect("Missing midi port");
        let port_name = format!("midibox {}", i);
        let output = MidiOutput::new(&port_name)?;

        if required_ports.contains(&i) {
            let conn = output.connect(port, &port_name)?;
            port_id_to_conn.insert(i, conn);
        }
    }

    let mut player = Player::new();

    info!("Player Starting.");
    while *running.lock().unwrap().get(name).unwrap() {
        debug!("Time: {}", player.time());
        for note in player.poll_channels(channels) {
            route_note(&player_config, &mut port_id_to_conn, &note, NOTE_ON_MSG)
        }
        player.do_tick(bpm);
        for note in player.clear_elapsed_notes() {
            route_note(&player_config, &mut port_id_to_conn, &note, NOTE_OFF_MSG)
        }
    }
    for note in player.clear_all_notes() {
        route_note(&player_config, &mut port_id_to_conn, &note, NOTE_OFF_MSG)
    }
    info!("Player Exiting.");
    Ok(())
}

fn route_note(
    player_config: &PlayerConfig,
    device_conn: &mut HashMap<usize, MidiOutputConnection>,
    playing: &PlayingNote,
    midi_status: u8
) {
    match playing.note.u8_maybe() {
        None => { /* resting */ }
        Some(v) => {
            let note: [u8; 3] = [
                midi_status, v, playing.note.velocity
            ];

            match player_config.route(playing.channel_id) {
                None => {
                    error!("No port configured for channel! channel_id = {}", playing.channel_id);
                }
                Some(port_id) => {
                    device_conn.get_mut(port_id)
                        .unwrap_or_else(|| panic!("Could not find connection for port {}", port_id))
                        .send(&note)
                        .unwrap_or_else(|err| panic!("Failed to send note to port {}, {}", port_id, err))
                }
            }
        }
    }
}
