#import "../lib.typ": render-ansi, terminal-frame

#set page(height: auto, width: auto, margin: 0.5in)

#terminal-frame(title: "cargo build", theme: "dracula")[
  \$ cargo build --release \
  Compiling conch v0.1.0 \
  Finished release target(s) in 3.2s
]

#v(20pt)

#terminal-frame(title: "test results", theme: "catppuccin")[
  #render-ansi(
    "\u{1b}[1;32mPASSED\u{1b}[0m test_shell_new\n\u{1b}[1;32mPASSED\u{1b}[0m test_echo\n\u{1b}[1;32mPASSED\u{1b}[0m test_pipe\n\u{1b}[1;31mFAILED\u{1b}[0m test_redirect\n\n\u{1b}[32m3 passed\u{1b}[0m, \u{1b}[31m1 failed\u{1b}[0m",
    theme: "catppuccin",
  )
]

#v(20pt)

#terminal-frame(title: "retro", theme: "retro")[
  READY. \
  10 PRINT "HELLO WORLD" \
  20 GOTO 10 \
  RUN
]
