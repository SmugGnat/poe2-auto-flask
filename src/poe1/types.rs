//! Flask identity is derived from the item's base metadata, never from its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlaskType {
    Life,
    Mana,
    Hybrid,
    Quicksilver,
    Granite,
    Quartz,
    Diamond,
    OtherUtility,
    Unknown,
}

impl FlaskType {
    pub fn from_metadata(path: &str) -> Self {
        let Some(base) = path.strip_prefix("Metadata/Items/Flasks/") else {
            return Self::Unknown;
        };
        match base {
            "FlaskUtility6" => Self::Quicksilver,
            "FlaskUtility5" => Self::Granite,
            "FlaskUtility8" => Self::Quartz,
            "FlaskUtility1" => Self::Diamond,
            _ if base.strip_prefix("FlaskLife").is_some_and(numeric) => Self::Life,
            _ if base.strip_prefix("FlaskMana").is_some_and(numeric) => Self::Mana,
            _ if base.strip_prefix("FlaskHybrid").is_some_and(numeric) => Self::Hybrid,
            _ if base.strip_prefix("FlaskUtility").is_some_and(numeric) => Self::OtherUtility,
            _ => Self::Unknown,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Life => "Life",
            Self::Mana => "Mana",
            Self::Hybrid => "Hybrid",
            Self::Quicksilver => "Quicksilver",
            Self::Granite => "Granite",
            Self::Quartz => "Quartz",
            Self::Diamond => "Diamond",
            Self::OtherUtility => "Other utility",
            Self::Unknown => "Unknown",
        }
    }
}

fn numeric(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::FlaskType as F;

    #[test]
    fn identifies_flasks_from_verified_metadata() {
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskUtility6"), F::Quicksilver);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskUtility5"), F::Granite);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskUtility8"), F::Quartz);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskUtility1"), F::Diamond);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskLife11"), F::Life);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskMana10"), F::Mana);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskHybrid2"), F::Hybrid);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskUtility14"), F::OtherUtility);
        assert_eq!(F::from_metadata("Metadata/Items/Other/FlaskLife11"), F::Unknown);
        assert_eq!(F::from_metadata("Metadata/Items/Flasks/FlaskManaOops"), F::Unknown);
    }
}
