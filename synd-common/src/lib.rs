use core::fmt;
use std::{collections::HashMap, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

pub use serde_json;

use crate::systime::SysTime;

pub mod systime;

#[derive(Serialize, Deserialize, Debug)]
pub enum SyndError {
	Uuid(String),
	Generic(String),
	InvalidParameter,
}

impl From<uuid::Error> for SyndError {
	fn from(er: uuid::Error) -> Self {
		SyndError::Uuid(er.to_string())
	}
}

pub type FollowId = uuid::Uuid;

#[derive(Debug, PartialEq, Clone, Hash, Eq)]
pub enum EntryIdent {
	// AtomId(uuid::Uuid),
	AtomId(String),
	RssGuid(String),
	RssLink(String),
}

impl FromStr for EntryIdent {
	type Err = &'static str;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut split = s.splitn(2, '(');
		let ty = split.next().ok_or("invalid value")?;
		let rest = split.next().ok_or("invalid value")?;
		// remove ')'
		let rest = &rest[..rest.len() - 1];
		match ty {
			"AtomId" => Ok(Self::AtomId(rest.to_string())),
			"RssGuid" => Ok(Self::RssGuid(rest.to_string())),
			"RssLink" => Ok(Self::RssLink(rest.to_string())),
			_ => Err("invalid enum kind"),
		}
	}
}

impl fmt::Display for EntryIdent {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::AtomId(v) => write!(f, "AtomId({v})"),
			Self::RssGuid(v) => write!(f, "RssGuid({v})"),
			Self::RssLink(v) => write!(f, "RssLink({v})"),
		}
	}
}

impl Serialize for EntryIdent {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&format!("{}", &self))
	}
}

impl<'de> Deserialize<'de> for EntryIdent {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		match s.parse::<Self>() {
			Ok(s) => Ok(s),
			Err(er) => Err(Error::custom(er)),
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
	Ack,
	NewId(uuid::Uuid),
	Bad(SyndError),
	FollowDbEntry(FollowedEntry),
	FollowDbList(HashMap<FollowId, FollowedEntry>),
	ReadDbList(HashMap<EntryIdent, ReadEntry>),
	TimeUntilFetch(u64),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FollowedEntry {
	pub name: Option<String>,
	pub url: String,
	pub read_from: SysTime,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ReadEntry {
	pub followed_id: uuid::Uuid,
	pub added_at: SysTime,
}

// (namespaces)
#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQuery {
	Feeds(FeedsCommand),
	Reads(ReadsCommand),
	MainLoop(MainLoopCommand),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum FeedsCommand {
	Follow {
		name: Option<String>,
		url: String,
	},
	Unfollow {
		id: uuid::Uuid,
	},
	Update {
		id_to_update: uuid::Uuid,
		name: Option<String>,
		url: Option<String>,
		read_from: Option<SysTime>,
	},
	List,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ReadsCommand {
	MarkRead { id: EntryIdent },
	MarkUnread { id: EntryIdent },
	ListAll,
	ListFromFeed { followed_id: uuid::Uuid },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum MainLoopCommand {
	GetTimeUntilNextFetch,
	ForceFetch,
}
