use std::{io::Write, os::unix::net::UnixStream, path::PathBuf};

use anyhow::Context;
use bpaf::{OptionParser, Parser, construct, long};

use synd::ToSerializedResponse;
use synd_common as synd;

fn options() -> OptionParser<synd::SocketQuery> {
	let follow_db = {
		let insert = {
			let name = long("name")
				.short('n')
				.help("name of the feed to follow")
				.argument("NAME")
				.optional();
			let url = long("url").short('u').help("url to follow").argument("URL");

			construct!(synd::FollowDbCommand::Insert { name, url })
				.to_options()
				.command("insert")
		};

		let remove = {
			let id = long("id")
				.short('i')
				.help("uuid of the feed to remove")
				.argument("ID");

			construct!(synd::FollowDbCommand::Remove { id })
				.to_options()
				.command("remove")
		};

		construct!([insert, remove])
			.to_options()
			.command("followdb")
			.map(synd::SocketQuery::FollowDb)
	};

	let feeds = {
		let get = {
			let id = long("id")
				.short('i')
				.help("uuid of the followed feed")
				.argument("ID");
			construct!(synd::FeedsCommand::Get { id })
				.to_options()
				.command("get")
		};

		let list = construct!(synd::FeedsCommand::List {})
			.to_options()
			.command("list");
		construct!([get, list])
			.to_options()
			.command("feeds")
			.map(synd::SocketQuery::Feeds)
	};

	let mainloop = {
		let get_time = construct!(synd::MainLoopCommand::GetTimeUntilNextFetch {})
			.to_options()
			.command("time-until-fetch");
		let force = construct!(synd::MainLoopCommand::ForceFetch {})
			.to_options()
			.command("force-fetch");

		construct!([get_time, force])
			.to_options()
			.command("mainloop")
			.map(synd::SocketQuery::MainLoop)
	};

	construct!([follow_db, feeds, mainloop]).to_options()
}

fn main() -> anyhow::Result<()> {
	let options = options().run();

	let rtdir = PathBuf::from(
		std::env::var("XDG_RUNTIME_DIR")
			.with_context(|| "while checking runtime dir envvar ($XDG_RUNTIME_DIR not set?)")?,
	)
	.join("synd")
	.join("con.sock");
	let mut sock = UnixStream::connect(&rtdir)
		.with_context(|| "while trying to connect to the socket (synd not running?)")?;

	let query = options
		.to_ser_response()
		.with_context(|| "while serializing query")?;

	println!("serialized\n\n{query}");

	sock.write_all(query.as_bytes())
		.with_context(|| "while sending message")
}
