use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};

/// WHOIS lookup command
/// Queries WHOIS servers for domain registration information
pub async fn whois_command(target: String, cli: Cli) -> Result<RcurlResponse, anyhow::Error> {
    // Determine the appropriate WHOIS server based on TLD
    let server = determine_whois_server(&target)?;

    if cli.verbosity > 0 {
        eprintln!("Connecting to WHOIS server: {}", server);
    }

    // Connect to WHOIS server (port 43)
    let stream = timeout(
        Duration::from_secs(10),
        tokio::net::TcpStream::connect((server, 43)),
    )
    .await??;

    if cli.verbosity > 0 {
        eprintln!("Connected, querying: {}", target);
    }

    let (mut reader, mut writer) = tokio::io::split(stream);

    // Send the query (domain + CRLF)
    let query = format!("{}\r\n", target);
    writer.write_all(query.as_bytes()).await?;
    writer.flush().await?;

    // Read response
    let mut buffer = vec![0u8; 8192];
    let mut response = String::new();

    loop {
        let n = timeout(Duration::from_secs(30), reader.read(&mut buffer)).await??;
        if n == 0 {
            break;
        }
        response.push_str(&String::from_utf8_lossy(&buffer[..n]));
    }

    // Print the WHOIS response
    print!("{}", response);

    Ok(RcurlResponse::Whois(()))
}

/// Determine the appropriate WHOIS server based on TLD
fn determine_whois_server(domain: &str) -> Result<&'static str, anyhow::Error> {
    let domain_lower = domain.to_lowercase();

    // Extract TLD
    let tld = domain_lower
        .rsplit('.')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid domain: {}", domain))?;

    // Common WHOIS servers
    let server = match tld {
        // Generic TLDs - Verisign managed
        "com" | "net" => "whois.verisign-grs.com",
        "org" => "whois.pir.org",
        "info" => "whois.afilias.net",
        "biz" => "whois.biz",
        "name" => "whois.nic.name",
        "mobi" => "whois.dotmobiregistry.net",
        "online" | "site" => "whois.nic.online",
        "xyz" => "whois.nic.xyz",
        "tech" => "whois.nic.tech",
        "app" => "whois.nic.google",
        "io" => "whois.nic.io",
        "ai" => "whois.nic.ai",
        "co" => "whois.nic.co",
        "me" => "whois.nic.me",
        "tv" => "whois.nic.tv",
        "gg" => "whois.nic.gg",
        "cc" => "whois.nic.cc",

        // Country code TLDs
        "us" => "whois.nic.us",
        "uk" | "co.uk" | "org.uk" => "whois.nic.uk",
        "de" => "whois.denic.de",
        "fr" => "whois.nic.fr",
        "cn" => "whois.cnnic.cn",
        "jp" => "whois.jprs.jp",
        "kr" => "whois.kr",
        "ru" => "whois.tld.ru",
        "ca" => "whois.ca.fury.ca",
        "au" => "whois.auda.org.au",
        "br" => "whois.registro.br",
        "nl" => "whois.domain-registry.nl",
        "es" => "whois.nic.es",
        "it" => "whois.nic.it",
        "ch" => "whois.nic.ch",
        "at" => "whois.nic.at",
        "be" => "whois.dns.be",
        "se" => "whois.iis.se",
        "no" => "whois.norid.no",
        "pl" => "whois.dns.pl",
        "dk" => "whois.dk-hostmaster.dk",
        "fi" => "whois.fi",
        "gr" => "whois.ripe.net",
        "eu" => "whois.eu",
        "in" => "whois.registry.in",
        "sg" => "whois.sgnic.sg",
        "my" => "whois.mynic.my",
        "th" => "whois.thnic.co.th",
        "vn" => "whois.nic.vn",
        "id" => "whois.id",
        "ph" => "whois.dot.ph",
        "tw" => "whois.twnic.net.tw",
        "hk" => "whois.hkirc.hk",
        "nz" => "whois.srs.net.nz",
        "za" => "whois.registry.net.za",
        "mx" => "whois.mx",

        // IP addresses - use ARIN or RIPE
        _ if domain_lower.parse::<std::net::IpAddr>().is_ok() => "whois.arin.net",

        // Default to IANA
        _ => "whois.iana.org",
    };

    Ok(server)
}
