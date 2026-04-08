#import "wasm.typ": _plugin

// =========================================================================
// Layer 3: Shell simulator — command engine powered by WASM
// =========================================================================

// --- Internal helpers ---

/// Process a line containing `\xNN` keyboard escape notation.
/// Returns an array of buffer states: (text, cursor, event).
#let _process-keyline(line) = json(_plugin.process_keyline(bytes(line)))

#let _find-raw(it) = {
  ""
  if type(it) == content {
    if it.func() == raw { it.text } else if it.func() == [].func() {
      it.children.map(_find-raw).join()
    }
  }
}

#let _parse-commands(body) = {
  let found = _find-raw(body)
  if found != "" { found.split("\n").filter(line => line.trim() != "") } else {
    ()
  }
}

#let _execute-session(user, hostname, files, commands) = {
  let home = "/home/" + user
  let today = datetime.today()
  let date-str = today.display(
    "[weekday repr:short] [month repr:short] [day padding:space] 00:00:00 UTC [year]",
  )
  let config = json.encode((
    user: user,
    hostname: hostname,
    home: home,
    files: files,
    commands: commands,
    date: date-str,
  ))
  json(_plugin.execute(bytes(config)))
}
