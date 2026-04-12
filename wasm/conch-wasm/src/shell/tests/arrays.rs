use super::*;

// ---------------------------------------------------------------------------
// Indexed array basics
// ---------------------------------------------------------------------------

#[test]
fn array_assignment_and_index() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, code, _) = s.run_line("echo ${arr[0]}");
    assert_eq!(code, 0);
    assert_eq!(out, "a\n");
}

#[test]
fn array_index_1() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${arr[1]}");
    assert_eq!(out, "b\n");
}

#[test]
fn array_index_2() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${arr[2]}");
    assert_eq!(out, "c\n");
}

#[test]
fn array_all_elements_at() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${arr[@]}");
    assert_eq!(out, "a b c\n");
}

#[test]
fn array_all_elements_star() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${arr[*]}");
    assert_eq!(out, "a b c\n");
}

#[test]
fn array_length() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${#arr[@]}");
    assert_eq!(out, "3\n");
}

#[test]
fn array_length_star() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${#arr[*]}");
    assert_eq!(out, "3\n");
}

#[test]
fn array_append() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    s.run_line("arr+=(d e)");
    let (out, _, _) = s.run_line("echo ${#arr[@]}");
    assert_eq!(out, "5\n");
    let (out2, _, _) = s.run_line("echo ${arr[3]}");
    assert_eq!(out2, "d\n");
    let (out3, _, _) = s.run_line("echo ${arr[4]}");
    assert_eq!(out3, "e\n");
}

#[test]
fn array_indices() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    let (out, _, _) = s.run_line("echo ${!arr[@]}");
    assert_eq!(out, "0 1 2\n");
}

#[test]
fn declare_a_creates_empty_array() {
    let mut s = shell();
    s.run_line("declare -a myarr");
    let (out, _, _) = s.run_line("echo ${#myarr[@]}");
    assert_eq!(out, "0\n");
}

// ---------------------------------------------------------------------------
// Indexed array element mutation
// ---------------------------------------------------------------------------

#[test]
fn array_element_assignment() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    s.run_line("arr[1]=X");
    let (out, _, _) = s.run_line("echo ${arr[1]}");
    assert_eq!(out, "X\n");
}

#[test]
fn array_element_assignment_extends() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    s.run_line("arr[5]=Z");
    let (out, _, _) = s.run_line("echo ${arr[5]}");
    assert_eq!(out, "Z\n");
    let (out2, _, _) = s.run_line("echo ${#arr[@]}");
    assert_eq!(out2, "6\n");
}

#[test]
fn unset_array_element() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    s.run_line("unset arr[1]");
    let (out, _, _) = s.run_line("echo ${arr[1]}");
    assert_eq!(out, "\n");
}

#[test]
fn unset_entire_array() {
    let mut s = shell();
    s.run_line("arr=(a b c)");
    s.run_line("unset arr");
    let (out, _, _) = s.run_line("echo ${#arr[@]}");
    assert_eq!(out, "0\n");
}

// ---------------------------------------------------------------------------
// Array iteration and construction
// ---------------------------------------------------------------------------

#[test]
fn array_for_loop() {
    let mut s = shell();
    let (out, code) = s.run_script("arr=(x y z)\nfor item in ${arr[@]}; do\n  echo $item\ndone");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.trim().lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines, vec!["x", "y", "z"]);
}

#[test]
fn array_with_quoted_elements() {
    let mut s = shell();
    s.run_line("arr=(\"hello world\" foo bar)");
    let (out, _, _) = s.run_line("echo ${arr[0]}");
    assert_eq!(out, "hello world\n");
    let (out2, _, _) = s.run_line("echo ${#arr[@]}");
    assert_eq!(out2, "3\n");
}

#[test]
fn array_empty() {
    let mut s = shell();
    s.run_line("arr=()");
    let (out, _, _) = s.run_line("echo ${#arr[@]}");
    assert_eq!(out, "0\n");
}

#[test]
fn array_with_variable_expansion() {
    let mut s = shell();
    s.run_line("x=hello");
    s.run_line("arr=($x world)");
    let (out, _, _) = s.run_line("echo ${arr[0]}");
    assert_eq!(out, "hello\n");
    let (out2, _, _) = s.run_line("echo ${arr[1]}");
    assert_eq!(out2, "world\n");
}

// ---------------------------------------------------------------------------
// Associative arrays (declare -A)
// ---------------------------------------------------------------------------

#[test]
fn assoc_array_declare_and_assign() {
    let mut s = shell();
    s.run_line("declare -A map");
    s.run_line("map[name]=alice");
    s.run_line("map[age]=30");
    let (out, _, _) = s.run_line("echo ${map[name]}");
    assert_eq!(out, "alice\n");
    let (out2, _, _) = s.run_line("echo ${map[age]}");
    assert_eq!(out2, "30\n");
}

#[test]
fn assoc_array_keys() {
    let mut s = shell();
    s.run_line("declare -A map");
    s.run_line("map[a]=1");
    s.run_line("map[b]=2");
    let (out, _, _) = s.run_line("echo ${!map[@]}");
    assert_eq!(out, "a b\n");
}

#[test]
fn assoc_array_count() {
    let mut s = shell();
    s.run_line("declare -A map");
    s.run_line("map[x]=1");
    s.run_line("map[y]=2");
    s.run_line("map[z]=3");
    let (out, _, _) = s.run_line("echo ${#map[@]}");
    assert_eq!(out, "3\n");
}

#[test]
fn assoc_array_unset_key() {
    let mut s = shell();
    s.run_line("declare -A map");
    s.run_line("map[a]=1");
    s.run_line("map[b]=2");
    s.run_line("unset map[a]");
    let (out, _, _) = s.run_line("echo ${#map[@]}");
    assert_eq!(out, "1\n");
    let (out2, _, _) = s.run_line("echo ${map[a]}");
    assert_eq!(out2, "\n");
}

#[test]
fn assoc_array_unset_whole() {
    let mut s = shell();
    s.run_line("declare -A map");
    s.run_line("map[a]=1");
    s.run_line("unset map");
    let (out, _, _) = s.run_line("echo ${#map[@]}");
    assert_eq!(out, "0\n");
}

#[test]
fn assoc_array_overwrite() {
    let mut s = shell();
    s.run_line("declare -A m");
    s.run_line("m[k]=old");
    s.run_line("m[k]=new");
    let (out, _, _) = s.run_line("echo ${m[k]}");
    assert_eq!(out, "new\n");
}

// ---------------------------------------------------------------------------
// mapfile / readarray
// ---------------------------------------------------------------------------

#[test]
fn mapfile_basic() {
    let mut s = shell();
    let (_, code) = s.run_script("printf 'a\\nb\\nc\\n' | mapfile lines");
    assert_eq!(code, 0);
    let arr = s.vars.arrays.get("lines").cloned().unwrap_or_default();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], "a\n");
    assert_eq!(arr[1], "b\n");
    assert_eq!(arr[2], "c\n");
}

#[test]
fn mapfile_strip_newlines() {
    let mut s = shell();
    let (_, code) = s.run_script("printf 'x\\ny\\n' | mapfile -t myarr");
    assert_eq!(code, 0);
    let arr = s.vars.arrays.get("myarr").cloned().unwrap_or_default();
    assert_eq!(arr, vec!["x".to_string(), "y".to_string()]);
}

#[test]
fn readarray_alias() {
    let mut s = shell();
    let (_, code) = s.run_script("printf 'one\\ntwo\\n' | readarray -t items");
    assert_eq!(code, 0);
    let arr = s.vars.arrays.get("items").cloned().unwrap_or_default();
    assert_eq!(arr, vec!["one".to_string(), "two".to_string()]);
}
