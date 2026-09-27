use nice_plug::midi::{Channel, Key, VoiceID};
use nice_plug::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct HeldNote {
    pub key: u8,
    pub channel: u8,
    pub voice_id: VoiceID,
    pub token: u64,
    down: bool,
    sustained: bool,
}

impl HeldNote {
    fn matches(self, voice_id: VoiceID, channel: Channel, key: Key) -> bool {
        (voice_id.is_wildcard() || voice_id == self.voice_id)
            && channel.number().is_none_or(|c| c == self.channel)
            && key.number().is_none_or(|k| k == self.key)
    }

    fn held(self) -> bool {
        self.down || self.sustained
    }
}

/// Fixed storage retains released identities as well as held notes, allowing a
/// later CLAP choke to silence the correct tail. Full capacity replaces the
/// oldest released record first, then the oldest held note, deterministically.
pub struct MidiNotes {
    notes: [Option<HeldNote>; 128],
    sustain: [bool; 16],
    serial: u64,
}

/// Translate host identities into independent DSP voice lifetimes. Release
/// closes excitation, whereas Choke fades the matching tail as well.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteAction {
    On { key: u8, token: u64, velocity: f32 },
    Release(u64),
    Choke(u64),
}

impl NoteAction {
    pub fn apply(self, engine: &mut spectral_dsp::SpectralResonator) {
        match self {
            Self::On {
                key,
                token,
                velocity,
            } => engine.note_on(i32::from(key), token, velocity),
            Self::Release(token) => engine.note_off(token),
            Self::Choke(token) => engine.choke(token),
        }
    }
}

impl Default for MidiNotes {
    fn default() -> Self {
        Self {
            notes: [None; 128],
            sustain: [false; 16],
            serial: 0,
        }
    }
}

impl MidiNotes {
    pub fn reset(&mut self) {
        // Panic/source changes let old DSP voices fade. Never reuse their
        // tokens for new notes while those tails still exist.
        self.notes.fill(None);
        self.sustain.fill(false);
    }

    #[cfg(test)]
    pub fn current(&self) -> Option<HeldNote> {
        self.notes
            .iter()
            .flatten()
            .copied()
            .filter(|n| n.held())
            .max_by_key(|n| n.token)
    }

    pub fn handle(&mut self, event: NoteEvent<()>, mut emit: impl FnMut(NoteAction)) {
        match event {
            NoteEvent::NoteOn {
                voice_id,
                channel,
                key,
                velocity,
                ..
            } => {
                let (Some(channel), Some(key)) = (channel.number(), key.number()) else {
                    return;
                };
                if channel >= 16 || !(33..=93).contains(&key) || !velocity.is_finite() {
                    return;
                }
                // MIDI 1 zero-velocity on is an off. Native CLAP zero velocity
                // also safely releases matching notes instead of hanging them.
                if velocity <= 0.0 {
                    self.release(
                        voice_id,
                        Channel::Number(channel),
                        Key::Number(key),
                        &mut emit,
                    );
                    return;
                }
                if !voice_id.is_wildcard() {
                    for entry in &mut self.notes {
                        if let Some(note) = entry
                            && note.voice_id == voice_id
                        {
                            emit(NoteAction::Choke(note.token));
                            *entry = None;
                        }
                    }
                }
                let slot = self
                    .notes
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or_else(|| {
                        self.notes
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, note)| {
                                let n = note.unwrap();
                                (n.held(), n.token)
                            })
                            .map(|(index, _)| index)
                            .unwrap_or(0)
                    });
                if let Some(old) = self.notes[slot] {
                    emit(NoteAction::Choke(old.token));
                }
                // Wrapping requires centuries of continuous notes. Reset all
                // identities if it ever occurs, rather than reversing priority.
                if self.serial == u64::MAX {
                    for note in self.notes.iter().flatten() {
                        emit(NoteAction::Choke(note.token));
                    }
                    self.reset();
                    self.serial = 0;
                }
                self.serial += 1;
                self.notes[slot] = Some(HeldNote {
                    key,
                    channel,
                    voice_id,
                    token: self.serial,
                    down: true,
                    sustained: false,
                });
                emit(NoteAction::On {
                    key,
                    token: self.serial,
                    velocity: velocity.min(1.0),
                });
            }
            NoteEvent::NoteOff {
                voice_id,
                channel,
                key,
                ..
            } => self.release(voice_id, channel, key, &mut emit),
            NoteEvent::Choke {
                voice_id,
                channel,
                key,
                ..
            } => {
                for entry in &mut self.notes {
                    if let Some(note) = entry
                        && note.matches(voice_id, channel, key)
                    {
                        emit(NoteAction::Choke(note.token));
                        *entry = None;
                    }
                }
            }
            NoteEvent::MidiCC {
                channel, cc, value, ..
            } if channel < 16 => match cc {
                64 | 121 => {
                    let down = cc == 64 && value >= 0.5;
                    self.sustain[channel as usize] = down;
                    if !down {
                        for note in self
                            .notes
                            .iter_mut()
                            .flatten()
                            .filter(|n| n.channel == channel)
                        {
                            if note.sustained && !note.down {
                                emit(NoteAction::Release(note.token));
                            }
                            note.sustained = false;
                        }
                    }
                }
                123 => self.release(
                    VoiceID::Wildcard,
                    Channel::Number(channel),
                    Key::Wildcard,
                    &mut emit,
                ),
                120 => {
                    for entry in &mut self.notes {
                        if let Some(note) = entry
                            && note.channel == channel
                        {
                            emit(NoteAction::Choke(note.token));
                            *entry = None;
                        }
                    }
                    self.sustain[channel as usize] = false;
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn release(
        &mut self,
        id: VoiceID,
        channel: Channel,
        key: Key,
        emit: &mut impl FnMut(NoteAction),
    ) {
        // CLAP wildcards match all applicable notes; do not invent a note ID
        // from channel/key, since Bitwig may omit IDs on note-off.
        for note in self.notes.iter_mut().flatten() {
            if note.down && note.matches(id, channel, key) {
                note.down = false;
                note.sustained = self.sustain[note.channel as usize];
                if !note.sustained {
                    emit(NoteAction::Release(note.token));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(id: i32, key: u8, channel: u8) -> NoteEvent<()> {
        NoteEvent::NoteOn {
            timing: 0,
            voice_id: VoiceID::ID(id),
            channel: Channel::Number(channel),
            key: Key::Number(key),
            velocity: 0.7,
        }
    }
    fn off(id: VoiceID, key: Key) -> NoteEvent<()> {
        NoteEvent::NoteOff {
            timing: 0,
            voice_id: id,
            channel: Channel::Wildcard,
            key,
            velocity: 0.0,
        }
    }
    fn cc(channel: u8, cc: u8, value: f32) -> NoteEvent<()> {
        NoteEvent::MidiCC {
            timing: 0,
            channel,
            cc,
            value,
        }
    }

    #[test]
    fn last_note_priority_repeated_ids_and_wildcard_release() {
        let mut notes = MidiNotes::default();
        for event in [on(1, 60, 0), on(2, 64, 0), on(3, 64, 0)] {
            notes.handle(event, |_| {});
        }
        notes.handle(off(VoiceID::ID(2), Key::Wildcard), |_| {});
        assert_eq!(notes.current().unwrap().voice_id, VoiceID::ID(3));
        notes.handle(off(VoiceID::Wildcard, Key::Number(64)), |_| {});
        assert_eq!(notes.current().unwrap().key, 60);
        notes.handle(off(VoiceID::Wildcard, Key::Wildcard), |_| {});
        assert!(notes.current().is_none());
    }

    #[test]
    fn sustain_all_notes_off_and_all_sound_off_respect_channels() {
        let mut notes = MidiNotes::default();
        for event in [on(1, 60, 0), on(2, 67, 1), cc(1, 64, 1.0), cc(1, 123, 0.0)] {
            notes.handle(event, |_| {});
        }
        assert_eq!(notes.current().unwrap().key, 67);
        notes.handle(cc(1, 64, 0.0), |_| {});
        assert_eq!(notes.current().unwrap().key, 60);
        let mut killed = Vec::new();
        notes.handle(cc(1, 120, 0.0), |token| killed.push(token));
        assert_eq!(killed, [NoteAction::Choke(2)]);
        assert_eq!(notes.current().unwrap().key, 60);
        notes.handle(cc(0, 120, 0.0), |_| {});
        assert!(notes.current().is_none());
    }

    #[test]
    fn out_of_range_notes_capacity_and_reset_cannot_stick() {
        let mut notes = MidiNotes::default();
        notes.handle(on(1, 12, 0), |_| {});
        assert!(notes.current().is_none());
        for id in 1..=200 {
            notes.handle(on(id, 60, 0), |_| {});
        }
        assert_eq!(notes.current().unwrap().voice_id, VoiceID::ID(200));
        notes.handle(off(VoiceID::Wildcard, Key::Wildcard), |_| {});
        assert!(notes.current().is_none());
        notes.handle(on(201, 61, 0), |_| {});
        notes.reset();
        assert!(notes.current().is_none());
    }

    #[test]
    fn polyphonic_actions_preserve_ids_and_defer_only_sustained_releases() {
        let mut notes = MidiNotes::default();
        let mut actions = Vec::new();
        for event in [
            on(1, 60, 0),
            on(2, 60, 0),
            cc(0, 64, 1.0),
            off(VoiceID::ID(1), Key::Wildcard),
        ] {
            notes.handle(event, |action| actions.push(action));
        }
        assert_eq!(
            actions,
            [
                NoteAction::On {
                    key: 60,
                    token: 1,
                    velocity: 0.7
                },
                NoteAction::On {
                    key: 60,
                    token: 2,
                    velocity: 0.7
                },
            ]
        );
        actions.clear();
        notes.handle(cc(0, 121, 0.0), |action| actions.push(action));
        assert_eq!(actions, [NoteAction::Release(1)]);
        actions.clear();
        notes.handle(off(VoiceID::Wildcard, Key::Number(60)), |action| {
            actions.push(action)
        });
        assert_eq!(actions, [NoteAction::Release(2)]);
        actions.clear();
        notes.handle(
            NoteEvent::Choke {
                timing: 0,
                voice_id: VoiceID::ID(1),
                channel: Channel::Wildcard,
                key: Key::Wildcard,
            },
            |action| actions.push(action),
        );
        assert_eq!(actions, [NoteAction::Choke(1)]);
    }

    #[test]
    fn reused_host_id_chokes_old_tail_and_reset_keeps_tokens_unique() {
        let mut notes = MidiNotes::default();
        notes.handle(on(1, 60, 0), |_| {});
        let mut actions = Vec::new();
        notes.handle(on(1, 67, 0), |action| actions.push(action));
        assert_eq!(
            actions,
            [
                NoteAction::Choke(1),
                NoteAction::On {
                    key: 67,
                    token: 2,
                    velocity: 0.7
                }
            ]
        );
        notes.reset();
        actions.clear();
        notes.handle(on(1, 69, 0), |action| actions.push(action));
        assert_eq!(
            actions,
            [NoteAction::On {
                key: 69,
                token: 3,
                velocity: 0.7
            }]
        );
        actions.clear();
        let mut zero = on(1, 69, 0);
        if let NoteEvent::NoteOn { velocity, .. } = &mut zero {
            *velocity = 0.0;
        }
        notes.handle(zero, |action| actions.push(action));
        assert_eq!(actions, [NoteAction::Release(3)]);
    }
}
