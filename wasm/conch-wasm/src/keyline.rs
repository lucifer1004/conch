//! Line buffer with keyboard event processing.
//!
//! Parses `\xNN` escape notation into key events, applies them to a line
//! buffer, and returns the buffer state after each event. Used by the
//! per-char animation to render typing corrections, cursor movement, etc.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum KeyEvent {
    Char(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
    CtrlC,
    CtrlL,
    HistoryUp,
    HistoryDown,
}

#[derive(Debug, Clone, Serialize)]
pub struct BufferState {
    pub text: String,
    pub cursor: usize,
    pub event: &'static str,
}

// ---------------------------------------------------------------------------
// Line buffer
// ---------------------------------------------------------------------------

struct LineBuffer {
    chars: Vec<char>,
    cursor: usize,
}

impl LineBuffer {
    fn new() -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
        }
    }

    fn insert(&mut self, ch: char) {
        self.chars.insert(self.cursor, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
            true
        } else {
            false
        }
    }

    fn delete(&mut self) -> bool {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
            true
        } else {
            false
        }
    }

    fn left(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else {
            false
        }
    }

    fn right(&mut self) -> bool {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn home(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor = 0;
            true
        } else {
            false
        }
    }

    fn end(&mut self) -> bool {
        if self.cursor < self.chars.len() {
            self.cursor = self.chars.len();
            true
        } else {
            false
        }
    }

    fn set_content(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }

    fn state(&self, event: &'static str) -> BufferState {
        BufferState {
            text: self.text(),
            cursor: self.cursor,
            event,
        }
    }
}

// ---------------------------------------------------------------------------
// Escape unescape: `\xNN` notation → raw bytes
// ---------------------------------------------------------------------------

/// Convert `\xNN` hex escapes in the input string to actual bytes.
/// `\\` is treated as a literal backslash.
fn unescape_hex(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'\\' => {
                    out.push(b'\\');
                    i += 2;
                }
                b'x' if i + 3 < bytes.len() => {
                    let hi = bytes[i + 2];
                    let lo = bytes[i + 3];
                    if let (Some(h), Some(l)) = (hex_val(hi), hex_val(lo)) {
                        out.push(h << 4 | l);
                        i += 4;
                    } else {
                        // Not valid hex, emit literal
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
                _ => {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }

    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Key event parser: byte stream → Vec<KeyEvent>
// ---------------------------------------------------------------------------

/// Parse a byte stream (after unescape) into key events.
pub fn parse_key_events(raw: &[u8]) -> Vec<KeyEvent> {
    let mut events = Vec::new();
    let mut i = 0;

    while i < raw.len() {
        match raw[i] {
            0x03 => {
                events.push(KeyEvent::CtrlC);
                i += 1;
            }
            0x0c => {
                events.push(KeyEvent::CtrlL);
                i += 1;
            }
            0x7f => {
                events.push(KeyEvent::Backspace);
                i += 1;
            }
            0x1b => {
                // CSI sequence: ESC [ ...
                if i + 1 < raw.len() && raw[i + 1] == b'[' {
                    // Find the terminating byte: CSI sequences end at
                    // the first byte in 0x40..=0x7E (@ through ~).
                    let csi_start = i + 2;
                    let mut end = csi_start;
                    while end < raw.len() && !(0x40..=0x7E).contains(&raw[end]) {
                        end += 1;
                    }
                    if end < raw.len() {
                        // We have a complete CSI sequence; check the final byte
                        let final_byte = raw[end];
                        let params = &raw[csi_start..end];
                        match final_byte {
                            b'A' => events.push(KeyEvent::HistoryUp),
                            b'B' => events.push(KeyEvent::HistoryDown),
                            b'C' => events.push(KeyEvent::Right),
                            b'D' => events.push(KeyEvent::Left),
                            b'H' => events.push(KeyEvent::Home),
                            b'F' => events.push(KeyEvent::End),
                            b'~' if params == b"3" => events.push(KeyEvent::Delete),
                            _ => {} // Unknown CSI, silently ignore
                        }
                        i = end + 1;
                    } else {
                        // Incomplete CSI at end of input, skip all remaining
                        i = raw.len();
                    }
                } else {
                    i += 1; // Lone ESC, skip
                }
            }
            _ => {
                // Regular UTF-8 character
                let s = &raw[i..];
                if let Some(ch) = std::str::from_utf8(s).ok().and_then(|s| s.chars().next()) {
                    let len = ch.len_utf8();
                    events.push(KeyEvent::Char(ch));
                    i += len;
                } else {
                    i += 1; // Invalid byte, skip
                }
            }
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Public API: process a line → buffer states
// ---------------------------------------------------------------------------

/// Process input keystrokes without history support.
///
/// Parses `\xNN` escape notation, applies key events to a line buffer,
/// and returns the buffer state after each visible change.
pub fn process(input: &str) -> Vec<BufferState> {
    process_with_history(input, &[])
}

/// Process input keystrokes with history navigation (Up/Down arrows).
pub fn process_with_history(input: &str, history: &[String]) -> Vec<BufferState> {
    let raw = unescape_hex(input);
    let events = parse_key_events(&raw);

    let mut buf = LineBuffer::new();
    let mut states = Vec::with_capacity(events.len());
    let mut prev_text = String::new();
    let mut prev_cursor: usize = 0;
    // History index: history.len() means "current (new) line"
    let mut hist_idx = history.len();
    let mut saved_current = String::new();

    for event in &events {
        let (changed, label) = match event {
            KeyEvent::Char(ch) => {
                buf.insert(*ch);
                (true, "char")
            }
            KeyEvent::Backspace => (buf.backspace(), "backspace"),
            KeyEvent::Delete => (buf.delete(), "delete"),
            KeyEvent::Left => (buf.left(), "left"),
            KeyEvent::Right => (buf.right(), "right"),
            KeyEvent::Home => (buf.home(), "home"),
            KeyEvent::End => (buf.end(), "end"),
            KeyEvent::HistoryUp => {
                if !history.is_empty() && hist_idx > 0 {
                    if hist_idx == history.len() {
                        saved_current = buf.text();
                    }
                    hist_idx -= 1;
                    buf.set_content(&history[hist_idx]);
                    (true, "history-up")
                } else {
                    (false, "history-up")
                }
            }
            KeyEvent::HistoryDown => {
                if hist_idx < history.len() {
                    hist_idx += 1;
                    if hist_idx == history.len() {
                        buf.set_content(&saved_current);
                    } else {
                        buf.set_content(&history[hist_idx]);
                    }
                    (true, "history-down")
                } else {
                    (false, "history-down")
                }
            }
            KeyEvent::CtrlC => {
                states.push(buf.state("ctrl-c"));
                prev_text = buf.text();
                prev_cursor = buf.cursor;
                continue;
            }
            KeyEvent::CtrlL => {
                states.push(buf.state("ctrl-l"));
                prev_text = buf.text();
                prev_cursor = buf.cursor;
                continue;
            }
        };

        // Suppress duplicate frames (no visible change)
        if changed || buf.text() != prev_text || buf.cursor != prev_cursor {
            states.push(buf.state(label));
            prev_text = buf.text();
            prev_cursor = buf.cursor;
        }
    }

    states
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_produces_char_events() {
        let states = process("abc");
        assert_eq!(states.len(), 3);
        assert_eq!(states[0].text, "a");
        assert_eq!(states[1].text, "ab");
        assert_eq!(states[2].text, "abc");
        assert_eq!(states[2].cursor, 3);
    }

    #[test]
    fn backspace_deletes_previous_char() -> Result<(), &'static str> {
        let states = process("ab\\x7fc");
        // a, ab, a (backspace), ac
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "ac");
        assert_eq!(last.cursor, 2);
        Ok(())
    }

    #[test]
    fn left_right_moves_cursor() -> Result<(), &'static str> {
        let states = process("ab\\x1b[D\\x1b[Dc\\x1b[C");
        // a, ab, ab| cursor at 1, ab| cursor at 0, cab| cursor at 1, cab| cursor at 2
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "cab");
        assert_eq!(last.cursor, 2);
        Ok(())
    }

    #[test]
    fn home_end_jumps() -> Result<(), &'static str> {
        let states = process("abc\\x1b[Hx\\x1b[F");
        // abc, cursor at 0 (home), xabc cursor at 1, xabc cursor at 4 (end)
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "xabc");
        assert_eq!(last.cursor, 4);
        Ok(())
    }

    #[test]
    fn ctrl_c_emits_interrupt() {
        let states = process("ab\\x03");
        assert_eq!(states.len(), 3);
        assert_eq!(states[2].event, "ctrl-c");
        assert_eq!(states[2].text, "ab");
    }

    #[test]
    fn backspace_at_zero_is_suppressed() {
        let states = process("\\x7fa");
        // Backspace at empty buffer → no-op (suppressed), then 'a'
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].text, "a");
    }

    #[test]
    fn escaped_backslash() -> Result<(), &'static str> {
        let states = process("a\\\\b");
        // a, a\, a\b
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "a\\b");
        Ok(())
    }

    #[test]
    fn delete_key() -> Result<(), &'static str> {
        let states = process("abc\\x1b[D\\x1b[3~");
        // abc, cursor left to 2, delete 'c' at cursor
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "ab");
        assert_eq!(last.cursor, 2);
        Ok(())
    }

    #[test]
    fn empty_input() {
        let states = process("");
        assert!(states.is_empty());
    }

    #[test]
    fn correction_scenario() -> Result<(), &'static str> {
        // Type "helo", backspace, type "lo" → "hello"
        let states = process("helo\\x7flo");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "hello");
        Ok(())
    }

    // --- CSI parser robustness ---

    #[test]
    fn up_down_without_history_are_noops() -> Result<(), &'static str> {
        // Without history, Up/Down should NOT change the buffer
        let states = process("ab\\x1b[A\\x1b[Bc");
        let last = states.last().ok_or("should have states")?;
        assert_eq!(last.text, "abc");
        assert_eq!(last.cursor, 3);
        Ok(())
    }

    #[test]
    fn up_arrow_navigates_history() -> Result<(), &'static str> {
        let history = vec!["echo hello".to_string(), "ls -la".to_string()];
        // Type "x", then press Up twice, then Down once
        let states = process_with_history("x\\x1b[A\\x1b[A\\x1b[B", &history);
        // After "x": buffer = "x"
        // After Up: buffer = "ls -la" (most recent)
        // After Up: buffer = "echo hello" (older)
        // After Down: buffer = "ls -la" (back to recent)
        let last = states.last().ok_or("should have states")?;
        assert_eq!(last.text, "ls -la");
        Ok(())
    }

    #[test]
    fn down_past_history_restores_current() -> Result<(), &'static str> {
        let history = vec!["old cmd".to_string()];
        // Type "new", Up (shows "old cmd"), Down (restores "new")
        let states = process_with_history("new\\x1b[A\\x1b[B", &history);
        let last = states.last().ok_or("should have states")?;
        assert_eq!(last.text, "new");
        Ok(())
    }

    #[test]
    fn unknown_csi_is_skipped() -> Result<(), &'static str> {
        // ESC[2J (clear screen) — unknown, should be silently skipped
        let states = process("a\\x1b[2Jb");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "ab");
        Ok(())
    }

    #[test]
    fn long_csi_with_params_is_skipped() -> Result<(), &'static str> {
        // ESC[38;5;196m (256-color) — multi-param CSI, should skip correctly
        let states = process("x\\x1b[38;5;196my");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "xy");
        Ok(())
    }

    #[test]
    fn incomplete_csi_at_end() -> Result<(), &'static str> {
        // ESC[ at end of input with no final byte
        let states = process("ab\\x1b[");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "ab");
        Ok(())
    }

    #[test]
    fn lone_esc_is_skipped() -> Result<(), &'static str> {
        let states = process("a\\x1bb");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "ab");
        Ok(())
    }

    // --- Boundary conditions ---

    #[test]
    fn left_at_zero_suppressed() {
        let states = process("a\\x1b[H\\x1b[D");
        // 'a' cursor 1, Home cursor 0, Left at 0 → no-op (suppressed)
        assert_eq!(states.len(), 2); // 'a' + home, no extra frame for left
    }

    #[test]
    fn right_past_end_suppressed() {
        let states = process("a\\x1b[C");
        // 'a' cursor 1, Right past end → no-op
        assert_eq!(states.len(), 1);
    }

    #[test]
    fn delete_at_end_suppressed() {
        let states = process("ab\\x1b[3~");
        // ab cursor 2, Delete at end → no-op
        assert_eq!(states.len(), 2); // just 'a' and 'ab'
    }

    #[test]
    fn multiple_backspaces_clear_all() -> Result<(), &'static str> {
        let states = process("abc\\x7f\\x7f\\x7f");
        // abc → ab → a → empty
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "");
        assert_eq!(last.cursor, 0);
        Ok(())
    }

    #[test]
    fn insert_mid_line_with_arrows() -> Result<(), &'static str> {
        // Type "ac", left, insert "b" → "abc"
        let states = process("ac\\x1b[Db");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "abc");
        assert_eq!(last.cursor, 2); // cursor after 'b', before 'c'
        Ok(())
    }

    #[test]
    fn ctrl_c_mid_typing_preserves_buffer() -> Result<(), &'static str> {
        let states = process("abc\\x03def");
        // Find the ctrl-c state
        let ctrl_c = states
            .iter()
            .find(|s| s.event == "ctrl-c")
            .ok_or("expected ctrl-c state")?;
        assert_eq!(ctrl_c.text, "abc");
        // Typing continues after ctrl-c (buffer is not reset)
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "abcdef");
        Ok(())
    }

    #[test]
    fn ctrl_l_emits_event() {
        let states = process("x\\x0c");
        assert_eq!(states.len(), 2);
        assert_eq!(states[1].event, "ctrl-l");
        assert_eq!(states[1].text, "x"); // buffer unchanged
    }

    #[test]
    fn unescape_only_valid_hex() -> Result<(), &'static str> {
        // \xZZ is not valid hex — should be left as literal text
        let states = process("\\xZZa");
        let last = states.last().ok_or("states should not be empty")?;
        assert!(last.text.contains('a'));
        assert!(last.text.contains('\\'));
        Ok(())
    }

    #[test]
    fn demo_gif_arrow_scenario() -> Result<(), &'static str> {
        // The exact sequence from demo.typ: eco + left + h + end + space + "done"
        let states = process("eco\\x1b[Dh\\x1b[F \"done\"");
        let last = states.last().ok_or("states should not be empty")?;
        assert_eq!(last.text, "echo \"done\"");
        Ok(())
    }
}
