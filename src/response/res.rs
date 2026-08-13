#[derive(Debug)]
pub enum RcurlResponse {
    Ftp(()),
    Http(()),
    Ping(()),
    DiskSize(()),
    Count(()),
    Telnet(()),
    Dns(()),
    Whois(()),
    Port(()),
}
