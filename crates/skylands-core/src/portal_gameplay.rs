#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Air,
    Life,
    Undead,
    Earth,
    Fire,
    Water,
    Magic,
    Tech,
    Kaos,
}

impl Element {
    pub fn name(&self) -> &'static str {
        match self {
            Element::Air => "Air",
            Element::Life => "Life",
            Element::Undead => "Undead",
            Element::Earth => "Earth",
            Element::Fire => "Fire",
            Element::Water => "Water",
            Element::Magic => "Magic",
            Element::Tech => "Tech",
            Element::Kaos => "Kaos",
        }
    }

    pub fn rgb(&self) -> (u8, u8, u8) {
        match self {
            Element::Air => (0, 220, 255),
            Element::Life => (0, 255, 50),
            Element::Undead => (160, 0, 255),
            Element::Earth => (210, 180, 30),
            Element::Fire => (255, 40, 0),
            Element::Water => (0, 140, 255),
            Element::Magic => (255, 0, 180),
            Element::Tech => (255, 120, 0),
            Element::Kaos => (255, 0, 60),
        }
    }
}

/// One playable or trappable character: `(id, name, element, archive, portrait)`.
///
/// Archive numbers in `assets/files/character` are internal file IDs, so each
/// entry pins the archive explicitly instead of deriving it from the ID.
pub type CharacterEntry = (
    u16,
    &'static str,
    Element,
    &'static str,
    Option<&'static str>,
);

pub const CHARACTERS: &[CharacterEntry] = &[
    // Trap Team showcase heroes (portal mock IDs).
    (
        400,
        "Snap Shot",
        Element::Water,
        "462_Water_Crocky.bld",
        Some("textures/characters/snapshot.png"),
    ),
    (
        401,
        "Food Fight",
        Element::Life,
        "476_Artichoke.bld",
        Some("textures/characters/foodfight.png"),
    ),
    (
        402,
        "Wildfire",
        Element::Fire,
        "458_HeatShield.bld",
        Some("textures/characters/wildfire.png"),
    ),
    (
        403,
        "Wallop",
        Element::Earth,
        "454_Earth_Hunter.bld",
        Some("textures/characters/wallop.png"),
    ),
    (
        404,
        "Gearshift",
        Element::Tech,
        "471_Tech_Helix.bld",
        Some("textures/characters/gearshift.png"),
    ),
    (
        405,
        "Krypt King",
        Element::Undead,
        "478_KingCut.bld",
        Some("textures/characters/kryptking.png"),
    ),
    (
        406,
        "Enigma",
        Element::Magic,
        "467_Void.bld",
        Some("textures/characters/enigma.png"),
    ),
    (
        407,
        "Gusto",
        Element::Air,
        "450_Typhoon.bld",
        Some("textures/characters/gusto.png"),
    ),
    // Spyro's Adventure core heroes.
    (0, "Whirlwind", Element::Air, "000_AirDragon.bld", None),
    (1, "Sonic Boom", Element::Air, "001_Gryphon.bld", None),
    (2, "Warnado", Element::Air, "002_SkyTurtle.bld", None),
    (3, "Lightning Rod", Element::Air, "003_StormGiant.bld", None),
    (4, "Bash", Element::Earth, "004_Rock_Dragon.bld", None),
    (5, "Terrafin", Element::Earth, "005_LandShark.bld", None),
    (6, "Dino-Rang", Element::Earth, "006_DinoRang.bld", None),
    (7, "Prism Break", Element::Earth, "007_GemGolem.bld", None),
    (8, "Sunburn", Element::Fire, "008_FireDragon.bld", None),
    (9, "Eruptor", Element::Fire, "009_Eruptor.bld", None),
    (10, "Ignitor", Element::Fire, "010_FlameKnight.bld", None),
    (11, "Flameslinger", Element::Fire, "011_FireElf.bld", None),
    (12, "Zap", Element::Water, "012_SeaDragon.bld", None),
    (13, "Wham-Shell", Element::Water, "013_Crab.bld", None),
    (
        14,
        "Gill Grunt",
        Element::Water,
        "014_GillGrunt.bld",
        Some("textures/characters/gillgrunt.png"),
    ),
    (15, "Slam Bam", Element::Water, "015_Yeti.bld", None),
    (
        16,
        "Spyro",
        Element::Magic,
        "016_Spyro.bld",
        Some("textures/characters/spyro.png"),
    ),
    (17, "Voodood", Element::Magic, "017_SkullOrc.bld", None),
    (18, "Double Trouble", Element::Magic, "018_Wizard.bld", None),
    (
        19,
        "Trigger Happy",
        Element::Tech,
        "019_GoldenCheat.bld",
        Some("textures/characters/triggerhappy.png"),
    ),
    (20, "Drobot", Element::Tech, "020_Metal_Dragon.bld", None),
    (
        21,
        "Drill Sergeant",
        Element::Tech,
        "021_DrillBot.bld",
        None,
    ),
    (22, "Boomer", Element::Tech, "022_Bomb_Troll.bld", None),
    (
        23,
        "Wrecking Ball",
        Element::Magic,
        "023_ForceFieldGrub.bld",
        None,
    ),
    (24, "Camo", Element::Life, "024_LifeDragon.bld", None),
    (25, "Zook", Element::Life, "025_BamBazooker.bld", None),
    (26, "Stealth Elf", Element::Life, "026_StealthElf.bld", None),
    (27, "Stump Smash", Element::Life, "027_Ent.bld", None),
    (29, "Hex", Element::Undead, "029_ShadowMaid.bld", None),
    (
        30,
        "Chop Chop",
        Element::Undead,
        "030_PandoranElite.bld",
        None,
    ),
    (
        31,
        "Ghost Roaster",
        Element::Undead,
        "031_GhostEater.bld",
        None,
    ),
    (32, "Cynder", Element::Undead, "032_Cynder.bld", None),
    // Trap Team heroes not covered by the showcase IDs above.
    (
        451,
        "Thunderbolt",
        Element::Air,
        "451_Air_MrTwister.bld",
        None,
    ),
    (
        452,
        "Fling Kong",
        Element::Air,
        "452_Air_DiscJockey.bld",
        None,
    ),
    (453, "Blades", Element::Air, "453_Air_AirStrike.bld", None),
    (
        455,
        "Head Rush",
        Element::Earth,
        "455_Earth_Rampage.bld",
        None,
    ),
    (
        456,
        "Fist Bump",
        Element::Earth,
        "456_Earth_Aftershock.bld",
        None,
    ),
    (
        457,
        "Rocky Roll",
        Element::Earth,
        "457_Earth_Boulderdash.bld",
        None,
    ),
    (459, "Ka-Boom", Element::Fire, "459_Barbarian.bld", None),
    (
        460,
        "Trail Blazer",
        Element::Fire,
        "460_Fire_PyroMane.bld",
        None,
    ),
    (461, "Torch", Element::Fire, "461_Bellows.bld", None),
    (463, "Lob-Star", Element::Water, "463_Lobstar.bld", None),
    (464, "Flip Wreck", Element::Water, "464_SeaSaw.bld", None),
    (465, "Echo", Element::Water, "465_Siren.bld", None),
    (
        466,
        "Blastermind",
        Element::Magic,
        "466_Magic_Brainwave.bld",
        None,
    ),
    (
        468,
        "Deja Vu",
        Element::Magic,
        "468_Magic_Paradox.bld",
        None,
    ),
    (
        469,
        "Cobra Cadabra",
        Element::Magic,
        "469_Magic_Musician.bld",
        None,
    ),
    (
        470,
        "Jawbreaker",
        Element::Tech,
        "470_Tech_BruteForce.bld",
        None,
    ),
    (
        472,
        "Chopper",
        Element::Tech,
        "472_Tech_Velocicopter.bld",
        None,
    ),
    (473, "Tread Head", Element::Tech, "473_Tech_Trax.bld", None),
    (474, "Bushwhack", Element::Life, "474_Bushwhack.bld", None),
    (
        475,
        "Tuff Luck",
        Element::Life,
        "475_Life_Harvester.bld",
        None,
    ),
    (
        477,
        "High Five",
        Element::Life,
        "477_Life_Buzzerker.bld",
        None,
    ),
    (
        479,
        "Short Cut",
        Element::Undead,
        "479_Undead_Snippit.bld",
        None,
    ),
    (
        480,
        "Bat Spin",
        Element::Undead,
        "480_Undead_Frightengale.bld",
        None,
    ),
    (481, "Funny Bone", Element::Undead, "481_Bonesaw.bld", None),
    // Villains (IDs are the archive file IDs).
    (
        1001,
        "Chompy Mage",
        Element::Life,
        "1001_ChompyMage.bld",
        Some("textures/characters/chompymage.png"),
    ),
    (
        1002,
        "Dr. Krankcase",
        Element::Tech,
        "1002_Crankcase.bld",
        Some("textures/characters/drkrankcase.png"),
    ),
    (
        1003,
        "Wolfgang",
        Element::Undead,
        "1003_Rocker.bld",
        Some("textures/characters/wolfgang.png"),
    ),
    (
        1004,
        "Chef Pepper Jack",
        Element::Fire,
        "1004_Chef.bld",
        Some("textures/characters/chefpepperjack.png"),
    ),
    (
        1007,
        "Golden Queen",
        Element::Earth,
        "1007_GoldenQueen.bld",
        Some("textures/characters/goldenqueen.png"),
    ),
    (
        1008,
        "Dreamcatcher",
        Element::Air,
        "1008_DreamTwin.bld",
        Some("textures/characters/dreamcatcher.png"),
    ),
    (
        1010,
        "Kaos",
        Element::Kaos,
        "1010_Kaos.bld",
        Some("textures/characters/kaos.png"),
    ),
    (
        1040,
        "Pain-Yatta",
        Element::Magic,
        "1040_Pinata.bld",
        Some("textures/characters/painyatta.png"),
    ),
];

pub fn find_character(character_id: u16) -> Option<&'static CharacterEntry> {
    let id = if character_id == 450 {
        407
    } else {
        character_id
    };
    CHARACTERS.iter().find(|entry| entry.0 == id)
}

pub fn get_character_info(character_id: u16) -> (&'static str, Element) {
    if let Some(&(_, name, element, _, _)) = find_character(character_id) {
        return (name, element);
    }
    match character_id {
        3000..=3100 => ("Trap", Element::Kaos),
        _ => ("Unknown Skylander", Element::Magic),
    }
}

pub fn resolve_character_id_by_name(name: &str) -> Option<u16> {
    let normalize = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase()
    };
    let clean = normalize(name);
    CHARACTERS
        .iter()
        .find(|entry| normalize(entry.1) == clean)
        .map(|entry| entry.0)
}

pub fn get_character_voice_path(character_id: u16) -> Option<&'static str> {
    match character_id {
        1010 => Some("audio/dialogue/vo_kaos_trapped.wav"),
        1001 => Some("audio/dialogue/vo_chompymage_trapped.wav"),
        1002 => Some("audio/dialogue/vo_drkrankcase_trapped.wav"),
        1003 => Some("audio/dialogue/vo_wolfgang_trapped.wav"),
        1004 => Some("audio/dialogue/vo_chefpepperjack_trapped.wav"),
        1007 => Some("audio/dialogue/vo_goldenqueen_trapped.wav"),
        1008 => Some("audio/dialogue/vo_dreamcatcher_trapped.wav"),
        1040 => Some("audio/dialogue/vo_painyatta_trapped.wav"),
        _ => None,
    }
}

pub fn resolve_character_model_archive(character_id: u16) -> Option<String> {
    if let Some(&(_, _, _, archive, _)) = find_character(character_id) {
        return Some(archive.to_string());
    }

    let search_dirs = [
        std::path::Path::new("assets/files/character"),
        std::path::Path::new("../../assets/files/character"),
        std::path::Path::new("../assets/files/character"),
    ];

    let prefix1 = format!("{}_", character_id);
    let prefix2 = format!("{:03}_", character_id);

    let is_auxiliary = |filename: &str| -> bool {
        let lower = filename.to_ascii_lowercase();
        lower.contains("musicmini")
            || lower.contains("outro")
            || lower.contains("stream")
            || lower.contains("card")
            || lower.contains("ally_icon")
            || lower.contains("_alt")
            || lower.contains("legendary")
    };

    let mut candidate_fallback = None;

    for dir in &search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if (name.starts_with(&prefix1) || name.starts_with(&prefix2))
                    && name.ends_with(".bld")
                {
                    if !is_auxiliary(&name) {
                        return Some(name);
                    } else if candidate_fallback.is_none() {
                        candidate_fallback = Some(name);
                    }
                }
            }
        }
    }

    if let Some(candidate) = candidate_fallback {
        return Some(candidate);
    }

    Some(format!("{}_Character.bld", character_id))
}

pub fn resolve_model_archive(category: &str, identifier: &str) -> Option<String> {
    let clean_id = identifier.replace('\\', "/");
    let file_name = clean_id.rsplit('/').next().unwrap_or(&clean_id);

    let base_name = if file_name.ends_with(".bld") || file_name.ends_with(".arc") {
        file_name.to_string()
    } else {
        format!("{}.bld", file_name)
    };

    let search_dirs = [
        format!("assets/files/{}", category),
        format!("../../assets/files/{}", category),
        format!("../assets/files/{}", category),
    ];

    for dir_str in &search_dirs {
        let dir = std::path::Path::new(dir_str);
        if !dir.is_dir() {
            continue;
        }

        let exact = dir.join(&base_name);
        if exact.exists() {
            return Some(base_name);
        }

        if let Ok(entries) = std::fs::read_dir(dir) {
            let lower_target = base_name.to_ascii_lowercase();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.to_ascii_lowercase() == lower_target
                    || name
                        .to_ascii_lowercase()
                        .starts_with(lower_target.strip_suffix(".bld").unwrap_or(&lower_target))
                {
                    return Some(name);
                }
            }
        }
    }

    Some(base_name)
}

pub fn get_character_asset_paths(character_id: u16) -> (&'static str, Option<&'static str>) {
    if let Some(&(_, _, _, archive, portrait)) = find_character(character_id) {
        return (archive, portrait);
    }
    match character_id {
        3000..=3100 => (
            "models/characters/trap.igb",
            Some("textures/characters/trap.png"),
        ),
        _ => ("models/hero.igb", None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_character_names_match_archives() {
        assert_eq!(get_character_info(1007).0, "Golden Queen");
        assert_eq!(
            resolve_character_model_archive(1007).unwrap(),
            "1007_GoldenQueen.bld"
        );
        assert_eq!(get_character_info(1004).0, "Chef Pepper Jack");
        assert_eq!(
            resolve_character_model_archive(1004).unwrap(),
            "1004_Chef.bld"
        );
        assert_eq!(get_character_asset_paths(402).0, "458_HeatShield.bld");
        assert_eq!(get_character_asset_paths(403).0, "454_Earth_Hunter.bld");
        assert_eq!(resolve_character_id_by_name("golden queen"), Some(1007));
        let mut ids: Vec<u16> = CHARACTERS.iter().map(|e| e.0).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CHARACTERS.len(), "duplicate character IDs");
    }

    #[test]
    fn test_get_character_asset_paths() {
        assert_eq!(
            get_character_asset_paths(400),
            (
                "462_Water_Crocky.bld",
                Some("textures/characters/snapshot.png")
            )
        );
        assert_eq!(
            get_character_asset_paths(1010),
            ("1010_Kaos.bld", Some("textures/characters/kaos.png"))
        );
        assert_eq!(
            get_character_asset_paths(1001),
            (
                "1001_ChompyMage.bld",
                Some("textures/characters/chompymage.png")
            )
        );
        assert_eq!(
            get_character_asset_paths(401),
            (
                "476_Artichoke.bld",
                Some("textures/characters/foodfight.png")
            )
        );
        assert_eq!(
            get_character_asset_paths(16),
            ("016_Spyro.bld", Some("textures/characters/spyro.png"))
        );
        assert_eq!(get_character_asset_paths(9999), ("models/hero.igb", None));
        assert_eq!(
            get_character_asset_paths(1007),
            (
                "1007_GoldenQueen.bld",
                Some("textures/characters/goldenqueen.png")
            )
        );

        assert_eq!(
            resolve_character_model_archive(400),
            Some("462_Water_Crocky.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(401),
            Some("476_Artichoke.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(1010),
            Some("1010_Kaos.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(1001),
            Some("1001_ChompyMage.bld".to_string())
        );
        assert!(resolve_character_model_archive(500).is_some());

        // Test canonical mapping / auxiliary filter for IDs with minigame/alt archives
        assert_eq!(
            resolve_character_model_archive(1),
            Some("001_Gryphon.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(2),
            Some("002_SkyTurtle.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(5),
            Some("005_LandShark.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(7),
            Some("007_GemGolem.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(8),
            Some("008_FireDragon.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(9),
            Some("009_Eruptor.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(17),
            Some("017_SkullOrc.bld".to_string())
        );
        assert_eq!(
            resolve_character_model_archive(20),
            Some("020_Metal_Dragon.bld".to_string())
        );

        assert_eq!(resolve_character_id_by_name("Snap Shot"), Some(400));
        assert_eq!(resolve_character_id_by_name("food-fight"), Some(401));
        assert_eq!(resolve_character_id_by_name("spyro"), Some(16));
        assert_eq!(resolve_character_id_by_name("NonExistent"), None);

        assert!(resolve_model_archive("outfit", "Hat_114").is_some());
        assert!(resolve_model_archive("item", "Item_HelpingHand").is_some());
        assert!(resolve_model_archive("character", "1001_ChompyMage").is_some());
    }
}
