#[derive(Debug)]
pub enum RcurlResponse {
    Ftp(()),
    Http(()),
    Ping(()),
    DiskSize(()),
    Telnet(()),
    Dns(()),
    Whois(()),
}
