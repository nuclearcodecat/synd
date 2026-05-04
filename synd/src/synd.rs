// #![allow(unused)]

// todos
//  - search for todo in %
//
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
	env,
	fs::{self, File},
	io::{BufRead, BufReader, Cursor, Read, Write},
	os::unix::net::{UnixListener, UnixStream},
	path::PathBuf,
	process::{Command, Stdio},
	sync::LazyLock,
	time::{Duration, Instant, UNIX_EPOCH},
};
use synd_common::{
	EntryIdent, FeedsCommand, FollowId, FollowedEntry, MainLoopCommand, ReadEntry, Response,
	SocketQuery, SyndError, systime::SysTime,
};

use crate::db::Db;

pub static HOMEDIR: LazyLock<PathBuf> = LazyLock::new(|| env::home_dir().expect("$HOME not set"));

pub static CONFIGDIR: LazyLock<PathBuf> = LazyLock::new(|| match env::var("XDG_CONFIG_DIR") {
	Ok(d) => PathBuf::from(d).join("synd"),
	Err(_) => {
		let homedir = HOMEDIR.clone();
		homedir.join(".config/synd")
	}
});

pub static DATADIR: LazyLock<PathBuf> = LazyLock::new(|| match env::var("XDG_DATA_HOME") {
	Ok(d) => PathBuf::from(d).join("synd"),
	Err(_) => {
		let homedir = HOMEDIR.clone();
		homedir.join(".local/share/synd")
	}
});

#[derive(Debug)]
pub struct Synd {
	config: Config,
	sockpath: PathBuf,
	sock: UnixListener,
	last_fetch: Option<Instant>,
	followed: Db<FollowId, FollowedEntry>,
	read: Db<EntryIdent, ReadEntry>,
	actioned: Db<EntryIdent, ActionedEntry>,
	feeds: Vec<SendEntry>,
}

impl Drop for Synd {
	fn drop(&mut self) {
		if let Err(er) = fs::remove_file(&self.sockpath) {
			eprintln!("=er= failed to remove socket file: {er:?} =er=");
		} else {
			println!("==== removed con.sock ====");
		};
	}
}

#[derive(Serialize, Deserialize, Debug)]
struct ActionedEntry {
	follow_id: FollowId,
	added_at: SysTime,
}

#[derive(Debug, Serialize)]
struct SendEntry {
	ident: EntryIdent,
	title: String,
	url: String,
	summary: Option<String>,
	pub_time: Option<atom_syndication::FixedDateTime>,
	followed_id: FollowId,
}

#[derive(Debug)]
struct Config {
	fetch_interval: Duration,
	action: Option<PathBuf>,
	removal_threshold: Option<Duration>,
	max_summary_length: usize,
	max_inferred_summary_length: usize,
}

impl Default for Config {
	fn default() -> Self {
		Self {
			fetch_interval: Duration::from_mins(10),
			action: None,
			removal_threshold: Some(Duration::from_days(90)),
			max_summary_length: 240,
			max_inferred_summary_length: 240,
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
							"=er= parsing error: {er}, invalid assignment \"{ass}\" for \"{var}\", using default value =er="
						);
						return;
					}
				};
				self.fetch_interval = Duration::from_secs(parsed);
			}
			"action" => {
				// todo | expect to be in configdir for now
				// todo maybe check whether file exists and is executable
				let filetail = ass;
				let mut configdir = CONFIGDIR.clone();
				configdir = configdir.join(filetail);
				self.action = Some(configdir);
			}
			// todo currently unimplemented and ambiguous whether it should work for actioned, read, or both
			"removal threshold" => {
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
							"=er= parsing error: {er}, invalid numeric value in {var}, using default value =er="
						);
						return;
					}
				};

				self.removal_threshold = if in_seconds == 0 {
					None
				} else {
					Some(Duration::from_secs(in_seconds))
				};
			}
			"max inferred summary length" => match ass.parse::<usize>() {
				Ok(len) => {
					self.max_inferred_summary_length = len;
				}
				Err(er) => {
					eprintln!(
						"=er= parsing error: {er}, invalid numeric value in {var}, using default value =er="
					);
				}
			},
			"max summary length" => match ass.parse::<usize>() {
				Ok(len) => {
					self.max_summary_length = len;
				}
				Err(er) => {
					eprintln!(
						"=er= parsing error: {er}, invalid numeric value in {var}, using default value =er="
					);
				}
			},
			_ => {
				eprintln!("=er= invalid config variable \"{var}\" =er=");
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
				eprintln!("=er= config file missing or unavailable ({er}). using defaults. =er=");
			}
		}
		Ok(new)
	}
}

impl Synd {
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
		let new = Self {
			config: Config::parse().with_context(|| "while getting config")?,
			last_fetch: None,
			followed: Db::new("followed.db")?,
			read: Db::new("read.db")?,
			actioned: Db::new("actioned.db")?,
			sock: Self::get_sock(&sockpath)?,
			sockpath,
			feeds: Vec::new(),
		};
		println!("{new:#?}");

		Ok(new)
	}

	fn write_to_stream(stream: &mut UnixStream, inner: Response) -> anyhow::Result<()> {
		stream
			.write_all(
				serde_json::to_string(&inner)
					.with_context(|| "while serializing response")?
					.as_bytes(),
			)
			.with_context(|| "while writing response to sock")?;
		Ok(())
	}

	fn handle_streams(&mut self) -> anyhow::Result<()> {
		for stream in self.sock.incoming() {
			match stream {
				Ok(mut stream) => {
					let nstream = stream.try_clone().with_context(|| "while cloning stream")?;
					let mut reader = BufReader::new(nstream);
					let mut recv = String::new();
					reader.read_line(&mut recv)?;
					match serde_json::from_str::<SocketQuery>(&recv) {
						Ok(query) => {
							println!("{query:#?}");
							match query {
								SocketQuery::Feeds(cmd) => match cmd {
									FeedsCommand::Follow { name, url } => {
										let id = uuid::Uuid::new_v4();
										let entry = FollowedEntry {
											name,
											url,
											read_from: SysTime::now(),
										};
										self.followed.inner.insert(id, entry);
										self.followed.write_to_file()?;
										Self::write_to_stream(&mut stream, Response::NewId(id))?;
									}
									FeedsCommand::Unfollow { id } => {
										if self.followed.inner.remove(&id).is_some() {
											self.followed.write_to_file()?;
											Self::write_to_stream(&mut stream, Response::Ack)?;
										} else {
											Self::write_to_stream(
												&mut stream,
												Response::Bad(SyndError::InvalidParameter),
											)?;
										}
									}
									FeedsCommand::List => {
										Self::write_to_stream(
											&mut stream,
											Response::FollowDbList(self.followed.inner.clone()),
										)?;
									}
									FeedsCommand::Update {
										id_to_update,
										name,
										url,
										read_from,
									} => {
										match self.followed.inner.get_mut(&id_to_update) {
											Some(e) => {
												if let Some(name) = name {
													e.name = if name.is_empty() {
														None
													} else {
														Some(name)
													};
												};
												if let Some(url) = url {
													e.url = url;
												};
												if let Some(read_from) = read_from {
													e.read_from = read_from;
												};
												Self::write_to_stream(&mut stream, Response::Ack)?;
											}
											None => Self::write_to_stream(
												&mut stream,
												Response::Bad(SyndError::InvalidParameter),
											)?,
										};
									}
								},
								SocketQuery::MainLoop(cmd) => match cmd {
									MainLoopCommand::GetTimeUntilNextFetch => {
										// this is probably fragile
										let duration = self.config.fetch_interval
											- self.last_fetch.unwrap().elapsed();
										Self::write_to_stream(
											&mut stream,
											Response::TimeUntilFetch(duration.as_secs()),
										)?;
									}
									MainLoopCommand::ForceFetch => {
										self.last_fetch = None;
										Self::write_to_stream(&mut stream, Response::Ack)?;
									}
								},
								SocketQuery::Reads(cmd) => match cmd {
									synd_common::ReadsCommand::MarkRead { id } => {
										match self.feeds.iter().find(|e| e.ident == id) {
											Some(e) => {
												let fid = e.followed_id;
												self.read.inner.insert(
													id,
													ReadEntry {
														followed_id: fid,
														added_at: SysTime::now(),
													},
												);
												Self::write_to_stream(&mut stream, Response::Ack)?;
											}
											None => {
												Self::write_to_stream(
													&mut stream,
													Response::Bad(SyndError::InvalidParameter),
												)?;
											}
										};
									}
									synd_common::ReadsCommand::MarkUnread { id } => {
										if self.read.inner.remove(&id).is_some() {
											self.read.write_to_file()?;
											Self::write_to_stream(&mut stream, Response::Ack)?;
										} else {
											Self::write_to_stream(
												&mut stream,
												Response::Bad(SyndError::InvalidParameter),
											)?;
										}
									}
									synd_common::ReadsCommand::ListAll => Self::write_to_stream(
										&mut stream,
										Response::ReadDbList(self.read.inner.clone()),
									)?,
									synd_common::ReadsCommand::ListFromFeed { followed_id } => {
										let mut map = self.read.inner.clone();
										map.retain(|_, e| e.followed_id == followed_id);
										Self::write_to_stream(
											&mut stream,
											Response::ReadDbList(map),
										)?
									}
								},
							}
						}
						Err(er) => Self::write_to_stream(
							&mut stream,
							Response::Bad(SyndError::Generic(er.to_string())),
						)?,
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

	fn action(&self, send_entry: &SendEntry) -> anyhow::Result<()> {
		let act = &self.config.action;
		if let Some(act) = act {
			let cmd = Command::new(act)
				.stdin(Stdio::piped())
				.spawn()
				.with_context(|| "while spawning action process")?;
			let serialized = serde_json::to_string(send_entry)
				.with_context(|| "while serializing SendEntry for stdin")?;
			cmd.stdin
				.with_context(|| "while taking stdin from actioned process")?
				.write_all(serialized.as_bytes())
				.with_context(|| "while writing data to child stdin")?;
		} else {
			println!("no action specified");
		}
		Ok(())
	}

	fn check_feeds(&mut self) -> anyhow::Result<()> {
		if let Some(last_fetch) = self.last_fetch
			&& last_fetch.elapsed() < self.config.fetch_interval
		{
			return Ok(());
		}
		// println!("==== fetching feeds ====");
		self.last_fetch = Some(Instant::now());

		for (followed_id, followed) in &self.followed.inner {
			// println!("==== iter ====");
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
			let mut items = Vec::new();
			if let Ok(feed) = atom_syndication::Feed::read_from(BufReader::new(Cursor::new(feed))) {
				let entries = feed
					.entries()
					.iter()
					.filter_map(|e| {
						let ident = EntryIdent::AtomId(e.id().to_owned());
						let title = e.title.to_string();
						match e.links.first().map(|l| l.href().to_owned()) {
							Some(url) => Some(SendEntry {
								followed_id: *followed_id,
								ident,
								title,
								url,
								summary: e.summary().map(|s| s.to_string().clone()),
								pub_time: e.published().copied(),
							}),
							None => {
								// link is required according to wikipedia
								eprintln!(
									"=er= link missing in entry titled \"{title}\" (from feed under \"{}\"), skipping =er=",
									followed.url
								);
								None
							}
						}
					})
					.collect::<Vec<_>>();
				items.extend(entries);
				// println!("=============== ATOM ENTRIES ===============\n\n\n{entries:#?}");
			} else if let Ok(feed) = rss::Channel::read_from(BufReader::new(Cursor::new(feed))) {
				let entries = feed
					.items()
					.iter()
					.filter_map(|i| {
						let ident = if let Some(guid) = i.guid() {
							EntryIdent::RssGuid(guid.value().to_owned())
						} else if let Some(link) = i.link() {
							EntryIdent::RssLink(link.to_owned())
						} else {
							eprintln!(
								"=er= unidentifiable item in rss feed under {} =er=",
								followed.url
							);
							return None;
						};
						// title is defined as mandatory by wikipedia but i'll include a sentinel
						let title = i.title().unwrap_or("[no title]").to_owned();
						// link is specified as mandatory by wikipedia
						let url = match i.link() {
							Some(u) => u.to_owned(),
							None => return None,
						};
						// also defined as mandatory
						let summary = if let Some(desc) = i.description() {
							if desc.len() > self.config.max_summary_length {
								Some(desc[..self.config.max_summary_length].to_owned())
							} else {
								Some(desc.to_owned())
							}
						} else if let Some(cont) = i.content() {
							if cont.len() > self.config.max_inferred_summary_length {
								let mut string =
									cont[..self.config.max_inferred_summary_length].to_owned();
								string.push_str("...");
								Some(string)
							} else {
								Some(cont.to_owned())
							}
						} else {
							None
						};
						let pub_time = i.pub_date().and_then(|d| {
							Some(
								match atom_syndication::FixedDateTime::parse_from_rfc2822(d) {
									Ok(d) => d,
									Err(er) => {
										eprintln!("=er= failed to parse rss pub date ({er}) =er=");
										return None;
									}
								},
							)
						});
						// println!("pub date for '{title}': {pub_time:?}, {:?}", i.pub_date());
						Some(SendEntry {
							followed_id: *followed_id,
							ident,
							title,
							url,
							summary,
							pub_time,
						})
					})
					.collect::<Vec<_>>();
				// println!("e {entries:#?}");
				items.extend(entries);
				// println!("=============== RSS ENTRIES ===============\n\n\n{entries:#?}");
			} else {
				eprintln!("=er= invalid feed under {}", followed.url);
			};
			// println!("{items:#?}");

			let read_from_dur = followed.read_from.duration_since(UNIX_EPOCH)?.as_secs();

			self.feeds = items;
			for entry in &self.feeds {
				let is_in_actioned = self.actioned.inner.iter().any(|(k, _)| k == &entry.ident);
				if is_in_actioned {
					// println!("entry {entry:?} was already actioned");
					continue;
				}
				let is_old = match entry.pub_time {
					Some(ts) => (ts.timestamp() as u64) < read_from_dur,
					None => true,
				};
				if is_old {
					// println!("entry {entry:?} is old");
				} else {
					// println!("entry {entry:?} should get actioned and added to the db");
					self.action(entry).with_context(|| "while actioning")?;
					self.actioned.inner.insert(
						entry.ident.clone(),
						ActionedEntry {
							follow_id: *followed_id,
							added_at: SysTime::now(),
						},
					);
					self.actioned.write_to_file()?;
				}
			}
		}
		// println!("{:#?}", self.actioned.inner);
		// Err(anyhow::Error::msg("intentional"))
		Ok(())
	}

	pub fn work(&mut self) -> anyhow::Result<()> {
		self.handle_streams()
			.with_context(|| "while handling streams")?;
		self.check_feeds().with_context(|| "while checking feeds")?;
		Ok(())
	}
}
