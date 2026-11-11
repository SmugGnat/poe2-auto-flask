//! Flask type is classified from base-item metadata, never the equipped slot.
//! All 18 numbered PoE1 utility bases follow the BaseItemTypes metadata IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlaskType {
    Life,
    Mana,
    Hybrid,
    Diamond,
    Ruby,
    Sapphire,
    Topaz,
    Granite,
    Quicksilver,
    Amethyst,
    Quartz,
    Jade,
    Basalt,
    Aquamarine,
    Stibnite,
    Sulphur,
    Silver,
    Bismuth,
    Gold,
    Corundum,
    Iron,
    OtherUtility,
    Unknown,
}

pub const UTILITY_TYPES: [FlaskType; 18] = [
    FlaskType::Quicksilver,
    FlaskType::Granite,
    FlaskType::Quartz,
    FlaskType::Diamond,
    FlaskType::Jade,
    FlaskType::Silver,
    FlaskType::Basalt,
    FlaskType::Stibnite,
    FlaskType::Sulphur,
    FlaskType::Ruby,
    FlaskType::Sapphire,
    FlaskType::Topaz,
    FlaskType::Amethyst,
    FlaskType::Bismuth,
    FlaskType::Aquamarine,
    FlaskType::Gold,
    FlaskType::Corundum,
    FlaskType::Iron,
];

impl FlaskType {
    pub fn from_metadata(path: &str) -> Self {
        let Some(base) = path.strip_prefix("Metadata/Items/Flasks/") else {
            return Self::Unknown;
        };
        if let Some(num) = base
            .strip_prefix("FlaskUtility")
            .filter(|part| numeric(part))
        {
            return match num.parse::<u8>() {
                Ok(1) => Self::Diamond,
                Ok(2) => Self::Ruby,
                Ok(3) => Self::Sapphire,
                Ok(4) => Self::Topaz,
                Ok(5) => Self::Granite,
                Ok(6) => Self::Quicksilver,
                Ok(7) => Self::Amethyst,
                Ok(8) => Self::Quartz,
                Ok(9) => Self::Jade,
                Ok(10) => Self::Basalt,
                Ok(11) => Self::Aquamarine,
                Ok(12) => Self::Stibnite,
                Ok(13) => Self::Sulphur,
                Ok(14) => Self::Silver,
                Ok(15) => Self::Bismuth,
                Ok(16) => Self::Gold,
                Ok(17) => Self::Corundum,
                Ok(18) => Self::Iron,
                _ => Self::OtherUtility,
            };
        }
        match base {
            _ if base.strip_prefix("FlaskLife").is_some_and(numeric) => Self::Life,
            _ if base.strip_prefix("FlaskMana").is_some_and(numeric) => Self::Mana,
            _ if base.strip_prefix("FlaskHybrid").is_some_and(numeric) => Self::Hybrid,
            _ => Self::Unknown,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Life => "Life",
            Self::Mana => "Mana",
            Self::Hybrid => "Hybrid",
            Self::Diamond => "Diamond",
            Self::Ruby => "Ruby",
            Self::Sapphire => "Sapphire",
            Self::Topaz => "Topaz",
            Self::Granite => "Granite",
            Self::Quicksilver => "Quicksilver",
            Self::Amethyst => "Amethyst",
            Self::Quartz => "Quartz",
            Self::Jade => "Jade",
            Self::Basalt => "Basalt",
            Self::Aquamarine => "Aquamarine",
            Self::Stibnite => "Stibnite",
            Self::Sulphur => "Sulphur",
            Self::Silver => "Silver",
            Self::Bismuth => "Bismuth",
            Self::Gold => "Gold",
            Self::Corundum => "Corundum",
            Self::Iron => "Iron",
            Self::OtherUtility => "Other utility",
            Self::Unknown => "Unknown",
        }
    }
}

fn numeric(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{FlaskType as F, UTILITY_TYPES};

    #[test]
    fn maps_all_18_poe1_utility_base_ids() {
        let expected = [
            F::Diamond,
            F::Ruby,
            F::Sapphire,
            F::Topaz,
            F::Granite,
            F::Quicksilver,
            F::Amethyst,
            F::Quartz,
            F::Jade,
            F::Basalt,
            F::Aquamarine,
            F::Stibnite,
            F::Sulphur,
            F::Silver,
            F::Bismuth,
            F::Gold,
            F::Corundum,
            F::Iron,
        ];
        for (index, kind) in expected.into_iter().enumerate() {
            let path = format!("Metadata/Items/Flasks/FlaskUtility{}", index + 1);
            assert_eq!(F::from_metadata(&path), kind, "{path}");
        }
        assert_eq!(UTILITY_TYPES.len(), 18);
        for kind in UTILITY_TYPES {
            assert!(expected.contains(&kind), "missing utility type {:?}", kind);
        }
    }

    #[test]
    fn recovery_and_unrecognized_bases_remain_distinct() {
        assert_eq!(
            F::from_metadata("Metadata/Items/Flasks/FlaskLife11"),
            F::Life
        );
        assert_eq!(
            F::from_metadata("Metadata/Items/Flasks/FlaskMana10"),
            F::Mana
        );
        assert_eq!(
            F::from_metadata("Metadata/Items/Flasks/FlaskHybrid2"),
            F::Hybrid
        );
        assert_eq!(
            F::from_metadata("Metadata/Items/Flasks/FlaskUtility19"),
            F::OtherUtility
        );
        assert_eq!(
            F::from_metadata("Metadata/Items/Other/FlaskUtility6"),
            F::Unknown
        );
        assert_eq!(
            F::from_metadata("Metadata/Items/Flasks/FlaskManaOops"),
            F::Unknown
        );
    }
}
