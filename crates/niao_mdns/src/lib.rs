//! mDNS / DNS-SD service discovery and announcement for Niao (~zeroconf).
//!
//! Pure-Rust DNS wire codec + multicast browse/register. No external crates.

mod dns;
mod error;
mod mdns;
mod service;
mod txt;

pub use dns::{
    build_query, decode_a_rdata, decode_aaaa_rdata, decode_message, decode_name, decode_ptr_rdata,
    decode_srv_rdata, encode_a_rdata, encode_aaaa_rdata, encode_message, encode_name,
    encode_ptr_rdata, encode_srv_rdata, join_labels, names_equal, normalize_name, split_labels,
    DnsMessage, Question, RecordType, ResourceRecord, CLASS_CACHE_FLUSH, CLASS_IN, QCLASS_UNICAST,
};
pub use error::{MdnsError, MdnsResult};
pub use mdns::{announce_and_browse, MdnsClient, MDNS_GROUP_V4, MDNS_PORT};
pub use service::{
    is_mdns_type, localhost_name, loopback_v4, normalize_service_type, parse_ip,
    services_from_message, DiscoveredService, ServiceInfo, DEFAULT_TTL,
};
pub use txt::{pack_txt, unpack_txt};
