# atom syndication / rss demon
the purpose of this demon is to accumulate rss and atom feeds, monitor them, announce new content to the user, and allow the user to mark entries as read and unread.  
currently highly unstable

## structure
this repo contains _synd_ – the syndication demon, _synctl_ – the syndication demon controller, and _synd-common_, the common library containing important types.  

## configfile
the configfile should be available to synd at `~/.config/synd/config`.  
### format:
key-value pairs separated by newline. the lines are split with " = ". comments are specified by "//".  
`key value = assignee   // very cute comment OwO`

### all keys along with their default values:
 - fetch interval: 600 – how often the followed feeds should be fetched for checking; in seconds
 - action: None – file to execute if a new item is available. currently synd only searches for the specified file in synd's config directory
 - removal threshold: 90 – normally expressed in days, append 's' to the end to specify in seconds. specify 0 to disable. currently does nothing. name will probably change. don't define
 - max summary length: 240 – max summary length, obtained from &lt;summary&gt; or &lt;description&gt;
 - max inferred summary length: 240 – max summary length, obtained from &lt;content&gt; if no &lt;description&gt; is available

the above two may become optional and None by default in the future.

## actions
an action is an executable file, for example a shell script.  
json-serialized data are passed through stdin. 
example nuscript for sending notifications (good for a setup with a notification manager):
```nu
#!/usr/bin/env -S nu --stdin

def main [] {
	let entry = $in | from json
	let notif = (notify-send --action="open=open the url in your web browser" --wait $entry.title $entry.summary)
	if $notif == "open" {
		xdg-open $entry.url
	}
}
```

## socket interface
for more advanced uses, one should use the socket, open at `$XDG_RUNTIME_DIR/synd/con.sock`. the relevant `SocketQuery` and `Response` types are available in _synd-common_. communication happens with these json-serialized data over the sock. please note that communication ends once a newline character is received, therefore pretty json shouldn't be sent. (this will probably change in the future)
