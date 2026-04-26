use std::{
	sync::atomic::{AtomicBool, Ordering},
	time::Duration,
};

use crate::synd::Synd;

mod synd;

static KILLED: AtomicBool = AtomicBool::new(false);

fn sig_handler() {
	KILLED.store(true, Ordering::Relaxed);
	// println!("killed correctly");
}

fn main() -> anyhow::Result<()> {
	let ptr = sig_handler as *const ();
	unsafe {
		let fun: extern "C" fn() = std::mem::transmute(ptr);
		libc::signal(libc::SIGINT, fun as usize);
		libc::signal(libc::SIGKILL, fun as usize);
	}

	let mut synd = Synd::new()?;
	while !KILLED.load(Ordering::Relaxed) {
		synd.work()?;
		std::thread::sleep(Duration::from_millis(30));
	}
	Ok(())
}
