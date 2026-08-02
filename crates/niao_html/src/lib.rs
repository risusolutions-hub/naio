//! Forgiving HTML5 parser, CSS selectors, tree walking, text extraction, escape/unescape.
//! (~BeautifulSoup4 subset)

mod batch;
mod error;
mod escape;
mod node;
mod parse;
mod select;
mod serialize;
mod text;
mod walk;

pub use batch::{parallel_extract_text, parallel_parse, parallel_select};
pub use error::{HtmlError, HtmlResult};
pub use escape::{escape, escape_attr, unescape};
pub use node::{
    attr, attrs, classes, has_attr, has_class, id_attr, is_comment, is_element, is_tag, is_text,
    tag,
};
pub use parse::{
    alloc_document, parse_document, parse_fragment, root_node, unpack_node, DocumentStore,
};
pub use select::{
    compile_selector, matches, parse_selector, select_nodes, select_one, select_with_handle,
    valid_selector, SelectorStore,
};
pub use serialize::{inner_html, outer_html, prettify};
pub use text::{extract_text, node_direct_text, node_text, strip_tags, TextOpts};
pub use walk::{
    ancestors, child_elements, children, descendants, find, find_all, next_sibling, node_type,
    parent, prev_sibling, siblings,
};
