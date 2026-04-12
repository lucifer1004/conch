use std::collections::{BTreeMap, BTreeSet};

use bare_vfs::MemFs;
use globset::Glob;

use crate::script::ast::Stmt;
use crate::types::*;
use crate::userdb::UserDb;
use crate::Str;

mod arrays;
mod builtins;
mod expand;
mod heredoc;
pub(crate) mod pipeline;
mod util;

pub use util::{expand_braces, process_ansi_c_escapes, process_dollar_single_quote};
pub(crate) use util::{has_unterminated_quote, parse_mode_digits, split_shell_words};

/// Shell options controllable via `set`.
#[derive(Default, Clone)]
pub(crate) struct ShellOpts {
    pub errexit: bool,   // -e
    pub xtrace: bool,    // -x
    pub nounset: bool,   // -u
    pub pipefail: bool,  // -o pipefail
    pub noglob: bool,    // -f
    pub noclobber: bool, // -C
}

/// Shell options controllable via `shopt`.
#[derive(Default, Clone)]
pub(crate) struct ShoptOpts {
    pub nullglob: bool,
    pub failglob: bool,
    pub dotglob: bool,
}

/// Identity information for the shell's current user/host.
pub(crate) struct ShellIdent {
    pub user: Str,
    pub hostname: Str,
    pub home: Str,
    pub users: UserDb,
}

/// All variable storage: env, arrays, namerefs, locals.
pub(crate) struct VarStore {
    pub env: BTreeMap<Str, String>,
    pub arrays: BTreeMap<Str, Vec<String>>,
    pub assoc_arrays: BTreeMap<Str, BTreeMap<String, String>>,
    pub namerefs: BTreeMap<Str, Str>,
    pub readonly: BTreeSet<Str>,
    pub local_frames: Vec<Vec<(Str, Option<String>)>>,
}

/// User-defined functions, aliases, and traps.
pub(crate) struct ShellDefs {
    pub functions: BTreeMap<Str, Vec<Stmt>>,
    pub aliases: BTreeMap<Str, String>,
    pub traps: BTreeMap<Str, String>,
}

/// Process status for the job table.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) enum ProcessStatus {
    Running,
    Exited(i32),
}

/// A recorded process (background job).
#[derive(Clone, Debug)]
pub(crate) struct Process {
    pub pid: u32,
    #[allow(dead_code)]
    pub ppid: u32,
    pub cmd: String,
    pub status: ProcessStatus,
}

/// PID allocator and job table.
#[derive(Clone)]
pub(crate) struct ProcessTable {
    next_pid: u32,
    /// The shell's own PID (stable for the lifetime of the shell).
    shell_pid: u32,
    /// The shell's parent PID (`$PPID`). Updated when entering a subshell.
    parent_pid: u32,
    /// PID of the currently executing command (changes per pipeline segment).
    current_pid: u32,
    /// PID of the most recent background job (`$!`).
    last_bg_pid: Option<u32>,
    /// Background job list.
    pub jobs: Vec<Process>,
}

impl ProcessTable {
    pub fn new() -> Self {
        let shell_pid = 2; // PID 1 = init; shell starts at 2
        Self {
            next_pid: shell_pid + 1,
            shell_pid,
            parent_pid: 1, // top-level shell's parent is init
            current_pid: shell_pid,
            last_bg_pid: None,
            jobs: Vec::new(),
        }
    }

    /// The shell's own PID (`$$`).
    pub fn shell_pid(&self) -> u32 {
        self.shell_pid
    }

    /// The shell's parent PID (`$PPID`).
    pub fn parent_pid(&self) -> u32 {
        self.parent_pid
    }

    /// PID of the currently executing command (`$BASHPID`).
    pub fn current_pid(&self) -> u32 {
        self.current_pid
    }

    /// PID of the most recent background job (`$!`).
    pub fn last_bg_pid(&self) -> Option<u32> {
        self.last_bg_pid
    }

    /// Allocate a new PID for a foreground command.
    pub fn spawn(&mut self, _cmd: &str) -> u32 {
        let pid = self.next_pid;
        self.next_pid = self.next_pid.wrapping_add(1);
        self.current_pid = pid;
        pid
    }

    /// Allocate a new PID without setting current_pid (for pre-allocation).
    pub fn alloc_pid(&mut self) -> u32 {
        let pid = self.next_pid;
        self.next_pid = self.next_pid.wrapping_add(1);
        pid
    }

    /// Record a pre-allocated PID as a background job and set `$!`.
    pub fn record_bg(&mut self, pid: u32, cmd: &str, exit_code: i32) {
        self.last_bg_pid = Some(pid);
        self.jobs.push(Process {
            pid,
            ppid: self.shell_pid,
            cmd: cmd.to_string(),
            status: ProcessStatus::Exited(exit_code),
        });
    }

    /// Record a pre-allocated PID as a running background job and set `$!`.
    pub fn record_bg_running(&mut self, pid: u32, cmd: &str) {
        self.last_bg_pid = Some(pid);
        self.jobs.push(Process {
            pid,
            ppid: self.shell_pid,
            cmd: cmd.to_string(),
            status: ProcessStatus::Running,
        });
    }

    /// Mark a job as exited by PID.
    pub fn finish_job(&mut self, pid: u32, exit_code: i32) {
        if let Some(proc) = self.jobs.iter_mut().find(|p| p.pid == pid) {
            proc.status = ProcessStatus::Exited(exit_code);
        }
    }

    /// Set current_pid explicitly (used for background pre-allocation).
    pub fn set_current(&mut self, pid: u32) {
        self.current_pid = pid;
    }

    /// Reset current_pid back to the shell's own PID.
    pub fn reset_current(&mut self) {
        self.current_pid = self.shell_pid;
    }

    /// Remove completed jobs, keeping the most recent `keep` entries.
    pub fn prune_done_jobs(&mut self, keep: usize) {
        if self.jobs.len() <= keep {
            return;
        }
        // Keep the last `keep` jobs; remove older completed ones.
        let excess = self.jobs.len() - keep;
        let mut removed = 0;
        self.jobs.retain(|p| {
            if removed >= excess {
                return true;
            }
            if matches!(p.status, ProcessStatus::Exited(_)) {
                removed += 1;
                false
            } else {
                true // keep Running jobs
            }
        });
    }

    /// Enter a subshell context: child's PPID = parent's shell_pid.
    pub fn enter_subshell(&mut self) {
        self.parent_pid = self.shell_pid;
    }

    /// Snapshot for subshell isolation.
    pub fn snapshot(&self) -> ProcessTableSnapshot {
        ProcessTableSnapshot {
            current_pid: self.current_pid,
            last_bg_pid: self.last_bg_pid,
            jobs_len: self.jobs.len(),
            parent_pid: self.parent_pid,
        }
    }

    /// Restore from snapshot.
    pub fn restore(&mut self, snap: ProcessTableSnapshot) {
        self.current_pid = snap.current_pid;
        self.last_bg_pid = snap.last_bg_pid;
        self.jobs.truncate(snap.jobs_len);
        self.parent_pid = snap.parent_pid;
    }
}

/// Snapshot for subshell isolation.
#[derive(Clone)]
pub(crate) struct ProcessTableSnapshot {
    pub current_pid: u32,
    pub last_bg_pid: Option<u32>,
    pub jobs_len: usize,
    pub parent_pid: u32,
}

/// Execution state: exit code, depth counters, shell options.
pub(crate) struct ExecState {
    pub last_exit_code: i32,
    pub call_depth: u32,
    pub in_condition: u32,
    pub opts: ShellOpts,
    /// Set by `exec` builtin to stop script execution after the command.
    pub exec_pending: bool,
    /// Current alias expansion depth (guards against indirect recursion).
    pub alias_depth: u32,
}

// ---------------------------------------------------------------------------
// Sub-struct methods
// ---------------------------------------------------------------------------

impl VarStore {
    /// Look up a variable, resolving namerefs.
    pub fn get(&self, name: &str) -> Option<&str> {
        let resolved = self.resolve_nameref(name);
        self.env.get(resolved.as_str()).map(|s| s.as_str())
    }

    /// Set a variable, checking identifier validity and readonly.
    pub fn set(&mut self, name: &str, val: String) -> Result<(), String> {
        if !is_valid_identifier(name) {
            return Err(format!("conch: `{}': not a valid identifier", name));
        }
        if self.readonly.contains(name) {
            return Err(format!("conch: {}: readonly variable", name));
        }
        let resolved = self.resolve_nameref(name);
        if resolved != name && self.readonly.contains(resolved.as_str()) {
            return Err(format!("conch: {}: readonly variable", resolved));
        }
        self.env.insert(resolved, val);
        Ok(())
    }

    /// Remove a variable, checking readonly. Cleans up all stores.
    pub fn unset(&mut self, name: &str) -> Result<(), String> {
        if self.readonly.contains(name) {
            return Err(format!("conch: unset: {}: readonly variable", name));
        }
        self.env.remove(name);
        self.arrays.remove(name);
        self.assoc_arrays.remove(name);
        self.namerefs.remove(name);
        Ok(())
    }

    /// Declare a local variable, saving the current value for restoration.
    /// Returns Err if the name is not a valid identifier.
    pub fn declare_local(&mut self, name: &str, val: Option<String>) -> Result<(), String> {
        if !is_valid_identifier(name) {
            return Err(format!(
                "conch: declare: `{}': not a valid identifier",
                name
            ));
        }
        if let Some(frame) = self.local_frames.last_mut() {
            let prev = self.env.get(name).cloned();
            frame.push((name.into(), prev));
        }
        match val {
            Some(v) => {
                self.env.insert(name.into(), v);
            }
            None => {
                self.env.entry(name.into()).or_default();
            }
        }
        Ok(())
    }

    /// Resolve nameref chain to find the actual variable name.
    pub fn resolve_nameref(&self, name: &str) -> Str {
        let mut current = Str::from(name);
        let mut depth = 0;
        while let Some(target) = self.namerefs.get(current.as_str()) {
            current = target.clone();
            depth += 1;
            if depth > 10 {
                break;
            }
        }
        current
    }

    /// Push a new local variable frame (for function calls).
    pub fn push_locals(&mut self) {
        self.local_frames.push(Vec::new());
    }

    /// Pop and restore local variables from the current frame.
    pub fn pop_locals(&mut self) {
        if let Some(frame) = self.local_frames.pop() {
            for (name, prev_val) in frame {
                match prev_val {
                    Some(v) => {
                        self.env.insert(name, v);
                    }
                    None => {
                        self.env.remove(name.as_str());
                    }
                }
            }
        }
    }
}

/// Complete snapshot of VarStore for isolation (subshell, bash, exec, cmdsubst).
#[derive(Clone)]
pub(crate) struct VarSnapshot {
    pub env: BTreeMap<Str, String>,
    pub arrays: BTreeMap<Str, Vec<String>>,
    pub assoc_arrays: BTreeMap<Str, BTreeMap<String, String>>,
    pub namerefs: BTreeMap<Str, Str>,
    pub readonly: BTreeSet<Str>,
    pub local_frames: Vec<Vec<(Str, Option<String>)>>,
}

impl VarStore {
    /// Take a full snapshot for isolation.
    pub fn snapshot(&self) -> VarSnapshot {
        VarSnapshot {
            env: self.env.clone(),
            arrays: self.arrays.clone(),
            assoc_arrays: self.assoc_arrays.clone(),
            namerefs: self.namerefs.clone(),
            readonly: self.readonly.clone(),
            local_frames: self.local_frames.clone(),
        }
    }

    /// Restore from a snapshot.
    pub fn restore(&mut self, snap: VarSnapshot) {
        self.env = snap.env;
        self.arrays = snap.arrays;
        self.assoc_arrays = snap.assoc_arrays;
        self.namerefs = snap.namerefs;
        self.readonly = snap.readonly;
        self.local_frames = snap.local_frames;
    }
}

/// Complete snapshot of ShellDefs for isolation.
#[derive(Clone)]
pub(crate) struct DefsSnapshot {
    pub functions: BTreeMap<Str, Vec<Stmt>>,
    pub aliases: BTreeMap<Str, String>,
    pub traps: BTreeMap<Str, String>,
}

/// Complete snapshot of ExecState for isolation.
#[derive(Clone)]
pub(crate) struct ExecSnapshot {
    pub last_exit_code: i32,
    pub call_depth: u32,
    pub in_condition: u32,
    pub opts: ShellOpts,
    pub exec_pending: bool,
    pub alias_depth: u32,
}

impl ExecState {
    pub fn snapshot(&self) -> ExecSnapshot {
        ExecSnapshot {
            last_exit_code: self.last_exit_code,
            call_depth: self.call_depth,
            in_condition: self.in_condition,
            opts: self.opts.clone(),
            exec_pending: self.exec_pending,
            alias_depth: self.alias_depth,
        }
    }

    pub fn restore(&mut self, snap: ExecSnapshot) {
        self.last_exit_code = snap.last_exit_code;
        self.call_depth = snap.call_depth;
        self.in_condition = snap.in_condition;
        self.opts = snap.opts;
        self.exec_pending = snap.exec_pending;
        self.alias_depth = snap.alias_depth;
    }
}

/// Complete subshell snapshot — ALL mutable state except filesystem.
pub(crate) struct SubshellSnapshot {
    pub vars: VarSnapshot,
    pub defs: DefsSnapshot,
    pub exec: ExecSnapshot,
    pub procs: ProcessTableSnapshot,
    pub cwd: Str,
    pub positional: Vec<(String, Option<String>)>,
}

impl ShellDefs {
    pub fn snapshot(&self) -> DefsSnapshot {
        DefsSnapshot {
            functions: self.functions.clone(),
            aliases: self.aliases.clone(),
            traps: self.traps.clone(),
        }
    }

    pub fn restore(&mut self, snap: DefsSnapshot) {
        self.functions = snap.functions;
        self.aliases = snap.aliases;
        self.traps = snap.traps;
    }
}

impl ShellDefs {
    /// Check if a function is defined.
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Check if an alias is defined.
    pub fn get_alias(&self, name: &str) -> Option<&str> {
        self.aliases.get(name).map(|s| s.as_str())
    }
}

impl Shell {
    /// Look up a variable by name (resolves namerefs).
    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars.get(name)
    }

    /// Snapshot ALL mutable state for subshell isolation.
    /// Used by: bash, ./exec, PATH scripts, subshell (), command substitution $().
    pub(crate) fn snapshot_subshell(&self) -> SubshellSnapshot {
        SubshellSnapshot {
            vars: self.vars.snapshot(),
            defs: self.defs.snapshot(),
            exec: self.exec.snapshot(),
            procs: self.procs.snapshot(),
            cwd: self.cwd.clone(),
            positional: self.save_positional_params(),
        }
    }

    /// Restore ALL state from a subshell snapshot.
    pub(crate) fn restore_subshell(&mut self, snap: SubshellSnapshot) {
        self.vars.restore(snap.vars);
        self.defs.restore(snap.defs);
        self.exec.restore(snap.exec);
        self.procs.restore(snap.procs);
        self.cwd = snap.cwd;
        self.restore_positional_params(snap.positional);
    }
}

/// Check if a string is a valid shell identifier (for variable names).
/// Parse a date string like "Sat Apr 12 00:00:00 UTC 2026" into Unix epoch seconds.
fn parse_date_to_epoch(date: &str) -> Option<u64> {
    let parts: Vec<&str> = date.split_whitespace().collect();
    // Expected: [weekday, month, day, time, timezone, year]
    if parts.len() < 6 {
        return None;
    }
    let month = match parts[1] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let day: u64 = parts[2].parse().ok()?;
    let year: u64 = parts[5].parse().ok()?;

    // Parse time HH:MM:SS
    let time_parts: Vec<&str> = parts[3].split(':').collect();
    let hours: u64 = time_parts.first()?.parse().ok()?;
    let minutes: u64 = time_parts.get(1)?.parse().ok()?;
    let seconds: u64 = time_parts.get(2)?.parse().ok()?;

    // Days from epoch (1970-01-01) to year-01-01, accounting for leap years
    let mut total_days: u64 = 0;
    for y in 1970..year {
        total_days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
    }
    // Days in preceding months of the target year
    let is_leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for days in days_in_month.iter().take((month - 1) as usize) {
        total_days += days;
    }
    total_days += day - 1; // day is 1-based

    Some(total_days * 86400 + hours * 3600 + minutes * 60 + seconds)
}

/// Convert VFS ticks to (year, month, day, hour, minute, second).
/// Ticks = epoch_seconds * TICKS_PER_SECOND.
pub(crate) fn ticks_to_datetime(ticks: u64) -> (u64, u8, u8, u8, u8, u8) {
    let epoch_secs = ticks / crate::shell::pipeline::TICKS_PER_SECOND;
    let secs_of_day = epoch_secs % 86400;
    let h = (secs_of_day / 3600) as u8;
    let m = ((secs_of_day % 3600) / 60) as u8;
    let s = (secs_of_day % 60) as u8;

    let mut remaining_days = epoch_secs / 86400;
    let mut year: u64 = 1970;
    loop {
        let days_in_year: u64 =
            if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
                366
            } else {
                365
            };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let is_leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days_in_month: [u64; 12] = [
        31,
        if is_leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month: u8 = 1;
    for &dim in &days_in_month {
        if remaining_days < dim {
            break;
        }
        remaining_days -= dim;
        month += 1;
    }
    let day = (remaining_days + 1) as u8;

    (year, month, day, h, m, s)
}

/// Compute day-of-week (0=Sun) for a given date using Zeller-like algorithm.
fn day_of_week(year: u64, month: u8, day: u8) -> u8 {
    // Tomohiko Sakamoto's algorithm
    let y = if month < 3 { year - 1 } else { year };
    let m = month as u64;
    let d = day as u64;
    let t: [u64; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let idx = (m as usize).wrapping_sub(1);
    let v = (y + y / 4 - y / 100 + y / 400 + t[idx] + d) % 7;
    v as u8
}

const WEEKDAY_SHORT: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAY_LONG: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTH_SHORT: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTH_LONG: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Format ticks as "YYYY-MM-DD HH:MM:SS.NNNNNNNNN +0000" (stat format).
pub(crate) fn format_timestamp(ticks: u64) -> String {
    let (y, mo, d, h, mi, s) = ticks_to_datetime(ticks);
    // sub-second part: ticks mod TICKS_PER_SECOND, scaled to nanoseconds
    let sub_ticks = ticks % crate::shell::pipeline::TICKS_PER_SECOND;
    // TICKS_PER_SECOND=1000, so each tick = 1ms = 1_000_000 ns
    let nanos = sub_ticks * 1_000_000;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:09} +0000",
        y, mo, d, h, mi, s, nanos
    )
}

/// Format ticks using date +FORMAT specifiers.
pub(crate) fn format_date_str(ticks: u64, format: &str) -> String {
    let (y, mo, d, h, mi, s) = ticks_to_datetime(ticks);
    let epoch_secs = ticks / crate::shell::pipeline::TICKS_PER_SECOND;
    let dow = day_of_week(y, mo, d);

    let mut out = String::new();
    let chars: Vec<char> = format.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' && i + 1 < chars.len() {
            i += 1;
            match chars[i] {
                'Y' => out.push_str(&format!("{:04}", y)),
                'y' => out.push_str(&format!("{:02}", y % 100)),
                'm' => out.push_str(&format!("{:02}", mo)),
                'd' => out.push_str(&format!("{:02}", d)),
                'H' => out.push_str(&format!("{:02}", h)),
                'M' => out.push_str(&format!("{:02}", mi)),
                'S' => out.push_str(&format!("{:02}", s)),
                's' => out.push_str(&format!("{}", epoch_secs)),
                'A' => out.push_str(WEEKDAY_LONG.get(dow as usize).unwrap_or(&"Sunday")),
                'a' => out.push_str(WEEKDAY_SHORT.get(dow as usize).unwrap_or(&"Sun")),
                'B' => out.push_str(
                    MONTH_LONG
                        .get((mo as usize).wrapping_sub(1))
                        .unwrap_or(&"January"),
                ),
                'b' => out.push_str(
                    MONTH_SHORT
                        .get((mo as usize).wrapping_sub(1))
                        .unwrap_or(&"Jan"),
                ),
                'T' => out.push_str(&format!("{:02}:{:02}:{:02}", h, mi, s)),
                'D' => out.push_str(&format!("{:02}/{:02}/{:02}", mo, d, y % 100)),
                'F' => out.push_str(&format!("{:04}-{:02}-{:02}", y, mo, d)),
                'n' => out.push('\n'),
                't' => out.push('\t'),
                '%' => out.push('%'),
                other => {
                    out.push('%');
                    out.push(other);
                }
            }
        } else {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

pub(crate) fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Virtual shell state
pub struct Shell {
    // Hot path — accessed by nearly every command
    pub(crate) fs: MemFs,
    pub(crate) cwd: Str,
    pub(crate) tmp_counter: u64,
    pub(crate) history: Vec<String>,

    /// Whether commands emit ANSI escape codes. Defaults to `true`; set to
    /// `false` in tests so output is plain text.
    pub color: bool,
    /// Language hint from the last executed command (e.g. `cat main.rs` → `Some("rust")`).
    /// Read by lib.rs to attach syntax highlighting info to OutputEntry.
    pub last_lang: Option<String>,

    // Grouped by domain
    pub(crate) ident: ShellIdent,
    pub(crate) vars: VarStore,
    pub(crate) defs: ShellDefs,
    pub(crate) exec: ExecState,
    pub(crate) procs: ProcessTable,

    // Background execution
    pub(crate) bg_mode: crate::types::BackgroundMode,
    pub(crate) bg_jobs: Vec<pipeline::BackgroundJob>,
    /// Completions from bg jobs that finished during the last foreground command.
    /// Drained by lib.rs into OutputEntry.bg_completions.
    pub(crate) pending_bg_completions: Vec<String>,

    /// Deferred >(cmd) process substitutions: (temp_file_path, cmd_to_run).
    /// After a pipeline completes, each deferred cmd is run with the file content as stdin.
    pub(crate) deferred_process_substs: Vec<(String, String)>,

    /// Directory stack for pushd/popd/dirs.
    pub(crate) dir_stack: Vec<String>,

    /// VFS clock tick at shell start — used to compute $SECONDS.
    pub(crate) start_time: u64,

    /// Options controllable via `shopt`.
    pub(crate) shopt: ShoptOpts,
}

impl Shell {
    pub fn new(config: &Config) -> Self {
        let mut fs = MemFs::new();
        let mut users_db = UserDb::new();

        // Resolve system config from either new `system` field or legacy flat fields
        let hostname: String;
        let home: String;

        if let Some(ref sys) = config.system {
            hostname = sys.hostname.clone();
            // Find main user's home from user specs, or default
            let main_spec = sys.users.iter().find(|u| u.name == config.user);
            home = main_spec
                .and_then(|s| s.home.clone())
                .unwrap_or_else(|| format!("/home/{}", config.user));
        } else {
            // Legacy mode
            hostname = config
                .hostname
                .clone()
                .unwrap_or_else(|| "conch".to_string());
            home = config
                .home
                .clone()
                .unwrap_or_else(|| format!("/home/{}", config.user));
        }

        // 1. Create root
        users_db.add_root();

        // 2. Create main user
        let main_uid;
        let main_gid;
        if let Some(ref sys) = config.system {
            let main_spec = sys.users.iter().find(|u| u.name == config.user);
            main_uid = main_spec.and_then(|s| s.uid).unwrap_or(1000);
            main_gid = main_uid;
        } else {
            main_uid = 1000;
            main_gid = 1000;
        }
        users_db.add_user_with_ids(&config.user, main_uid, main_gid, &home);

        // 3. Create additional users and groups from system spec
        if let Some(ref sys) = config.system {
            for spec in &sys.users {
                if spec.name == config.user {
                    continue;
                }
                let uid = spec.uid.unwrap_or_else(|| users_db.next_available_uid());
                let user_home = spec
                    .home
                    .clone()
                    .unwrap_or_else(|| format!("/home/{}", spec.name));
                users_db.add_user_with_ids(&spec.name, uid, uid, &user_home);
                let _ = fs.create_dir_all(&user_home);
                fs.chown(&user_home, uid, uid).unwrap_or(());
            }

            // 4. Create groups from group specs
            for spec in &sys.groups {
                if let Some(g) = spec.gid {
                    users_db.add_group_with_id(&spec.name, g);
                } else {
                    users_db.add_group(&spec.name);
                }
                for member in &spec.members {
                    users_db.add_user_to_group(member, &spec.name).ok();
                }
            }

            // 5. Add users to groups specified in user specs
            for spec in &sys.users {
                for group_name in &spec.groups {
                    if users_db.get_group_by_name(group_name).is_none() {
                        users_db.add_group(group_name);
                    }
                    users_db.add_user_to_group(&spec.name, group_name).ok();
                }
            }
        }

        // Create standard system directories as root before switching to user.
        // /tmp is world-writable (0o1777 on real systems, 0o777 here)
        let _ = fs.create_dir_all("/tmp");
        let _ = fs.set_mode("/tmp", 0o777);
        let _ = fs.create_dir_all("/dev");
        let _ = fs.write("/dev/null", b"");
        let _ = fs.set_mode("/dev/null", 0o666);
        // Create home hierarchy as root so intermediate dirs (e.g. /home) are
        // owned by root with 0o755. Switch to the main user only after setup.
        let _ = fs.create_dir_all(&home);
        fs.chown(&home, main_uid, main_gid).unwrap_or(());

        let empty_files: BTreeMap<String, FileSpec> = BTreeMap::new();
        let files = if let Some(ref sys) = config.system {
            &sys.files
        } else {
            config.files.as_ref().unwrap_or(&empty_files)
        };

        // Seed files as root (uid=0) so all configured paths can be created
        // regardless of directory ownership. The fs starts as uid=0.
        // Chown each seeded file to the main user so they can write to them.
        for (file_path, spec) in files {
            let full = if file_path.starts_with('/') {
                file_path.clone()
            } else {
                format!("{}/{}", home, file_path)
            };

            if let Some(parent) = MemFs::parent(&full) {
                let _ = fs.create_dir_all(parent);
            }

            match spec {
                FileSpec::Content(content) => {
                    let _ = fs.write(&full, content.as_bytes());
                }
                FileSpec::WithMode { content, mode } => {
                    // User provides mode as "octal-looking" decimal (e.g., 755).
                    // Convert: 755 decimal → parse digits as octal → 0o755.
                    let octal = parse_mode_digits(*mode);
                    let _ = fs.write_with_mode(&full, content.as_bytes(), octal);
                }
            };
            fs.chown(&full, main_uid, main_gid).unwrap_or(());
        }

        // Switch to the main user after all files are seeded
        fs.set_current_user(main_uid, main_gid);

        let mut env = BTreeMap::new();
        env.insert(Str::from("HOME"), home.clone());
        env.insert(Str::from("USER"), config.user.clone());
        env.insert(Str::from("HOSTNAME"), hostname.clone());
        env.insert(Str::from("PWD"), home.clone());
        env.insert(Str::from("SHELL"), "/bin/conch".to_string());
        env.insert(Str::from("0"), "conch".to_string());
        env.insert(
            Str::from("PATH"),
            "/usr/local/bin:/usr/bin:/bin".to_string(),
        );
        if let Some(ref date) = config.date {
            env.insert(Str::from("DATE"), date.clone());
            // Initialize VFS clock from date so stat/mtime show realistic values.
            // Format: "Sat Apr 12 00:00:00 UTC 2026"
            if let Some(epoch) = parse_date_to_epoch(date) {
                let ticks = epoch * pipeline::TICKS_PER_SECOND;
                fs.set_time(ticks);
            }
        }

        let start_time = fs.time();
        Shell {
            fs,
            cwd: Str::from(home.as_str()),
            tmp_counter: 0,
            history: Vec::new(),
            ident: ShellIdent {
                user: Str::from(config.user.as_str()),
                hostname: Str::from(hostname.as_str()),
                home: Str::from(home.as_str()),
                users: users_db,
            },
            vars: VarStore {
                env,
                arrays: BTreeMap::new(),
                assoc_arrays: BTreeMap::new(),
                namerefs: BTreeMap::new(),
                readonly: BTreeSet::new(),
                local_frames: Vec::new(),
            },
            defs: ShellDefs {
                functions: BTreeMap::new(),
                aliases: BTreeMap::new(),
                traps: BTreeMap::new(),
            },
            exec: ExecState {
                last_exit_code: 0,
                call_depth: 0,
                in_condition: 0,
                opts: ShellOpts::default(),
                exec_pending: false,
                alias_depth: 0,
            },
            procs: ProcessTable::new(),
            bg_mode: config.background_mode,
            bg_jobs: Vec::new(),
            pending_bg_completions: Vec::new(),
            deferred_process_substs: Vec::new(),
            dir_stack: Vec::new(),
            color: true,
            last_lang: None,
            start_time,
            shopt: ShoptOpts::default(),
        }
    }

    /// Display path: replace home prefix with ~
    pub fn display_path(&self) -> String {
        if self.cwd == self.ident.home {
            "~".to_string()
        } else if let Some(rest) = self.cwd.strip_prefix(self.ident.home.as_str()) {
            format!("~{}", rest)
        } else {
            self.cwd.to_string()
        }
    }

    /// Resolve a possibly-relative path to a normalized absolute path
    pub fn resolve(&mut self, path: &str) -> String {
        let expanded = self.expand(path);
        let abs = if expanded.starts_with('/') {
            expanded
        } else {
            format!("{}/{}", self.cwd, expanded)
        };
        MemFs::normalize(&abs)
    }

    /// List direct children of a directory, sorted by name.
    /// Returns (name, is_dir, mode).
    pub fn list_dir(&self, dir: &str) -> Vec<(String, bool, u16)> {
        self.fs
            .read_dir(dir)
            .unwrap_or_default()
            .into_iter()
            .map(|e| (e.name, e.is_dir, e.mode))
            .collect()
    }

    /// Expand glob patterns in arguments
    pub fn expand_globs(&mut self, args: &[String]) -> Vec<String> {
        let mut result = Vec::new();
        for arg in args {
            if !self.exec.opts.noglob && (arg.contains('*') || arg.contains('?')) {
                if let Some(expanded) = self.glob_expand(arg) {
                    result.extend(expanded);
                    continue;
                }
            }
            result.push(arg.clone());
        }
        result
    }

    fn glob_expand(&mut self, pattern: &str) -> Option<Vec<String>> {
        let (dir, file_pattern) = if let Some((d, f)) = pattern.rsplit_once('/') {
            (
                self.resolve(if d.is_empty() { "/" } else { d }),
                f.to_string(),
            )
        } else {
            (self.cwd.to_string(), pattern.to_string())
        };

        let glob = Glob::new(&file_pattern).ok()?.compile_matcher();
        let children = self.list_dir(&dir);

        let mut entries: Vec<String> = children
            .into_iter()
            .filter(|(name, _, _)| glob.is_match(name.as_str()))
            .map(|(name, _, _)| {
                if pattern.contains('/') {
                    format!("{}/{}", dir, name)
                } else {
                    name
                }
            })
            .collect();

        if entries.is_empty() {
            None
        } else {
            entries.sort();
            Some(entries)
        }
    }

    /// Create directory and all parents
    pub fn mkdir_p(&mut self, abs_path: &str) -> Result<(), bare_vfs::VfsError> {
        self.fs.create_dir_all(abs_path)
    }

    /// Run a command line and return (output, exit_code, lang_hint).
    /// This is the core execution engine used by both interactive and script modes.
    pub fn run_line(&mut self, line: &str) -> (String, i32, Option<String>) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            self.history.push(trimmed.to_string());
        }
        if trimmed.is_empty() {
            return (String::new(), 0, None);
        }
        // Check for unterminated quotes before parsing
        if has_unterminated_quote(trimmed) {
            return (
                "conch: syntax error: unterminated quote".to_string(),
                2,
                None,
            );
        }
        let cmd_list = crate::script::word_parser::parse_command_line(trimmed);
        self.exec_command_list(&cmd_list)
    }

    /// Execute a full command line (handles pipes, redirects, chaining).
    /// Returns an OutputEntry for terminal display.
    #[cfg(test)]
    pub fn execute(&mut self, line: &str) -> OutputEntry {
        let display = self.display_path();
        let pre_user = self.ident.user.clone();
        let pre_hostname = self.ident.hostname.clone();
        let (output, code, lang) = self.run_line(line);
        let bg = std::mem::take(&mut self.pending_bg_completions);
        OutputEntry {
            user: pre_user,
            hostname: pre_hostname,
            path: display,
            command: line.to_string(),
            output,
            exit_code: code,
            lang,
            first_line: None,
            last_line: None,
            bg_completions: bg,
        }
    }

    /// Run a command line with optional initial stdin provided to the first
    /// pipeline segment. Used for heredoc support.
    pub fn run_line_with_stdin(
        &mut self,
        line: &str,
        stdin: Option<&str>,
    ) -> (String, i32, Option<String>) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            self.history.push(trimmed.to_string());
        }
        if trimmed.is_empty() {
            return (String::new(), 0, None);
        }
        let cmd_list = crate::script::word_parser::parse_command_line(trimmed);
        self.exec_command_list_with_stdin(&cmd_list, stdin)
    }

    /// Execute a script with EXIT trap handling (normal entry point).
    pub fn run_script(&mut self, script: &str) -> (String, i32) {
        self.run_script_inner(script, true)
    }

    /// Execute a script WITHOUT EXIT trap handling (used by `source`).
    /// `source` runs in the current shell — EXIT trap should persist
    /// until the shell itself exits, not fire when the sourced script ends.
    pub fn run_script_no_exit_trap(&mut self, script: &str) -> (String, i32) {
        self.run_script_inner(script, false)
    }

    fn run_script_inner(&mut self, script: &str, run_exit_trap: bool) -> (String, i32) {
        let heredoc_lines = self.preprocess_heredocs(script);

        // Check if any line has a heredoc body attached
        let has_heredocs = heredoc_lines.iter().any(|(_, body)| body.is_some());

        if !has_heredocs {
            // No heredocs: use the normal AST path
            let script_text: String = heredoc_lines
                .into_iter()
                .map(|(line, _)| line)
                .collect::<Vec<_>>()
                .join("\n");
            match crate::script::parse_script(&script_text) {
                Ok(ast) => {
                    let mut output = Vec::new();
                    let flow = self.interpret_stmts(&ast.stmts, &mut output);
                    let code = match &flow {
                        crate::script::interp::ControlFlow::Normal(c) => *c,
                        crate::script::interp::ControlFlow::Return(c) => {
                            if run_exit_trap {
                                // Non-source script: return at top level is an error
                                output.push("conch: return: can only `return` from a function or sourced script".to_string());
                                1
                            } else {
                                // source'd script: return is valid, exit with code
                                *c
                            }
                        }
                        crate::script::interp::ControlFlow::Break(_) => {
                            output.push("conch: break: only meaningful in a loop".to_string());
                            1
                        }
                        crate::script::interp::ControlFlow::Continue(_) => {
                            output.push("conch: continue: only meaningful in a loop".to_string());
                            1
                        }
                    };
                    // Execute EXIT trap only if requested (not for `source`)
                    if run_exit_trap {
                        if let Some(exit_cmd) = self.defs.traps.remove("EXIT") {
                            if !exit_cmd.is_empty() {
                                let (out, _, _) = self.run_line(&exit_cmd);
                                if !out.is_empty() {
                                    output.push(out);
                                }
                            }
                        }
                    }
                    (output.join("\n"), code)
                }
                Err(e) => (format!("conch: {}", e), 2),
            }
        } else {
            // With heredocs: split script into chunks. Consecutive non-heredoc
            // lines are grouped and parsed as AST; heredoc lines are executed
            // individually via run_line_with_stdin.
            let mut all_output = Vec::new();
            let mut last_code: i32 = 0;
            let mut pending_lines: Vec<String> = Vec::new();

            let flush_pending =
                |shell: &mut Shell, pending: &mut Vec<String>, output: &mut Vec<String>| -> i32 {
                    if pending.is_empty() {
                        return -1; // sentinel: no lines flushed
                    }
                    let script_text = pending.join("\n");
                    pending.clear();
                    match crate::script::parse_script(&script_text) {
                        Ok(ast) => {
                            let flow = shell.interpret_stmts(&ast.stmts, output);
                            flow.exit_code()
                        }
                        Err(e) => {
                            output.push(format!("conch: {}", e));
                            2
                        }
                    }
                };

            for (line, heredoc_body) in heredoc_lines {
                if let Some(body) = heredoc_body {
                    // Flush any pending non-heredoc lines first
                    let flush_code = flush_pending(self, &mut pending_lines, &mut all_output);
                    // Abort on parse error (code 2) — don't continue executing
                    if flush_code == 2 {
                        last_code = 2;
                        break;
                    }
                    // Execute this heredoc line with stdin
                    let (out, code, _) = self.run_line_with_stdin(&line, Some(&body));
                    if !out.is_empty() {
                        all_output.push(out);
                    }
                    last_code = code;
                } else {
                    pending_lines.push(line);
                }
            }
            // Flush remaining lines — only overwrite last_code if there were lines
            let flush_code = flush_pending(self, &mut pending_lines, &mut all_output);
            if flush_code >= 0 {
                last_code = flush_code;
            }

            // Execute EXIT trap only if requested
            if run_exit_trap {
                if let Some(exit_cmd) = self.defs.traps.remove("EXIT") {
                    if !exit_cmd.is_empty() {
                        let (out, _, _) = self.run_line(&exit_cmd);
                        if !out.is_empty() {
                            all_output.push(out);
                        }
                    }
                }
            }

            (all_output.join("\n"), last_code)
        }
    }
}

mod exec_structured;
mod expand_word;

#[cfg(test)]
mod tests;
