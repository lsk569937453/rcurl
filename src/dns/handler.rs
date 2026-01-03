use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use chrono::Local;
use hickory_resolver::TokioResolver;

use hickory_resolver::lookup::Lookup;
use hickory_resolver::name_server::TokioConnectionProvider;
use std::time::Instant;
/// DNS lookup command (like dig)
pub async fn dns_command(domain: String, _cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    // 记录开始时间
    let start = Instant::now();

    // 使用系统 DNS（等价于 dig 默认）
    let resolver = TokioResolver::builder(TokioConnectionProvider::default())?.build();
    // 查询 A 记录
    let response = resolver
        .lookup(domain.clone(), hickory_resolver::proto::rr::RecordType::A)
        .await?;

    let elapsed = start.elapsed().as_millis();

    // ---- 模拟 dig 输出 ----
    println!(
        "; <<>> Rust DiG (hickory-resolver 0.25.2) <<>> {}",
        domain.clone()
    );
    println!(";; global options: +cmd");
    println!(";; Got answer:");
    println!(
        ";; ->>HEADER<<- opcode: QUERY, status: NOERROR, id: {}",
        rand_id()
    );
    println!(
        ";; flags: qr rd ra; QUERY: 1, ANSWER: {}, AUTHORITY: 0, ADDITIONAL: 1",
        response.iter().count()
    );

    println!("\n;; QUESTION SECTION:");
    println!(";{} \t\tIN\tA", domain);

    println!("\n;; ANSWER SECTION:");
    for ip in response.iter() {
        if ip.is_a() {
            // TTL 在 high-level API 中不可直接获取，dig 一般是从 DNS RR 里拿
            println!("{} \t600\tIN\tA\t{}", domain, ip);
        }
    }

    println!("\n;; Query time: {} msec", elapsed);

    // 显示 DNS server
    if let Some(server) = resolver.config().name_servers().first() {
        println!(
            ";; SERVER: {}#{} ({})",
            server.socket_addr.ip(),
            server.socket_addr.port(),
            server.socket_addr.ip()
        );
    }

    println!(
        ";; WHEN: {}",
        Local::now().format("%a %b %d %H:%M:%S %Z %Y")
    );

    println!(";; MSG SIZE  rcvd: {}", estimate_msg_size(response));

    Ok(RcurlResponse::Dns(()))
}

fn rand_id() -> u16 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % u16::MAX as u32) as u16
}

/// 简单估算返回消息大小（非精确）
fn estimate_msg_size(resp: Lookup) -> usize {
    32 + resp.iter().count() * 16
}
