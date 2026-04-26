use crate::synd::Synd;

mod synd;

fn main() -> anyhow::Result<()> {
	let mut synd = Synd::new()?;
	loop {
		synd.work()?
	}
}
