use std::collections::HashMap;
use std::fs;
use std::net::{Ipv4Addr, Ipv6Addr};

#[derive(Debug, Clone)]
pub struct NetConnection {
    pub protocol: String,     // "tcp", "tcp6", "udp", "udp6"
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
}

/// Collect network connections for a specific process.
pub fn collect_process_connections(pid: i32) -> Vec<NetConnection> {
    let mut connections = Vec::new();

    // Get socket inodes owned by this process
    let socket_inodes = get_socket_inodes(pid);
    if socket_inodes.is_empty() {
        return connections;
    }

    // Parse /proc/net/* for all connections, filter by inodes
    for (proto, path) in &[
        ("tcp", format!("/proc/{}/net/tcp", pid)),
        ("tcp6", format!("/proc/{}/net/tcp6", pid)),
        ("udp", format!("/proc/{}/net/udp", pid)),
        ("udp6", format!("/proc/{}/net/udp6", pid)),
    ] {
        if let Ok(content) = fs::read_to_string(path) {
            for line in content.lines().skip(1) {
                if let Some(conn) = parse_net_line(line, proto, &socket_inodes) {
                    connections.push(conn);
                }
            }
        }
    }

    connections
}

fn get_socket_inodes(pid: i32) -> HashMap<u64, ()> {
    let mut inodes = HashMap::new();
    let fd_dir = format!("/proc/{}/fd", pid);

    if let Ok(entries) = fs::read_dir(&fd_dir) {
        for entry in entries.flatten() {
            if let Ok(link) = fs::read_link(entry.path()) {
                let link_str = link.to_string_lossy();
                // Socket links look like "socket:[12345]"
                if let Some(rest) = link_str.strip_prefix("socket:[") {
                    if let Some(inode_str) = rest.strip_suffix(']') {
                        if let Ok(inode) = inode_str.parse::<u64>() {
                            inodes.insert(inode, ());
                        }
                    }
                }
            }
        }
    }

    inodes
}

fn parse_net_line(line: &str, protocol: &str, socket_inodes: &HashMap<u64, ()>) -> Option<NetConnection> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 10 {
        return None;
    }

    // Field layout: sl local_address rem_address st ... inode
    let inode: u64 = fields[9].parse().ok()?;

    if !socket_inodes.contains_key(&inode) {
        return None;
    }

    let (local_addr, local_port) = parse_addr_port(fields[1], protocol)?;
    let (remote_addr, remote_port) = parse_addr_port(fields[2], protocol)?;
    let state_num: u8 = u8::from_str_radix(fields[3], 16).ok()?;
    let state = tcp_state_name(state_num).to_string();

    Some(NetConnection {
        protocol: protocol.to_string(),
        local_addr,
        local_port,
        remote_addr,
        remote_port,
        state,
    })
}

fn parse_addr_port(addr_str: &str, protocol: &str) -> Option<(String, u16)> {
    let parts: Vec<&str> = addr_str.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let port = u16::from_str_radix(parts[1], 16).ok()?;

    let addr = if protocol.ends_with('6') {
        // IPv6: 32 hex chars
        parse_ipv6_hex(parts[0])
    } else {
        // IPv4: 8 hex chars in little-endian
        parse_ipv4_hex(parts[0])
    };

    Some((addr?, port))
}

fn parse_ipv4_hex(hex: &str) -> Option<String> {
    let num = u32::from_str_radix(hex, 16).ok()?;
    // Linux stores in network byte order but the hex is actually reversed
    let bytes = num.to_le_bytes();
    Some(format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3]))
}

fn parse_ipv6_hex(hex: &str) -> Option<String> {
    if hex.len() != 32 {
        return None;
    }
    // IPv6 in /proc/net is stored as 4 groups of 8 hex chars, each group in network byte order
    let mut segments = [0u16; 8];
    for i in 0..4 {
        let group = &hex[i * 8..(i + 1) * 8];
        let val = u32::from_str_radix(group, 16).ok()?;
        let bytes = val.to_le_bytes();
        segments[i * 2] = u16::from_be_bytes([bytes[0], bytes[1]]);
        segments[i * 2 + 1] = u16::from_be_bytes([bytes[2], bytes[3]]);
    }
    let addr = Ipv6Addr::new(
        segments[0], segments[1], segments[2], segments[3],
        segments[4], segments[5], segments[6], segments[7],
    );
    Some(addr.to_string())
}

fn tcp_state_name(state: u8) -> &'static str {
    match state {
        0x01 => "ESTABLISHED",
        0x02 => "SYN_SENT",
        0x03 => "SYN_RECV",
        0x04 => "FIN_WAIT1",
        0x05 => "FIN_WAIT2",
        0x06 => "TIME_WAIT",
        0x07 => "CLOSE",
        0x08 => "CLOSE_WAIT",
        0x09 => "LAST_ACK",
        0x0A => "LISTEN",
        0x0B => "CLOSING",
        _ => "UNKNOWN",
    }
}
