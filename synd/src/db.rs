use std::{
	fs::{File, OpenOptions},
	io::{Seek, Write},
	sync::LazyLock,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::synd::CONFIGDIR;

#[derive(Debug)]
pub struct Db<T: for<'a> Deserialize<'a> + Serialize> {
	pub inner: Vec<T>,
	pub file: File,
}

static OPENOPT: LazyLock<OpenOptions> = LazyLock::new(|| {
	let mut oo = OpenOptions::new();
	oo.write(true).read(true).create(true);
	oo
});

impl<T: Serialize + for<'a> Deserialize<'a>> Db<T> {
	pub fn new(fname: &'static str) -> anyhow::Result<Self> {
		let fp = CONFIGDIR.clone().join(fname);
		// println!("{fp:?}");
		let file = OPENOPT
			.open(fp)
			.with_context(|| "while opening followed.db file")?;
		Ok(Self {
			inner: serde_json::from_reader(file.try_clone()?)?,
			file,
		})
	}

	pub fn write_to_file(&mut self) -> anyhow::Result<()> {
		let ser =
			serde_json::to_string(&self.inner).with_context(|| "while serializing db file")?;
		self.file.rewind().with_context(|| "while rewinding Seek")?;
		self.file
			.set_len(0)
			.with_context(|| "while truncating file")?;
		self.file
			.write(ser.as_bytes())
			.with_context(|| "while writing to db file")?;
		Ok(())
	}
}
