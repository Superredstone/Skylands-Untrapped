//! Original-format save files (`Save_Header`, `Save_Slot_1..4`), see
//! `skylands_formats::save`. Only fields this runtime understands are written; every other
//! bit of a loaded save is kept as is.

use crate::resources::{CampaignProgress, GameAssetServer};
use bevy::prelude::*;
use skylands_formats::{SaveFile, SaveSchema, ScriptSet};
use std::path::{Path, PathBuf};

pub const SAVE_SLOTS: usize = 4;
pub const HEADER_FILE: &str = "Save_Header";

/// `global.bld` "System - Save - Get/Adjust Level Saved Flags": bit 0 of an adventure's
/// 20-bit `level flags` field is "completed" (then chests 1-3, villain safe, level up
/// tokens 1-4, elemental gates 1-4, story tablet hint objective, and six achievements).
pub const LEVEL_FLAG_COMPLETED: u32 = 0;

pub fn slot_file(slot: usize) -> String {
    format!("Save_Slot_{}", slot + 1)
}

/// Save field index of story chapter `chapter` (1..=18). The index is the position in
/// `"adventure data"` after "Global - Order Adventure Data" sorts it along the unlock
/// chain (`Init_Setup_Adventure.ai`), expansions after the story. Chapter 1 = index 0 is
/// confirmed by an EU save; the rest follow that order, unverified.
pub fn chapter_save_index(chapter: u8) -> Option<u32> {
    (1..=18).contains(&chapter).then(|| chapter as u32 - 1)
}

#[derive(Resource, Debug, Clone)]
pub struct SaveGame {
    pub schema: SaveSchema,
    pub header: SaveFile,
    pub slots: Vec<SaveFile>,
    /// Slot in play (0-based).
    pub slot: usize,
    /// Directory written by `write`; `None` keeps the save in memory only.
    pub dir: Option<PathBuf>,
}

impl SaveGame {
    /// Reads the header and slots from `dir`. Missing or corrupt files (bad checksum)
    /// start fresh from the script defaults.
    pub fn load(schema: SaveSchema, dir: Option<&Path>) -> Self {
        let read = |name: &str| {
            let bytes = std::fs::read(dir?.join(name)).ok()?;
            match SaveFile::parse(&bytes) {
                Ok(f) => Some(f),
                Err(e) => {
                    warn!("Ignoring save file {name}: {e}");
                    None
                }
            }
        };
        let header = read(HEADER_FILE).unwrap_or_else(|| schema.new_file(true));
        let slots = (0..SAVE_SLOTS)
            .map(|s| read(&slot_file(s)).unwrap_or_else(|| schema.new_file(false)))
            .collect();
        Self {
            schema,
            header,
            slots,
            slot: 0,
            dir: dir.map(Path::to_path_buf),
        }
    }

    /// Writes the header and the slot in play to `dir`.
    pub fn write(&self) -> std::io::Result<()> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join(HEADER_FILE), self.header.to_bytes())?;
        std::fs::write(
            dir.join(slot_file(self.slot)),
            self.slots[self.slot].to_bytes(),
        )
    }

    fn level_completed(&self, index: u32) -> Option<bool> {
        let field = self.schema.field(false, "level flags")?;
        Some(field.get(&self.slots[self.slot], index)? >> LEVEL_FLAG_COMPLETED & 1 == 1)
    }

    /// Story chapters completed in the slot in play.
    pub fn completed_chapters(&self) -> Vec<u8> {
        (1..=18)
            .filter(|&c| chapter_save_index(c).and_then(|i| self.level_completed(i)) == Some(true))
            .collect()
    }

    pub fn apply_to(&self, campaign: &mut CampaignProgress) {
        for chapter in self.completed_chapters() {
            campaign.complete_chapter(chapter);
        }
    }

    /// Sets the completed flag of every chapter `campaign` has completed. `true` when
    /// the slot changed.
    pub fn record(&mut self, campaign: &CampaignProgress) -> bool {
        let Some(field) = self.schema.field(false, "level flags") else {
            return false;
        };
        let file = &mut self.slots[self.slot];
        let mut changed = false;
        for &chapter in &campaign.completed_chapters {
            let Some(i) = chapter_save_index(chapter) else {
                continue;
            };
            let Some(flags) = field.get(file, i) else {
                continue;
            };
            let set = flags | 1 << LEVEL_FLAG_COMPLETED;
            if set != flags {
                field.set(file, i, set);
                changed = true;
            }
        }
        changed
    }
}

impl GameAssetServer {
    /// Save layout from `Init_Setup.bld` and `global.bld`.
    pub fn save_schema(&self) -> Option<SaveSchema> {
        let parse = |p: &str| {
            let bytes = self.loader.lock().ok()?.load_raw(p).ok()?;
            ScriptSet::parse(&bytes)
        };
        SaveSchema::from_scripts(&parse("Init_Setup.bld")?, parse("global.bld").as_ref())
    }
}

/// Mirrors completed chapters into the save and writes it when they change.
pub fn save_progress_system(campaign: Res<CampaignProgress>, save: Option<ResMut<SaveGame>>) {
    let Some(mut save) = save else {
        return;
    };
    if campaign.is_changed() && save.record(&campaign) {
        if let Err(e) = save.write() {
            warn!("Writing save failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use skylands_formats::{SaveCount, SaveParamDef};

    /// Body with one `level flags` param (22 x 20 bits) after 8 unrelated bits.
    fn schema() -> SaveSchema {
        let p = |name: &str, fields, bits| SaveParamDef {
            name: name.into(),
            header: false,
            signed: false,
            fields: SaveCount::Fixed(fields),
            bits,
            limit: 0,
            defaults: Vec::new(),
        };
        SaveSchema {
            params: vec![p("savegame version", 1, 8), p("level flags", 22, 20)],
            levels: Vec::new(),
        }
    }

    #[test]
    fn test_save_round_trips_completed_chapters() {
        let dir = std::env::temp_dir().join(format!("skylands-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut save = SaveGame::load(schema(), Some(&dir));
        assert!(save.completed_chapters().is_empty());
        // An unrelated bit set in the slot must survive.
        save.slots[0].set_bits(3, 1, 1);
        let mut campaign = CampaignProgress::default();
        campaign.complete_chapter(1);
        campaign.complete_chapter(3);
        assert!(save.record(&campaign));
        assert!(!save.record(&campaign), "already recorded");
        save.write().unwrap();

        let loaded = SaveGame::load(schema(), Some(&dir));
        assert_eq!(loaded.completed_chapters(), [1, 3]);
        assert_eq!(loaded.slots[0].get_bits(3, 1), Some(1));
        // Chapter 1 = level flags field 0, bit 0.
        assert_eq!(loaded.slots[0].get_bits(8, 1), Some(1));
        let mut fresh = CampaignProgress::default();
        loaded.apply_to(&mut fresh);
        assert_eq!(fresh.completed_chapters, [1, 3]);
        assert_eq!(fresh.highest_unlocked_chapter, 4);

        // A corrupted file is ignored.
        std::fs::write(dir.join("Save_Slot_1"), [0u8; 8]).unwrap();
        assert!(SaveGame::load(schema(), Some(&dir))
            .completed_chapters()
            .is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Local-only: the user's EU save after chapter 1 (`Save_Slot_1`, gitignored).
    #[test]
    fn test_real_save_chapter_one_completed() {
        let dir = [".", "../.."]
            .into_iter()
            .map(Path::new)
            .find(|d| d.join("Save_Slot_1").exists() && d.join("assets/files").exists());
        let Some(dir) = dir else { return };
        let mut loader = skylands_formats::AssetLoader::new(Default::default());
        let files = dir.join("assets/files");
        loader.mount(files.join("permanent/global.bld")).unwrap();
        loader
            .mount(files.join("character/Init_Setup.bld"))
            .unwrap();
        let schema = GameAssetServer::new(loader).save_schema().expect("schema");
        let save = SaveGame::load(schema, Some(dir));
        assert_eq!(save.completed_chapters(), [1]);
        // Recording what is already there changes nothing.
        let mut campaign = CampaignProgress::default();
        save.apply_to(&mut campaign);
        let mut copy = save.clone();
        assert!(!copy.record(&campaign));
        assert_eq!(copy.slots[0], save.slots[0]);
    }
}
