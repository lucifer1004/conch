use crate::commands::CmdResult;
use crate::shell::Shell;

impl Shell {
    pub fn cmd_source(&mut self, args: &[String]) -> CmdResult {
        if args.is_empty() {
            return ("source: filename argument required".into(), 2, None);
        }
        let path = self.resolve(&args[0]);
        let content = match self.fs.read_to_string(&path) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("source: {}: {}", args[0], e), 1, None),
        };
        let mut last_output = String::new();
        let mut last_code = 0;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (out, code, _) = self.run_line(line);
            last_output = out;
            last_code = code;
        }
        (last_output, last_code, None)
    }

    pub fn cmd_bash(&mut self, args: &[String]) -> (String, i32) {
        if args.is_empty() {
            return ("bash: missing script file".into(), 1);
        }
        let path = self.resolve(&args[0]);
        let script = match self.fs.read_to_string(&path) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("bash: {}: {}", args[0], e), 1),
        };
        self.run_script(&script)
    }

    /// Execute a file as a script (for `./script.sh` invocation).
    /// Requires the file to have execute permission.
    pub fn cmd_exec(&mut self, cmd: &str, _args: &[String]) -> (String, i32) {
        let path = self.resolve(cmd);
        let meta = match self.fs.metadata(&path) {
            Ok(m) if m.is_dir() => return (format!("conch: {}: Is a directory", cmd), 126),
            Ok(m) => m,
            Err(_) => return (format!("conch: {}: No such file or directory", cmd), 127),
        };
        if !meta.is_readable() {
            return (format!("conch: {}: Permission denied", cmd), 126);
        }
        if !meta.is_executable() {
            return (format!("conch: {}: Permission denied", cmd), 126);
        }
        let script = match self.fs.read_to_string(&path) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("conch: {}: {}", cmd, e), 126),
        };
        self.run_script(&script)
    }
}
