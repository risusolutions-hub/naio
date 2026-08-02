//! Micro-benchmarks for `niao_feed` hot paths.
//! Run: cargo run -p niao_feed --bin nfeed_bench --release

use niao_feed::{
    emit, format_date, parallel_parse, parse, parse_bytes, strip_html, EmitFormat, EmitOptions,
    ParseOptions,
};
use std::time::Instant;

const RSS_SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Benchmark Feed</title>
    <link>https://example.com/</link>
    <description>A moderately sized RSS feed for micro-benchmarks</description>
    <language>en</language>
    <lastBuildDate>Mon, 06 Sep 2010 00:01:00 +0000</lastBuildDate>
    <image>
      <url>https://example.com/logo.png</url>
      <title>Example</title>
      <link>https://example.com/</link>
    </image>
    <item>
      <title>First post with a longer title for realism</title>
      <link>https://example.com/posts/1</link>
      <guid isPermaLink="true">https://example.com/posts/1</guid>
      <pubDate>Mon, 06 Sep 2010 00:01:00 +0000</pubDate>
      <description><![CDATA[<p>Hello <strong>world</strong> from entry one.</p>]]></description>
      <category domain="https://example.com/tags">news</category>
      <enclosure url="https://example.com/audio/1.mp3" length="12345" type="audio/mpeg"/>
    </item>
    <item>
      <title>Second post</title>
      <link>https://example.com/posts/2</link>
      <guid>uuid-2</guid>
      <pubDate>Tue, 07 Sep 2010 12:00:00 +0000</pubDate>
      <description>Plain text summary for the second item.</description>
    </item>
    <item>
      <title>Third post</title>
      <link>https://example.com/posts/3</link>
      <description>Another item body with enough text to matter.</description>
    </item>
  </channel>
</rss>"#;

const ATOM_SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Benchmark</title>
  <link href="https://example.com/"/>
  <id>urn:uuid:feed-bench</id>
  <updated>2010-09-06T00:01:00Z</updated>
  <entry>
    <title>Atom entry one</title>
    <link href="https://example.com/a1"/>
    <id>urn:uuid:e1</id>
    <updated>2010-09-06T00:01:00Z</updated>
    <summary type="html">&lt;p&gt;Summary&lt;/p&gt;</summary>
    <content type="html">&lt;p&gt;Full content&lt;/p&gt;</content>
  </entry>
  <entry>
    <title>Atom entry two</title>
    <link href="https://example.com/a2"/>
    <id>urn:uuid:e2</id>
    <updated>2010-09-07T12:00:00Z</updated>
    <summary>text only</summary>
  </entry>
</feed>"#;

fn bench<F: FnMut() -> u64>(name: &str, mut f: F, iters: u64) {
    for _ in 0..3 {
        f();
    }
    let start = Instant::now();
    let n = f();
    let elapsed = start.elapsed();
    let mean_ns = elapsed.as_nanos() as f64 / iters as f64;
    println!(
        "{name}: n={n} mean={mean_ns:.0} ns total={} ms",
        elapsed.as_millis()
    );
}

fn main() {
    bench(
        "parse_rss x10k",
        || {
            for _ in 0..10_000 {
                let _ = parse(RSS_SAMPLE, &ParseOptions::default()).unwrap();
            }
            10_000
        },
        10_000,
    );

    bench(
        "parse_atom x10k",
        || {
            for _ in 0..10_000 {
                let _ = parse(ATOM_SAMPLE, &ParseOptions::default()).unwrap();
            }
            10_000
        },
        10_000,
    );

    let doc = parse(RSS_SAMPLE, &ParseOptions::default()).unwrap();
    bench(
        "emit_rss2 x10k",
        || {
            for _ in 0..10_000 {
                let _ = emit(
                    &doc,
                    &EmitOptions {
                        format: EmitFormat::Rss2,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            10_000
        },
        10_000,
    );

    bench(
        "emit_atom x10k",
        || {
            for _ in 0..10_000 {
                let _ = emit(
                    &doc,
                    &EmitOptions {
                        format: EmitFormat::Atom,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            10_000
        },
        10_000,
    );

    bench(
        "emit_json x10k",
        || {
            for _ in 0..10_000 {
                let _ = emit(
                    &doc,
                    &EmitOptions {
                        format: EmitFormat::Json,
                        pretty: false,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            10_000
        },
        10_000,
    );

    let html = "<p>Hello <b>world</b> and <a href='/'>link</a></p>".repeat(20);
    bench(
        "strip_html x100k",
        || {
            for _ in 0..100_000 {
                let _ = strip_html(&html);
            }
            100_000
        },
        100_000,
    );

    bench(
        "parse_bytes x10k",
        || {
            let bytes = RSS_SAMPLE.as_bytes();
            for _ in 0..10_000 {
                let _ = parse_bytes(bytes, &ParseOptions::default()).unwrap();
            }
            10_000
        },
        10_000,
    );

    let inputs: Vec<String> = (0..64).map(|_| RSS_SAMPLE.to_string()).collect();
    bench(
        "parallel_parse 64 feeds",
        || {
            let _ = parallel_parse(&inputs, &ParseOptions::default(), 8);
            64
        },
        64,
    );

    bench(
        "format_date x100k",
        || {
            for i in 0..100_000 {
                let _ = format_date(1_280_000_000_000 + i);
            }
            100_000
        },
        100_000,
    );
}
