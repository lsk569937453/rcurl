use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::anyhow;
use std::process::Command;

#[derive(Debug, Clone)]
struct PortInfo {
    protocol: String,
    local_address: String,
    foreign_address: String,
    state: String,
    pid: String,
}

/// Parse netstat output on Windows
fn parse_netstat_windows(output: &str) -> Vec<PortInfo> {
    let mut ports = Vec::new();

    for line in output.lines().skip(4) {
        // Skip empty lines and header separators
        if line.trim().is_empty() || line.trim().starts_with('-') {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            let protocol = parts[0].to_string();
            let local_address = parts[1].to_string();
            let foreign_address = if parts.len() > 2 { parts[2].to_string() } else { "N/A".to_string() };
            let state = if parts.len() > 3 { parts[3].to_string() } else { "N/A".to_string() };
            let pid = if parts.len() > 4 { parts[4].to_string() } else { "N/A".to_string() };

            // Only show LISTENING ports
            if state.contains("LISTENING") {
                ports.push(PortInfo {
                    protocol,
                    local_address,
                    foreign_address,
                    state,
                    pid,
                });
            }
        }
    }

    ports
}

/// Parse netstat output on Linux/Mac
fn parse_netstat_unix(output: &str) -> Vec<PortInfo> {
    let mut ports = Vec::new();

    for line in output.lines() {
        // Skip empty lines and headers
        if line.trim().is_empty() || line.starts_with("Active") || line.starts_with("Proto") {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 6 {
            let protocol = parts[0].to_string();
            let local_address = parts[3].to_string();
            let foreign_address = parts[4].to_string();
            let state = parts[5].to_string();
            let pid = if parts.len() > 6 {
                // Extract PID from "program/name"
                parts[6].split('/').next().unwrap_or("N/A").to_string()
            } else {
                "N/A".to_string()
            };

            // Only show LISTEN ports
            if state.contains("LISTEN") {
                ports.push(PortInfo {
                    protocol,
                    local_address,
                    foreign_address,
                    state,
                    pid,
                });
            }
        }
    }

    ports
}

/// Get all listening ports
fn get_listening_ports() -> Result<Vec<PortInfo>, anyhow::Error> {
    let output = if cfg!(target_os = "windows") {
        Command::new("netstat")
            .args(&["-ano"])
            .output()
            .map_err(|e| anyhow!("Failed to execute netstat: {}", e))?
    } else {
        Command::new("netstat")
            .args(&["-tulnp"])
            .output()
            .map_err(|e| anyhow!("Failed to execute netstat: {}", e))?
    };

    let output_str = String::from_utf8_lossy(&output.stdout);

    let ports = if cfg!(target_os = "windows") {
        parse_netstat_windows(&output_str)
    } else {
        parse_netstat_unix(&output_str)
    };

    Ok(ports)
}

/// Find process ID by port number
fn find_pid_by_port(port: u16) -> Result<Option<String>, anyhow::Error> {
    let ports = get_listening_ports()?;

    for port_info in ports {
        // Extract port number from local address (e.g., "0.0.0.0:8080" or "[::]:8080")
        if let Some(pos) = port_info.local_address.rfind(':') {
            if let Ok(port_num) = port_info.local_address[pos + 1..].parse::<u16>() {
                if port_num == port {
                    return Ok(Some(port_info.pid));
                }
            }
        }
    }

    Ok(None)
}

/// Kill process by PID
fn kill_process(pid: &str) -> Result<(), anyhow::Error> {
    if pid == "N/A" || pid.is_empty() {
        return Err(anyhow!("Invalid PID"));
    }

    let result = if cfg!(target_os = "windows") {
        Command::new("taskkill")
            .args(&["/F", "/PID", pid])
            .output()
    } else {
        Command::new("kill")
            .args(&["-9", pid])
            .output()
    };

    match result {
        Ok(output) => {
            if output.status.success() {
                Ok(())
            } else {
                let error = String::from_utf8_lossy(&output.stderr);
                Err(anyhow!("Failed to kill process: {}", error))
            }
        }
        Err(e) => Err(anyhow!("Failed to execute kill command: {}", e)),
    }
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

    for port in ports {
        let addr = if port.local_address.contains('[') {
            // IPv6 address format
            if let Some(end) = port.local_address.rfind(']') {
                &port.local_address[..=end]
            } else {
                port.local_address.as_str()
            }
        } else {
            port.local_address.as_str()
        };

        // Truncate address if too long
        let addr_display = if addr.len() > 25 {
            format!("{}...", &addr[..22])
        } else {
            addr.to_string()
        };

        println!("{:<10} {:<25} {:<10} {:<10}", port.protocol, addr_display, port.state, port.pid);
    }

    println!();
    println!("Total: {} listening port(s)", ports.len());
}

/// Display port query result
fn display_port_result(port: u16, pid: Option<&str>) {
    if let Some(pid) = pid {
        println!("Port {} is in use by process ID: {}", port, pid);
    } else {
        println!("Port {} is not in use.", port);
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
    display_port_result(port, pid.as_deref());
    Ok(RcurlResponse::Port(()))
}

/// Port command - kill process using the specified port
pub async fn port_kill_command(port: u16, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let pid = find_pid_by_port(port)?;

    match pid {
        Some(pid_str) => {
            println!("Killing process {} using port {}...", pid_str, port);
            kill_process(&pid_str)?;
            println!("Successfully killed process {}.", pid_str);
        }
        None => {
            println!("Port {} is not in use. No process to kill.", port);
        }
    }

    Ok(RcurlResponse::Port(()))
}
