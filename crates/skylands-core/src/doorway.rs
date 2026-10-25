use bevy::prelude::*;
use skylands_formats::igb::LevelMarker;

use crate::resources::{
    chapter_from_level_archive, chapter_level_archive, chapter_title, CampaignProgress,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DoorwayDestination {
    pub target_zone: String,
    pub target_level: Option<String>,
    pub target_spawn: Vec3,
    pub prompt: String,
}

pub fn resolve_doorway_destination(
    current_level: &str,
    marker_name: &str,
    marker_position: [f32; 3],
    all_markers: &[LevelMarker],
    campaign: &CampaignProgress,
) -> DoorwayDestination {
    let lower_marker = marker_name.to_ascii_lowercase();
    let lower_level = current_level.to_ascii_lowercase();
    let is_hub = lower_level.contains("hub");

    // 1. Transit crystals (format: Transit_XX_<Source>-<Destination>)
    if let Some(dash_idx) = marker_name.find('-') {
        let prefix = &marker_name[..dash_idx];
        let suffix = &marker_name[dash_idx + 1..];
        let source = prefix.rsplit('_').next().unwrap_or(prefix);
        let dest = suffix.split('_').next().unwrap_or(suffix);

        let partner_tag = format!("{dest}-{source}");
        let partner = all_markers.iter().find(|m| m.name.contains(&partner_tag));

        let formatted_dest = dest
            .chars()
            .enumerate()
            .fold(String::new(), |mut acc, (i, c)| {
                if i > 0 && c.is_uppercase() {
                    acc.push(' ');
                }
                acc.push(c);
                acc
            });

        if let Some(p) = partner {
            return DoorwayDestination {
                target_zone: formatted_dest.clone(),
                target_level: None,
                target_spawn: Vec3::new(p.position[0], p.position[1] + 0.5, p.position[2] + 2.0),
                prompt: format!("Press A to warp to {formatted_dest}"),
            };
        }
    }

    // 2. Hub-specific landmarks and doors
    if is_hub {
        if lower_marker.starts_with("door_mainentrance") {
            return DoorwayDestination {
                target_zone: "Academy Main Hall".to_string(),
                target_level: None,
                target_spawn: Vec3::new(-12.0, -0.6, -130.0),
                prompt: "Press A to enter Academy Main Hall".to_string(),
            };
        }
        if lower_marker.starts_with("door_persephone") {
            return DoorwayDestination {
                target_zone: "Persephone's Room".to_string(),
                target_level: None,
                target_spawn: Vec3::new(75.0, -10.5, -25.0),
                prompt: "Press A to visit Persephone".to_string(),
            };
        }
        if lower_marker.contains("horde") || lower_marker.contains("arena") {
            return DoorwayDestination {
                target_zone: "Brock's Arena".to_string(),
                target_level: Some("Arena_Challenge_300_000.bld".to_string()),
                target_spawn: Vec3::ZERO,
                prompt: "Press A to enter Arena Challenge".to_string(),
            };
        }

        // Chapter gateway / portal in hub
        for ch in 1..=18 {
            let ch_tag = format!("chapter_{ch}");
            let ch_tag_alt = format!("chapter{ch}");
            if lower_marker.contains(&ch_tag) || lower_marker.contains(&ch_tag_alt) {
                let title = chapter_title(ch).unwrap_or("Unknown Chapter");
                let archive = chapter_level_archive(ch).unwrap_or("Level_Hub_2014_01.bld");
                if campaign.is_chapter_unlocked(ch) {
                    return DoorwayDestination {
                        target_zone: title.to_string(),
                        target_level: Some(archive.to_string()),
                        target_spawn: Vec3::ZERO,
                        prompt: format!("Press A to travel to Chapter {ch}: {title}"),
                    };
                } else {
                    return DoorwayDestination {
                        target_zone: title.to_string(),
                        target_level: None,
                        target_spawn: Vec3::new(
                            marker_position[0],
                            marker_position[1],
                            marker_position[2],
                        ),
                        prompt: format!("Chapter {ch} is locked"),
                    };
                }
            }
        }
    } else {
        // 3. Exits in non-hub levels: return to Academy Hub
        if lower_marker.contains("exit")
            || lower_marker.contains("hub")
            || lower_marker.contains("return")
            || lower_marker == "exit_spot"
        {
            let current_ch = chapter_from_level_archive(current_level);
            let prompt = if let Some(ch) = current_ch {
                format!("Press A to complete Chapter {ch} and return to Academy")
            } else {
                "Press A to return to Academy".to_string()
            };

            return DoorwayDestination {
                target_zone: "Academy Courtyard".to_string(),
                target_level: Some("Level_Hub_2014_01.bld".to_string()),
                target_spawn: Vec3::new(0.0, 0.0, 0.0),
                prompt,
            };
        }
    }

    // 4. Fallback: local teleport 2 units forward
    let clean_name = marker_name.replace('_', " ");
    DoorwayDestination {
        target_zone: clean_name.clone(),
        target_level: None,
        target_spawn: Vec3::new(
            marker_position[0],
            marker_position[1],
            marker_position[2] + 2.0,
        ),
        prompt: format!("Press A to enter {clean_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transit_crystal_reciprocal_matching() {
        let markers = vec![
            LevelMarker {
                name: "Transit_01_LandingArea-MainHall".to_string(),
                kind: skylands_formats::igb::MarkerKind::Doorway,
                position: [-31.6, -2.1, 22.2],
                world_transform: [0.0; 16],
                node_index: 0,
                geometry_index: None,
                mesh: None,
                drawn_by_level: false,
                script: None,
                animated: false,
                actor_model_index: None,
            },
            LevelMarker {
                name: "Transit_00_MainHall-LandingArea".to_string(),
                kind: skylands_formats::igb::MarkerKind::Doorway,
                position: [-12.0, -0.6, -134.5],
                world_transform: [0.0; 16],
                node_index: 1,
                geometry_index: None,
                mesh: None,
                drawn_by_level: false,
                script: None,
                animated: false,
                actor_model_index: None,
            },
        ];

        let campaign = CampaignProgress::default();
        let dest = resolve_doorway_destination(
            "Level_Hub_2014_01.bld",
            "Transit_01_LandingArea-MainHall",
            [-31.6, -2.1, 22.2],
            &markers,
            &campaign,
        );

        assert_eq!(dest.target_zone, "Main Hall");
        assert!(dest.target_level.is_none());
        assert!((dest.target_spawn.x - (-12.0)).abs() < 0.1);
        assert!((dest.target_spawn.z - (-132.5)).abs() < 0.1);
    }

    #[test]
    fn test_hub_door_main_entrance() {
        let campaign = CampaignProgress::default();
        let dest = resolve_doorway_destination(
            "Level_Hub_2014_01.bld",
            "Door_MainEntrance_L",
            [-2.0, -0.5, -59.7],
            &[],
            &campaign,
        );
        assert_eq!(dest.target_zone, "Academy Main Hall");
        assert_eq!(dest.prompt, "Press A to enter Academy Main Hall");
    }

    #[test]
    fn test_chapter_exit_returns_to_hub() {
        let campaign = CampaignProgress::default();
        let dest = resolve_doorway_destination(
            "Level_313_Tutorial.bld",
            "Door_Exit",
            [100.0, 10.0, 50.0],
            &[],
            &campaign,
        );
        assert_eq!(dest.target_level.as_deref(), Some("Level_Hub_2014_01.bld"));
        assert_eq!(dest.target_zone, "Academy Courtyard");
        assert!(dest.prompt.contains("complete Chapter 1"));
    }
}
