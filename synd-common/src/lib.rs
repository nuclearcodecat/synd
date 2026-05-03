use serde::{Deserialize, Serialize};

pub use serde_json;

use crate::systime::SysTime;

pub mod systime;

#[derive(Serialize, Deserialize, Debug)]
pub enum SyndError {
	InvalidParameter,
	Uuid(String),
	Generic(String),
	InvalidId,
	InvalidSysTime,
}

impl From<uuid::Error> for SyndError {
	fn from(er: uuid::Error) -> Self {
		SyndError::Uuid(er.to_string())
	}
}

pub type FollowId = uuid::Uuid;

#[derive(Serialize, Deserialize, Debug)]
pub enum Response {
	Ack,
	NewId(uuid::Uuid),
	Bad(SyndError),
	FollowDbEntry(FollowedEntry),
	FollowDbList(Vec<FollowedEntry>),
	TimeUntilFetch(u64),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FollowedEntry {
	pub id: FollowId,
	pub name: Option<String>,
	pub url: String,
	pub read_from: SysTime,
}

// (namespaces)
#[derive(Serialize, Deserialize, Debug)]
pub enum SocketQuery {
	Feeds(FeedsCommand),
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
pub enum MainLoopCommand {
	GetTimeUntilNextFetch,
	ForceFetch,
}
