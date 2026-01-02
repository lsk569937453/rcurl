use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use ping::ping;
use std::net::IpAddr;
use std::net::ToSocketAddrs;
use std::time::Duration;
use std::time::Instant;
fn resolve_host(host: &str) -> anyhow::Result<IpAddr> {
    let addr = format!("{}:0", host);
    let sock = addr
        .to_socket_addrs()
        .map_err(|e| anyhow!("ping: cannot resolve host {}", host))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("resolve failed"))?;
    Ok(sock.ip())
}
pub async fn ping_command(host: String, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    let count = 4;
    let payload_size = 32; // 对齐 Windows ping

    let ip = resolve_host(&host)?;
    println!(
        "Pinging {} [{}] with {} bytes of data:",
        host, ip, payload_size
    );

    let mut transmitted = 0;
    let mut received = 0;
    let mut rtts = Vec::new();

    for seq in 1..=count {
        transmitted += 1;

        // 构造 ping 实例
        let mut p = ping::new(ip);

        let start = Instant::now();
        match p.send() {
            Ok(_) => {
                let rtt = start.elapsed().as_millis();
                received += 1;
                rtts.push(rtt);
                println!("Reply from {}: bytes={} time={}ms", ip, payload_size, rtt);
            }
            Err(e) => {
                println!("icmp_seq={} error: {}", seq, e);
            }
        }

        std::thread::sleep(Duration::from_secs(1));
    }

    // 统计输出
    println!();
    println!("Ping statistics for {}:", ip);
    println!(
        "    Packets: Sent = {}, Received = {}, Lost = {} ({}% loss),",
        transmitted,
        received,
        transmitted - received,
        (transmitted - received) * 100 / transmitted
    );
    if !rtts.is_empty() {
        let min = rtts.iter().min().cloned().unwrap_or_default();
        let max = rtts.iter().max().cloned().unwrap_or_default();
        let sum: u128 = rtts.iter().sum();
        let avg = sum / rtts.len() as u128;
        println!("Approximate round trip times in milli-seconds:");
        println!(
            "    Minimum = {}ms, Maximum = {}ms, Average = {}ms",
            min, max, avg
        );
    }
    Ok(RcurlResponse::Ping(()))
}
