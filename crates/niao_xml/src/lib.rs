//! XML DOM + streaming parser, namespaces, XPath subset, pretty-print.

mod dom;
mod emit;
mod error;
mod parse;
mod stream;
mod xpath;

pub use dom::{
    deep_copy_element, parent_path, resolve_element, resolve_element_mut, Attr, Document, Element,
    Node, NodePath, XmlOpts,
};
pub use emit::{pretty, pretty_doc, to_string_doc, to_string_element};
pub use error::{XmlError, MAX_BYTES, MAX_NODES};
pub use parse::{parse, parse_bytes};
pub use stream::{stream_collect, StreamEvent, StreamOpts, XmlStreamOwned};
pub use xpath::{find, findall, findtext, iter_elements};

use niao_parallel::map as parallel_map;

/// Parallel-parse many XML strings.
pub fn parallel_parse(
    inputs: &[String],
    opts: &XmlOpts,
    threads: usize,
) -> Vec<Result<Document, XmlError>> {
    parallel_map(inputs, threads, |s| parse(s, opts))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let xml = r#"<root><a k="v">text</a></root>"#;
        let doc = parse(xml, &XmlOpts::default()).unwrap();
        let out = to_string_doc(&doc, &XmlOpts::default()).unwrap();
        let doc2 = parse(&out, &XmlOpts::default()).unwrap();
        assert_eq!(
            doc.root.as_ref().unwrap().child_elements()[0].get_attr("k"),
            doc2.root.as_ref().unwrap().child_elements()[0].get_attr("k")
        );
    }
}
