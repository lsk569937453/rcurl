use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::anyhow;
use netstat2::{AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState, get_sockets_info};
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

/// Get all listening ports using netstat2 (cross-platform)
fn get_listening_ports() -> Result<Vec<PortInfo>, anyhow::Error> {
    let mut ports = Vec::new();

    let af_flags = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let proto_flags = ProtocolFlags::TCP | ProtocolFlags::UDP;

    let socket_info = get_sockets_info(af_flags, proto_flags)
        .map_err(|e| anyhow!("Failed to get socket info: {}", e))?;

    for info in socket_info {
        let apids = info.associated_pids;

        match info.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp_si) => {
                // Listening sockets have remote_port == 0 and state == Listen
                if tcp_si.local_port > 0
                    && tcp_si.remote_port == 0
                    && tcp_si.state == TcpState::Listen
                {
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

/// Display all listening ports
fn display_all_ports(ports: &[PortInfo]) {
    if ports.is_empty() {
        println!("No listening ports found.");
        return;
    }

    println!("Listening ports:");
    println!();
    println!(
        "{:<10} {:<25} {:<10} {:<10}",
        "Protocol", "Address", "State", "PID"
    );
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
        let cmd: Vec<String> = process
            .cmd()
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
            println!(
                "Port {} is in use but PID information is not available.",
                port
            );
        }
        None => {
            println!("Port {} is not in use. No process to kill.", port);
        }
    }

    Ok(RcurlResponse::Port(()))
}
