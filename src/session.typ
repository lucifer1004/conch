#import "wasm.typ": _plugin

// =========================================================================
// Layer 3: Shell simulator — command engine powered by WASM
// =========================================================================

// --- Internal helpers ---

/// Process a line containing `\xNN` keyboard escape notation.
/// Returns an array of buffer states: (text, cursor, event).
#let _process-keyline(line) = json(_plugin.process_keyline(bytes(line)))

/// Process keyline with history for Up/Down arrow navigation.
#let _process-keyline-with-history(line, history) = json(
  _plugin.process_keyline_with_history(
    bytes(json.encode((input: line, history: history))),
  ),
)

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

#let _execute-session(user, system, commands, include-files: false) = {
  let today = datetime.today()
  let date-str = today.display(
    "[weekday repr:short] [month repr:short] [day padding:space] 00:00:00 UTC [year]",
  )
  let config = json.encode((
    user: user,
    system: system,
    commands: commands,
    date: date-str,
    include-files: include-files,
  ))
  json(_plugin.execute(bytes(config)))
}
