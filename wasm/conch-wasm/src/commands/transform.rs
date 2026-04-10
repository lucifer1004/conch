use crate::shell::Shell;

impl Shell {
    pub fn cmd_diff(&self, args: &[String]) -> (String, i32) {
        let files: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
        if files.len() < 2 {
            return ("diff: missing operand".into(), 1);
        }

        let path_a = self.resolve(files[0]);
        let path_b = self.resolve(files[1]);

        let content_a = match self.fs.read_to_string(&path_a) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("diff: {}: {}", files[0], e), 1),
        };
        let content_b = match self.fs.read_to_string(&path_b) {
            Ok(s) => s.to_string(),
            Err(e) => return (format!("diff: {}: {}", files[1], e), 1),
        };

        let lines_a: Vec<&str> = content_a.lines().collect();
        let lines_b: Vec<&str> = content_b.lines().collect();

        let mut out = Vec::new();
        let len_a = lines_a.len();
        let len_b = lines_b.len();
        let common = len_a.min(len_b);

        for i in 0..common {
            if lines_a[i] != lines_b[i] {
                out.push(format!("{} c {}", i + 1, i + 1));
                out.push(format!("< {}", lines_a[i]));
                out.push("---".to_string());
                out.push(format!("> {}", lines_b[i]));
            }
        }

        if len_a > len_b {
            for (i, line) in lines_a.iter().enumerate().skip(common) {
                out.push(format!("{} d {}", i + 1, len_b));
                out.push(format!("< {}", line));
            }
        } else if len_b > len_a {
            for (i, line) in lines_b.iter().enumerate().skip(common) {
                out.push(format!("{} a {}", len_a, i + 1));
                out.push(format!("> {}", line));
            }
        }

        let exit_code = if out.is_empty() { 0 } else { 1 };
        (out.join("\n"), exit_code)
    }

    pub fn cmd_sed(&mut self, args: &[String], stdin: Option<&str>) -> (String, i32) {
        let mut in_place = false;
        let mut expression: Option<String> = None;
        let mut file: Option<String> = None;

        let mut i = 0;
        while i < args.len() {
            match args[i].as_str() {
                "-i" => {
                    in_place = true;
                    i += 1;
                }
                "-e" if i + 1 < args.len() => {
                    expression = Some(args[i + 1].clone());
                    i += 2;
                }
                s if !s.starts_with('-') && expression.is_none() => {
                    expression = Some(args[i].clone());
                    i += 1;
                }
                s if !s.starts_with('-') => {
                    file = Some(args[i].clone());
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        let expr = match expression {
            Some(e) => e,
            None => return ("sed: missing expression".into(), 1),
        };

        let (pattern, replacement, global) = match parse_sed_expr(&expr) {
            Some(r) => r,
            None => return (format!("sed: invalid expression: {}", expr), 1),
        };

        let input = match self.resolve_input(file.as_deref(), stdin) {
            Ok(s) => s,
            Err(e) => return (format!("sed: {}", e), 1),
        };

        let output_lines: Vec<String> = input
            .lines()
            .map(|line| {
                if global {
                    line.replace(pattern.as_str(), replacement.as_str())
                } else {
                    if let Some(pos) = line.find(pattern.as_str()) {
                        format!(
                            "{}{}{}",
                            &line[..pos],
                            replacement,
                            &line[pos + pattern.len()..]
                        )
                    } else {
                        line.to_string()
                    }
                }
            })
            .collect();

        let output = output_lines.join("\n");

        if in_place {
            if let Some(ref f) = file {
                let path = self.resolve(f);
                self.fs.write(&path, output.as_bytes());
            }
        }

        (output, 0)
    }

    pub fn cmd_xxd(&self, args: &[String]) -> (String, i32) {
        let file = args.iter().find(|a| !a.starts_with('-'));
        let path = match file {
            Some(f) => self.resolve(f),
            None => return ("xxd: missing file operand".into(), 1),
        };

        let bytes = match self.fs.read(&path) {
            Ok(b) => b,
            Err(e) => return (format!("xxd: {}: {}", file.unwrap(), e), 1),
        };

        let mut out = Vec::new();
        let mut offset = 0usize;

        for chunk in bytes.chunks(16) {
            // Hex part: groups of 2 bytes separated by space
            let hex_pairs: Vec<String> = chunk
                .chunks(2)
                .map(|pair| {
                    pair.iter()
                        .map(|b| format!("{:02x}", b))
                        .collect::<String>()
                })
                .collect();
            let hex_str = hex_pairs.join(" ");

            // ASCII part
            let ascii: String = chunk
                .iter()
                .map(|&b| {
                    if (0x20..0x7f).contains(&b) {
                        b as char
                    } else {
                        '.'
                    }
                })
                .collect();

            out.push(format!("{:08x}: {:<48}  {}", offset, hex_str, ascii));
            offset += chunk.len();
        }

        (out.join("\n"), 0)
    }

    pub fn cmd_base64(&self, args: &[String]) -> (String, i32) {
        let mut decode = false;
        let mut file: Option<String> = None;

        for arg in args {
            match arg.as_str() {
                "-d" | "--decode" => decode = true,
                s if !s.starts_with('-') => file = Some(arg.clone()),
                _ => {}
            }
        }

        if decode {
            let input = match self.resolve_input(file.as_deref(), None) {
                Ok(s) => s,
                Err(e) => return (format!("base64: {}", e), 1),
            };
            let cleaned: String = input.chars().filter(|c| !c.is_whitespace()).collect();
            match base64_decode(&cleaned) {
                Ok(bytes) => {
                    let s = String::from_utf8(bytes).unwrap_or_else(|_| String::new());
                    (s, 0)
                }
                Err(e) => (format!("base64: invalid input: {}", e), 1),
            }
        } else {
            let bytes = match file {
                Some(ref f) => {
                    let path = self.resolve(f);
                    match self.fs.read(&path) {
                        Ok(b) => b.to_vec(),
                        Err(e) => return (format!("base64: {}: {}", f, e), 1),
                    }
                }
                None => Vec::new(),
            };
            (base64_encode(&bytes), 0)
        }
    }
}

/// Parse a sed substitution expression: `s/pattern/replacement/` or `s/pattern/replacement/g`
fn parse_sed_expr(expr: &str) -> Option<(String, String, bool)> {
    if !expr.starts_with("s/") {
        return None;
    }
    let rest = &expr[2..];
    // Find the pattern end (first unescaped `/`)
    let pat_end = rest.find('/')?;
    let pattern = rest[..pat_end].to_string();
    let rest2 = &rest[pat_end + 1..];
    // Find the replacement end
    let repl_end = rest2.find('/')?;
    let replacement = rest2[..repl_end].to_string();
    let flags = &rest2[repl_end + 1..];
    let global = flags.contains('g');
    Some((pattern, replacement, global))
}

const BASE64_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < input.len() {
        let b0 = input[i] as u32;
        let b1 = if i + 1 < input.len() {
            input[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < input.len() {
            input[i + 2] as u32
        } else {
            0
        };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        out.push(BASE64_CHARS[((triple >> 18) & 0x3F) as usize] as char);
        out.push(BASE64_CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < input.len() {
            out.push(BASE64_CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
        if i + 2 < input.len() {
            out.push(BASE64_CHARS[(triple & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }

        i += 3;
    }
    out
}

fn base64_char_value(c: char) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a' + 26),
        '0'..='9' => Some(c as u8 - b'0' + 52),
        '+' => Some(62),
        '/' => Some(63),
        _ => None,
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, &'static str> {
    if !input.len().is_multiple_of(4) {
        return Err("invalid length");
    }
    let mut out = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c0 = base64_char_value(chars[i]).ok_or("invalid char")?;
        let c1 = base64_char_value(chars[i + 1]).ok_or("invalid char")?;
        out.push((c0 << 2) | (c1 >> 4));

        if chars[i + 2] != '=' {
            let c2 = base64_char_value(chars[i + 2]).ok_or("invalid char")?;
            out.push(((c1 & 0x0F) << 4) | (c2 >> 2));
            if chars[i + 3] != '=' {
                let c3 = base64_char_value(chars[i + 3]).ok_or("invalid char")?;
                out.push(((c2 & 0x03) << 6) | c3);
            }
        }
        i += 4;
    }
    Ok(out)
}
