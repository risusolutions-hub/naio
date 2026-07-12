//! WPT urltestdata.json subset (WHATWG URL Standard).

use super::parse_url;

struct WptCase {
    input: &'static str,
    scheme: Option<&'static str>,
    host: Option<&'static str>,
    port: Option<u16>,
    path: Option<&'static str>,
    query: Option<&'static str>,
    fragment: Option<&'static str>,
    fail: bool,
}

const CASES: &[WptCase] = &[
    WptCase {
        input: "http://example.com/",
        scheme: Some("http"),
        host: Some("example.com"),
        port: None,
        path: Some("/"),
        query: None,
        fragment: None,
        fail: false,
    },
    WptCase {
        input: "http://example.com/path/to/file",
        scheme: Some("http"),
        host: Some("example.com"),
        port: None,
        path: Some("/path/to/file"),
        query: None,
        fragment: None,
        fail: false,
    },
    WptCase {
        input: "https://example.com:443/",
        scheme: Some("https"),
        host: Some("example.com"),
        port: None,
        path: Some("/"),
        query: None,
        fragment: None,
        fail: false,
    },
    WptCase {
        input: "http://user:pass@example.com:8080/x?y=1#z",
        scheme: Some("http"),
        host: Some("example.com"),
        port: Some(8080),
        path: Some("/x"),
        query: Some("y=1"),
        fragment: Some("z"),
        fail: false,
    },
    WptCase {
        input: "http://[::1]/",
        scheme: Some("http"),
        host: Some("::1"),
        port: None,
        path: Some("/"),
        query: None,
        fragment: None,
        fail: false,
    },
    WptCase {
        input: "http://EXAMPLE.com",
        scheme: Some("http"),
        host: Some("EXAMPLE.com"),
        port: None,
        path: Some("/"),
        query: None,
        fragment: None,
        fail: false,
    },
    WptCase {
        input: "http://example.com?a=b&c",
        scheme: Some("http"),
        host: Some("example.com"),
        port: None,
        path: Some("/"),
        query: Some("a=b&c"),
        fragment: None,
        fail: false,
    },
    WptCase {
        input: "not a url",
        scheme: None,
        host: None,
        port: None,
        path: None,
        query: None,
        fragment: None,
        fail: true,
    },
    WptCase {
        input: "http:///path-only",
        scheme: None,
        host: None,
        port: None,
        path: None,
        query: None,
        fragment: None,
        fail: true,
    },
];

#[test]
fn wpt_url_subset() {
    for case in CASES {
        let result = parse_url(case.input);
        if case.fail {
            assert!(result.is_err(), "expected fail for {:?}", case.input);
            continue;
        }
        let u = result.unwrap_or_else(|e| panic!("parse {:?}: {e}", case.input));
        if let Some(s) = case.scheme {
            assert_eq!(u.scheme, s, "scheme for {:?}", case.input);
        }
        if let Some(h) = case.host {
            assert_eq!(u.host, h, "host for {:?}", case.input);
        }
        if let Some(p) = case.port {
            assert_eq!(u.port, p, "port for {:?}", case.input);
        } else {
            let c = u.components();
            assert_eq!(c.port, None, "default port for {:?}", case.input);
        }
        if let Some(p) = case.path {
            assert_eq!(u.path, p, "path for {:?}", case.input);
        }
        if let Some(q) = case.query {
            assert_eq!(u.query, q, "query for {:?}", case.input);
        }
        if let Some(f) = case.fragment {
            assert_eq!(u.fragment, f, "fragment for {:?}", case.input);
        }
    }
}

#[test]
fn wpt_resolution_subset() {
    let base = parse_url("http://www.example.com/a/b/c").unwrap();
    let joined = base.join("d/e").unwrap();
    assert_eq!(joined.path, "/a/b/d/e");
    assert_eq!(joined.origin(), "http://www.example.com");

    let joined = base.join("/root").unwrap();
    assert_eq!(joined.path, "/root");

    let joined = base.join("?q=1").unwrap();
    assert_eq!(joined.query, "q=1");
    assert_eq!(joined.path, "/a/b/c");
}
