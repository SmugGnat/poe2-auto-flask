//! Explicit game identity: no implicit switching between different live clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Poe1,
    Poe2,
}

impl Game {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Poe1 => "Path of Exile 1",
            Self::Poe2 => "Path of Exile 2",
        }
    }

    pub const fn steam_app_id(self) -> &'static str {
        match self {
            Self::Poe1 => "238960",
            Self::Poe2 => "2694490",
        }
    }
}

impl std::str::FromStr for Game {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "poe1" => Ok(Self::Poe1),
            "poe2" => Ok(Self::Poe2),
            _ => Err("game must be poe1 or poe2"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Game;

    #[test]
    fn game_identity_is_explicit() {
        assert_eq!("poe1".parse::<Game>(), Ok(Game::Poe1));
        assert_eq!("poe2".parse::<Game>(), Ok(Game::Poe2));
        assert!("poe3".parse::<Game>().is_err());
        assert_eq!(Game::Poe1.steam_app_id(), "238960");
        assert_eq!(Game::Poe2.steam_app_id(), "2694490");
    }
}
