use std::net::{IpAddr, Ipv4Addr};

pub fn lan_ipv4_values() -> Vec<String> {
    let entries = local_ip_address::list_afinet_netifas().unwrap_or_default();
    let primary_ipv4 = local_ip_address::local_ip().ok().and_then(|ip| match ip {
        IpAddr::V4(ipv4) if is_usable_lan_ipv4(ipv4) => Some(ipv4),
        _ => None,
    });

    collect_lan_ipv4_values(entries, primary_ipv4)
}

fn collect_lan_ipv4_values(
    entries: Vec<(String, IpAddr)>,
    primary_ipv4: Option<Ipv4Addr>,
) -> Vec<String> {
    let mut candidates = Vec::new();

    for (name, ip) in entries {
        let IpAddr::V4(ipv4) = ip else {
            continue;
        };
        if !is_usable_lan_ipv4(ipv4) || is_virtual_interface_name(&name) {
            continue;
        }

        push_unique(&mut candidates, ipv4.to_string());
    }

    if let Some(primary_ipv4) = primary_ipv4 {
        let primary = primary_ipv4.to_string();
        if let Some(index) = candidates
            .iter()
            .position(|candidate| candidate == &primary)
        {
            let primary = candidates.remove(index);
            candidates.insert(0, primary);
        }
    }

    candidates
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn is_usable_lan_ipv4(ipv4: Ipv4Addr) -> bool {
    ipv4.is_private()
        && !ipv4.is_loopback()
        && !ipv4.is_link_local()
        && !ipv4.is_broadcast()
        && !ipv4.is_unspecified()
        && !ipv4.is_multicast()
        && !is_documentation_ipv4(ipv4)
}

fn is_documentation_ipv4(ipv4: Ipv4Addr) -> bool {
    matches!(
        ipv4.octets(),
        [192, 0, 2, _] | [198, 51, 100, _] | [203, 0, 113, _]
    )
}

fn is_virtual_interface_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "loopback",
        "docker",
        "hyper-v",
        "vethernet",
        "vmware",
        "virtualbox",
        "vbox",
        "wsl",
        "npcap",
        "bluetooth",
        "bridge",
        "zerotier",
        "tailscale",
        "wireguard",
        "wintun",
        "vpn",
        "tun",
        "tap",
        "clash",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_primary_physical_ipv4_first() {
        let values = collect_lan_ipv4_values(
            vec![
                (
                    "Ethernet".to_string(),
                    IpAddr::V4(Ipv4Addr::new(192, 168, 31, 20)),
                ),
                (
                    "WLAN".to_string(),
                    IpAddr::V4(Ipv4Addr::new(10, 9, 103, 31)),
                ),
                (
                    "vEthernet (Default Switch)".to_string(),
                    IpAddr::V4(Ipv4Addr::new(172, 28, 112, 1)),
                ),
            ],
            Some(Ipv4Addr::new(10, 9, 103, 31)),
        );

        assert_eq!(values, vec!["10.9.103.31", "192.168.31.20"]);
    }

    #[test]
    fn keeps_non_virtual_private_ipv4_values_when_primary_is_missing() {
        let values = collect_lan_ipv4_values(
            vec![
                (
                    "WLAN".to_string(),
                    IpAddr::V4(Ipv4Addr::new(10, 9, 103, 31)),
                ),
                (
                    "Ethernet".to_string(),
                    IpAddr::V4(Ipv4Addr::new(192, 168, 31, 20)),
                ),
                (
                    "Loopback Pseudo-Interface 1".to_string(),
                    IpAddr::V4(Ipv4Addr::LOCALHOST),
                ),
                (
                    "DockerNAT".to_string(),
                    IpAddr::V4(Ipv4Addr::new(192, 168, 65, 1)),
                ),
                (
                    "vEthernet (WSL)".to_string(),
                    IpAddr::V4(Ipv4Addr::new(172, 27, 224, 1)),
                ),
                (
                    "WLAN".to_string(),
                    IpAddr::V4(Ipv4Addr::new(169, 254, 10, 20)),
                ),
                ("WLAN".to_string(), IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))),
            ],
            None,
        );

        assert_eq!(values, vec!["10.9.103.31", "192.168.31.20"]);
    }

    #[test]
    fn ignores_virtual_private_ipv4_values_when_no_physical_candidate_exists() {
        let values = collect_lan_ipv4_values(
            vec![
                (
                    "ZeroTier One".to_string(),
                    IpAddr::V4(Ipv4Addr::new(10, 15, 20, 9)),
                ),
                (
                    "vEthernet (Default Switch)".to_string(),
                    IpAddr::V4(Ipv4Addr::new(172, 28, 112, 1)),
                ),
                (
                    "DockerNAT".to_string(),
                    IpAddr::V4(Ipv4Addr::new(192, 168, 65, 1)),
                ),
            ],
            Some(Ipv4Addr::new(10, 15, 20, 9)),
        );

        assert!(values.is_empty());
    }
}
