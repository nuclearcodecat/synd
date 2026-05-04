use std::{
	io::{Read, Write},
	os::unix::net::UnixStream,
	path::PathBuf,
};

use anyhow::Context;
use bpaf::{OptionParser, Parser, construct, long};

use synd_common::{self as synd, Response};

fn options() -> OptionParser<synd::SocketQuery> {
	let feeds = {
		let follow = {
			let name = long("name")
				.short('n')
				.help("name of the feed to follow")
				.argument("NAME")
				.optional();
			let url = long("url").short('u').help("url to follow").argument("URL");

			construct!(synd::FeedsCommand::Follow { name, url })
				.to_options()
				.command("follow")
		};

		let unfollow = {
			let id = long("id")
				.short('i')
				.help("uuid of the feed to unfollow")
				.argument("ID");

			construct!(synd::FeedsCommand::Unfollow { id })
				.to_options()
				.command("unfollow")
		};

		let update = {
			let id_to_update = long("id")
				.short('i')
				.help("uuid of the feed to update")
				.argument("ID");
			let name = long("name")
				.short('n')
				.help("new name (send empty string for no name)")
				.argument("NAME")
				.optional();
			let url = long("url")
				.short('u')
				.help("new url")
				.argument("URL")
				.optional();
			let read_from = long("read-from")
				.short('u')
				.help("new read from marker (from what point to ignore unread entries)")
				.argument("READ_FROM")
				.optional();
			construct!(synd::FeedsCommand::Update {
				id_to_update,
				name,
				url,
				read_from,
			})
			.to_options()
			.command("update")
		};

		let list = construct!(synd::FeedsCommand::List {})
			.to_options()
			.command("list");
		construct!([follow, unfollow, update, list])
			.to_options()
			.command("feeds")
			.map(synd::SocketQuery::Feeds)
	};

	let reads = {
		let mark_read = {
			let id = long("id")
				.short('i')
				.help("guid of the article to mark as read. use the same id as given by List")
				.argument("ID");

			construct!(synd::ReadsCommand::MarkRead { id })
				.to_options()
				.command("mark-read")
		};

		let mark_unread = {
			let id = long("id")
				.short('i')
				.help("guid of the article to mark as unread. use the same id as given by List")
				.argument("ID");

			construct!(synd::ReadsCommand::MarkUnread { id })
				.to_options()
				.command("mark-unread")
		};

		let list_all = construct!(synd::ReadsCommand::ListAll {})
			.to_options()
			.command("list");

		let list_from_feed = {
			let followed_id = long("id")
				.short('i')
				.help("id of the feed to list read articles from")
				.argument("ID");

			construct!(synd::ReadsCommand::ListFromFeed { followed_id })
				.to_options()
				.command("list-from-feed")
		};

		construct!([mark_read, mark_unread, list_all, list_from_feed])
			.to_options()
			.command("reads")
			.map(synd::SocketQuery::Reads)
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

	construct!([feeds, reads, mainloop]).to_options()
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

	let mut query =
		synd::serde_json::to_string(&options).with_context(|| "while serializing query")?;
	query.push('\n');

	sock.write_all(query.as_bytes())
		.with_context(|| "while sending message")?;

	let mut buf = String::new();
	sock.read_to_string(&mut buf)
		.with_context(|| "while reading response")?;

	let serialized = synd::serde_json::from_str::<Response>(&buf)?;
	println!("{serialized:#?}");

	// println!("{buf}");
	Ok(())
}
