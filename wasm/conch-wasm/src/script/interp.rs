use crate::shell::Shell;
use crate::Str;

use super::ast::*;

/// Maximum iterations for a single loop to prevent infinite loops in WASM.
const MAX_LOOP_ITERATIONS: u32 = 10_000;

/// Maximum function call depth to prevent stack overflow from recursion.
const MAX_CALL_DEPTH: u32 = 64;

/// Per-statement execution result, used by `interpret_stmts_collecting`.
pub(crate) struct StmtResult {
    /// Shell state captured *before* this statement executed.
    pub user: Str,
    pub hostname: Str,
    pub display_path: String,
    /// Source text of the statement (extracted from the script).
    pub command_source: String,
    /// Source span of the statement.
    pub first_line: u32,
    pub last_line: u32,
    /// Output lines produced by this statement.
    pub output: Vec<String>,
    pub exit_code: i32,
    pub lang: Option<String>,
    pub bg_completions: Vec<String>,
    /// The control flow returned by this statement.
    pub flow: ControlFlow,
}

/// Control flow result from executing a statement.
pub(crate) enum ControlFlow {
    /// Normal execution completed with the given exit code.
    Normal(i32),
    /// `break [n]` — exit N enclosing loops.
    Break(u32),
    /// `continue [n]` — skip to next iteration of Nth enclosing loop.
    Continue(u32),
    /// `return [n]` — exit the current function with the given code.
    Return(i32),
}

impl ControlFlow {
    pub(crate) fn exit_code(&self) -> i32 {
        match self {
            ControlFlow::Normal(c) | ControlFlow::Return(c) => *c,
            ControlFlow::Break(_) | ControlFlow::Continue(_) => 0,
        }
    }
}

/// Return an error message for bare return/break/continue at the top level.
pub(crate) fn top_level_flow_error(flow: &ControlFlow) -> Option<String> {
    match flow {
        ControlFlow::Return(_) => {
            Some("conch: return: can only `return` from a function or sourced script\n".into())
        }
        ControlFlow::Break(_) => {
            Some("conch: break: only meaningful in a `for`, `while`, or `until` loop\n".into())
        }
        ControlFlow::Continue(_) => {
            Some("conch: continue: only meaningful in a `for`, `while`, or `until` loop\n".into())
        }
        ControlFlow::Normal(_) => None,
    }
}

impl Shell {
    /// Execute a list of parsed statements, collecting output into a flat Vec.
    pub(crate) fn interpret_stmts(
        &mut self,
        stmts: &[Stmt],
        output: &mut Vec<String>,
    ) -> ControlFlow {
        let mut last = ControlFlow::Normal(0);
        for stmt in stmts {
            last = self.interpret_stmt(stmt, output);
            match &last {
                ControlFlow::Normal(code) => {
                    // errexit (-e): abort script on non-zero exit code
                    if self.exec.opts.errexit && *code != 0 && self.exec.in_condition == 0 {
                        return last;
                    }
                }
                _ => return last,
            }
            // exec builtin: stop script execution
            if self.exec.exec_pending {
                return last;
            }
        }
        last
    }

    /// Execute statements, collecting per-statement results.
    ///
    /// Each `StmtResult` captures the shell state before execution plus
    /// the output, exit code, lang hint, and background completions produced
    /// by that statement. `script_source` is the original script text used
    /// to extract command source strings and push history entries.
    /// Errexit / exec_pending are handled internally.
    pub(crate) fn interpret_stmts_collecting(
        &mut self,
        stmts: &[Stmt],
        script_source: &str,
    ) -> (Vec<StmtResult>, ControlFlow) {
        let mut results = Vec::new();
        let mut last = ControlFlow::Normal(0);
        for stmt in stmts {
            let span = stmt.span();
            let command_source = script_source
                .get(span.start_byte as usize..span.end_byte as usize)
                .unwrap_or("")
                .to_string();

            // Record in history (like interactive bash)
            let trimmed = command_source.trim();
            if !trimmed.is_empty() {
                self.history.push(trimmed.to_string());
            }

            let pre_user = self.ident.user.clone();
            let pre_hostname = self.ident.hostname.clone();
            let pre_path = self.display_path();

            let mut output = Vec::new();
            last = self.interpret_stmt(stmt, &mut output);

            let exit_code = last.exit_code();
            let lang = self.last_lang.take();
            let bg = std::mem::take(&mut self.pending_bg_completions);

            results.push(StmtResult {
                user: pre_user,
                hostname: pre_hostname,
                display_path: pre_path,
                command_source,
                first_line: span.start_line,
                last_line: span.end_line,
                output,
                exit_code,
                lang,
                bg_completions: bg,
                flow: match &last {
                    ControlFlow::Normal(c) => ControlFlow::Normal(*c),
                    ControlFlow::Return(c) => ControlFlow::Return(*c),
                    ControlFlow::Break(n) => ControlFlow::Break(*n),
                    ControlFlow::Continue(n) => ControlFlow::Continue(*n),
                },
            });

            match &last {
                ControlFlow::Normal(code) => {
                    // errexit (-e): abort script on non-zero exit code
                    if self.exec.opts.errexit && *code != 0 && self.exec.in_condition == 0 {
                        return (results, last);
                    }
                }
                _ => return (results, last),
            }
            // exec builtin: stop script execution
            if self.exec.exec_pending {
                return (results, last);
            }
        }
        (results, last)
    }

    fn interpret_stmt(&mut self, stmt: &Stmt, output: &mut Vec<String>) -> ControlFlow {
        // Update LINENO (1-based) from the statement's source span
        let line = stmt.span().start_line;
        self.vars
            .env
            .insert("LINENO".into(), (line + 1).to_string());
        match stmt {
            Stmt::Structured { cmd: cmd_list, .. } => {
                let (out, code, lang) = self.exec_command_list(cmd_list);
                self.last_lang = lang;
                if !out.is_empty() {
                    output.push(out);
                }
                ControlFlow::Normal(code)
            }
            Stmt::If {
                clauses, else_body, ..
            } => self.interpret_if(clauses, else_body, output),
            Stmt::For {
                var, words, body, ..
            } => self.interpret_for(var, words, body, output),
            Stmt::While {
                condition, body, ..
            } => self.interpret_while(condition, body, output),
            Stmt::Until {
                condition, body, ..
            } => self.interpret_until(condition, body, output),
            Stmt::Case { word, arms, .. } => self.interpret_case(word, arms, output),
            Stmt::FunctionDef { name, body, .. } => {
                self.defs.functions.insert(name.clone(), body.clone());
                ControlFlow::Normal(0)
            }
            Stmt::BraceGroup { body, .. } => {
                // Brace group runs in current shell — no isolation
                self.interpret_stmts(body, output)
            }
            Stmt::Subshell { body, .. } => {
                // Full isolation — filesystem changes persist, everything else restored.
                let snap = self.snapshot_subshell();
                let flow = self.interpret_stmts(body, output);
                let code = flow.exit_code();

                // Execute EXIT trap for this subshell before restoring
                let trap_out = self.fire_exit_trap();
                if !trap_out.is_empty() {
                    output.push(trap_out);
                }

                self.restore_subshell(snap);
                ControlFlow::Normal(code)
            }
            Stmt::ForArith {
                init,
                cond,
                step,
                body,
                ..
            } => self.interpret_for_arith(init, cond, step, body, output),
            Stmt::Break(n, _) => ControlFlow::Break(n.unwrap_or(1)),
            Stmt::Continue(n, _) => ControlFlow::Continue(n.unwrap_or(1)),
            Stmt::Return(arg, _) => {
                let code = arg
                    .as_ref()
                    .map(|s| self.expand(s))
                    .and_then(|s| s.parse::<i32>().ok())
                    .unwrap_or(self.exec.last_exit_code);
                ControlFlow::Return(code)
            }
        }
    }

    fn interpret_if(
        &mut self,
        clauses: &[IfClause],
        else_body: &Option<Vec<Stmt>>,
        output: &mut Vec<String>,
    ) -> ControlFlow {
        for clause in clauses {
            self.exec.in_condition += 1;
            let flow = self.interpret_stmts(&clause.condition, output);
            self.exec.in_condition -= 1;
            match flow {
                ControlFlow::Normal(code) => {
                    if code == 0 {
                        return self.interpret_stmts(&clause.body, output);
                    }
                }
                other => return other,
            }
        }
        if let Some(body) = else_body {
            self.interpret_stmts(body, output)
        } else {
            ControlFlow::Normal(0)
        }
    }

    fn interpret_for(
        &mut self,
        var: &str,
        words: &[Str],
        body: &[Stmt],
        output: &mut Vec<String>,
    ) -> ControlFlow {
        let expanded = self.expand_for_words(words);
        let mut last_code = 0;
        let mut iterations = 0u32;
        for word in &expanded {
            iterations += 1;
            if iterations > MAX_LOOP_ITERATIONS {
                output.push(format!(
                    "conch: for loop exceeded {} iterations",
                    MAX_LOOP_ITERATIONS
                ));
                return ControlFlow::Normal(1);
            }
            self.vars.env.insert(var.into(), word.clone());
            match self.interpret_stmts(body, output) {
                ControlFlow::Normal(code) => last_code = code,
                ControlFlow::Break(n) => {
                    if n > 1 {
                        return ControlFlow::Break(n - 1);
                    }
                    break;
                }
                ControlFlow::Continue(n) => {
                    if n > 1 {
                        return ControlFlow::Continue(n - 1);
                    }
                    continue;
                }
                ret @ ControlFlow::Return(_) => return ret,
            }
        }
        ControlFlow::Normal(last_code)
    }

    fn interpret_while(
        &mut self,
        condition: &[Stmt],
        body: &[Stmt],
        output: &mut Vec<String>,
    ) -> ControlFlow {
        let mut last_code = 0;
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > MAX_LOOP_ITERATIONS {
                output.push(format!(
                    "conch: while loop exceeded {} iterations",
                    MAX_LOOP_ITERATIONS
                ));
                return ControlFlow::Normal(1);
            }
            self.exec.in_condition += 1;
            let cond_flow = self.interpret_stmts(condition, output);
            self.exec.in_condition -= 1;
            match cond_flow {
                ControlFlow::Normal(code) => {
                    if code != 0 {
                        break;
                    }
                }
                other => return other,
            }
            match self.interpret_stmts(body, output) {
                ControlFlow::Normal(code) => last_code = code,
                ControlFlow::Break(n) => {
                    if n > 1 {
                        return ControlFlow::Break(n - 1);
                    }
                    break;
                }
                ControlFlow::Continue(n) => {
                    if n > 1 {
                        return ControlFlow::Continue(n - 1);
                    }
                    continue;
                }
                ret @ ControlFlow::Return(_) => return ret,
            }
        }
        ControlFlow::Normal(last_code)
    }

    fn interpret_until(
        &mut self,
        condition: &[Stmt],
        body: &[Stmt],
        output: &mut Vec<String>,
    ) -> ControlFlow {
        let mut last_code = 0;
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > MAX_LOOP_ITERATIONS {
                output.push(format!(
                    "conch: until loop exceeded {} iterations",
                    MAX_LOOP_ITERATIONS
                ));
                return ControlFlow::Normal(1);
            }
            self.exec.in_condition += 1;
            let cond_flow = self.interpret_stmts(condition, output);
            self.exec.in_condition -= 1;
            match cond_flow {
                ControlFlow::Normal(code) => {
                    // until: loop while condition is FALSE (non-zero)
                    if code == 0 {
                        break;
                    }
                }
                other => return other,
            }
            match self.interpret_stmts(body, output) {
                ControlFlow::Normal(code) => last_code = code,
                ControlFlow::Break(n) => {
                    if n > 1 {
                        return ControlFlow::Break(n - 1);
                    }
                    break;
                }
                ControlFlow::Continue(n) => {
                    if n > 1 {
                        return ControlFlow::Continue(n - 1);
                    }
                    continue;
                }
                ret @ ControlFlow::Return(_) => return ret,
            }
        }
        ControlFlow::Normal(last_code)
    }

    fn interpret_for_arith(
        &mut self,
        init: &str,
        cond: &str,
        step: &str,
        body: &[Stmt],
        output: &mut Vec<String>,
    ) -> ControlFlow {
        // Evaluate init expression (side effects only, e.g. i=0)
        if !init.is_empty() {
            self.eval_arith_expr(init);
        }
        let mut last_code = 0;
        let mut iterations = 0u32;
        loop {
            iterations += 1;
            if iterations > MAX_LOOP_ITERATIONS {
                output.push(format!(
                    "conch: for (( )) loop exceeded {} iterations",
                    MAX_LOOP_ITERATIONS
                ));
                return ControlFlow::Normal(1);
            }
            // Evaluate condition; empty cond is always true (like bash)
            if !cond.is_empty() {
                let cond_val = self.eval_arith_expr(cond);
                if cond_val == 0 {
                    break;
                }
            }
            match self.interpret_stmts(body, output) {
                ControlFlow::Normal(code) => last_code = code,
                ControlFlow::Break(n) => {
                    if n > 1 {
                        return ControlFlow::Break(n - 1);
                    }
                    break;
                }
                ControlFlow::Continue(n) => {
                    if n > 1 {
                        return ControlFlow::Continue(n - 1);
                    }
                    // fall through to step
                }
                ret @ ControlFlow::Return(_) => return ret,
            }
            // Evaluate step expression
            if !step.is_empty() {
                self.eval_arith_expr(step);
            }
        }
        ControlFlow::Normal(last_code)
    }

    /// Format a function definition for `declare -f`.
    fn format_function(&self, name: &str, body: &[Stmt]) -> String {
        let mut out = format!("{} ()\n{{\n", name);
        for stmt in body {
            let line = match stmt {
                Stmt::Structured { cmd, .. } => cmd.to_source(),
                _ => format!("{:?}", stmt),
            };
            out.push_str(&format!("    {}\n", line));
        }
        out.push('}');
        out
    }

    /// Call a user-defined function with the given arguments.
    /// Pushes positional parameters, executes the body, then restores them.
    pub fn call_function(&mut self, name: &str, args: &[String]) -> (String, i32) {
        if self.exec.call_depth >= MAX_CALL_DEPTH {
            return (
                format!("conch: {}: maximum recursion depth exceeded", name),
                1,
            );
        }

        let body = match self.defs.functions.get(name) {
            Some(b) => b.clone(),
            None => return (format!("conch: {}: not a function", name), 127),
        };

        // Save current positional parameters
        let saved = self.save_positional_params();

        // Save and set $FUNCNAME and $BASH_SOURCE
        let saved_funcname = self.var("FUNCNAME").map(|s| s.to_string());
        let saved_bash_source = self.var("BASH_SOURCE").map(|s| s.to_string());
        self.vars.env.insert("FUNCNAME".into(), name.to_string());
        self.vars.env.insert("BASH_SOURCE".into(), String::new());

        // Install new positional parameters ($1 = first arg, etc.)
        self.set_positional_params(args);

        // Push a local variable frame
        self.vars.push_locals();

        // Execute function body
        self.exec.call_depth += 1;
        let mut output = Vec::new();
        let flow = self.interpret_stmts(&body, &mut output);
        self.exec.call_depth -= 1;

        // Pop and restore local variables
        self.vars.pop_locals();

        // Restore $FUNCNAME and $BASH_SOURCE
        match saved_funcname {
            Some(v) => {
                self.vars.env.insert("FUNCNAME".into(), v);
            }
            None => {
                self.vars.env.remove("FUNCNAME");
            }
        }
        match saved_bash_source {
            Some(v) => {
                self.vars.env.insert("BASH_SOURCE".into(), v);
            }
            None => {
                self.vars.env.remove("BASH_SOURCE");
            }
        }

        // Restore previous positional parameters
        self.restore_positional_params(saved);

        let code = match &flow {
            ControlFlow::Return(c) | ControlFlow::Normal(c) => *c,
            other => {
                if let Some(msg) = top_level_flow_error(other) {
                    output.push(msg);
                }
                1
            }
        };
        (output.join("\n"), code)
    }

    /// Save current positional parameters from env (including $0).
    pub(crate) fn save_positional_params(&self) -> Vec<(String, Option<String>)> {
        let mut saved = Vec::new();
        for key in &[
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "@", "#", "*",
        ] {
            saved.push((key.to_string(), self.vars.env.get(*key).cloned()));
        }
        saved
    }

    /// Set $0 (script/function name).
    pub(crate) fn set_zero(&mut self, name: &str) {
        self.vars.env.insert("0".into(), name.to_string());
    }

    /// Set positional parameters in env from an argument list.
    /// `args[0]` → `$1`, `args[1]` → `$2`, etc. (1-based, matching bash).
    pub(crate) fn set_positional_params(&mut self, args: &[String]) {
        // Clear old positional params $1..$9
        for i in 1..=9 {
            self.vars.env.remove(i.to_string().as_str());
        }
        // Set new ones: args[0] → $1, args[1] → $2, etc.
        for (i, arg) in args.iter().enumerate() {
            if i < 9 {
                self.vars
                    .env
                    .insert((i + 1).to_string().into(), arg.clone());
            }
        }
        // $@ and $* = all args joined by space
        let all = args.join(" ");
        self.vars.env.insert("@".into(), all.clone());
        self.vars.env.insert("*".into(), all);
        // $# = argument count
        self.vars.env.insert("#".into(), args.len().to_string());
    }

    /// Restore positional parameters from a saved snapshot.
    pub(crate) fn restore_positional_params(&mut self, saved: Vec<(String, Option<String>)>) {
        for (key, val) in saved {
            match val {
                Some(v) => {
                    self.vars.env.insert(key.into(), v);
                }
                None => {
                    self.vars.env.remove(key.as_str());
                }
            }
        }
    }

    /// `shift [n]` — remove the first N positional parameters.
    pub fn cmd_shift(&mut self, args: &[String]) -> (String, i32) {
        let n: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(1);

        // Collect current positional params
        let mut params: Vec<String> = Vec::new();
        let count: usize = self
            .vars
            .env
            .get("#")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        for i in 1..=count {
            if let Some(val) = self.vars.env.get(i.to_string().as_str()) {
                params.push(val.clone());
            }
        }

        if n > params.len() {
            return ("shift: shift count exceeds positional parameters".into(), 1);
        }

        // Remove first n, re-index
        let shifted: Vec<String> = params[n..].to_vec();

        // Clear old
        for i in 1..=count {
            self.vars.env.remove(i.to_string().as_str());
        }
        // Set new
        for (i, val) in shifted.iter().enumerate() {
            self.vars
                .env
                .insert((i + 1).to_string().into(), val.clone());
        }
        let all = shifted.join(" ");
        self.vars.env.insert("@".into(), all.clone());
        self.vars.env.insert("*".into(), all);
        self.vars.env.insert("#".into(), shifted.len().to_string());

        (String::new(), 0)
    }

    fn interpret_case(
        &mut self,
        word: &str,
        arms: &[super::ast::CaseArm],
        output: &mut Vec<String>,
    ) -> ControlFlow {
        let expanded = self.expand_full(word);
        for arm in arms {
            for pattern in &arm.patterns {
                let pat = self.expand(pattern);
                if Self::glob_match(&pat, &expanded) {
                    return self.interpret_stmts(&arm.body, output);
                }
            }
        }
        ControlFlow::Normal(0)
    }

    /// Simple glob matching for case patterns. Supports `*`, `?`, and literal chars.
    pub fn glob_match_str(pattern: &str, text: &str) -> bool {
        Self::glob_match(pattern, text)
    }

    fn glob_match(pattern: &str, text: &str) -> bool {
        let p: Vec<char> = pattern.chars().collect();
        let t: Vec<char> = text.chars().collect();
        Self::glob_match_inner(&p, &t)
    }

    fn glob_match_inner(p: &[char], t: &[char]) -> bool {
        // Check for extended glob patterns: ?(pat), *(pat), +(pat), @(pat), !(pat)
        if p.len() >= 2 && p[1] == '(' {
            let prefix = p[0];
            if "?*+@!".contains(prefix) {
                if let Some((alternatives, rest_p)) = Self::extract_extglob_alternatives(&p[2..]) {
                    return Self::match_extglob(prefix, &alternatives, rest_p, t);
                }
            }
        }

        match (p.first(), t.first()) {
            (None, None) => true,
            (Some('*'), _) => {
                // * matches zero or more chars
                Self::glob_match_inner(&p[1..], t) // zero chars
                    || (!t.is_empty() && Self::glob_match_inner(p, &t[1..])) // one+ chars
            }
            (Some('?'), Some(_)) => Self::glob_match_inner(&p[1..], &t[1..]),
            (Some('['), Some(tc)) => {
                // Character class: [abc], [a-z], [!abc], [!a-z]
                if let Some((matched, rest)) = Self::match_char_class(&p[1..], *tc) {
                    matched && Self::glob_match_inner(rest, &t[1..])
                } else {
                    // Malformed class — treat '[' as literal
                    p.first() == t.first() && Self::glob_match_inner(&p[1..], &t[1..])
                }
            }
            (Some(pc), Some(tc)) if pc == tc => Self::glob_match_inner(&p[1..], &t[1..]),
            _ => false,
        }
    }

    /// Extract pipe-separated alternatives from inside an extended glob `(a|b|c)`.
    /// Returns (Vec of alternative patterns as Vec<char>, rest of pattern after closing paren).
    fn extract_extglob_alternatives(p: &[char]) -> Option<(Vec<Vec<char>>, &[char])> {
        let mut depth = 1u32;
        let mut i = 0;
        while i < p.len() {
            match p[i] {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        let inner: Vec<char> = p[..i].to_vec();
                        // Split on top-level `|`
                        let alternatives = Self::split_extglob_pipes(&inner);
                        return Some((alternatives, &p[i + 1..]));
                    }
                }
                _ => {}
            }
            i += 1;
        }
        None
    }

    /// Split extglob inner content on `|` respecting nested parens.
    fn split_extglob_pipes(chars: &[char]) -> Vec<Vec<char>> {
        let mut alternatives = Vec::new();
        let mut current = Vec::new();
        let mut depth = 0u32;
        for &c in chars {
            match c {
                '(' => {
                    depth += 1;
                    current.push(c);
                }
                ')' => {
                    depth -= 1;
                    current.push(c);
                }
                '|' if depth == 0 => {
                    alternatives.push(current.clone());
                    current.clear();
                }
                _ => current.push(c),
            }
        }
        alternatives.push(current);
        alternatives
    }

    /// Match an extended glob pattern against text.
    fn match_extglob(
        prefix: char,
        alternatives: &[Vec<char>],
        rest_p: &[char],
        t: &[char],
    ) -> bool {
        match prefix {
            '?' => {
                // ?(pat) — zero or one occurrence of any alternative
                // Try zero occurrences
                if Self::glob_match_inner(rest_p, t) {
                    return true;
                }
                // Try one occurrence of each alternative
                for alt in alternatives {
                    for split in 0..=t.len() {
                        if Self::glob_match_inner(alt, &t[..split])
                            && Self::glob_match_inner(rest_p, &t[split..])
                        {
                            return true;
                        }
                    }
                }
                false
            }
            '@' => {
                // @(pat) — exactly one occurrence of any alternative
                for alt in alternatives {
                    for split in 0..=t.len() {
                        if Self::glob_match_inner(alt, &t[..split])
                            && Self::glob_match_inner(rest_p, &t[split..])
                        {
                            return true;
                        }
                    }
                }
                false
            }
            '+' => {
                // +(pat) — one or more occurrences
                Self::match_extglob_repeat(alternatives, rest_p, t, 1)
            }
            '*' if !(alternatives.is_empty()
                || alternatives.len() == 1 && alternatives[0].is_empty()) =>
            {
                // *(pat) — zero or more occurrences
                Self::match_extglob_repeat(alternatives, rest_p, t, 0)
            }
            '!' => {
                // !(pat) — match anything that doesn't match any alternative
                // The text must match rest_p but NOT match any alternative followed by rest_p
                // Implementation: text matches if rest_p matches the whole text
                // AND no split matches alt+rest_p
                for split in 0..=t.len() {
                    let candidate = &t[..split];
                    let remainder = &t[split..];
                    if Self::glob_match_inner(rest_p, remainder) {
                        // Check that candidate doesn't match any alternative
                        let matches_any = alternatives
                            .iter()
                            .any(|alt| Self::glob_match_inner(alt, candidate));
                        if !matches_any {
                            return true;
                        }
                    }
                }
                false
            }
            _ => {
                // Fallback: treat * normally
                Self::glob_match_inner(rest_p, t)
            }
        }
    }

    /// Helper for +(pat) and *(pat): match min_count or more occurrences of alternatives.
    fn match_extglob_repeat(
        alternatives: &[Vec<char>],
        rest_p: &[char],
        t: &[char],
        min_count: usize,
    ) -> bool {
        // Try matching rest_p directly if min_count is 0
        if min_count == 0 && Self::glob_match_inner(rest_p, t) {
            return true;
        }
        // Try each alternative for the first occurrence
        for alt in alternatives {
            for split in 1..=t.len() {
                if Self::glob_match_inner(alt, &t[..split]) {
                    let remaining = &t[split..];
                    let next_min = if min_count > 0 { min_count - 1 } else { 0 };
                    if Self::match_extglob_repeat(alternatives, rest_p, remaining, next_min) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Parse a character class after the opening `[`.
    /// Returns `(matched, rest_of_pattern_after_closing_bracket)` or None if malformed.
    fn match_char_class(p: &[char], tc: char) -> Option<(bool, &[char])> {
        let mut i = 0;
        let negate = if i < p.len() && p[i] == '!' {
            i += 1;
            true
        } else {
            false
        };
        // A leading ']' is treated as a literal member of the class
        let start = i;
        let mut matched = false;
        loop {
            if i >= p.len() {
                return None; // no closing ']'
            }
            if p[i] == ']' && i > start {
                let result = if negate { !matched } else { matched };
                return Some((result, &p[i + 1..]));
            }
            // Range: a-z
            if i + 2 < p.len() && p[i + 1] == '-' && p[i + 2] != ']' {
                let lo = p[i];
                let hi = p[i + 2];
                if tc >= lo && tc <= hi {
                    matched = true;
                }
                i += 3;
            } else {
                if p[i] == tc {
                    matched = true;
                }
                i += 1;
            }
        }
    }

    /// `local var=value` / `declare [-a|-A|-n|-p|-i] var[=value]` — set a variable.
    /// The previous value is saved in the current local frame and restored on return.
    pub fn cmd_local(&mut self, args: &[String], is_local: bool) -> (String, i32) {
        // Parse flags: -a (indexed array), -A (assoc array), -n (nameref),
        //              -p (print), -i (integer arithmetic evaluation),
        //              -x (export), -r (readonly), -f (list functions),
        //              -F (list function names only)
        let mut flag_a = false;
        let mut flag_big_a = false;
        let mut flag_n = false;
        let mut flag_p = false;
        let mut flag_i = false;
        let mut flag_x = false;
        let mut flag_r = false;
        let mut flag_f = false;
        let mut flag_big_f = false;
        let mut remaining = Vec::new();
        for arg in args {
            match arg.as_str() {
                "-a" => flag_a = true,
                "-A" => flag_big_a = true,
                "-n" => flag_n = true,
                "-p" => flag_p = true,
                "-i" => flag_i = true,
                "-x" => flag_x = true,
                "-r" => flag_r = true,
                "-f" => flag_f = true,
                "-F" => flag_big_f = true,
                _ => remaining.push(arg.clone()),
            }
        }

        // declare -F: list function names only
        if flag_big_f {
            let mut lines: Vec<String> = self
                .defs
                .functions
                .keys()
                .map(|k| format!("declare -f {}", k))
                .collect();
            lines.sort();
            return (lines.join("\n"), 0);
        }

        // declare -f [name]: show function definition(s)
        if flag_f {
            if remaining.is_empty() {
                // List all functions with bodies
                let mut out = Vec::new();
                for (name, body) in &self.defs.functions {
                    out.push(self.format_function(name, body));
                }
                return (out.join("\n"), 0);
            }
            // Show specific function(s)
            let mut out = Vec::new();
            let mut code = 0;
            for name in &remaining {
                if let Some(body) = self.defs.functions.get(name.as_str()) {
                    out.push(self.format_function(name, &body.clone()));
                } else {
                    code = 1;
                }
            }
            return (out.join("\n"), code);
        }

        // declare -p: print variable declarations
        if flag_p {
            if remaining.is_empty() {
                // Print all variables
                let mut lines: Vec<String> = self
                    .vars
                    .env
                    .iter()
                    .filter(|(k, _)| k.as_str() != "?")
                    .map(|(k, v)| format!("declare -- {}=\"{}\"", k, v))
                    .collect();
                // Also print arrays
                for (k, arr) in &self.vars.arrays {
                    let elems: Vec<String> = arr
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| !v.is_empty())
                        .map(|(i, v)| format!("[{}]=\"{}\"", i, v))
                        .collect();
                    lines.push(format!("declare -a {}=({})", k, elems.join(" ")));
                }
                lines.sort();
                return (lines.join("\n"), 0);
            }
            // Print specific variables
            let mut out = Vec::new();
            for name in &remaining {
                if let Some(val) = self.vars.env.get(name.as_str()) {
                    out.push(format!("declare -- {}=\"{}\"", name, val));
                } else if let Some(arr) = self.vars.arrays.get(name.as_str()) {
                    let elems: Vec<String> = arr
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| !v.is_empty())
                        .map(|(i, v)| format!("[{}]=\"{}\"", i, v))
                        .collect();
                    out.push(format!("declare -a {}=({})", name, elems.join(" ")));
                } else if let Some(assoc) = self.vars.assoc_arrays.get(name.as_str()) {
                    let elems: Vec<String> = assoc
                        .iter()
                        .map(|(k, v)| format!("[{}]=\"{}\"", k, v))
                        .collect();
                    out.push(format!("declare -A {}=({})", name, elems.join(" ")));
                }
            }
            return (out.join("\n"), 0);
        }

        // `local` outside function is error, but `declare` at top-level is fine
        if is_local && self.vars.local_frames.is_empty() {
            return ("conch: local: can only be used in a function".into(), 1);
        }

        for arg in &remaining {
            let (name, new_val) = if let Some((n, v)) = arg.split_once('=') {
                (n.to_string(), Some(v.to_string()))
            } else {
                (arg.clone(), None)
            };

            // Check readonly
            if self.vars.readonly.contains(name.as_str()) {
                return (format!("conch: declare: {}: readonly variable", name), 1);
            }

            if flag_big_a {
                // declare -A: create associative array (save frame first)
                if let Err(e) = self.vars.declare_local(&name, None) {
                    return (e, 1);
                }
                self.vars.assoc_arrays.entry(name.into()).or_default();
            } else if flag_a {
                // declare -a: create indexed array
                if let Err(e) = self.vars.declare_local(&name, None) {
                    return (e, 1);
                }
                self.vars.arrays.entry(name.into()).or_default();
            } else if flag_n {
                // declare -n name=target: create nameref
                if let Err(e) = self.vars.declare_local(&name, None) {
                    return (e, 1);
                }
                if let Some(target) = new_val {
                    let expanded = self.expand(&target);
                    self.vars.namerefs.insert(name.into(), expanded.into());
                } else {
                    self.vars.namerefs.entry(name.into()).or_default();
                }
            } else if flag_i {
                // declare -i: integer variable — evaluate RHS as arithmetic
                let val = new_val.map(|v| {
                    let expanded = self.expand(&v);
                    self.eval_arith_expr(&expanded).to_string()
                });
                if let Err(e) = self.vars.declare_local(&name, val) {
                    return (e, 1);
                }
            } else if flag_x {
                // declare -x: export variable (set it in env)
                let expanded = new_val.map(|v| self.expand(&v));
                if let Some(val) = expanded {
                    self.vars.env.insert(name.clone().into(), val);
                }
                // Variable is already in env, which cmd_env reads
            } else if flag_r {
                // declare -r: mark variable as readonly
                let expanded = new_val.map(|v| self.expand(&v));
                if let Some(val) = expanded {
                    self.vars.env.insert(name.clone().into(), val);
                }
                self.vars.readonly.insert(name.into());
            } else {
                // Normal variable
                let expanded = new_val.map(|v| self.expand(&v));
                if let Err(e) = self.vars.declare_local(&name, expanded) {
                    return (e, 1);
                }
            }
        }
        (String::new(), 0)
    }

    /// `read [-r] [-p prompt] [-a array] var1 var2 ...` — read from stdin into variables.
    /// In conch's context, stdin comes from pipes. Without piped input, does nothing.
    pub fn cmd_read(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let full_input = match stdin {
            Some(s) => s.to_string(),
            None => return (String::new(), 1), // no input available
        };

        // Parse flags
        let mut raw = false;
        let mut array_name: Option<String> = None;
        let mut delimiter: Option<char> = None;
        let mut nchars: Option<usize> = None;
        let mut var_names = Vec::new();
        let mut iter = args.iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-r" => raw = true,
                "-p" => {
                    iter.next();
                } // skip prompt (not displayed in non-interactive)
                "-a" => {
                    if let Some(name) = iter.next() {
                        array_name = Some(name.clone());
                    }
                }
                "-d" => {
                    if let Some(d) = iter.next() {
                        delimiter = d.chars().next();
                    }
                }
                "-n" => {
                    if let Some(n) = iter.next() {
                        nchars = n.parse().ok();
                    }
                }
                _ => var_names.push(arg.clone()),
            }
        }

        // Determine input_line based on delimiter or nchars
        let input_line = if let Some(n) = nchars {
            // read -n N: read exactly N characters
            full_input.chars().take(n).collect::<String>()
        } else if let Some(delim) = delimiter {
            // read -d DELIM: read up to delimiter
            match full_input.find(delim) {
                Some(pos) => full_input[..pos].to_string(),
                None => full_input.lines().next().unwrap_or("").to_string(),
            }
        } else {
            full_input.lines().next().unwrap_or("").to_string()
        };

        let line = if raw {
            input_line
        } else {
            input_line.replace("\\n", "\n").replace("\\t", "\t")
        };

        // Split input using $IFS (default: " \t\n")
        let ifs = self
            .vars
            .env
            .get("IFS")
            .cloned()
            .unwrap_or_else(|| " \t\n".to_string());
        let ifs_chars: Vec<char> = ifs.chars().collect();

        // read -a ARRAY: split all words into indexed array
        if let Some(arr_name) = array_name {
            let words = ifs_split_all(&line, &ifs_chars);
            self.vars.arrays.insert(arr_name.into(), words);
            return (String::new(), 0);
        }

        if var_names.is_empty() {
            var_names.push("REPLY".to_string());
        }

        // Split into at most N parts where N = number of variables.
        // The last variable gets the remainder (including any delimiters).
        let parts = ifs_splitn(&line, &ifs_chars, var_names.len());
        for (i, name) in var_names.iter().enumerate() {
            let val = parts.get(i).cloned().unwrap_or_default();
            self.vars.env.insert(name.clone().into(), val);
        }

        (String::new(), 0)
    }

    /// Execute a command found via PATH lookup.
    pub fn exec_from_path(&mut self, cmd: &str, args: &[String]) -> (String, i32) {
        let full_path = match self.which_path(cmd) {
            Some(p) => p,
            None => return (format!("conch: command not found: {}", cmd), 127),
        };
        let script = match self.fs.read_to_string(&full_path) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("conch: {}: {}", cmd, e), 126),
        };

        let snap = self.snapshot_subshell();
        self.set_positional_params(args);
        let (out, code) = self.run_script(&script);
        self.restore_subshell(snap);
        (out, code)
    }

    /// Check if a command name can be resolved via `$PATH`.
    pub fn is_in_path(&self, cmd: &str) -> bool {
        self.which_path(cmd).is_some()
    }

    /// Return the full path of a command found in `$PATH`, or None.
    pub fn which_path(&self, cmd: &str) -> Option<String> {
        let path_var = self.var("PATH")?;
        for dir in path_var.split(':') {
            if dir.is_empty() {
                continue;
            }
            let full = format!("{}/{}", dir, cmd);
            if let Ok(meta) = self.fs.metadata(&full) {
                if !meta.is_dir() && meta.is_executable() && meta.is_readable() {
                    return Some(full);
                }
            }
        }
        None
    }

    /// Expand for-loop word list: brace expansion, command substitution,
    /// variable expansion, word splitting, and glob expansion. Unquoted words
    /// containing `$` are split on whitespace after expansion (simplified IFS
    /// splitting).
    fn expand_for_words(&mut self, words: &[Str]) -> Vec<String> {
        let mut result = Vec::new();
        for word in words {
            // Apply brace expansion first, then process each result
            let brace_expanded = crate::shell::expand_braces(word);
            for bword in brace_expanded {
                // Process $'...' ANSI-C quoting
                let bword = crate::shell::process_dollar_single_quote(&bword);
                let expanded = self.expand_full(&bword);
                let should_split = has_unquoted_dollar(&bword);
                if should_split {
                    // Word splitting on unquoted variable expansions
                    for part in expanded.split_whitespace() {
                        let globbed = self.expand_globs(&[part.to_string()]);
                        result.extend(globbed);
                    }
                } else {
                    let globbed = self.expand_globs(&[expanded]);
                    result.extend(globbed);
                }
            }
        }
        result
    }
}

/// Check if a word contains an unquoted `$` (including `$@`, `$*`, `$VAR`).
/// Tracks single and double quote state to determine if the `$` is quoted.
/// A `$` inside single quotes is always literal. A `$` inside double quotes
/// is still "quoted" for the purpose of word splitting (no field splitting).
fn has_unquoted_dollar(word: &str) -> bool {
    let mut in_single = false;
    let mut in_double = false;
    let bytes = word.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' if !in_double => {
                in_single = !in_single;
            }
            b'"' if !in_single => {
                in_double = !in_double;
            }
            b'\\' if !in_single => {
                i += 1; // skip escaped char
            }
            b'$' if !in_single && !in_double => {
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Split a string into at most `n` parts using IFS characters.
/// Leading/trailing IFS whitespace is trimmed. The last field gets the
/// un-split remainder (preserving internal delimiters).
fn ifs_splitn(s: &str, ifs_chars: &[char], n: usize) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    // Trim leading IFS characters
    let s = s.trim_start_matches(|c: char| ifs_chars.contains(&c));
    if n == 1 {
        // Last variable gets the remainder, trimmed of trailing IFS whitespace
        let trimmed = s.trim_end_matches(|c: char| ifs_chars.contains(&c));
        return vec![trimmed.to_string()];
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() && parts.len() < n - 1 {
        if ifs_chars.contains(&chars[i]) {
            parts.push(chars[start..i].iter().collect::<String>());
            // Skip consecutive IFS characters between fields
            while i < chars.len() && ifs_chars.contains(&chars[i]) {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    // Remaining goes into last field (trimmed of trailing IFS)
    let remainder: String = chars[start..].iter().collect();
    let remainder = remainder.trim_end_matches(|c: char| ifs_chars.contains(&c));
    parts.push(remainder.to_string());
    parts
}

/// Split a string into all words using IFS characters (for `read -a`).
fn ifs_split_all(s: &str, ifs_chars: &[char]) -> Vec<String> {
    let s = s.trim_matches(|c: char| ifs_chars.contains(&c));
    if s.is_empty() {
        return Vec::new();
    }
    let mut parts = Vec::new();
    let mut start = 0;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if ifs_chars.contains(&chars[i]) {
            parts.push(chars[start..i].iter().collect::<String>());
            while i < chars.len() && ifs_chars.contains(&chars[i]) {
                i += 1;
            }
            start = i;
        } else {
            i += 1;
        }
    }
    if start < chars.len() {
        parts.push(chars[start..].iter().collect::<String>());
    }
    parts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::types::*;

    fn shell() -> crate::shell::Shell {
        let config = Config {
            user: "user".into(),
            system: Some(SystemSpec {
                hostname: "test".into(),
                users: vec![UserSpec {
                    name: "user".into(),
                    uid: Some(1000),
                    home: Some("/home/user".into()),
                    groups: vec![],
                }],
                groups: vec![],
                files: Default::default(),
            }),
            hostname: None,
            home: None,
            files: None,
            commands: vec![],
            date: None,
            include_files: false,
            background_mode: Default::default(),
        };
        crate::shell::Shell::new(&config)
    }

    // -- if --

    #[test]
    fn if_true_branch() {
        let mut sh = shell();
        let (out, code) = sh.run_script("if true; then echo yes; fi");
        assert_eq!(out, "yes\n");
        assert_eq!(code, 0);
    }

    #[test]
    fn if_false_branch() {
        let mut sh = shell();
        let (out, code) = sh.run_script("if false; then echo yes; fi");
        assert_eq!(out, "");
        assert_eq!(code, 0);
    }

    #[test]
    fn if_else() {
        let mut sh = shell();
        let (out, _) = sh.run_script("if false; then echo yes; else echo no; fi");
        assert_eq!(out, "no\n");
    }

    #[test]
    fn if_elif() {
        let mut sh = shell();
        let (out, _) =
            sh.run_script("if false; then echo 1; elif true; then echo 2; else echo 3; fi");
        assert_eq!(out, "2\n");
    }

    #[test]
    fn if_with_test_command() {
        let mut sh = shell();
        sh.run_line("mkdir -p /tmp/testdir");
        let (out, _) =
            sh.run_script("if [ -d /tmp/testdir ]; then echo exists; else echo missing; fi");
        assert_eq!(out, "exists\n");
    }

    #[test]
    fn nested_if() {
        let mut sh = shell();
        let (out, _) =
            sh.run_script("if true; then\n  if false; then echo inner; else echo outer; fi\nfi");
        assert_eq!(out, "outer\n");
    }

    // -- for --

    #[test]
    fn for_basic() {
        let mut sh = shell();
        let (out, _) = sh.run_script("for x in a b c; do echo $x; done");
        assert_eq!(out, "a\n\nb\n\nc\n");
    }

    #[test]
    fn for_variable_expansion() {
        let mut sh = shell();
        sh.run_line("export ITEMS=\"hello\"");
        let (out, _) = sh.run_script("for x in $ITEMS world; do echo $x; done");
        assert_eq!(out, "hello\n\nworld\n");
    }

    #[test]
    fn for_glob_expansion() {
        let mut sh = shell();
        sh.run_line("touch a.txt b.txt c.log");
        let (out, _) = sh.run_script("for f in *.txt; do echo $f; done");
        assert_eq!(out, "a.txt\n\nb.txt\n");
    }

    #[test]
    fn for_with_break() {
        let mut sh = shell();
        let (out, _) =
            sh.run_script("for x in a b c d; do\n  if [ $x = c ]; then break; fi\n  echo $x\ndone");
        assert_eq!(out, "a\n\nb\n");
    }

    #[test]
    fn for_with_continue() {
        let mut sh = shell();
        let (out, _) = sh.run_script(
            "for x in a b c d; do\n  if [ $x = b ]; then continue; fi\n  echo $x\ndone",
        );
        assert_eq!(out, "a\n\nc\n\nd\n");
    }

    // -- while --

    #[test]
    fn while_with_test() {
        let mut sh = shell();
        // Use a file as a counter: create it, loop removes it to stop
        sh.run_line("touch /tmp/flag");
        let (out, _) =
            sh.run_script("while [ -f /tmp/flag ]; do\n  echo looping\n  rm /tmp/flag\ndone");
        assert_eq!(out, "looping\n");
    }

    #[test]
    fn while_false_never_runs() {
        let mut sh = shell();
        let (out, _) = sh.run_script("while false; do echo never; done");
        assert_eq!(out, "");
    }

    // -- until --

    #[test]
    fn until_basic() {
        let mut sh = shell();
        sh.run_line("touch /tmp/flag");
        let (out, _) =
            sh.run_script("until [ ! -f /tmp/flag ]; do\n  echo waiting\n  rm /tmp/flag\ndone");
        assert_eq!(out, "waiting\n");
    }

    // -- functions --

    #[test]
    fn function_def_stored() {
        let mut sh = shell();
        let (_, code) = sh.run_script("greet() { echo hello; }");
        assert_eq!(code, 0);
        assert!(sh.defs.functions.contains_key("greet"));
    }

    #[test]
    fn function_call_simple() {
        let mut sh = shell();
        let (out, _) = sh.run_script("greet() { echo hello; }\ngreet");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn function_with_args() {
        let mut sh = shell();
        let (out, _) = sh.run_script("greet() { echo Hello, $1!; }\ngreet world");
        assert_eq!(out, "Hello, world!\n");
    }

    #[test]
    fn function_arg_count() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { echo $#; }\nf a b c");
        assert_eq!(out, "3\n");
    }

    #[test]
    fn function_all_args() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { echo $@; }\nf x y z");
        assert_eq!(out, "x y z\n");
    }

    #[test]
    fn function_params_restored() {
        let mut sh = shell();
        // Set outer positional params, call function, verify they're restored
        sh.vars.env.insert("1".into(), "outer".to_string());
        sh.vars.env.insert("#".into(), "1".to_string());
        let (out, _) = sh.run_script("f() { echo $1; }\nf inner\necho $1");
        assert_eq!(out, "inner\n\nouter\n");
    }

    #[test]
    fn function_with_return() {
        let mut sh = shell();
        let (out, code) = sh.run_script("f() { echo before; return 42; echo after; }\nf");
        assert_eq!(out, "before\n");
        assert_eq!(code, 42);
    }

    #[test]
    fn function_with_control_flow() {
        let mut sh = shell();
        let (out, _) = sh.run_script("count() { for x in $@; do echo $x; done; }\ncount a b c");
        assert_eq!(out, "a\n\nb\n\nc\n");
    }

    #[test]
    fn function_keyword_syntax() {
        let mut sh = shell();
        let (out, _) = sh.run_script("function greet { echo hi; }\ngreet");
        assert_eq!(out, "hi\n");
    }

    #[test]
    fn function_recursive() {
        let mut sh = shell();
        let (out, _) = sh.run_script(
            "countdown() {\n  echo $1\n  if [ $1 = 0 ]; then return; fi\n  countdown 0\n}\ncountdown 2",
        );
        assert_eq!(out, "2\n\n0\n");
    }

    #[test]
    fn function_recursion_limit() {
        let mut sh = shell();
        let (out, code) = sh.run_script("f() { f; }\nf");
        assert!(out.contains("maximum recursion depth"), "got: {}", out);
        assert_ne!(code, 0);
    }

    #[test]
    fn function_in_pipeline() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { echo hello world; }\nf | wc -w");
        assert_eq!(out, "  2\n");
    }

    #[test]
    fn shift_basic() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { shift; echo $@; }\nf a b c");
        assert_eq!(out, "b c\n");
    }

    #[test]
    fn shift_n() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { shift 2; echo $@; }\nf a b c d");
        assert_eq!(out, "c d\n");
    }

    #[test]
    fn type_reports_function() {
        let mut sh = shell();
        let (out, _) = sh.run_script("greet() { echo hi; }\ntype greet");
        assert_eq!(out, "greet is a function\n");
    }

    #[test]
    fn which_reports_function() {
        let mut sh = shell();
        let (out, code) = sh.run_script("greet() { echo hi; }\nwhich greet");
        assert_eq!(out, "/bin/greet\n");
        assert_eq!(code, 0);
    }

    // -- PATH discovery --

    /// Helper: install a script into PATH as root, then switch back to user.
    fn install_path_script(sh: &mut crate::shell::Shell, name: &str, content: &[u8]) {
        sh.fs.set_current_user(0, 0);
        let _ = sh.fs.create_dir_all("/usr/local/bin");
        let result = sh
            .fs
            .write_with_mode(&format!("/usr/local/bin/{}", name), content, 0o755);
        assert!(
            result.is_ok(),
            "install_path_script write failed: {:?}",
            result.err()
        );
        sh.fs.set_current_user(1000, 1000);
    }

    #[test]
    fn path_command_found_and_executed() {
        let mut sh = shell();
        install_path_script(&mut sh, "greet", b"echo Hello from PATH!");
        let (out, code, _) = sh.run_line("greet");
        assert_eq!(out, "Hello from PATH!\n");
        assert_eq!(code, 0);
    }

    #[test]
    fn path_command_with_args() {
        let mut sh = shell();
        install_path_script(&mut sh, "say", b"echo $1 $2");
        let (out, _, _) = sh.run_line("say hello world");
        assert_eq!(out, "hello world\n");
    }

    #[test]
    fn path_command_not_found() {
        let mut sh = shell();
        let (out, code, _) = sh.run_line("nonexistent");
        assert!(out.contains("command not found"));
        assert_eq!(code, 127);
    }

    #[test]
    fn path_not_executable_skipped() {
        let mut sh = shell();
        // File exists but no execute permission (mode 644)
        sh.fs.set_current_user(0, 0);
        let _ = sh.fs.create_dir_all("/usr/local/bin");
        let result = sh.fs.write("/usr/local/bin/noexec", b"echo should not run");
        assert!(result.is_ok(), "write failed: {:?}", result.err());
        sh.fs.set_current_user(1000, 1000);
        let (out, code, _) = sh.run_line("noexec");
        assert!(out.contains("command not found"));
        assert_eq!(code, 127);
    }

    #[test]
    fn which_reports_path_command() {
        let mut sh = shell();
        install_path_script(&mut sh, "mytool", b"echo hi");
        let (out, code, _) = sh.run_line("which mytool");
        assert_eq!(out, "/usr/local/bin/mytool\n");
        assert_eq!(code, 0);
    }

    #[test]
    fn type_reports_path_command() {
        let mut sh = shell();
        install_path_script(&mut sh, "mytool", b"echo hi");
        let (out, _, _) = sh.run_line("type mytool");
        assert_eq!(out, "mytool is /usr/local/bin/mytool\n");
    }

    #[test]
    fn path_script_with_control_flow() {
        let mut sh = shell();
        install_path_script(&mut sh, "count", b"for x in $@; do\n  echo $x\ndone");
        let (out, _, _) = sh.run_line("count a b c");
        assert_eq!(out, "a\n\nb\n\nc\n");
    }

    // -- command substitution --

    #[test]
    fn cmd_subst_basic() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo hello $(echo world)");
        assert_eq!(out, "hello world\n");
    }

    #[test]
    fn cmd_subst_in_argument() {
        let mut sh = shell();
        sh.run_line("echo content > /tmp/file.txt");
        let (out, _, _) = sh.run_line("echo file contains $(cat /tmp/file.txt)");
        assert_eq!(out, "file contains content\n");
    }

    #[test]
    fn cmd_subst_nested() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo $(echo $(echo deep))");
        assert_eq!(out, "deep\n");
    }

    #[test]
    fn cmd_subst_backtick() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo hello `echo world`");
        assert_eq!(out, "hello world\n");
    }

    #[test]
    fn cmd_subst_in_for_loop() {
        let mut sh = shell();
        sh.run_line("touch a.txt b.txt c.log");
        let (out, _) = sh.run_script("for f in $(ls *.txt); do echo $f; done");
        assert_eq!(out, "a.txt\n\nb.txt\n");
    }

    #[test]
    fn cmd_subst_strips_trailing_newlines() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo -$(echo hello)-");
        assert_eq!(out, "-hello-\n");
    }

    #[test]
    fn cmd_subst_as_command_name() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("$(echo echo) hello");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn cmd_subst_in_variable_assignment() {
        let mut sh = shell();
        sh.run_line("export NAME=$(whoami)");
        let (out, _, _) = sh.run_line("echo $NAME");
        assert_eq!(out, "user\n");
    }

    // -- arithmetic --

    #[test]
    fn arith_expansion_basic() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo $((2 + 3))");
        assert_eq!(out, "5\n");
    }

    #[test]
    fn arith_expansion_with_vars() {
        let mut sh = shell();
        sh.run_line("export x=10");
        let (out, _, _) = sh.run_line("echo $((x * 3))");
        assert_eq!(out, "30\n");
    }

    #[test]
    fn arith_expansion_nested_in_string() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo result=$((5 + 5))!");
        assert_eq!(out, "result=10!\n");
    }

    #[test]
    fn arith_command_true() {
        let mut sh = shell();
        let (_, code, _) = sh.run_line("(( 5 > 3 ))");
        assert_eq!(code, 0);
    }

    #[test]
    fn arith_command_false() {
        let mut sh = shell();
        let (_, code, _) = sh.run_line("(( 3 > 5 ))");
        assert_eq!(code, 1);
    }

    #[test]
    fn arith_command_assignment() {
        let mut sh = shell();
        sh.run_line("(( x = 42 ))");
        let (out, _, _) = sh.run_line("echo $x");
        assert_eq!(out, "42\n");
    }

    #[test]
    fn arith_command_increment() {
        let mut sh = shell();
        sh.run_line("export i=0");
        sh.run_line("(( i += 1 ))");
        let (out, _, _) = sh.run_line("echo $i");
        assert_eq!(out, "1\n");
    }

    #[test]
    fn arith_in_if_condition() {
        let mut sh = shell();
        sh.run_line("export x=5");
        let (out, _) = sh.run_script("if (( x > 3 )); then echo big; else echo small; fi");
        assert_eq!(out, "big\n");
    }

    // -- heredocs --

    #[test]
    fn heredoc_basic() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<EOF\nhello world\nEOF");
        assert_eq!(out, "hello world\n");
    }

    #[test]
    fn heredoc_multiline() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<END\nline 1\nline 2\nline 3\nEND");
        assert_eq!(out, "line 1\nline 2\nline 3\n");
    }

    #[test]
    fn heredoc_variable_expansion() {
        let mut sh = shell();
        sh.run_line("export NAME=Alice");
        let (out, _) = sh.run_script("cat <<EOF\nHello, $NAME!\nEOF");
        assert_eq!(out, "Hello, Alice!\n");
    }

    #[test]
    fn heredoc_quoted_no_expansion() {
        let mut sh = shell();
        sh.run_line("export NAME=Alice");
        let (out, _) = sh.run_script("cat <<'EOF'\nHello, $NAME!\nEOF");
        assert_eq!(out, "Hello, $NAME!\n");
    }

    #[test]
    fn heredoc_with_pipe() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<EOF | wc -l\nalpha\nbeta\ngamma\nEOF");
        assert_eq!(out, "  3\n");
    }

    #[test]
    fn heredoc_tab_stripping() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<-EOF\n\t\thello\n\t\tworld\nEOF");
        assert_eq!(out, "hello\nworld\n");
    }

    #[test]
    fn heredoc_to_grep() {
        let mut sh = shell();
        let (out, _) = sh.run_script("grep -c foo <<EOF\nhello\nfoo bar\nbaz\nEOF");
        assert_eq!(out, "1\n");
    }

    #[test]
    fn heredoc_empty_body() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<EOF\nEOF");
        assert_eq!(out, "");
    }

    // -- case/esac --

    #[test]
    fn case_basic() {
        let mut sh = shell();
        let (out, _) =
            sh.run_script("case hello in\n  hello) echo matched;;\n  *) echo default;;\nesac");
        assert_eq!(out, "matched\n");
    }

    #[test]
    fn case_wildcard() {
        let mut sh = shell();
        let (out, _) = sh.run_script("case xyz in\n  abc) echo 1;;\n  *) echo default;;\nesac");
        assert_eq!(out, "default\n");
    }

    #[test]
    fn case_glob_pattern() {
        let mut sh = shell();
        let (out, _) = sh.run_script(
            "case hello.txt in\n  *.txt) echo text;;\n  *.log) echo log;;\n  *) echo other;;\nesac",
        );
        assert_eq!(out, "text\n");
    }

    #[test]
    fn case_pipe_alternatives() {
        let mut sh = shell();
        let (out, _) = sh
            .run_script("case yes in\n  y|yes) echo affirmative;;\n  n|no) echo negative;;\nesac");
        assert_eq!(out, "affirmative\n");
    }

    #[test]
    fn case_with_variable() {
        let mut sh = shell();
        sh.run_line("export EXT=log");
        let (out, _) =
            sh.run_script("case $EXT in\n  txt) echo text;;\n  log) echo logfile;;\nesac");
        assert_eq!(out, "logfile\n");
    }

    #[test]
    fn case_no_match() {
        let mut sh = shell();
        let (out, code) = sh.run_script("case nope in\n  a) echo a;;\n  b) echo b;;\nesac");
        assert_eq!(out, "");
        assert_eq!(code, 0);
    }

    // -- local --

    #[test]
    fn local_basic() {
        let mut sh = shell();
        sh.run_line("export x=outer");
        let (out, _) = sh.run_script("f() { local x=inner; echo $x; }\nf\necho $x");
        assert_eq!(out, "inner\n\nouter\n");
    }

    #[test]
    fn local_without_value() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { local y; echo ${y:-empty}; }\nf");
        assert_eq!(out, "empty\n");
    }

    // -- read --

    #[test]
    fn read_from_pipe() {
        let mut sh = shell();
        let (_, _, _) = sh.run_line("echo hello | read word");
        // read sets variable, but in a pipeline subshell it doesn't persist
        // Test via heredoc which uses cat | cmd
    }

    #[test]
    fn read_splits_words() {
        let mut sh = shell();
        let (out, _) = sh.run_script("echo 'alice bob charlie' | read a b c\necho $a $b $c");
        // In a pipeline, read happens in the same shell (conch doesn't fork)
        assert_eq!(out, "alice bob charlie\n");
    }

    // -- string manipulation --

    #[test]
    fn string_length() {
        let mut sh = shell();
        sh.run_line("export word=hello");
        let (out, _, _) = sh.run_line("echo ${#word}");
        assert_eq!(out, "5\n");
    }

    #[test]
    fn string_default_value() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo ${UNSET:-fallback}");
        assert_eq!(out, "fallback\n");
    }

    #[test]
    fn string_default_when_set() {
        let mut sh = shell();
        sh.run_line("export VAR=real");
        let (out, _, _) = sh.run_line("echo ${VAR:-fallback}");
        assert_eq!(out, "real\n");
    }

    #[test]
    fn string_alt_value() {
        let mut sh = shell();
        sh.run_line("export VAR=real");
        let (out, _, _) = sh.run_line("echo ${VAR:+alt}");
        assert_eq!(out, "alt\n");
    }

    #[test]
    fn string_alt_empty() {
        let mut sh = shell();
        let (out, _, _) = sh.run_line("echo ${UNSET:+alt}");
        assert_eq!(out, "\n");
    }

    #[test]
    fn string_strip_prefix() {
        let mut sh = shell();
        sh.run_line("export path=/home/user/file.txt");
        let (out, _, _) = sh.run_line("echo ${path#*/}");
        assert_eq!(out, "home/user/file.txt\n");
    }

    #[test]
    fn string_strip_prefix_longest() {
        let mut sh = shell();
        sh.run_line("export path=/home/user/file.txt");
        let (out, _, _) = sh.run_line("echo ${path##*/}");
        assert_eq!(out, "file.txt\n");
    }

    #[test]
    fn string_strip_suffix() {
        let mut sh = shell();
        sh.run_line("export file=hello.tar.gz");
        let (out, _, _) = sh.run_line("echo ${file%.*}");
        assert_eq!(out, "hello.tar\n");
    }

    #[test]
    fn string_strip_suffix_longest() {
        let mut sh = shell();
        sh.run_line("export file=hello.tar.gz");
        let (out, _, _) = sh.run_line("echo ${file%%.*}");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn string_replace_first() {
        let mut sh = shell();
        sh.run_line("export s=hello_world_hello");
        let (out, _, _) = sh.run_line("echo ${s/hello/hi}");
        assert_eq!(out, "hi_world_hello\n");
    }

    #[test]
    fn string_replace_all() {
        let mut sh = shell();
        sh.run_line("export s=hello_world_hello");
        let (out, _, _) = sh.run_line("echo ${s//hello/hi}");
        assert_eq!(out, "hi_world_hi\n");
    }

    #[test]
    fn string_substring() {
        let mut sh = shell();
        sh.run_line("export s=hello_world");
        let (out, _, _) = sh.run_line("echo ${s:6}");
        assert_eq!(out, "world\n");
    }

    #[test]
    fn string_substring_with_length() {
        let mut sh = shell();
        sh.run_line("export s=hello_world");
        let (out, _, _) = sh.run_line("echo ${s:0:5}");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn function_takes_precedence_over_path() {
        let mut sh = shell();
        install_path_script(&mut sh, "greet", b"echo from PATH");
        let (out, _) = sh.run_script("greet() { echo from function; }\ngreet");
        assert_eq!(out, "from function\n");
    }

    // -- nested control flow --

    #[test]
    fn for_inside_if() {
        let mut sh = shell();
        let (out, _) = sh.run_script("if true; then\n  for x in a b; do echo $x; done\nfi");
        assert_eq!(out, "a\n\nb\n");
    }

    #[test]
    fn if_inside_for() {
        let mut sh = shell();
        let (out, _) =
            sh.run_script("for x in 1 2 3; do\n  if [ $x = 2 ]; then echo found; fi\ndone");
        assert_eq!(out, "found\n");
    }

    // -- script with mixed statements --

    #[test]
    fn mixed_script() {
        let mut sh = shell();
        let (out, _) = sh.run_script("echo start\nfor x in a b; do echo $x; done\necho end");
        assert_eq!(out, "start\n\na\n\nb\n\nend\n");
    }

    // -- backward compat: simple scripts still work --

    #[test]
    fn simple_script_unchanged() {
        let mut sh = shell();
        let (out, code) = sh.run_script("echo hello\necho world");
        assert_eq!(out, "hello\n\nworld\n");
        assert_eq!(code, 0);
    }

    #[test]
    fn script_with_comments() {
        let mut sh = shell();
        let (out, _) = sh.run_script("# comment\necho hello\n# another");
        assert_eq!(out, "hello\n");
    }

    #[test]
    fn script_with_pipes() {
        let mut sh = shell();
        sh.run_line("echo -e 'alpha\nbeta\ngamma' > /tmp/list");
        let (out, _) = sh.run_script("cat /tmp/list | head -2");
        assert_eq!(out, "alpha\nbeta\n");
    }

    // -- parse error --

    #[test]
    fn parse_error_reported() {
        let mut sh = shell();
        let (out, code) = sh.run_script("if true; then echo oops");
        assert!(
            out.contains("parse error") || out.contains("expected"),
            "got: {}",
            out
        );
        assert_ne!(code, 0);
    }

    // -- Fix #2: heredoc proper stdin --

    #[test]
    fn heredoc_passes_stdin_to_command() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<EOF\nhello world\nEOF");
        assert_eq!(out, "hello world\n");
    }

    #[test]
    fn heredoc_with_command_receives_stdin() {
        let mut sh = shell();
        // wc -l receives the heredoc body as stdin
        let (out, _) = sh.run_script("wc -l <<EOF\nline1\nline2\nline3\nEOF");
        assert_eq!(out, "  3\n");
    }

    #[test]
    fn heredoc_stdin_variable_expansion() {
        let mut sh = shell();
        sh.run_line("export NAME=World");
        let (out, _) = sh.run_script("cat <<EOF\nHello $NAME\nEOF");
        assert_eq!(out, "Hello World\n");
    }

    #[test]
    fn heredoc_stdin_quoted_no_expansion() {
        let mut sh = shell();
        sh.run_line("export NAME=World");
        let (out, _) = sh.run_script("cat <<'EOF'\nHello $NAME\nEOF");
        assert_eq!(out, "Hello $NAME\n");
    }

    // -- Fix #3: $@ word splitting with quote tracking --

    #[test]
    fn dollar_at_unquoted_splits() {
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { for x in $@; do echo $x; done; }\nf a b c");
        assert_eq!(out, "a\n\nb\n\nc\n");
    }

    #[test]
    fn dollar_at_quoted_not_split() {
        // "$@" starts with a quote, so has_unquoted_dollar returns false,
        // meaning the expanded value is not word-split — it stays as a
        // single iteration of the for loop.
        let mut sh = shell();
        let (out, _) = sh.run_script("f() { for x in \"$@\"; do echo $x; done; }\nf a b c");
        // "$@" is quoted: single iteration, all args together
        // The output includes the quote chars from the token.
        assert_eq!(
            out.lines().count(),
            1,
            "should be single iteration, got: {}",
            out
        );
    }

    // -- Fix #6: glob character classes --

    #[test]
    fn glob_char_class_basic() {
        assert!(crate::shell::Shell::glob_match_str("[abc]", "a"));
        assert!(crate::shell::Shell::glob_match_str("[abc]", "b"));
        assert!(!crate::shell::Shell::glob_match_str("[abc]", "d"));
    }

    #[test]
    fn glob_char_class_range() {
        assert!(crate::shell::Shell::glob_match_str("[a-z]", "m"));
        assert!(!crate::shell::Shell::glob_match_str("[a-z]", "M"));
        assert!(crate::shell::Shell::glob_match_str("[0-9]", "5"));
        assert!(!crate::shell::Shell::glob_match_str("[0-9]", "a"));
    }

    #[test]
    fn glob_char_class_negated() {
        assert!(!crate::shell::Shell::glob_match_str("[!abc]", "a"));
        assert!(crate::shell::Shell::glob_match_str("[!abc]", "d"));
        assert!(!crate::shell::Shell::glob_match_str("[!0-9]", "5"));
        assert!(crate::shell::Shell::glob_match_str("[!0-9]", "x"));
    }

    #[test]
    fn glob_char_class_in_pattern() {
        assert!(crate::shell::Shell::glob_match_str(
            "file[0-9].txt",
            "file3.txt"
        ));
        assert!(!crate::shell::Shell::glob_match_str(
            "file[0-9].txt",
            "fileA.txt"
        ));
        assert!(crate::shell::Shell::glob_match_str("*.[ch]", "main.c"));
        assert!(crate::shell::Shell::glob_match_str("*.[ch]", "main.h"));
        assert!(!crate::shell::Shell::glob_match_str("*.[ch]", "main.o"));
    }

    #[test]
    fn case_with_char_class() {
        let mut sh = shell();
        let (out, _) =
            sh.run_script("case a in\n  [a-z]) echo lower;;\n  [A-Z]) echo upper;;\nesac");
        assert_eq!(out, "lower\n");
    }

    // -- Fix #8: local outside function errors --

    #[test]
    fn local_outside_function_errors() {
        let mut sh = shell();
        let (out, code) = sh.run_script("local x=5");
        assert_ne!(code, 0);
        assert!(
            out.contains("can only be used in a function"),
            "got: {}",
            out
        );
    }

    #[test]
    fn local_inside_function_works() {
        let mut sh = shell();
        let (out, code) =
            sh.run_script("export x=outer\nf() { local x=inner; echo $x; }\nf\necho $x");
        assert_eq!(code, 0);
        assert_eq!(out, "inner\n\nouter\n");
    }

    // -- Fix #9: expand_full in heredocs and arithmetic --

    #[test]
    fn heredoc_command_substitution() {
        let mut sh = shell();
        let (out, _) = sh.run_script("cat <<EOF\nuser is $(echo admin)\nEOF");
        assert_eq!(out, "user is admin\n");
    }

    #[test]
    fn arith_command_substitution() {
        let mut sh = shell();
        let (out, _) = sh.run_script("echo $(( $(echo 2) + $(echo 3) ))");
        assert_eq!(out, "5\n");
    }

    // -- Fix #10: read with IFS --

    #[test]
    fn read_with_custom_ifs() {
        let mut sh = shell();
        sh.vars.env.insert("IFS".into(), ":".to_string());
        // Use cmd_read directly to test IFS splitting
        sh.cmd_read(&["x".into(), "y".into(), "z".into()], Some("a:b:c"));
        assert_eq!(sh.vars.env.get("x").map(|s| s.as_str()), Some("a"));
        assert_eq!(sh.vars.env.get("y").map(|s| s.as_str()), Some("b"));
        assert_eq!(sh.vars.env.get("z").map(|s| s.as_str()), Some("c"));
    }

    #[test]
    fn read_with_default_ifs() {
        let mut sh = shell();
        // Default IFS is space/tab/newline; last var gets remainder
        sh.cmd_read(&["a".into(), "b".into()], Some("hello world foo"));
        assert_eq!(sh.vars.env.get("a").map(|s| s.as_str()), Some("hello"));
        assert_eq!(sh.vars.env.get("b").map(|s| s.as_str()), Some("world foo"));
    }

    #[test]
    fn read_with_custom_ifs_remainder() {
        let mut sh = shell();
        sh.vars.env.insert("IFS".into(), ":".to_string());
        // Two vars, three fields: last var gets remainder
        sh.cmd_read(&["x".into(), "y".into()], Some("a:b:c"));
        assert_eq!(sh.vars.env.get("x").map(|s| s.as_str()), Some("a"));
        assert_eq!(sh.vars.env.get("y").map(|s| s.as_str()), Some("b:c"));
    }

    // -- Fix #5: arithmetic command detection --

    #[test]
    fn arith_command_basic() {
        let mut sh = shell();
        let (_, code) = sh.run_script("(( 1 + 1 ))");
        assert_eq!(code, 0); // non-zero result => exit 0
    }

    #[test]
    fn arith_command_zero_exits_nonzero() {
        let mut sh = shell();
        let (_, code) = sh.run_script("(( 0 ))");
        assert_ne!(code, 0); // zero result => exit 1
    }
}
