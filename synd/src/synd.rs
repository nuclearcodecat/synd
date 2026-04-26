#![allow(unused)]

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
	cell::LazyCell,
	env,
	fs::{File, OpenOptions},
	io::Read,
	path::{Path, PathBuf},
	slice::Split,
	sync::LazyLock,
	time::{Duration, Instant},
};

static HOMEDIR: LazyLock<PathBuf> = LazyLock::new(|| env::home_dir().expect("$HOME not set"));

static CONFIGDIR: LazyLock<PathBuf> = LazyLock::new(|| match env::var("XDG_CONFIG_DIR") {
	Ok(d) => PathBuf::from(d),
	Err(_) => {
		let homedir = HOMEDIR.clone();
		homedir.join(".config/synd")
	}
});

pub struct Synd {
	config: Config,
	last_fetch: Option<Instant>,
	followed: Vec<FollowedEntry>,
}

#[derive(Serialize, Deserialize, Debug)]
struct FollowedEntry {
	uuid: uuid::Uuid,
	name: Option<String>,
	url: String,
}

#[derive(Debug)]
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
				let parsed = match ass.parse() {
					Ok(x) => x,
					Err(er) => {
						eprintln!(
							"parsing error: {er}, invalid assignment \"{ass}\" for \"{var}\", using default value"
						);
						return;
					}
				};
				self.fetch_interval = Duration::from_secs(parsed);
			}
			"action" => {
				// expect to be in configdir for now
				let filetail = ass;
				let mut configdir = CONFIGDIR.clone();
				configdir = configdir.join(filetail);
				self.action = Some(configdir);
			}
			_ => {
				eprintln!("invalid config variable \"{var}\"");
			}
		}
	}

	fn parse() -> Self {
		let mut new = Self::default();
		let configpath = CONFIGDIR.clone().join("config");
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
			}
			Err(er) => {
				eprintln!("config file missing or unavailable ({er}). using defaults.");
			}
		}
		new
	}
}

impl Synd {
	fn fill_followed(&mut self) -> anyhow::Result<()> {
		let mut oo = OpenOptions::new();
		oo.write(true).read(true).create(true);
		let fp = CONFIGDIR.clone().join("followed.db");
		println!("{fp:?}");
		let mut file = oo
			.open(fp)
			.with_context(|| "while opening followed.db file")?;
		let new: Vec<FollowedEntry> = serde_json::from_reader(file)?;
		println!("followed: {new:#?}");
		self.followed = new;

		Ok(())
	}

	pub fn new() -> anyhow::Result<Self> {
		let config = Config::parse();
		println!("=== finished parsing config ===\n{config:#?}");
		let mut new = Self {
			config,
			last_fetch: None,
			followed: Vec::new(),
		};
		new.fill_followed()?;
		Ok(new)
	}

	fn fetch_feeds(&mut self) -> anyhow::Result<()> {
		if let Some(last_fetch) = self.last_fetch
			&& last_fetch.elapsed() < self.config.fetch_interval
		{
			return Ok(());
		}
		self.last_fetch = Some(Instant::now());
		println!("=== fetching feeds ===");
		Ok(())
	}

	pub fn work(&mut self) -> anyhow::Result<()> {
		self.fetch_feeds();
		Ok(())
	}
}
