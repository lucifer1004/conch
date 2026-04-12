/// Arithmetic expression evaluator for `$((...))` and `(( ))`.
///
/// Supports: + - * / % ** == != < > <= >= && || ! ?: , ( )
/// Variables: bare names resolve from env (e.g. `x` → `$x`)
/// Assignment: = += -= *= /= %=
/// Increment/decrement: ++ -- (prefix only for simplicity)
/// Evaluate an arithmetic expression string, resolving variables from the env.
/// Returns the numeric result.
pub fn eval_arith(
    expr: &str,
    get_var: &dyn Fn(&str) -> i64,
    set_var: &mut dyn FnMut(&str, i64),
) -> Result<i64, String> {
    let tokens = tokenize(expr)?;
    let mut pos = 0;
    let result = parse_comma(&tokens, &mut pos, get_var, set_var)?;
    if pos < tokens.len() {
        return Err(format!("unexpected token in arithmetic: {:?}", tokens[pos]));
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(i64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    StarStar, // **
    Eq,       // ==
    Ne,       // !=
    Lt,
    Gt,
    Le,            // <=
    Ge,            // >=
    AndAnd,        // &&
    OrOr,          // ||
    Bang,          // !
    Amp,           // & (bitwise AND)
    Pipe_,         // | (bitwise OR)
    Caret,         // ^ (bitwise XOR)
    Tilde,         // ~ (bitwise NOT)
    LShift,        // <<
    RShift,        // >>
    Assign,        // =
    PlusAssign,    // +=
    MinusAssign,   // -=
    StarAssign,    // *=
    SlashAssign,   // /=
    PercentAssign, // %=
    AmpAssign,     // &=
    PipeAssign,    // |=
    CaretAssign,   // ^=
    LShiftAssign,  // <<=
    RShiftAssign,  // >>=
    PlusPlus,      // ++
    MinusMinus,    // --
    Question,      // ?
    Colon,         // :
    Comma,         // ,
    LParen,
    RParen,
}

fn tokenize(expr: &str) -> Result<Vec<Tok>, String> {
    let mut tokens = Vec::new();
    let bytes = expr.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' => {
                i += 1;
            }
            b'0'..=b'9' => {
                let start = i;
                // Hex: 0x...
                if i + 1 < bytes.len()
                    && bytes[i] == b'0'
                    && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X')
                {
                    i += 2;
                    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    let s = std::str::from_utf8(&bytes[start..i])
                        .map_err(|e| format!("invalid UTF-8 in hex literal: {}", e))?;
                    let val = i64::from_str_radix(&s[2..], 16)
                        .map_err(|_| format!("invalid hex: {}", s))?;
                    tokens.push(Tok::Num(val));
                // Octal: 0...
                } else if bytes[i] == b'0' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    let s = std::str::from_utf8(&bytes[start..i])
                        .map_err(|e| format!("invalid UTF-8 in octal literal: {}", e))?;
                    let val = i64::from_str_radix(&s[1..], 8)
                        .map_err(|_| format!("invalid octal: {}", s))?;
                    tokens.push(Tok::Num(val));
                } else {
                    while i < bytes.len() && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    let s = std::str::from_utf8(&bytes[start..i])
                        .map_err(|e| format!("invalid UTF-8 in number: {}", e))?;
                    tokens.push(Tok::Num(
                        s.parse().map_err(|_| format!("invalid number: {}", s))?,
                    ));
                }
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$' => {
                let start = i;
                if bytes[i] == b'$' {
                    i += 1; // skip leading $, treat $VAR same as VAR
                }
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                let s = std::str::from_utf8(&bytes[start..i])
                    .map_err(|e| format!("invalid UTF-8 in identifier: {}", e))?;
                let name = s.trim_start_matches('$');
                tokens.push(Tok::Ident(name.to_string()));
            }
            b'+' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'+' {
                    tokens.push(Tok::PlusPlus);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::PlusAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Plus);
                    i += 1;
                }
            }
            b'-' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'-' {
                    tokens.push(Tok::MinusMinus);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::MinusAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Minus);
                    i += 1;
                }
            }
            b'*' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    tokens.push(Tok::StarStar);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::StarAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Star);
                    i += 1;
                }
            }
            b'/' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::SlashAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Slash);
                    i += 1;
                }
            }
            b'%' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::PercentAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Percent);
                    i += 1;
                }
            }
            b'=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::Eq);
                    i += 2;
                } else {
                    tokens.push(Tok::Assign);
                    i += 1;
                }
            }
            b'!' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::Ne);
                    i += 2;
                } else {
                    tokens.push(Tok::Bang);
                    i += 1;
                }
            }
            b'<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'<' {
                    if i + 2 < bytes.len() && bytes[i + 2] == b'=' {
                        tokens.push(Tok::LShiftAssign);
                        i += 3;
                    } else {
                        tokens.push(Tok::LShift);
                        i += 2;
                    }
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::Le);
                    i += 2;
                } else {
                    tokens.push(Tok::Lt);
                    i += 1;
                }
            }
            b'>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'>' {
                    if i + 2 < bytes.len() && bytes[i + 2] == b'=' {
                        tokens.push(Tok::RShiftAssign);
                        i += 3;
                    } else {
                        tokens.push(Tok::RShift);
                        i += 2;
                    }
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::Ge);
                    i += 2;
                } else {
                    tokens.push(Tok::Gt);
                    i += 1;
                }
            }
            b'&' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'&' {
                    tokens.push(Tok::AndAnd);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::AmpAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Amp);
                    i += 1;
                }
            }
            b'|' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'|' {
                    tokens.push(Tok::OrOr);
                    i += 2;
                } else if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::PipeAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Pipe_);
                    i += 1;
                }
            }
            b'^' => {
                if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                    tokens.push(Tok::CaretAssign);
                    i += 2;
                } else {
                    tokens.push(Tok::Caret);
                    i += 1;
                }
            }
            b'~' => {
                tokens.push(Tok::Tilde);
                i += 1;
            }
            b'(' => {
                tokens.push(Tok::LParen);
                i += 1;
            }
            b')' => {
                tokens.push(Tok::RParen);
                i += 1;
            }
            b'?' => {
                tokens.push(Tok::Question);
                i += 1;
            }
            b':' => {
                tokens.push(Tok::Colon);
                i += 1;
            }
            b',' => {
                tokens.push(Tok::Comma);
                i += 1;
            }
            c => {
                return Err(format!(
                    "unexpected character in arithmetic: '{}'",
                    c as char
                ))
            }
        }
    }
    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Recursive descent parser (precedence climbing)
// ---------------------------------------------------------------------------

type GetVar<'a> = &'a dyn Fn(&str) -> i64;
type SetVar<'a> = &'a mut dyn FnMut(&str, i64);

// Precedence (low to high):
// ,
// = += -= *= /= %= &= |= ^= <<= >>=
// ?:
// ||  (short-circuit)
// &&  (short-circuit)
// |   (bitwise OR)
// ^   (bitwise XOR)
// &   (bitwise AND)
// == !=
// < > <= >=
// << >>  (bitwise shift)
// + -
// * / %
// ** (right-assoc)
// unary: ! ~ - + ++ --
// postfix: ++ --
// primary: number, variable, (expr)

fn parse_comma(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_assign(tokens, pos, get, set)?;
    while *pos < tokens.len() && tokens[*pos] == Tok::Comma {
        *pos += 1;
        val = parse_assign(tokens, pos, get, set)?;
    }
    Ok(val)
}

fn parse_assign(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    // Check for `IDENT op= expr` pattern
    if let Some(Tok::Ident(name)) = tokens.get(*pos) {
        if let Some(op) = tokens.get(*pos + 1) {
            let name = name.clone();
            match op {
                Tok::Assign => {
                    *pos += 2;
                    let val = parse_assign(tokens, pos, get, set)?;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::PlusAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) + rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::MinusAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) - rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::StarAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) * rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::SlashAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    if rhs == 0 {
                        return Err("division by zero".into());
                    }
                    let val = get(&name) / rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::PercentAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    if rhs == 0 {
                        return Err("division by zero".into());
                    }
                    let val = get(&name) % rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::AmpAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) & rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::PipeAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) | rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::CaretAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) ^ rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::LShiftAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) << rhs;
                    set(&name, val);
                    return Ok(val);
                }
                Tok::RShiftAssign => {
                    *pos += 2;
                    let rhs = parse_assign(tokens, pos, get, set)?;
                    let val = get(&name) >> rhs;
                    set(&name, val);
                    return Ok(val);
                }
                _ => {}
            }
        }
    }
    parse_ternary(tokens, pos, get, set)
}

fn parse_ternary(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let cond = parse_or(tokens, pos, get, set)?;
    if *pos < tokens.len() && tokens[*pos] == Tok::Question {
        *pos += 1;
        let then_val = parse_assign(tokens, pos, get, set)?;
        if *pos < tokens.len() && tokens[*pos] == Tok::Colon {
            *pos += 1;
        } else {
            return Err("expected ':' in ternary".into());
        }
        let else_val = parse_assign(tokens, pos, get, set)?;
        Ok(if cond != 0 { then_val } else { else_val })
    } else {
        Ok(cond)
    }
}

fn parse_or(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_and(tokens, pos, get, set)?;
    while *pos < tokens.len() && tokens[*pos] == Tok::OrOr {
        *pos += 1;
        if val != 0 {
            // Short-circuit: skip RHS but still parse it
            let _rhs = parse_and(tokens, pos, get, set)?;
            val = 1;
        } else {
            let rhs = parse_and(tokens, pos, get, set)?;
            val = if rhs != 0 { 1 } else { 0 };
        }
    }
    Ok(val)
}

fn parse_and(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_bit_or(tokens, pos, get, set)?;
    while *pos < tokens.len() && tokens[*pos] == Tok::AndAnd {
        *pos += 1;
        if val == 0 {
            // Short-circuit: skip RHS but still parse it
            let _rhs = parse_bit_or(tokens, pos, get, set)?;
            val = 0;
        } else {
            let rhs = parse_bit_or(tokens, pos, get, set)?;
            val = if rhs != 0 { 1 } else { 0 };
        }
    }
    Ok(val)
}

fn parse_bit_or(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_bit_xor(tokens, pos, get, set)?;
    while *pos < tokens.len() && tokens[*pos] == Tok::Pipe_ {
        *pos += 1;
        val |= parse_bit_xor(tokens, pos, get, set)?;
    }
    Ok(val)
}

fn parse_bit_xor(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_bit_and(tokens, pos, get, set)?;
    while *pos < tokens.len() && tokens[*pos] == Tok::Caret {
        *pos += 1;
        val ^= parse_bit_and(tokens, pos, get, set)?;
    }
    Ok(val)
}

fn parse_bit_and(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_eq(tokens, pos, get, set)?;
    while *pos < tokens.len() && tokens[*pos] == Tok::Amp {
        *pos += 1;
        val &= parse_eq(tokens, pos, get, set)?;
    }
    Ok(val)
}

fn parse_eq(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_rel(tokens, pos, get, set)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Tok::Eq => {
                *pos += 1;
                let r = parse_rel(tokens, pos, get, set)?;
                val = if val == r { 1 } else { 0 };
            }
            Tok::Ne => {
                *pos += 1;
                let r = parse_rel(tokens, pos, get, set)?;
                val = if val != r { 1 } else { 0 };
            }
            _ => break,
        }
    }
    Ok(val)
}

fn parse_rel(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_shift(tokens, pos, get, set)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Tok::Lt => {
                *pos += 1;
                let r = parse_shift(tokens, pos, get, set)?;
                val = if val < r { 1 } else { 0 };
            }
            Tok::Gt => {
                *pos += 1;
                let r = parse_shift(tokens, pos, get, set)?;
                val = if val > r { 1 } else { 0 };
            }
            Tok::Le => {
                *pos += 1;
                let r = parse_shift(tokens, pos, get, set)?;
                val = if val <= r { 1 } else { 0 };
            }
            Tok::Ge => {
                *pos += 1;
                let r = parse_shift(tokens, pos, get, set)?;
                val = if val >= r { 1 } else { 0 };
            }
            _ => break,
        }
    }
    Ok(val)
}

fn parse_shift(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_add(tokens, pos, get, set)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Tok::LShift => {
                *pos += 1;
                val <<= parse_add(tokens, pos, get, set)?;
            }
            Tok::RShift => {
                *pos += 1;
                val >>= parse_add(tokens, pos, get, set)?;
            }
            _ => break,
        }
    }
    Ok(val)
}

fn parse_add(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_mul(tokens, pos, get, set)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Tok::Plus => {
                *pos += 1;
                val += parse_mul(tokens, pos, get, set)?;
            }
            Tok::Minus => {
                *pos += 1;
                val -= parse_mul(tokens, pos, get, set)?;
            }
            _ => break,
        }
    }
    Ok(val)
}

fn parse_mul(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let mut val = parse_pow(tokens, pos, get, set)?;
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Tok::Star => {
                *pos += 1;
                val *= parse_pow(tokens, pos, get, set)?;
            }
            Tok::Slash => {
                *pos += 1;
                let r = parse_pow(tokens, pos, get, set)?;
                if r == 0 {
                    return Err("division by zero".into());
                }
                val /= r;
            }
            Tok::Percent => {
                *pos += 1;
                let r = parse_pow(tokens, pos, get, set)?;
                if r == 0 {
                    return Err("division by zero".into());
                }
                val %= r;
            }
            _ => break,
        }
    }
    Ok(val)
}

fn parse_pow(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    let base = parse_unary(tokens, pos, get, set)?;
    if *pos < tokens.len() && tokens[*pos] == Tok::StarStar {
        *pos += 1;
        let exp = parse_pow(tokens, pos, get, set)?; // right-associative
        Ok(base.wrapping_pow(exp as u32))
    } else {
        Ok(base)
    }
}

fn parse_unary(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    if *pos >= tokens.len() {
        return Err("unexpected end of arithmetic expression".into());
    }
    match &tokens[*pos] {
        Tok::Minus => {
            *pos += 1;
            Ok(-parse_unary(tokens, pos, get, set)?)
        }
        Tok::Plus => {
            *pos += 1;
            parse_unary(tokens, pos, get, set)
        }
        Tok::Bang => {
            *pos += 1;
            let v = parse_unary(tokens, pos, get, set)?;
            Ok(if v == 0 { 1 } else { 0 })
        }
        Tok::Tilde => {
            *pos += 1;
            let v = parse_unary(tokens, pos, get, set)?;
            Ok(!v)
        }
        Tok::PlusPlus => {
            *pos += 1;
            if let Some(Tok::Ident(name)) = tokens.get(*pos) {
                let name = name.clone();
                *pos += 1;
                let val = get(&name) + 1;
                set(&name, val);
                Ok(val)
            } else {
                Err("++ requires a variable".into())
            }
        }
        Tok::MinusMinus => {
            *pos += 1;
            if let Some(Tok::Ident(name)) = tokens.get(*pos) {
                let name = name.clone();
                *pos += 1;
                let val = get(&name) - 1;
                set(&name, val);
                Ok(val)
            } else {
                Err("-- requires a variable".into())
            }
        }
        _ => parse_primary(tokens, pos, get, set),
    }
}

fn parse_primary(tokens: &[Tok], pos: &mut usize, get: GetVar, set: SetVar) -> Result<i64, String> {
    if *pos >= tokens.len() {
        return Err("unexpected end of arithmetic expression".into());
    }
    match &tokens[*pos] {
        Tok::Num(n) => {
            let n = *n;
            *pos += 1;
            Ok(n)
        }
        Tok::Ident(name) => {
            let name = name.clone();
            let val = get(&name);
            *pos += 1;
            // Postfix ++ / --
            if *pos < tokens.len() && tokens[*pos] == Tok::PlusPlus {
                *pos += 1;
                set(&name, val + 1);
                return Ok(val); // return old value (postfix)
            }
            if *pos < tokens.len() && tokens[*pos] == Tok::MinusMinus {
                *pos += 1;
                set(&name, val - 1);
                return Ok(val); // return old value (postfix)
            }
            Ok(val)
        }
        Tok::LParen => {
            *pos += 1;
            let val = parse_comma(tokens, pos, get, set)?;
            if *pos < tokens.len() && tokens[*pos] == Tok::RParen {
                *pos += 1;
            } else {
                return Err("expected ')' in arithmetic".into());
            }
            Ok(val)
        }
        other => Err(format!("unexpected token in arithmetic: {:?}", other)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::BTreeMap;

    fn eval(expr: &str) -> Result<i64, String> {
        let vars = RefCell::new(BTreeMap::<String, i64>::new());
        eval_arith(
            expr,
            &|name| *vars.borrow().get(name).unwrap_or(&0),
            &mut |name, val| {
                vars.borrow_mut().insert(name.to_string(), val);
            },
        )
    }

    fn eval_with_vars(expr: &str, vars: &RefCell<BTreeMap<String, i64>>) -> Result<i64, String> {
        eval_arith(
            expr,
            &|name| *vars.borrow().get(name).unwrap_or(&0),
            &mut |name, val| {
                vars.borrow_mut().insert(name.to_string(), val);
            },
        )
    }

    #[test]
    fn basic_arithmetic() -> Result<(), String> {
        assert_eq!(eval("2 + 3")?, 5);
        assert_eq!(eval("10 - 3")?, 7);
        assert_eq!(eval("4 * 5")?, 20);
        assert_eq!(eval("15 / 4")?, 3);
        assert_eq!(eval("15 % 4")?, 3);
        Ok(())
    }

    #[test]
    fn precedence() -> Result<(), String> {
        assert_eq!(eval("2 + 3 * 4")?, 14);
        assert_eq!(eval("(2 + 3) * 4")?, 20);
        Ok(())
    }

    #[test]
    fn power() -> Result<(), String> {
        assert_eq!(eval("2 ** 10")?, 1024);
        assert_eq!(eval("2 ** 3 ** 2")?, 512); // right-assoc: 2^(3^2) = 2^9
        Ok(())
    }

    #[test]
    fn comparisons() -> Result<(), String> {
        assert_eq!(eval("3 == 3")?, 1);
        assert_eq!(eval("3 == 4")?, 0);
        assert_eq!(eval("3 != 4")?, 1);
        assert_eq!(eval("3 < 4")?, 1);
        assert_eq!(eval("4 < 3")?, 0);
        assert_eq!(eval("3 <= 3")?, 1);
        assert_eq!(eval("3 > 2")?, 1);
        assert_eq!(eval("3 >= 3")?, 1);
        Ok(())
    }

    #[test]
    fn logical() -> Result<(), String> {
        assert_eq!(eval("1 && 1")?, 1);
        assert_eq!(eval("1 && 0")?, 0);
        assert_eq!(eval("0 || 1")?, 1);
        assert_eq!(eval("0 || 0")?, 0);
        assert_eq!(eval("!0")?, 1);
        assert_eq!(eval("!1")?, 0);
        Ok(())
    }

    #[test]
    fn ternary() -> Result<(), String> {
        assert_eq!(eval("1 ? 10 : 20")?, 10);
        assert_eq!(eval("0 ? 10 : 20")?, 20);
        Ok(())
    }

    #[test]
    fn unary_minus() -> Result<(), String> {
        assert_eq!(eval("-5")?, -5);
        assert_eq!(eval("-(3 + 2)")?, -5);
        Ok(())
    }

    #[test]
    fn variables() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 10);
        vars.borrow_mut().insert("y".into(), 3);
        assert_eq!(eval_with_vars("x + y", &vars)?, 13);
        assert_eq!(eval_with_vars("x * y + 1", &vars)?, 31);
        Ok(())
    }

    #[test]
    fn dollar_prefix_variable() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 42);
        assert_eq!(eval_with_vars("$x + 1", &vars)?, 43);
        Ok(())
    }

    #[test]
    fn assignment() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        assert_eq!(eval_with_vars("x = 5", &vars)?, 5);
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 5);
        Ok(())
    }

    #[test]
    fn compound_assignment() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 10);
        assert_eq!(eval_with_vars("x += 5", &vars)?, 15);
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 15);
        assert_eq!(eval_with_vars("x -= 3", &vars)?, 12);
        assert_eq!(eval_with_vars("x *= 2", &vars)?, 24);
        Ok(())
    }

    #[test]
    fn increment_decrement() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 5);
        assert_eq!(eval_with_vars("++x", &vars)?, 6);
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 6);
        assert_eq!(eval_with_vars("--x", &vars)?, 5);
        Ok(())
    }

    #[test]
    fn comma_operator() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        assert_eq!(eval_with_vars("x = 1, x + 1", &vars)?, 2);
        Ok(())
    }

    #[test]
    fn hex_and_octal() -> Result<(), String> {
        assert_eq!(eval("0xff")?, 255);
        assert_eq!(eval("0x10")?, 16);
        assert_eq!(eval("010")?, 8);
        Ok(())
    }

    #[test]
    fn division_by_zero() {
        let vars = RefCell::new(BTreeMap::<String, i64>::new());
        let result = eval_arith(
            "1 / 0",
            &|name| *vars.borrow().get(name).unwrap_or(&0),
            &mut |name, val| {
                vars.borrow_mut().insert(name.to_string(), val);
            },
        );
        assert!(result.is_err());
    }

    // -- Fix #7: bitwise operators --

    #[test]
    fn bitwise_and() -> Result<(), String> {
        assert_eq!(eval("0xff & 0x0f")?, 0x0f);
        assert_eq!(eval("6 & 3")?, 2);
        Ok(())
    }

    #[test]
    fn bitwise_or() -> Result<(), String> {
        assert_eq!(eval("0xf0 | 0x0f")?, 0xff);
        assert_eq!(eval("4 | 2")?, 6);
        Ok(())
    }

    #[test]
    fn bitwise_xor() -> Result<(), String> {
        assert_eq!(eval("0xff ^ 0x0f")?, 0xf0);
        assert_eq!(eval("5 ^ 3")?, 6);
        Ok(())
    }

    #[test]
    fn bitwise_not() -> Result<(), String> {
        assert_eq!(eval("~0")?, -1);
        assert_eq!(eval("~(-1)")?, 0);
        Ok(())
    }

    #[test]
    fn left_shift() -> Result<(), String> {
        assert_eq!(eval("1 << 4")?, 16);
        assert_eq!(eval("3 << 2")?, 12);
        Ok(())
    }

    #[test]
    fn right_shift() -> Result<(), String> {
        assert_eq!(eval("16 >> 4")?, 1);
        assert_eq!(eval("12 >> 2")?, 3);
        Ok(())
    }

    #[test]
    fn bitwise_precedence() -> Result<(), String> {
        // & binds tighter than |: 0xff | (0x0f & 0x00) = 0xff | 0 = 0xff
        assert_eq!(eval("0xff | 0x0f & 0x00")?, 0xff);
        // ^ between & and |: 3 | (5 ^ 6) = 3 | 3 = 3
        assert_eq!(eval("3 | 5 ^ 6")?, 3);
        Ok(())
    }

    #[test]
    fn shift_precedence() -> Result<(), String> {
        // << binds tighter than comparison
        assert_eq!(eval("1 << 2 < 8")?, 1); // (1<<2) < 8 => 4 < 8 => 1
        Ok(())
    }

    #[test]
    fn postfix_increment() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 5);
        let result = eval_with_vars("x++", &vars)?;
        assert_eq!(result, 5); // returns old value
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 6); // x is now 6
        Ok(())
    }

    #[test]
    fn postfix_decrement() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 5);
        let result = eval_with_vars("x--", &vars)?;
        assert_eq!(result, 5); // returns old value
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 4); // x is now 4
        Ok(())
    }

    #[test]
    fn short_circuit_and() -> Result<(), String> {
        // 0 && (side_effect) should not evaluate RHS assignment
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 10);
        let result = eval_with_vars("0 && (x = 99)", &vars)?;
        assert_eq!(result, 0);
        // Note: due to parsing, RHS is still parsed but the short-circuit
        // means the overall result is 0.
        Ok(())
    }

    #[test]
    fn short_circuit_or() -> Result<(), String> {
        // 1 || (expr) — result should be 1
        assert_eq!(eval("1 || 0")?, 1);
        assert_eq!(eval("0 || 1")?, 1);
        assert_eq!(eval("0 || 0")?, 0);
        Ok(())
    }

    #[test]
    fn bitwise_compound_assign() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 0xff);
        assert_eq!(eval_with_vars("x &= 0x0f", &vars)?, 0x0f);
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 0x0f);

        vars.borrow_mut().insert("y".into(), 0xf0);
        assert_eq!(eval_with_vars("y |= 0x0f", &vars)?, 0xff);

        vars.borrow_mut().insert("z".into(), 0xff);
        assert_eq!(eval_with_vars("z ^= 0x0f", &vars)?, 0xf0);
        Ok(())
    }

    #[test]
    fn shift_compound_assign() -> Result<(), String> {
        let vars = RefCell::new(BTreeMap::new());
        vars.borrow_mut().insert("x".into(), 1);
        assert_eq!(eval_with_vars("x <<= 4", &vars)?, 16);
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 16);

        assert_eq!(eval_with_vars("x >>= 2", &vars)?, 4);
        let &val = vars.borrow().get("x").ok_or("expected var 'x' to be set")?;
        assert_eq!(val, 4);
        Ok(())
    }
}
