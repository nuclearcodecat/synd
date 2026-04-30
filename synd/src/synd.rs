// #![allow(unused)]

use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
	env,
	fs::{self, File},
	io::{BufRead, BufReader, Cursor, Read, Write},
	os::unix::net::UnixListener,
	path::PathBuf,
	sync::LazyLock,
	time::{Duration, Instant, SystemTime},
};
use synd_common::{
	FeedsCommand, FollowDbCommand, MainLoopCommand, SocketQuery, SocketResponse,
	SocketResponseStatus, SyndError, ToSerializedResponse,
};

use crate::{db::Db, systime::SysTime};

pub static HOMEDIR: LazyLock<PathBuf> = LazyLock::new(|| env::home_dir().expect("$HOME not set"));

pub static CONFIGDIR: LazyLock<PathBuf> = LazyLock::new(|| match env::var("XDG_CONFIG_DIR") {
	Ok(d) => PathBuf::from(d),
	Err(_) => {
		let homedir = HOMEDIR.clone();
		homedir.join(".config/synd")
	}
});

#[derive(Debug)]
pub struct Synd {
	config: Config,
	sockpath: PathBuf,
	sock: UnixListener,
	last_fetch: Option<Instant>,
	followed: Db<FollowedEntry>,
	read: Db<ReadEntry>,
}

impl Drop for Synd {
	fn drop(&mut self) {
		if let Err(er) = fs::remove_file(&self.sockpath) {
			eprintln!("failed to remove socket file: {er:?}");
		} else {
			println!("removed con.sock");
		};
	}
}

type FollowId = uuid::Uuid;

#[derive(Serialize, Deserialize, Debug)]
struct FollowedEntry {
	uuid: FollowId,
	name: Option<String>,
	url: String,
}

#[derive(Serialize, Deserialize, Debug)]
enum ReadIdent {
	// AtomId(uuid::Uuid),
	AtomId(String),
	RssGuid(String),
	RssLink(String),
}

#[derive(Serialize, Deserialize, Debug)]
struct ReadEntry {
	follow_id: FollowId,
	read_id: ReadIdent,
	added_at: SysTime,
	// added_at: u64,
}

#[derive(Debug)]
struct Config {
	fetch_interval: Duration,
	action: Option<PathBuf>,
	removal_threshold: Option<Duration>,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			fetch_interval: Duration::from_mins(10),
			action: None,
			removal_threshold: None,
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
			"removal threshold" => {
				// 90 days
				const DEFAULT: u64 = 90 * 60 * 60 * 24;
				let thr = ass.as_bytes();
				let kar = thr[thr.len() - 1] as char;
				let is_in_seconds = kar == 's';
				let digits = if is_in_seconds {
					&ass[..ass.len() - 1]
				} else {
					ass
				};
				let in_seconds = match digits.parse::<u64>() {
					Ok(x) => {
						if is_in_seconds {
							x
						} else {
							x * 60 * 60 * 24
						}
					}
					Err(er) => {
						eprintln!(
							"parsing error: {er}, invalid numeric value in removal threshold, using default value"
						);
						DEFAULT
					}
				};

				self.removal_threshold = if in_seconds == 0 {
					None
				} else {
					Some(Duration::from_secs(in_seconds))
				};
			}
			_ => {
				eprintln!("invalid config variable \"{var}\"");
			}
		}
	}

	fn parse() -> anyhow::Result<Self> {
		let mut new = Self::default();
		let configpath = CONFIGDIR.clone().join("config");
		let mut contents = String::new();
		match File::open(&configpath) {
			Ok(mut file) => {
				file.read_to_string(&mut contents)
					.with_context(|| "while reading config")?;
				let lines = contents.lines();
				let valid = lines
					.filter_map(|line| {
						// ignore comments
						line.split("//").next().map(|s| s.trim())
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
		Ok(new)
	}
}

impl Synd {
	fn get_followed() -> anyhow::Result<Db<FollowedEntry>> {
		let new = Db::new("followed.db")?;
		println!("followed: {new:#?}");
		Ok(new)
	}

	fn get_read() -> anyhow::Result<Db<ReadEntry>> {
		let to_ser = ReadEntry {
			follow_id: uuid::Uuid::new_v4(),
			read_id: ReadIdent::AtomId(uuid::Uuid::new_v4().to_string()),
			added_at: SysTime(SystemTime::now()),
		};
		let x = serde_json::to_string_pretty(&to_ser).unwrap();
		println!("ser\n{x}");
		let new = Db::new("read.db")?;
		println!("read: {new:#?}");
		Ok(new)
	}

	fn get_sockpath() -> anyhow::Result<PathBuf> {
		let rtdir = PathBuf::from(
			std::env::var("XDG_RUNTIME_DIR")
				.with_context(|| "while checking runtime dir envvar ($XDG_RUNTIME_DIR not set?)")?,
		);
		let sockdir = rtdir.join("synd");
		fs::create_dir_all(&sockdir).with_context(|| "while creating sock dir")?;

		Ok(sockdir.join("con.sock"))
	}

	fn get_sock(sockpath: &PathBuf) -> anyhow::Result<UnixListener> {
		let sock = UnixListener::bind(sockpath).with_context(|| "while binding to socket")?;
		sock.set_nonblocking(true)
			.with_context(|| "while setting nonblocking socket")?;

		Ok(sock)
	}

	pub fn new() -> anyhow::Result<Self> {
		let sockpath = Self::get_sockpath().with_context(|| "while getting sock path")?;
		let config = Config::parse()?;
		println!("{config:#?}");
		let new = Self {
			config,
			last_fetch: None,
			followed: Self::get_followed()?,
			read: Self::get_read()?,
			sock: Self::get_sock(&sockpath)?,
			sockpath,
		};
		println!("{new:#?}");

		Ok(new)
	}

	fn handle_streams(&mut self) -> anyhow::Result<()> {
		for stream in self.sock.incoming() {
			match stream {
				Ok(mut stream) => {
					let nstream = stream.try_clone()?;
					let mut reader = BufReader::new(nstream);
					let mut recv = String::new();
					reader.read_line(&mut recv)?;
					recv.truncate(recv.len().saturating_sub(1));
					let recv = String::leak(recv);
					match serde_json::from_str::<SocketQuery>(recv) {
						Ok(eft) => match eft {
							SocketQuery::FollowDb(fdc) => match fdc {
								FollowDbCommand::Insert { name, url } => {
									let entry = FollowedEntry {
										uuid: uuid::Uuid::new_v4(),
										name,
										url,
									};
									self.followed.inner.push(entry);
									self.followed.write_to_file()?;
								}
								FollowDbCommand::Remove { uuid } => {
									let ix = self
										.followed
										.inner
										.iter()
										.enumerate()
										.find(|(_, e)| e.uuid == uuid)
										.map(|(ix, _)| ix);
									if let Some(ix) = ix {
										self.followed.inner.swap_remove(ix);
										self.followed.write_to_file()?;
									} else {
										stream.write_all(
											SyndError::InvalidParameter
												.to_ser_response()?
												.as_bytes(),
										)?;
									}
								}
							},
							SocketQuery::Feeds(fc) => match fc {
								FeedsCommand::Get { uuid } => {
									// todo DRY this
									//
									//
									let ix = self
										.followed
										.inner
										.iter()
										.enumerate()
										.find(|(_, e)| e.uuid == uuid)
										.map(|(ix, _)| ix);
									if let Some(ix) = ix {
										stream.write_all(
											SocketResponse {
												status: SocketResponseStatus::Good,
												inner: Some(&self.followed.inner[ix]),
											}
											.to_ser_response()?
											.as_bytes(),
										)?;
									} else {
										stream.write_all(
											SyndError::InvalidParameter
												.to_ser_response()?
												.as_bytes(),
										)?;
									}
								}
								FeedsCommand::List => {
									let x = serde_json::to_string(&self.followed.inner)?;
									// println!("WRITING\n\n\n\n\n{x}");
									stream.write_all(
										SocketResponse {
											status: SocketResponseStatus::Good,
											inner: Some(&x),
										}
										.to_ser_response()?
										.as_bytes(),
									)?;
								}
							},
							SocketQuery::MainLoop(mlc) => match mlc {
								MainLoopCommand::GetTimeUntilNextFetch => {
									// this is probably fragile
									let duration = self.config.fetch_interval
										- self.last_fetch.unwrap().elapsed();
									stream.write_all(
										SocketResponse {
											status: SocketResponseStatus::Good,
											inner: Some(&duration.as_secs()),
										}
										.to_ser_response()?
										.as_bytes(),
									)?;
								}
								MainLoopCommand::ForceFetch => {
									self.last_fetch = None;
									stream.write_all(
										SocketResponse::<()> {
											status: SocketResponseStatus::Good,
											inner: None,
										}
										.to_ser_response()?
										.as_bytes(),
									)?;
								}
							},
						},
						Err(er) => stream.write_all(er.to_ser_response()?.as_bytes())?,
					};
					stream.flush()?;
				}
				Err(er) => match er.kind() {
					std::io::ErrorKind::WouldBlock => {
						break;
					}
					_ => return Err(er.into()),
				},
			}
		}
		Ok(())
	}

	fn fetch_feeds(&mut self) -> anyhow::Result<()> {
		if let Some(last_fetch) = self.last_fetch
			&& last_fetch.elapsed() < self.config.fetch_interval
		{
			return Ok(());
		}
		self.last_fetch = Some(Instant::now());
		for followed in &self.followed.inner {
			let res = match minreq::get(&followed.url).send() {
				Ok(r) => r,
				Err(er) => {
					eprintln!("=er= failed to GET feed under {} =er=\n{er}", followed.url);
					continue;
				}
			};
			let res = res.as_str();
			let feed = match res {
				Ok(r) => r,
				Err(er) => {
					eprintln!(
						"=er= failed to decode feed response to a string under {} =er=\n{er}",
						followed.url
					);
					continue;
				}
			};
			let atom = atom_syndication::Feed::read_from(BufReader::new(Cursor::new(feed)));
			println!("{atom:#?}");
		}
		println!("==== fetching feeds ====");
		Ok(())
	}

	fn alarm(&mut self) -> anyhow::Result<()> {
		println!("{:#?}", self.read);
		Ok(())
	}

	pub fn work(&mut self) -> anyhow::Result<()> {
		self.handle_streams()?;
		self.fetch_feeds()?;
		self.alarm()?;
		Ok(())
	}
}
