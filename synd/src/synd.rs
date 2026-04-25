#![allow(unused)]

use std::{
	cell::LazyCell,
	env,
	fs::File,
	io::Read,
	path::{Path, PathBuf},
	slice::Split,
	sync::LazyLock,
	time::Duration,
};

static CONFIGDIR: LazyLock<PathBuf> = LazyLock::new(|| match env::var("XDG_CONFIG_DIR") {
	Ok(d) => PathBuf::from(d),
	Err(_) => {
		let homedir = env::home_dir().expect("$HOME nor $XDG_CONFIG_DIR not set");
		homedir.join(".config");
		homedir
	}
});

pub struct Synd {
	config: Config,
}

struct Config {
	fetch_interval: Duration,
	action: Option<PathBuf>,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			fetch_interval: Duration::from_mins(10),
			action: None,
		}
	}
}

impl Config {
	fn update_from_config_pair(&mut self, var: &str, ass: &str) {
		match var {
			"fetch interval" => {
				let parsed = match ass.parse::<usize>() {
					Ok(x) => x,
					Err(er) => {
						eprintln!(
							"parsing error: {er}, invalid assignment \"{ass}\" for \"{var}\", using default value"
						);
						return;
					}
				};
			}
			"action" => {
				// expect to be in configdir for now
				// let
				// self.action = Some()
			}
			_ => {
				eprintln!("invalid config variable \"{var}\"");
			}
		}
	}

	fn parse() -> Self {
		let mut new = Self::default();
		let configdir = CONFIGDIR.join("synd");
		let configpath = configdir.join("config");
		let mut contents = String::new();
		match File::open(&configpath) {
			Ok(mut file) => {
				file.read_to_string(&mut contents);
				let lines = contents.lines();
				let valid = lines
					.filter_map(|line| {
						// ignore comments
						line.split("//").next()
						// .map(|s| s.trim())
						// .filter(|s| !s.is_empty())
					})
					.filter_map(|valid| valid.split_once(" = "))
					.collect::<Vec<_>>();
				for (var, ass) in valid {
					new.update_from_config_pair(var, ass);
				}
				todo!()
			}
			Err(er) => {
				eprintln!("config file missing or unavailable ({er}). using defaults.");
				Self::default()
			}
		};

		todo!()
	}
}

impl Synd {
	pub fn new() -> Self {
		let config = Config::parse();
		Self { config }
	}

	pub fn work() -> anyhow::Result<()> {
		Ok(())
	}
}
