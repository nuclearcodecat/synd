// MODIFIED SERIALIZER FOR SYSTEMTIME FROM SERDE'S CODE; WITHOUT SUBNANOS

use std::{
	ops::Deref,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error, ser::Error as _};

#[derive(Debug)]
pub struct SysTime(pub SystemTime);

impl Deref for SysTime {
	type Target = SystemTime;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl Serialize for SysTime {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		// use serde::ser::SerializeStruct;
		let duration_since_epoch = match self.0.duration_since(UNIX_EPOCH) {
			Ok(duration_since_epoch) => duration_since_epoch.as_secs(),
			Err(_) => return Err(S::Error::custom("SystemTime must be later than UNIX_EPOCH")),
		};
		serializer.serialize_u64(duration_since_epoch)
	}
}

impl<'de> Deserialize<'de> for SysTime {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: Deserializer<'de>,
	{
		let secs = u64::deserialize(deserializer)?;
		UNIX_EPOCH
			.checked_add(Duration::from_secs(secs))
			.ok_or_else(|| D::Error::custom("overflow deserializing SystemTime"))
			.map(SysTime)
	}
}
