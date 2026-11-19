//! Explicit game identity: no implicit switching between different live clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Poe1,
    Poe2,
}

impl Game {
    pub fn from_steam_app_id(id: &str) -> Option<Self> {
        match id.trim() {
            "238960" => Some(Self::Poe1),
            "2694490" => Some(Self::Poe2),
            _ => None,
        }
    }

    pub fn from_steam_context() -> std::io::Result<Option<Self>> {
        let app_ids = ["STEAM_COMPAT_APP_ID", "SteamAppId", "SteamGameId"]
            .map(|name| std::env::var(name).ok());
        let compat_data = std::env::var("STEAM_COMPAT_DATA_PATH").ok();
        game_from_context(
            app_ids.iter().filter_map(|id| id.as_deref()),
            compat_data.as_deref(),
        )
    }

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

fn game_from_context<'a>(
    app_ids: impl Iterator<Item = &'a str>,
    compat_data: Option<&str>,
) -> std::io::Result<Option<Game>> {
    // Split both Unix and Wine path forms, including a trailing separator.
    let prefix_game = compat_data
        .and_then(|path| {
            path.trim_end_matches(['/', '\\'])
                .rsplit(['/', '\\'])
                .next()
        })
        .and_then(Game::from_steam_app_id);
    let mut selected = prefix_game;
    for game in app_ids.filter_map(Game::from_steam_app_id) {
        if selected.is_some_and(|previous| previous != game) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Steam game IDs conflict with the Proton context. Pass --game poe1 or --game poe2 explicitly.",
            ));
        }
        selected = Some(game);
    }
    Ok(selected)
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
    use super::{game_from_context, Game};

    #[test]
    fn proton_prefix_selects_the_game_without_app_id_variables() {
        for (path, expected) in [
            (
                "/mnt/data/SteamLibrary/steamapps/compatdata/238960",
                Game::Poe1,
            ),
            (
                "/mnt/data/SteamLibrary/steamapps/compatdata/2694490/",
                Game::Poe2,
            ),
            (
                "Z:\\Steam Library\\steamapps\\compatdata\\238960\\",
                Game::Poe1,
            ),
        ] {
            assert_eq!(
                game_from_context([].into_iter(), Some(path)).unwrap(),
                Some(expected)
            );
        }
        assert_eq!(
            game_from_context([].into_iter(), Some("/tmp/custom-prefix")).unwrap(),
            None
        );
    }

    #[test]
    fn steam_ids_and_prefix_must_agree_when_both_identify_a_game() {
        assert_eq!(
            game_from_context(["238960", "0"].into_iter(), Some("/compatdata/238960")).unwrap(),
            Some(Game::Poe1)
        );
        assert_eq!(
            game_from_context(["2694490"].into_iter(), None).unwrap(),
            Some(Game::Poe2)
        );
        assert!(game_from_context(["2694490"].into_iter(), Some("/compatdata/238960")).is_err());
        assert!(game_from_context(["238960", "2694490"].into_iter(), None).is_err());
    }

    #[test]
    fn game_identity_is_explicit() {
        assert_eq!("poe1".parse::<Game>(), Ok(Game::Poe1));
        assert_eq!("poe2".parse::<Game>(), Ok(Game::Poe2));
        assert!("poe3".parse::<Game>().is_err());
        assert_eq!(Game::Poe1.steam_app_id(), "238960");
        assert_eq!(Game::Poe2.steam_app_id(), "2694490");
    }
}
