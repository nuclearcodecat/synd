use std::fmt::Display;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub enum ParseError {
	InvalidNamespace,
	InvalidCommand,
	MissingArgument(&'static str),
	General,
	Uuid(String),
}

impl From<uuid::Error> for ParseError {
	fn from(er: uuid::Error) -> Self {
		ParseError::Uuid(er.to_string())
	}
}

// from rssd, might be useless
impl Display for ParseError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ParseError::MissingArgument(arg) => write!(f, "missing {arg} argument"),
			ParseError::InvalidCommand => write!(f, "invalid command"),
			ParseError::InvalidNamespace => write!(f, "invalid namespace"),
			ParseError::General => write!(f, "message mangled"),
			ParseError::Uuid(er) => write!(f, "{er}"),
		}
	}
}

impl ParseError {
	pub fn to_socket_response(&self) -> serde_json::Result<String> {
		serde_json::to_string(&self)
	}
}

#[derive(Deserialize)]
pub enum SocketInput {
	FollowDb(FollowDbCommand),
	Feeds(FeedsCommand),
	MainLoop(MainLoopCommand),
}

#[derive(Deserialize)]
pub enum FollowDbCommand {
	Insert { name: String, url: String },
	Remove { uuid: uuid::Uuid },
}

#[derive(Deserialize)]
pub enum FeedsCommand {
	Get { uuid: uuid::Uuid },
	List,
}

#[derive(Deserialize)]
pub enum MainLoopCommand {
	GetTimeUntilNextFetch,
	ForceFetch,
}
