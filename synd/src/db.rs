use std::{
	collections::HashMap,
	fs::{self, File, OpenOptions},
	hash::Hash,
	io::{Seek, Write},
	sync::LazyLock,
	time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::synd::DATADIR;

pub trait SerdeK = for<'a> Deserialize<'a> + Serialize + Hash + Eq;
pub trait SerdeV = for<'a> Deserialize<'a> + Serialize;

#[derive(Debug)]
pub struct Db<K: SerdeK, V: SerdeV> {
	pub inner: HashMap<K, V>,
	pub file: File,
	pub name: &'static str,
}

static OPENOPT: LazyLock<OpenOptions> = LazyLock::new(|| {
	let mut oo = OpenOptions::new();
	oo.write(true).read(true).create(true);
	oo
});

impl<K: SerdeK, V: SerdeV> Db<K, V> {
	pub fn new(fname: &'static str) -> anyhow::Result<Self> {
		let fp = DATADIR.clone();
		fs::create_dir_all(&fp).with_context(|| "while creating synd data dir")?;
		let fp = fp.join(fname);
		// println!("{fp:?}");
		let mut file = OPENOPT
			.open(&fp)
			.with_context(|| format!("while opening {fname} file ({fp:?}))"))?;
		let mut make_instantly = false;
		let inner = match serde_json::from_reader(file.try_clone()?) {
			Ok(i) => i,
			Err(er) => {
				println!("=wa= invalid or nonexistent {fname} ({er}) =wa=");
				if file
					.stream_len()
					.with_context(|| "while checking stream len")?
					> 0
				{
					let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
					let mut fp_bak = fp.clone();
					let mut bak = fp_bak
						.file_name()
						.context("while getting file name")?
						.to_string_lossy()
						.to_string();
					bak.push('-');
					bak.push_str(&now.to_string());
					fp_bak.set_file_name(&bak);
					fp_bak.set_extension("bak");
					println!("==== saving old contents to {fp_bak:?} file ====");
					fs::copy(&fp, &fp_bak).unwrap_or_else(|_| {
						panic!("=er= failed to write {fname} backup. exiting immediately =er=")
					});
				}
				make_instantly = true;
				HashMap::new()
			}
		};
		let mut new = Self {
			inner,
			file,
			name: fname,
		};
		if make_instantly {
			println!("==== creating new db file ====");
			new.write_to_file()
				.with_context(|| "while writing to db file")?;
		}
		Ok(new)
	}

	pub fn write_to_file(&mut self) -> anyhow::Result<()> {
		let fname = self.name;
		let ser = serde_json::to_string_pretty(&self.inner)
			.with_context(|| format!("while serializing {fname} file"))?;
		self.file
			.rewind()
			.with_context(|| format!("while rewinding Seek for {fname}"))?;
		self.file
			.set_len(0)
			.with_context(|| format!("while truncating {fname} file"))?;
		self.file
			.write(ser.as_bytes())
			.with_context(|| format!("while writing to {fname} file"))?;
		Ok(())
	}
}
