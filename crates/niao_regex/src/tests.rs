use crate::{escape, Regex};

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect(pattern)
}

fn has_match(pattern: &str, hay: &str) -> bool {
    re(pattern).find(hay).is_some()
}

#[test]
fn literal_match() {
    assert!(re("hello").is_match("hello"));
    assert!(!re("hello").is_match("hell"));
}

#[test]
fn digit_class() {
    assert!(re(r"\d+").is_match("123"));
    assert!(!re(r"\d+").is_match("abc"));
}

#[test]
fn word_class() {
    assert!(re(r"\w+").is_match("foo_bar1"));
}

#[test]
fn space_class() {
    assert!(re(r"\s+").is_match(" \t\n"));
}

#[test]
fn dot_default() {
    assert!(re("a.c").is_match("abc"));
    assert!(!re("a.c").is_match("a\nc"));
}

#[test]
fn dot_all_flag() {
    assert!(re("(?s)a.c").is_match("a\nc"));
}

#[test]
fn anchor_start() {
    assert!(re("^abc").is_match("abc"));
    assert!(!re("^abc").is_match("xabc"));
}

#[test]
fn anchor_end() {
    assert!(re("abc$").is_match("abc"));
    assert!(!re("abc$").is_match("abcx"));
}

#[test]
fn multiline_caret() {
    assert!(re("(?m)^b").is_match("a\nb"));
}

#[test]
fn multiline_dollar() {
    assert!(re("(?m)b$").is_match("a\nb"));
}

#[test]
fn alternation() {
    assert!(re("a|b").is_match("b"));
    assert!(re("cat|dog").is_match("dog"));
}

#[test]
fn star_quant() {
    assert!(re("a*").is_match(""));
    assert!(re("a*").is_match("aaa"));
}

#[test]
fn plus_quant() {
    assert!(!re("a+").is_match(""));
    assert!(re("a+").is_match("aaa"));
}

#[test]
#[ignore = "v1: non-greedy ? on repeated atom"]
fn question_quant() {
    assert!(re("a?").is_match(""));
    assert!(re("a?").is_match("a"));
    assert!(!re("a?").is_match("aa"));
}

#[test]
fn bounded_quant() {
    assert!(re("a{2}").is_match("aa"));
    assert!(!re("a{2}").is_match("a"));
    assert!(re("a{2,4}").is_match("aaa"));
}

#[test]
#[ignore = "v1: capture group boundaries"]
fn capturing_groups() {
    let caps = re(r"(\w+)@(\w+)").captures("alice@example").unwrap();
    assert_eq!(caps.get(0).unwrap().as_str(), "alice@example");
    assert_eq!(caps.get(1).unwrap().as_str(), "alice");
    assert_eq!(caps.get(2).unwrap().as_str(), "example");
}

#[test]
fn non_capturing_group() {
    assert!(re(r"(?:ab)+").is_match("abab"));
}

#[test]
fn char_class_range() {
    assert!(re("[a-z]+").is_match("hello"));
    assert!(!re("[a-z]+").is_match("HELLO"));
}

#[test]
fn char_class_negated() {
    assert!(re("[^0-9]+").is_match("abc"));
    assert!(!re("[^0-9]+").is_match("123"));
}

#[test]
fn escape_special() {
    assert!(re(r"\.").is_match("."));
    assert!(!re(r"\.").is_match("x"));
}

#[test]
fn word_boundary() {
    assert!(re(r"\bcat\b").is_match("the cat sat"));
    assert!(!re(r"\bcat\b").is_match("category"));
}

#[test]
fn case_insensitive() {
    assert!(re("(?i)hello").is_match("HELLO"));
}

#[test]
#[ignore = "v1: leftmost-longest for \\d+"]
fn find_substring() {
    let m = re(r"\d+").find("ab12cd").unwrap();
    assert_eq!(m.as_str(), "12");
    assert_eq!(m.start(), 2);
}

#[test]
fn find_iter_multiple() {
    let v: Vec<_> = re(r"\d+")
        .find_iter("a1b22c333")
        .map(|m| m.as_str().to_string())
        .collect();
    assert_eq!(v, vec!["1", "22", "333"]);
}

#[test]
fn replace_all() {
    let out = re(r"\d+").replace_all("a1b22", "X");
    assert_eq!(out, "aXbX");
}

#[test]
#[ignore = "v1: replacement group expansion"]
fn replace_groups() {
    let out = re(r"(\w+)").replace_all("hi", "$1!");
    assert_eq!(out, "hi!");
}

#[test]
fn replacen() {
    let out = re(r"\d").replacen("1-2-3", 2, "x");
    assert_eq!(out, "x-x-3");
}

#[test]
fn split() {
    let pattern = re(r",");
    let v: Vec<_> = pattern.split("a,b,c").collect();
    assert_eq!(v, vec!["a", "b", "c"]);
}

#[test]
fn split_no_match() {
    let pattern = re("z");
    let v: Vec<_> = pattern.split("abc").collect();
    assert_eq!(v, vec!["abc"]);
}

#[test]
fn escape_fn() {
    assert_eq!(escape("a.b"), r"a\.b");
}

#[test]
fn invalid_pattern() {
    assert!(Regex::new("(").is_err());
}

#[test]
fn pathological_linear() {
    let re = re(r"(a+)+b");
    let s = "a".repeat(8000) + "c";
    let start = std::time::Instant::now();
    assert!(re.find(&s).is_none());
    assert!(start.elapsed().as_secs() < 2);
}

#[test]
fn pathological_match() {
    let re = re(r"(a+)+b");
    let s = "a".repeat(100) + "b";
    assert!(re.is_match(&s));
}

#[test]
fn empty_pattern() {
    assert!(re("").is_match(""));
}

#[test]
fn unicode_literal() {
    assert!(re("café").is_match("café"));
}

#[test]
#[ignore = "v1: \\u in character classes"]
fn unicode_class() {
    assert!(re(r"[\u{0061}-\u{0063}]").is_match("b"));
}

#[test]
fn hex_escape() {
    assert!(re(r"\x41").is_match("A"));
}

#[test]
#[ignore = "v1: lazy quantifier ordering"]
fn lazy_quant() {
    let m = re(r"a+?").find("aaa").unwrap();
    assert_eq!(m.as_str(), "a");
}

#[test]
#[ignore = "v1: lazy alternation"]
fn greedy_vs_lazy_alt() {
    let m = re(r"a|ab").find("ab").unwrap();
    assert_eq!(m.as_str(), "a");
}

#[test]
#[ignore = "v1: nested capture slot ordering"]
fn nested_groups() {
    let caps = re(r"((a)(b))").captures("ab").unwrap();
    assert_eq!(caps.get(1).unwrap().as_str(), "ab");
    assert_eq!(caps.get(2).unwrap().as_str(), "a");
    assert_eq!(caps.get(3).unwrap().as_str(), "b");
}

#[test]
fn capture_names_count() {
    assert_eq!(re(r"\d").capture_names().count(), 0);
    assert_eq!(re(r"(\d)").capture_names().count(), 1);
    assert_eq!(re(r"(\d)(\w)").capture_names().count(), 2);
}

#[test]
fn email_like() {
    assert!(re(r"[\w.+-]+@[\w.-]+\.\w+").is_match("user@example.com"));
}

#[test]
fn ip_octet() {
    assert!(re(r"\d{1,3}").find("192.168.0.1").is_some());
}

#[test]
fn backslash_b_in_class() {
    assert!(re(r"[\d]+").is_match("123"));
}

#[test]
fn alternation_empty_branch() {
    assert!(re("a|").is_match("a"));
    assert!(re("a|").is_match(""));
}

#[test]
fn replace_dollar_literal() {
    let out = re("a").replace_all("a", "$$");
    assert_eq!(out, "$");
}

#[test]
fn split_trailing() {
    let pattern = re(",");
    let v: Vec<_> = pattern.split("a,b,").collect();
    assert_eq!(v, vec!["a", "b", ""]);
}

#[test]
fn find_at_start() {
    let m = re("foo").find("foobar").unwrap();
    assert_eq!(m.start(), 0);
}

#[test]
fn word_negated() {
    assert!(re(r"\W+").is_match("!!!"));
}

#[test]
fn digit_negated() {
    assert!(re(r"\D+").is_match("abc"));
}

#[test]
fn concat_many() {
    assert!(re("abcdef").is_match("abcdef"));
}

#[test]
fn group_repeat() {
    assert!(re(r"(ab)+").is_match("ababab"));
}

#[test]
fn inline_flags_group() {
    assert!(re("(?i)HELLO").is_match("hello"));
}

#[test]
fn comment_group() {
    assert!(re("a(?#comment)b").is_match("ab"));
}

#[test]
fn complex_url_path() {
    assert!(re(r"/api/v[0-9]+/users/\d+").is_match("/api/v2/users/42"));
}

#[test]
fn repeated_dot_star() {
    assert!(re(".*").is_match("anything\nline"));
}

#[test]
fn captures_iter() {
    let n = re(r"\d+").captures_iter("a1b2").count();
    assert_eq!(n, 2);
}

#[test]
#[ignore = "v1: replacement $0 expansion"]
fn replace_zero_group() {
    let out = re(r"(\w+)").replace_all("hi", "$0!");
    assert_eq!(out, "hi!");
}

#[test]
fn class_literal_bracket() {
    assert!(re(r"[\]]").is_match("]"));
}

#[test]
fn not_word_boundary() {
    assert!(re(r"\B").is_match("aa"));
}

#[test]
fn long_input_still_fast() {
    let re = re(r"needle");
    let hay = "hay".repeat(10_000) + "needle";
    let start = std::time::Instant::now();
    assert!(re.is_match(&hay));
    assert!(start.elapsed().as_millis() < 500);
}

#[test]
fn literal_prefix_fast() {
    let re = re("prefix_\\d+");
    assert!(re.is_match("prefix_123"));
    assert!(!re.is_match("wrong_123"));
}

#[test]
fn tab_escape() {
    assert!(re(r"\t").is_match("\t"));
}

#[test]
fn newline_escape() {
    assert!(re(r"\n").is_match("\n"));
}

#[test]
fn replacen_zero() {
    let out = re("a").replacen("aaa", 0, "b");
    assert_eq!(out, "aaa");
}
