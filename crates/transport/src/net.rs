//! IPv4 interfaces: the inviter's address for its code, and the joiner's
//! candidate addresses (spec: discovery; design D11, D12).

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

use pairing::Code;

use crate::discovery::Sighting;

/// One IPv4 interface address and its prefix length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interface {
    pub addr: Ipv4Addr,
    pub prefix: u8,
}

/// The machine's IPv4 interfaces that are up, and the source address of its
/// default route.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Network {
    pub interfaces: Vec<Interface>,
    pub default_route: Option<Ipv4Addr>,
}

/// The operating system's view: up, non-loopback IPv4 interfaces, and the
/// default route's source address, found without sending a packet.
pub fn os_network() -> Network {
    let interfaces = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback() && i.is_oper_up())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) => Some(Interface {
                addr: v4.ip,
                prefix: prefix_len(v4.netmask),
            }),
            if_addrs::IfAddr::V6(_) => None,
        })
        .collect();
    Network {
        interfaces,
        default_route: default_route(),
    }
}

/// The source address the OS would use for a public destination. Connecting a
/// UDP socket selects a route and sends nothing. 192.0.2.1 is TEST-NET-1.
fn default_route() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(v4) if !v4.ip().is_unspecified() => Some(*v4.ip()),
        _ => None,
    }
}

fn prefix_len(netmask: Ipv4Addr) -> u8 {
    // A contiguous mask's ones; at most 32.
    u8::try_from(u32::from(netmask).count_ones()).unwrap_or(32)
}

/// The interface whose address goes into the inviter's code: the default
/// route's, else the first private one, else the first one.
pub fn inviter_interface(network: &Network) -> Option<Interface> {
    let by_route = network
        .default_route
        .and_then(|route| network.interfaces.iter().find(|i| i.addr == route));
    by_route
        .or_else(|| network.interfaces.iter().find(|i| i.addr.is_private()))
        .or_else(|| network.interfaces.first())
        .copied()
}

/// Where a joiner looks for the inviter: the code's locator on each of its own
/// interfaces (on `dial_port`), and every advertisement in pairing mode whose
/// IPv4 address ends in the locator (on its advertised port). Each address
/// once, in that order.
pub fn joiner_candidates(
    code: &Code,
    network: &Network,
    sightings: &[Sighting],
    dial_port: u16,
) -> Vec<SocketAddr> {
    let rebuilt = network
        .interfaces
        .iter()
        .map(|i| SocketAddr::from((code.inviter_addr(i.addr), dial_port)));
    let advertised = sightings.iter().filter(|s| s.pairing).flat_map(|s| {
        s.addrs
            .iter()
            .filter(|a| code.inviter_addr(**a) == **a)
            .map(|a| SocketAddr::from((*a, s.port)))
    });
    let mut out: Vec<SocketAddr> = Vec::new();
    for addr in rebuilt.chain(advertised) {
        if !out.contains(&addr) {
            out.push(addr);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use model::PeerId;

    fn iface(a: [u8; 4], prefix: u8) -> Interface {
        Interface {
            addr: Ipv4Addr::from(a),
            prefix,
        }
    }

    /// A valid code with this locator.
    fn code_with_locator(addr: [u8; 4], prefix: u8) -> Code {
        let mut rng = rand_core::OsRng;
        Code::generate(Ipv4Addr::from(addr), prefix, &mut rng)
    }

    #[test]
    fn laptop_with_wifi_and_a_vpn() {
        let network = Network {
            interfaces: vec![iface([10, 8, 0, 5], 24), iface([192, 168, 1, 137], 24)],
            default_route: Some(Ipv4Addr::new(192, 168, 1, 137)),
        };
        let chosen = inviter_interface(&network).unwrap();
        assert_eq!(chosen, iface([192, 168, 1, 137], 24));
        let c = Code::generate(chosen.addr, chosen.prefix, &mut rand_core::OsRng);
        assert_eq!(c.locator_text(), "137");
    }

    #[test]
    fn without_a_default_route_a_private_address_is_preferred() {
        let network = Network {
            interfaces: vec![iface([100, 64, 0, 9], 10), iface([172, 20, 3, 4], 16)],
            default_route: None,
        };
        assert_eq!(
            inviter_interface(&network),
            Some(iface([172, 20, 3, 4], 16))
        );
        let public_only = Network {
            interfaces: vec![iface([100, 64, 0, 9], 10)],
            default_route: Some(Ipv4Addr::new(8, 8, 8, 8)),
        };
        assert_eq!(
            inviter_interface(&public_only),
            Some(iface([100, 64, 0, 9], 10))
        );
    }

    #[test]
    fn no_ipv4_network() {
        assert_eq!(inviter_interface(&Network::default()), None);
    }

    #[test]
    fn same_subnet() {
        let c = code_with_locator([192, 168, 1, 137], 24);
        let network = Network {
            interfaces: vec![iface([192, 168, 1, 52], 24)],
            default_route: None,
        };
        assert_eq!(
            joiner_candidates(&c, &network, &[], 47391),
            ["192.168.1.137:47391".parse().unwrap()]
        );
    }

    #[test]
    fn different_subnet_found_by_mdns() {
        let c = code_with_locator([10, 20, 1, 137], 16);
        let network = Network {
            interfaces: vec![iface([192, 168, 7, 9], 24)],
            default_route: None,
        };
        let sightings = [
            Sighting {
                id: PeerId([1; 32]),
                addrs: vec![Ipv4Addr::new(10, 20, 1, 137), Ipv4Addr::new(10, 20, 9, 9)],
                port: 50000,
                pairing: true,
            },
            Sighting {
                id: PeerId([2; 32]),
                addrs: vec![Ipv4Addr::new(10, 30, 1, 137)],
                port: 50001,
                pairing: false,
            },
        ];
        assert_eq!(
            joiner_candidates(&c, &network, &sightings, 47391),
            [
                "192.168.1.137:47391".parse().unwrap(),
                "10.20.1.137:50000".parse().unwrap()
            ]
        );
    }

    #[test]
    fn candidates_are_deduplicated() {
        let c = code_with_locator([192, 168, 1, 137], 24);
        let network = Network {
            interfaces: vec![iface([192, 168, 1, 52], 24), iface([192, 168, 1, 53], 24)],
            default_route: None,
        };
        let sightings = [Sighting {
            id: PeerId([1; 32]),
            addrs: vec![Ipv4Addr::new(192, 168, 1, 137)],
            port: 47391,
            pairing: true,
        }];
        assert_eq!(joiner_candidates(&c, &network, &sightings, 47391).len(), 1);
    }

    #[test]
    fn prefix_from_netmask() {
        assert_eq!(prefix_len(Ipv4Addr::new(255, 255, 255, 0)), 24);
        assert_eq!(prefix_len(Ipv4Addr::new(255, 255, 0, 0)), 16);
        assert_eq!(prefix_len(Ipv4Addr::new(255, 255, 255, 255)), 32);
    }
}
