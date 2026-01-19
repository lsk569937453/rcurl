use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::anyhow;
use sysinfo::{Pid, System};

#[derive(Debug, Clone)]
struct PortInfo {
    protocol: String,
    local_address: String,
    local_port: u16,
    state: String,
    pid: u32,
}

/// Find process ID by port number
fn find_pid_by_port(port: u16) -> Result<Option<u32>, anyhow::Error> {
    let ports = get_listening_ports()?;

    for port_info in ports {
        if port_info.local_port == port {
            return Ok(Some(port_info.pid));
        }
    }

    Ok(None)
}

/// Kill process by PID
fn kill_process(pid: u32) -> Result<(), anyhow::Error> {
    if pid == 0 {
        return Err(anyhow!("Invalid PID"));
    }

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::everything(),
    );

    if let Some(process) = sys.process(Pid::from_u32(pid)) {
        if process.kill() {
            Ok(())
        } else {
            Err(anyhow!("Failed to kill process: {}", pid))
        }
    } else {
        Err(anyhow!("Process not found: {}", pid))
    }
}

// Windows implementation using netstat2
#[cfg(windows)]
mod windows_impl {
    use super::PortInfo;
    use anyhow::anyhow;
    use netstat2::{ProtocolSocketInfo, TcpState};

    pub fn get_listening_ports_windows() -> Result<Vec<PortInfo>, anyhow::Error> {
        let mut ports = Vec::new();

        // Use netstat2 to get socket information
        let af_flags = netstat2::AddressFamilyFlags::IPV4 | netstat2::AddressFamilyFlags::IPV6;
        let proto_flags = netstat2::ProtocolFlags::TCP | netstat2::ProtocolFlags::UDP;

        let socket_info = netstat2::get_sockets_info(af_flags, proto_flags)
            .map_err(|e| anyhow!("Failed to get socket info: {}", e))?;

        for info in socket_info {
            let apids = info.associated_pids;

            // Check if this is a listening socket and extract port info
            match info.protocol_socket_info {
                ProtocolSocketInfo::Tcp(tcp_si) => {
                    // Listening sockets have remote_port == 0 and state == Listen
                    if tcp_si.local_port > 0 && tcp_si.remote_port == 0 && tcp_si.state == TcpState::Listen {
                        ports.push(PortInfo {
                            protocol: "TCP".to_string(),
                            local_address: format!("{}", tcp_si.local_addr),
                            local_port: tcp_si.local_port,
                            state: format!("{:?}", tcp_si.state),
                            pid: apids.first().copied().unwrap_or(0),
                        });
                    }
                }
                ProtocolSocketInfo::Udp(udp_si) => {
                    // UDP sockets are always listening
                    if udp_si.local_port > 0 {
                        ports.push(PortInfo {
                            protocol: "UDP".to_string(),
                            local_address: format!("{}", udp_si.local_addr),
                            local_port: udp_si.local_port,
                            state: "LISTENING".to_string(),
                            pid: apids.first().copied().unwrap_or(0),
                        });
                    }
                }
            }
        }

        Ok(ports)
    }
}

// Unix implementation (Linux/macOS)
#[cfg(unix)]
mod unix_impl {
    use super::PortInfo;
    use anyhow::anyhow;
    use std::fs;
    use std::path::Path;

    fn hex_to_ip(hex: &str, is_v6: bool) -> String {
        if is_v6 {
            let hex_clean = hex.trim_start_matches('0').trim_start_matches("0000");
            if hex_clean.is_empty() {
                return "[::]".to_string();
            }
            let chars: Vec<char> = hex.chars().collect();
            let mut parts = Vec::new();
            for i in 0..8 {
                let start = i * 4;
                if start + 4 <= chars.len() {
                    let part: String = chars[start..start + 4].iter().collect();
                    parts.push(format!("{:x}", u16::from_str_radix(&part, 16).unwrap_or(0)));
                }
            }
            format!("[{}]", parts.join(":"))
        } else {
            let hex_val = u32::from_str_radix(hex.trim(), 16).unwrap_or(0);
            let bytes = hex_val.to_be_bytes();
            format!("{}.{}.{}.{}", bytes[3], bytes[2], bytes[1], bytes[0])
        }
    }

    fn parse_proc_net(file: &str, protocol: &str, is_v6: bool) -> Vec<PortInfo> {
        let mut ports = Vec::new();

        if let Ok(content) = fs::read_to_string(file) {
            for line in content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 10 {
                    let local_addr = parts[1];
                    let state_hex = parts[3];

                    // State 0A = LISTEN for TCP
                    let is_listening = protocol == "UDP" || state_hex == "0A";

                    if is_listening {
                        let addr_parts: Vec<&str> = local_addr.rsplitn(2, ':').collect();
                        if addr_parts.len() == 2 {
                            let addr = hex_to_ip(addr_parts[1], is_v6);
                            let port = u16::from_str_radix(addr_parts[0].trim(), 16).unwrap_or(0);

                            let inode = parts[9];
                            let pid = find_pid_by_inode(inode);

                            ports.push(PortInfo {
                                protocol: protocol.to_string(),
                                local_address: addr,
                                local_port: port,
                                state: "LISTEN".to_string(),
                                pid,
                            });
                        }
                    }
                }
            }
        }

        ports
    }

    fn find_pid_by_inode(inode: &str) -> u32 {
        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(pid_str) = path.file_name().and_then(|n| n.to_str()) {
                    if pid_str.chars().all(|c| c.is_ascii_digit()) {
                        if let Ok(pid_num) = pid_str.parse::<u32>() {
                            let fd_path = path.join("fd");
                            if let Ok(fd_entries) = fs::read_dir(&fd_path) {
                                for fd_entry in fd_entries.flatten() {
                                    if let Ok(link) = fs::read_link(fd_entry.path()) {
                                        if let Some(link_str) = link.to_str() {
                                            if link_str.contains(&format!("socket:[{}]", inode)) {
                                                return pid_num;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        0
    }

    pub fn get_listening_ports_unix() -> Result<Vec<PortInfo>, anyhow::Error> {
        let mut ports = Vec::new();

        if Path::new("/proc/net/tcp").exists() {
            ports.extend(parse_proc_net("/proc/net/tcp", "TCP", false));
        }
        if Path::new("/proc/net/tcp6").exists() {
            ports.extend(parse_proc_net("/proc/net/tcp6", "TCP", true));
        }
        if Path::new("/proc/net/udp").exists() {
            ports.extend(parse_proc_net("/proc/net/udp", "UDP", false));
        }
        if Path::new("/proc/net/udp6").exists() {
            ports.extend(parse_proc_net("/proc/net/udp6", "UDP", true));
        }

        Ok(ports)
    }
}

#[cfg(windows)]
use windows_impl::get_listening_ports_windows as get_ports_platform;

#[cfg(unix)]
use unix_impl::get_listening_ports_unix as get_ports_platform;

fn get_listening_ports() -> Result<Vec<PortInfo>, anyhow::Error> {
    get_ports_platform()
}

/// Display all listening ports
fn display_all_ports(ports: &[PortInfo]) {
    if ports.is_empty() {
        println!("No listening ports found.");
        return;
    }

    println!("Listening ports:");
    println!();
    println!("{:<10} {:<25} {:<10} {:<10}", "Protocol", "Address", "State", "PID");
    println!("{}", "-".repeat(60));

    // Sort by port number
    let mut sorted_ports = ports.to_vec();
    sorted_ports.sort_by(|a, b| a.local_port.cmp(&b.local_port));

    for port in &sorted_ports {
        let addr = if port.local_address.contains('[') {
            port.local_address.as_str()
        } else {
            port.local_address.as_str()
        };

        let addr_display = if addr.len() > 25 {
            format!("{}...", &addr[..22])
        } else {
            addr.to_string()
        };

        println!(
            "{:<10} {:<25} {:<10} {:<10}",
            port.protocol,
            format!("{}:{}", addr_display, port.local_port),
            port.state,
            if port.pid > 0 {
                port.pid.to_string()
            } else {
                "N/A".to_string()
            }
        );
    }

    println!();
    println!("Total: {} listening port(s)", sorted_ports.len());
}

/// Get process command line string
fn get_process_command(pid: u32) -> Option<String> {
    use sysinfo::ProcessRefreshKind;

    let mut sys = System::new();
    // Refresh with full process information including command line
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::everything(),
    );

    if let Some(process) = sys.process(Pid::from_u32(pid)) {
        let name = process.name();
        let cmd: Vec<String> = process.cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect();
        if !cmd.is_empty() {
            Some(cmd.join(" "))
        } else {
            Some(name.to_string_lossy().to_string())
        }
    } else {
        None
    }
}

/// Display port query result with process command
fn display_port_result(port: u16, pid: Option<u32>) {
    match pid {
        Some(p) if p > 0 => {
            println!("Port {} is in use by process ID: {}", port, p);
            if let Some(cmd) = get_process_command(p) {
                println!("Command: {}", cmd);
            }
        }
        Some(_) => {
            println!("Port {} is in use (PID information not available).", port);
        }
        None => {
            println!("Port {} is not in use.", port);
        }
    }
}

/// Port command - list all listening ports (no arguments)
pub async fn port_list_command(_cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let ports = get_listening_ports()?;
    display_all_ports(&ports);
    Ok(RcurlResponse::Port(()))
}

/// Port command - find process by port number
pub async fn port_find_command(port: u16, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let pid = find_pid_by_port(port)?;
    display_port_result(port, pid);
    Ok(RcurlResponse::Port(()))
}

/// Port command - kill process using the specified port
pub async fn port_kill_command(port: u16, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let pid = find_pid_by_port(port)?;

    match pid {
        Some(p) if p > 0 => {
            println!("Killing process {} using port {}...", p, port);
            kill_process(p)?;
            println!("Successfully killed process {}.", p);
        }
        Some(_) => {
            println!("Port {} is in use but PID information is not available.", port);
        }
        None => {
            println!("Port {} is not in use. No process to kill.", port);
        }
    }

    Ok(RcurlResponse::Port(()))
}
