//! Shared arguments for the console entry point and the Slint application.
use std::io;
use std::path::PathBuf;

use crate::game::Game;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct LaunchOptions {
    pub debug: bool,
    pub nearby: bool,
    pub game: Option<Game>,
    pub config_dir: Option<PathBuf>,
    pub femtovg: bool,
    pub help: bool,
}

impl LaunchOptions {
    pub const HELP: &str = "PoE Auto Flask
Usage: poe2-auto-flask.exe [--auto | --debug] [--game poe1|poe2] [--nearby]
                          [--config-dir PATH] [--renderer=software|femtovg]
Normal launch opens the UI with automation disabled.
--debug runs read-only terminal diagnostics.
--nearby enables the experimental PoE1 diagnostic census.";

    pub fn parse(args: impl Iterator<Item = String>) -> io::Result<Self> {
        let mut options = Self::default();
        let mut mode_seen = false;
        let mut args = args;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--auto" | "--debug" if !mode_seen => {
                    options.debug = arg == "--debug";
                    mode_seen = true;
                }
                "--nearby" if !options.nearby => options.nearby = true,
                "--game" if options.game.is_none() => {
                    options.game = Some(
                        args.next()
                            .ok_or_else(|| invalid("--game requires poe1 or poe2"))?
                            .parse()
                            .map_err(invalid)?,
                    );
                }
                "--config-dir" if options.config_dir.is_none() => {
                    options.config_dir = Some(PathBuf::from(
                        args.next()
                            .ok_or_else(|| invalid("--config-dir requires a path"))?,
                    ));
                }
                "--renderer=software" => options.femtovg = false,
                "--renderer=femtovg" => options.femtovg = true,
                "--help" | "-h" => options.help = true,
                _ => return Err(invalid(format!("Unknown or repeated argument {arg}"))),
            }
        }
        if options.nearby && (!options.debug || options.game == Some(Game::Poe2)) {
            return Err(invalid("--nearby requires PoE1 --debug"));
        }
        Ok(options)
    }

    #[cfg(windows)]
    pub fn backend(&self) -> crate::backend::Options {
        crate::backend::Options {
            mode: if self.debug {
                crate::backend::Mode::Debug
            } else {
                crate::backend::Mode::Auto
            },
            game: self.game,
            nearby_debug: self.nearby,
            config_dir: self.config_dir.clone(),
        }
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> io::Result<LaunchOptions> {
        LaunchOptions::parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn launch_modes_keep_diagnostics_read_only_and_validate_arguments() {
        assert!(!parse(&[]).unwrap().debug);
        assert!(
            parse(&["--debug", "--game", "poe1", "--nearby"])
                .unwrap()
                .debug
        );
        for args in [
            vec!["--nearby"],
            vec!["--debug", "--nearby", "--game", "poe2"],
            vec!["--debug", "--auto"],
            vec!["--game"],
            vec!["--game", "poe3"],
        ] {
            assert!(parse(&args).is_err());
        }
        let options = parse(&["--config-dir", "C:\\profile path", "--game", "poe2"]).unwrap();
        assert_eq!(
            options.config_dir.unwrap(),
            PathBuf::from("C:\\profile path")
        );
        assert_eq!(options.game, Some(Game::Poe2));
    }
}
