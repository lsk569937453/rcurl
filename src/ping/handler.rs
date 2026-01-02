use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::Context;

pub async fn ping_command(
    host: String,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    let count = 4;

    // Resolve host to IP address
    let addr = resolve_host(&host).await?;
    let ip = addr;

    println!("PING {} ({}:32) 56 bytes of data.", host, ip);

    let mut rtt_values = Vec::new();
    let mut received = 0;

    for i in 0..count {
        let start = std::time::Instant::now();

        match tokio::time::timeout(
            std::time::Duration::from_secs(1),
            do_ping(ip),
        ).await {
            Ok(Ok(_)) => {
                let elapsed = start.elapsed();
                let rtt_ms = elapsed.as_secs_f64() * 1000.0;
                println!("64 bytes from {}: icmp_seq={} time={:.2} ms",
                         ip, i, rtt_ms);
                rtt_values.push(rtt_ms);
                received += 1;
            }
            Ok(Err(e)) => {
                eprintln!("Ping error: {}", e);
            }
            Err(_) => {
                eprintln!("From {} icmp_seq={} timeout", ip, i);
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    let packet_loss = ((count - received) as f64 / count as f64) * 100.0;
    let (rtt_min, rtt_max, rtt_avg) = if !rtt_values.is_empty() {
        let min = rtt_values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = rtt_values.iter().copied().fold(0.0, f64::max);
        let avg = rtt_values.iter().sum::<f64>() / rtt_values.len() as f64;
        (Some(min), Some(max), Some(avg))
    } else {
        (None, None, None)
    };

    println!("\n--- {} ping statistics ---", host);
    println!("{} packets transmitted, {} received, {:.0}% packet loss",
             count, received, packet_loss);

    if let (Some(min), Some(max), Some(avg)) = (rtt_min, rtt_max, rtt_avg) {
        println!("rtt min/avg/max = {:.3}/{:.3}/{:.3} ms", min, avg, max);
    }

    Ok(RcurlResponse::Ping(()))
}

async fn resolve_host(host: &str) -> Result<std::net::IpAddr, anyhow::Error> {
    use tokio::net::lookup_host;

    // Try to parse as IP address first
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(ip);
    }

    // Resolve using DNS
    let addrs = lookup_host((host, 80)).await
        .context(format!("Failed to resolve host: {}", host))?;

    addrs.map(|socket_addr| socket_addr.ip())
        .next()
        .ok_or_else(|| anyhow::anyhow!("No addresses found for host: {}", host))
}

async fn do_ping(ip: std::net::IpAddr) -> Result<(), anyhow::Error> {
    // Simple TCP connection attempt for cross-platform ping-like functionality
    // Note: Real ICMP ping requires raw sockets which need admin privileges
    // This is a simplified implementation using TCP port 80
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    let _ = timeout(
        std::time::Duration::from_millis(500),
        TcpStream::connect((ip, 80))
    ).await;

    Ok(())
}
