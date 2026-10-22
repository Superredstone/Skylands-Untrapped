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

pub fn get_character_info(character_id: u16) -> (&'static str, Element) {
    match character_id {
        14 => ("Gill Grunt", Element::Water),
        16 => ("Spyro", Element::Magic),
        19 => ("Trigger Happy", Element::Tech),
        400 => ("Snap Shot", Element::Water),
        401 => ("Food Fight", Element::Life),
        402 => ("Wildfire", Element::Fire),
        403 => ("Wallop", Element::Earth),
        404 => ("Gearshift", Element::Tech),
        405 => ("Krypt King", Element::Undead),
        406 => ("Enigma", Element::Magic),
        407 | 450 => ("Gusto", Element::Air),
        1010 => ("Kaos", Element::Kaos),
        1001 => ("Chompy Mage", Element::Life),
        1002 => ("Wolfgang", Element::Undead),
        1003 => ("Chompy Mage", Element::Life),
        1004 => ("Golden Queen", Element::Earth),
        1005 => ("Chef Pepper Jack", Element::Fire),
        1006 => ("Dr. Krankcase", Element::Tech),
        1007 => ("Dreamcatcher", Element::Air),
        1008 => ("Pain-Yatta", Element::Magic),
        3000..=3100 => ("Trap", Element::Kaos),
        _ => ("Unknown Skylander", Element::Magic),
    }
}

pub fn resolve_character_id_by_name(name: &str) -> Option<u16> {
    let clean: String = name
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    match clean.as_str() {
        "snapshot" => Some(400),
        "foodfight" => Some(401),
        "wildfire" => Some(402),
        "wallop" => Some(403),
        "gearshift" => Some(404),
        "kryptking" => Some(405),
        "enigma" => Some(406),
        "gusto" => Some(407),
        "spyro" => Some(16),
        "gillgrunt" => Some(14),
        "triggerhappy" => Some(19),
        "kaos" => Some(1010),
        "chompymage" => Some(1001),
        _ => None,
    }
}

pub fn get_character_voice_path(character_id: u16) -> Option<&'static str> {
    match character_id {
        1010 => Some("audio/dialogue/vo_kaos_trapped.wav"),
        1001 => Some("audio/dialogue/vo_thegulper_trapped.wav"),
        1002 => Some("audio/dialogue/vo_wolfgang_trapped.wav"),
        1003 => Some("audio/dialogue/vo_chompymage_trapped.wav"),
        1004 => Some("audio/dialogue/vo_goldenqueen_trapped.wav"),
        1005 => Some("audio/dialogue/vo_chefpepperjack_trapped.wav"),
        1006 => Some("audio/dialogue/vo_drkrankcase_trapped.wav"),
        1007 => Some("audio/dialogue/vo_dreamcatcher_trapped.wav"),
        1008 => Some("audio/dialogue/vo_painyatta_trapped.wav"),
        _ => None,
    }
}

pub fn resolve_character_model_archive(character_id: u16) -> Option<String> {
    match character_id {
        0 => return Some("000_AirDragon.bld".to_string()),
        1 => return Some("001_Gryphon.bld".to_string()),
        2 => return Some("002_SkyTurtle.bld".to_string()),
        3 => return Some("003_StormGiant.bld".to_string()),
        4 => return Some("004_Rock_Dragon.bld".to_string()),
        5 => return Some("005_LandShark.bld".to_string()),
        6 => return Some("006_DinoRang.bld".to_string()),
        7 => return Some("007_GemGolem.bld".to_string()),
        8 => return Some("008_FireDragon.bld".to_string()),
        9 => return Some("009_Eruptor.bld".to_string()),
        10 => return Some("010_FlameKnight.bld".to_string()),
        11 => return Some("011_FireElf.bld".to_string()),
        12 => return Some("012_SeaDragon.bld".to_string()),
        13 => return Some("013_Crab.bld".to_string()),
        14 => return Some("014_GillGrunt.bld".to_string()),
        15 => return Some("015_Yeti.bld".to_string()),
        16 => return Some("016_Spyro.bld".to_string()),
        17 => return Some("017_SkullOrc.bld".to_string()),
        18 => return Some("018_Wizard.bld".to_string()),
        19 => return Some("019_GoldenCheat.bld".to_string()),
        20 => return Some("020_Metal_Dragon.bld".to_string()),
        21 => return Some("021_DrillBot.bld".to_string()),
        22 => return Some("022_Bomb_Troll.bld".to_string()),
        23 => return Some("023_ForceFieldGrub.bld".to_string()),
        24 => return Some("024_LifeDragon.bld".to_string()),
        25 => return Some("025_BamBazooker.bld".to_string()),
        26 => return Some("026_StealthElf.bld".to_string()),
        27 => return Some("027_Ent.bld".to_string()),
        29 => return Some("029_ShadowMaid.bld".to_string()),
        30 => return Some("030_PandoranElite.bld".to_string()),
        31 => return Some("031_GhostEater.bld".to_string()),
        32 => return Some("032_Cynder.bld".to_string()),
        400 | 462 => return Some("462_Water_Crocky.bld".to_string()),
        401 => return Some("476_Artichoke.bld".to_string()),
        402 => return Some("459_Barbarian.bld".to_string()),
        403 => return Some("457_Earth_Boulderdash.bld".to_string()),
        404 => return Some("470_Tech_BruteForce.bld".to_string()),
        405 => return Some("481_Bonesaw.bld".to_string()),
        406 => return Some("467_Void.bld".to_string()),
        407 | 450 => return Some("450_Typhoon.bld".to_string()),
        1010 => return Some("1010_Kaos.bld".to_string()),
        1001 => return Some("1001_ChompyMage.bld".to_string()),
        1002 => return Some("1002_Crankcase.bld".to_string()),
        1003 => return Some("1003_Rocker.bld".to_string()),
        1004 => return Some("1004_Chef.bld".to_string()),
        1005 => return Some("1005_Thief.bld".to_string()),
        1006 => return Some("1006_LightVillain.bld".to_string()),
        1007 => return Some("1007_GoldenQueen.bld".to_string()),
        1008 => return Some("1008_DreamTwin.bld".to_string()),
        _ => {}
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
    match character_id {
        400 | 462 => (
            "462_Water_Crocky.bld",
            Some("textures/characters/snapshot.png"),
        ),
        401 => (
            "476_Artichoke.bld",
            Some("textures/characters/foodfight.png"),
        ),
        402 => (
            "459_Barbarian.bld",
            Some("textures/characters/wildfire.png"),
        ),
        403 => (
            "457_Earth_Boulderdash.bld",
            Some("textures/characters/wallop.png"),
        ),
        404 => (
            "470_Tech_BruteForce.bld",
            Some("textures/characters/gearshift.png"),
        ),
        405 => ("481_Bonesaw.bld", Some("textures/characters/kryptking.png")),
        406 => ("467_Void.bld", Some("textures/characters/enigma.png")),
        407 | 450 => ("450_Typhoon.bld", Some("textures/characters/gusto.png")),
        14 => (
            "014_GillGrunt.bld",
            Some("textures/characters/gillgrunt.png"),
        ),
        16 => ("016_Spyro.bld", Some("textures/characters/spyro.png")),
        19 => (
            "019_GoldenCheat.bld",
            Some("textures/characters/triggerhappy.png"),
        ),
        1010 => ("1010_Kaos.bld", Some("textures/characters/kaos.png")),
        1001 => (
            "1001_ChompyMage.bld",
            Some("textures/characters/chompymage.png"),
        ),
        1002 => (
            "1002_Crankcase.bld",
            Some("textures/characters/wolfgang.png"),
        ),
        1003 => (
            "1003_Rocker.bld",
            Some("textures/characters/chompymage.png"),
        ),
        1004 => ("1004_Chef.bld", Some("textures/characters/goldenqueen.png")),
        1005 => (
            "1005_Thief.bld",
            Some("textures/characters/chefpepperjack.png"),
        ),
        1006 => (
            "1006_LightVillain.bld",
            Some("textures/characters/drkrankcase.png"),
        ),
        1007 => (
            "1007_GoldenQueen.bld",
            Some("textures/characters/dreamcatcher.png"),
        ),
        1008 => (
            "1008_DreamTwin.bld",
            Some("textures/characters/painyatta.png"),
        ),
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
