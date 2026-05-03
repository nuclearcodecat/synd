use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum SyndError {
	InvalidParameter,
	Uuid(String),
}

impl From<uuid::Error> for SyndError {
	fn from(er: uuid::Error) -> Self {
		SyndError::Uuid(er.to_string())
	}
}

#[derive(Deserialize, Serialize)]
pub enum SocketResponseStatus {
	Good,
	Bad,
}

#[derive(Serialize)]
pub struct SocketResponse<T: Serialize> {
	pub status: SocketResponseStatus,
	pub inner: Option<T>,
}

pub trait ToSerializedResponse {
	fn to_ser_response(self) -> serde_json::Result<String>;
}

impl ToSerializedResponse for serde_json::Error {
	fn to_ser_response(self) -> serde_json::Result<String> {
		let res = SocketResponse {
			status: SocketResponseStatus::Bad,
			inner: Some(self.to_string()),
		};
		serde_json::to_string(&res)
	}
}

impl ToSerializedResponse for SyndError {
	fn to_ser_response(self) -> serde_json::Result<String> {
		let res = SocketResponse {
			status: SocketResponseStatus::Bad,
			inner: Some(format!("{self:#?}")),
		};
		serde_json::to_string(&res)
	}
}

impl<T: Serialize> ToSerializedResponse for SocketResponse<T> {
	fn to_ser_response(self) -> serde_json::Result<String> {
		serde_json::to_string(&self)
	}
}

// (namespaces)
#[derive(Serialize, Deserialize)]
pub enum SocketQuery {
	FollowDb(FollowDbCommand),
	Feeds(FeedsCommand),
	MainLoop(MainLoopCommand),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum FollowDbCommand {
	Insert { name: Option<String>, url: String },
	Remove { id: uuid::Uuid },
}

#[derive(Serialize, Deserialize, Clone)]
pub enum FeedsCommand {
	Get { id: uuid::Uuid },
	List,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum MainLoopCommand {
	GetTimeUntilNextFetch,
	ForceFetch,
}

impl ToSerializedResponse for SocketQuery {
	fn to_ser_response(self) -> serde_json::Result<String> {
		serde_json::to_string(&self)
	}
}
