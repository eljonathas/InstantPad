use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use specta::Type;
use thiserror::Error;
use uuid::Uuid;

use crate::hotkeys::Hotkey;
use crate::palette;

pub const MAX_ROWS: u32 = 8;
pub const MAX_COLUMNS: u32 = 10;

pub const DEFAULT_ROWS: u32 = 4;
pub const DEFAULT_COLUMNS: u32 = 6;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BoardError {
    #[error("the grid must have between 1x1 and {MAX_ROWS}x{MAX_COLUMNS} pads")]
    InvalidGrid,
    #[error("{capacity} pads do not fit {sounds} sounds")]
    GridTooSmall { capacity: u32, sounds: u32 },
    #[error("the board needs at least one interface")]
    LastInterface,
    #[error("no interface with that id")]
    UnknownInterface,
}

fn new_id() -> String {
    Uuid::new_v4().simple().to_string()
}

/// The part of an interface the user configures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct InterfaceLayout {
    pub name: String,
    pub rows: u32,
    pub columns: u32,
    pub accent: String,
}

impl InterfaceLayout {
    pub fn capacity(&self) -> u32 {
        self.rows * self.columns
    }

    fn check_grid(&self) -> Result<(), BoardError> {
        let fits = (1..=MAX_ROWS).contains(&self.rows) && (1..=MAX_COLUMNS).contains(&self.columns);
        fits.then_some(()).ok_or(BoardError::InvalidGrid)
    }
}

/// A pad layout of its own. The user can create as many as they want.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PadInterface {
    pub id: String,
    #[serde(flatten)]
    pub layout: InterfaceLayout,
}

impl PadInterface {
    fn new(layout: InterfaceLayout) -> Self {
        Self {
            id: new_id(),
            layout,
        }
    }
}

/// A sound assigned to a pad.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sound {
    pub id: String,
    pub name: String,
    pub path: String,
    pub color: String,
    pub volume: f64,
    pub hotkey: Option<Hotkey>,
    pub interface_id: String,
    /// Position inside the interface, left to right, top to bottom.
    pub slot: u32,
}

/// All interfaces and the sounds on their pads, with the rules that keep them consistent.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PadBoard {
    interfaces: Vec<PadInterface>,
    sounds: Vec<Sound>,
}

impl PadBoard {
    /// A board always has an interface to show, so a new one gets `name`.
    pub fn ensure_interface(&mut self, name: &str) {
        if self.interfaces.is_empty() {
            self.interfaces.push(PadInterface::new(InterfaceLayout {
                name: name.to_owned(),
                rows: DEFAULT_ROWS,
                columns: DEFAULT_COLUMNS,
                accent: palette::ACCENTS[0].to_owned(),
            }));
        }
    }

    pub fn interfaces(&self) -> &[PadInterface] {
        &self.interfaces
    }

    pub fn sounds(&self) -> &[Sound] {
        &self.sounds
    }

    pub fn interface(&self, id: &str) -> Option<&PadInterface> {
        self.interfaces.iter().find(|item| item.id == id)
    }

    pub fn sound_mut(&mut self, id: &str) -> Option<&mut Sound> {
        self.sounds.iter_mut().find(|sound| sound.id == id)
    }

    pub fn sounds_of(&self, interface_id: &str) -> impl Iterator<Item = &Sound> {
        self.sounds
            .iter()
            .filter(move |sound| sound.interface_id == interface_id)
    }

    pub fn sound_at(&self, interface_id: &str, slot: u32) -> Option<&Sound> {
        self.sounds_of(interface_id)
            .find(|sound| sound.slot == slot)
    }

    pub fn first_free_slot(&self, interface_id: &str) -> Option<u32> {
        let target = self.interface(interface_id)?;
        (0..target.layout.capacity()).find(|&slot| self.sound_at(interface_id, slot).is_none())
    }

    /// Returns the id of the new interface.
    pub fn add_interface(&mut self, layout: InterfaceLayout) -> Result<String, BoardError> {
        layout.check_grid()?;
        let created = PadInterface::new(layout);
        let id = created.id.clone();
        self.interfaces.push(created);
        Ok(id)
    }

    pub fn reconfigure(&mut self, id: &str, layout: InterfaceLayout) -> Result<(), BoardError> {
        layout.check_grid()?;
        let sounds = self.sounds_of(id).count() as u32;
        if layout.capacity() < sounds {
            return Err(BoardError::GridTooSmall {
                capacity: layout.capacity(),
                sounds,
            });
        }

        let capacity = layout.capacity();
        self.interfaces
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or(BoardError::UnknownInterface)?
            .layout = layout;
        self.move_overflow_into_free_slots(id, capacity);
        Ok(())
    }

    /// Removes the interface and its sounds. Returns the id of the interface to show in its place.
    pub fn delete_interface(&mut self, id: &str) -> Result<String, BoardError> {
        if self.interfaces.len() == 1 {
            return Err(BoardError::LastInterface);
        }
        let index = self.index_of(id).ok_or(BoardError::UnknownInterface)?;

        self.sounds.retain(|sound| sound.interface_id != id);
        self.interfaces.remove(index);
        Ok(self.interfaces[index.min(self.interfaces.len() - 1)]
            .id
            .clone())
    }

    pub fn can_move_interface(&self, id: &str, offset: i32) -> bool {
        self.index_of(id).is_some_and(|from| {
            let to = from as i32 + offset;
            to >= 0 && (to as usize) < self.interfaces.len()
        })
    }

    pub fn move_interface(&mut self, id: &str, offset: i32) {
        if !self.can_move_interface(id, offset) {
            return;
        }
        if let Some(from) = self.index_of(id) {
            let moved = self.interfaces.remove(from);
            self.interfaces
                .insert((from as i32 + offset) as usize, moved);
        }
    }

    /// The interface `offset` positions away, wrapping at both ends.
    pub fn neighbor(&self, id: &str, offset: i32) -> Option<&PadInterface> {
        let count = self.interfaces.len() as i32;
        let index = self.index_of(id)? as i32;
        self.interfaces
            .get((index + offset).rem_euclid(count) as usize)
    }

    /// Puts a new sound on a pad, replacing whatever was there.
    pub fn place(
        &mut self,
        interface_id: &str,
        slot: u32,
        name: &str,
        path: &str,
        color: &str,
    ) -> &Sound {
        self.sounds
            .retain(|sound| sound.interface_id != interface_id || sound.slot != slot);
        self.sounds.push(Sound {
            id: new_id(),
            name: name.to_owned(),
            path: path.to_owned(),
            color: palette::visible_color(color, slot),
            volume: 1.0,
            hotkey: None,
            interface_id: interface_id.to_owned(),
            slot,
        });
        &self.sounds[self.sounds.len() - 1]
    }

    pub fn remove(&mut self, sound_id: &str) {
        self.sounds.retain(|sound| sound.id != sound_id);
    }

    /// Moves a sound to another pad, swapping with the sound already there.
    pub fn swap(&mut self, interface_id: &str, from_slot: u32, to_slot: u32) {
        if from_slot == to_slot || self.sound_at(interface_id, from_slot).is_none() {
            return;
        }
        for sound in self
            .sounds
            .iter_mut()
            .filter(|sound| sound.interface_id == interface_id)
        {
            if sound.slot == from_slot {
                sound.slot = to_slot;
            } else if sound.slot == to_slot {
                sound.slot = from_slot;
            }
        }
    }

    /// Returns false when the target interface has no free pad.
    pub fn send_to(&mut self, sound_id: &str, target_id: &str) -> bool {
        let Some(free) = self.first_free_slot(target_id) else {
            return false;
        };
        let Some(sound) = self.sound_mut(sound_id) else {
            return false;
        };
        sound.interface_id = target_id.to_owned();
        sound.slot = free;
        true
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.interfaces.iter().position(|item| item.id == id)
    }

    fn move_overflow_into_free_slots(&mut self, id: &str, capacity: u32) {
        let mut used: HashSet<u32> = self
            .sounds_of(id)
            .map(|sound| sound.slot)
            .filter(|&slot| slot < capacity)
            .collect();

        let mut overflow: Vec<&mut Sound> = self
            .sounds
            .iter_mut()
            .filter(|sound| sound.interface_id == id && sound.slot >= capacity)
            .collect();
        overflow.sort_by_key(|sound| sound.slot);

        let mut free = 0;
        for sound in overflow {
            while used.contains(&free) {
                free += 1;
            }
            sound.slot = free;
            used.insert(free);
        }
    }
}
