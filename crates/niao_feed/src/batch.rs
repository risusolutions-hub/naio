use crate::error::FeedResult;
use crate::model::FeedDocument;
use crate::parse::{parse, ParseOptions};
use niao_parallel::map as parallel_map;

/// Parallel-parse many feed strings.
///
/// >>> use niao_feed::{parallel_parse, ParseOptions};
/// >>> let xml = "<rss version=\"2.0\"><channel><title>A</title></channel></rss>";
/// >>> let out = parallel_parse(&[xml.into(), xml.into()], &ParseOptions::default(), 2);
/// >>> out.len() == 2
/// true
pub fn parallel_parse(
    inputs: &[String],
    opts: &ParseOptions,
    threads: usize,
) -> Vec<FeedResult<FeedDocument>> {
    parallel_map(inputs, threads, |s| parse(s, opts))
}
