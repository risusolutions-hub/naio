//! IPv4/IPv6 addresses, CIDR networks, ranges, subnet math, membership checks.
//! (~Python `ipaddress` subset)

mod addr;
mod batch;
mod error;
mod fmt;
mod network;
mod parse;
mod range;

pub use addr::{
    add_v4, add_v6, compare_v4, compare_v6, reverse_ptr_v4, reverse_ptr_v6, v4_is_global,
    v4_is_link_local, v4_is_loopback, v4_is_multicast, v4_is_private, v4_is_reserved,
    v4_is_unspecified, v6_is_global, v6_is_link_local, v6_is_loopback, v6_is_multicast,
    v6_is_private, v6_is_reserved, v6_is_site_local, v6_is_unspecified,
};
pub use batch::{contains_many, filter_containing};
pub use error::{IpError, IpResult};
pub use fmt::{ipv6_compressed, ipv6_exploded};
pub use network::{
    address_exclude_v4, address_exclude_v6, broadcast_v4, collect_hosts_v4, collect_hosts_v6,
    entity_contains, entity_overlaps, hostmask_v4, iface_ip, iface_network, netmask_v4,
    num_addresses_v4, num_addresses_v6, subnet_of, subnets_v4, subnets_v6, supernet_of,
    supernet_v4, supernet_v6, with_hostmask_v4, with_netmask_v4, with_prefixlen, DEFAULT_MAX_HOSTS,
};
pub use parse::{
    hostmask_to_prefix_v4, netmask_to_prefix_v4, parse_address, parse_interface,
    parse_ipv4_address, parse_ipv4_network, parse_ipv6_address, parse_ipv6_network, parse_network,
    prefix_to_hostmask_v4, prefix_to_netmask_v4, valid_address, valid_interface, valid_network,
    IpEntity,
};
pub use range::{collapse_networks, summarize_range};
