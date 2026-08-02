//! Parallel batch membership checks.

use crate::error::IpResult;
use crate::network::entity_contains;
use crate::parse::IpEntity;
use rayon::prelude::*;

/// Test many addresses against one network in parallel.
pub fn contains_many(network: &IpEntity, candidates: &[IpEntity]) -> IpResult<Vec<bool>> {
    candidates
        .par_iter()
        .map(|c| entity_contains(network, c))
        .collect()
}

/// Return only candidates contained in the network.
pub fn filter_containing(network: &IpEntity, candidates: &[IpEntity]) -> IpResult<Vec<IpEntity>> {
    let flags = contains_many(network, candidates)?;
    Ok(candidates
        .iter()
        .zip(flags)
        .filter_map(|(c, ok)| if ok { Some(c.clone()) } else { None })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{parse_address, parse_network};
    use std::str::FromStr;

    #[test]
    fn batch_contains() {
        let net = parse_network("10.0.0.0/8", true).unwrap();
        let addrs: Vec<IpEntity> = ["10.0.0.1", "192.168.0.1", "10.1.2.3"]
            .iter()
            .map(|s| parse_address(s).unwrap())
            .collect();
        let flags = contains_many(&net, &addrs).unwrap();
        assert_eq!(flags, vec![true, false, true]);
    }
}
