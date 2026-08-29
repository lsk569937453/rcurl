#[derive(Debug)]
pub enum RcurlResponse {
    Ftp(()),
    Http(()),
    Ping(()),
    DiskSize(()),
    Count(()),
    GitStat(()),
    Telnet(()),
    Dns(()),
    Whois(()),
    Port(()),
    LoadTest(()),
}
