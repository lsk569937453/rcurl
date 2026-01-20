use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[derive(Subcommand, Serialize, Deserialize, Debug, Clone)]
pub enum QuickCommand {
    /// Ping a host to check connectivity
    #[command(alias = "p")]
    Ping {
        /// Host to ping (domain or IP address)
        target: String,
    },
    /// Check disk size for a path
    #[command(alias = "d")]
    Disk {
        /// Path to check (default: current directory)
        #[arg(default_value = ".")]
        target: String,
    },
    /// Telnet to a host and port
    #[command(alias = "t")]
    Telnet {
        /// Host to connect (domain or IP address)
        host: String,
        /// Port to connect
        port: u16,
    },
    /// DNS lookup (like dig)
    #[command(alias = "ns")]
    Ns {
        /// Domain name to query
        domain: String,
    },
    /// WHOIS lookup for domain registration information
    #[command(alias = "w")]
    Whois {
        /// Domain name or IP to query
        target: String,
    },
    /// Port query - list all listening ports or find/kill process by port
    #[command(alias = "l")]
    Port {
        /// Port number to query (optional, if not specified lists all listening ports)
        port: Option<u16>,
        /// Kill the process using the specified port
        #[arg(long)]
        kill: bool,
    },
}

#[derive(Parser, Default, Serialize, Deserialize, Debug, Clone)]
#[command(author, version, about, long_about, after_help = "Examples:
  rcurl http://example.com                    # Simple HTTP GET request
  rcurl https://example.com                   # HTTPS GET request
  rcurl -k https://example.com                # HTTPS with insecure (skip cert verify)
  rcurl -X POST -d 'data' http://example.com  # POST with data
  rcurl -H 'Content-Type: application/json' -d '{\"key\":\"value\"}' http://example.com  # POST JSON
  rcurl -v http://example.com                 # Verbose mode (debug level)
  rcurl -vv https://example.com               # More verbose (trace level)
  rcurl -o output.html http://example.com     # Save output to file
  rcurl https://example.com --time            # Show timing breakdown
  rcurl http://example.com --time --noproxy   # Timing without proxy
  rcurl ftp://ftp.example.com                 # FTP request
  rcurl ftps://ftp.example.com                # FTPS (FTP over TLS)
  rcurl sftp://sftp.example.com               # SFTP (SSH File Transfer)
  rcurl -u user:pass ftp://ftp.example.com    # FTP with authentication

File Upload:
  rcurl -T file.txt http://example.com/upload        # Upload file via PUT
  rcurl --upload-file image.jpg http://example.com   # Upload with long option
  rcurl -T data.bin -X POST http://example.com/api   # Upload with POST method
  rcurl -F 'file=@/path/to/file' http://example.com  # Upload as multipart form

File Download:
  rcurl -o output.html http://example.com            # Save response to file
  rcurl -o image.jpg http://example.com/image.jpg    # Download and save image
  rcurl -o data.json http://api.example.com/data     # Download JSON data

Proxy Support:
  Set environment variables to use proxy:
    export ALL_PROXY=http://127.0.0.1:7890    # Unix/Linux/MacOS
    set ALL_PROXY=http://127.0.0.1:7890       # Windows CMD
    $env:ALL_PROXY='http://127.0.0.1:7890'    # Windows PowerShell
  Or use separate settings:
    export HTTPS_PROXY=http://127.0.0.1:7890  # For HTTPS requests
    export HTTP_PROXY=http://127.0.0.1:7890   # For HTTP requests
  Bypass proxy for specific hosts:
    export NO_PROXY=example.com,localhost     # Skip proxy for these hosts
  Disable proxy for single request:
    rcurl https://example.com --noproxy       # Skip proxy for this request

Debug Levels:
  -v   verbose (debug level)
  -vv  more verbose (trace level)

TLS/Certificate Information:
  rcurl https://example.com --tls-info       # Show TLS handshake info
  rcurl https://example.com --cert-info      # Show certificate details
  rcurl https://example.com --tls-info --cert-info  # Show both
  rcurl https://example.com --tls-info-json  # Show TLS handshake info in JSON
  rcurl https://example.com --cert-info-json # Show certificate details in JSON

Quick Commands:
  rcurl ping google.com                       # Ping a host
  rcurl p 8.8.8.8                             # Ping an IP address (shorthand)
  rcurl disk .                                # Check disk size (current directory)
  rcurl d /home                               # Check disk size for specific path (shorthand)
  rcurl telnet example.com 80                 # Telnet to host:port
  rcurl t 192.168.1.1 23                     # Telnet with shorthand
  rcurl ns google.com                         # DNS lookup (like dig)
  rcurl ns www.example.com                    # DNS query with shorthand
  rcurl whois google.com                      # WHOIS lookup
  rcurl w example.com                         # WHOIS with shorthand
  rcurl port                                  # List all listening ports
  rcurl l                                     # List all listening ports (shorthand)
  rcurl port 8080                             # Find process using port 8080
  rcurl port 3000 --kill                      # Kill process using port 3000

Project home: https://github.com/lsk569937453/rcurl")]
pub struct Cli {
    /// The request url,like http://www.google.com
    pub url: Option<String>,
    ///  Specify request method to use
    #[arg(short = 'X', long = "request", value_name = "method")]
    pub method_option: Option<String>,
    /// HTTP POST data.
    #[arg(short = 'd', long = "data", value_name = "data")]
    pub body_option: Option<String>,
    /// Specify multipart MIME data.
    #[arg(short = 'F', long = "form", value_name = "name=content")]
    pub form_option: Vec<String>,
    /// The http headers.
    #[arg(short = 'H', long = "header", value_name = "header/@file")]
    pub headers: Vec<String>,
    /// The pem path.
    #[arg(short = 'c', long)]
    pub certificate_path_option: Option<String>,
    /// Server user and password
    #[arg(short = 'u', long = "user", value_name = "user:password")]
    pub authority_option: Option<String>,
    ///  Send User-Agent <name> to server
    #[arg(short = 'A', long = "user-agent", value_name = "name")]
    pub user_agent_option: Option<String>,
    /// The Cookie option.
    #[arg(short = 'b', long = "cookie", value_name = "data|filename")]
    pub cookie_option: Option<String>,
    ///  Referrer URL
    #[arg(short = 'e', long = "referer", value_name = "URL")]
    pub refer_option: Option<String>,
    ///  Write to file instead of stdout.
    #[arg(
        global = true,
        long = "output",
        short = 'o',
        value_name = "file",
        default_missing_value = "none"
    )]
    pub file_path_option: Option<String>,
    ///  Transfer local FILE to destination
    #[arg(long = "upload-file", short = 'T', value_name = "file")]
    pub uploadfile_option: Option<String>,

    ///  Send command(s) to server before transfer
    #[arg(long = "quote", short = 'Q', value_name = "command")]
    pub quote_option: Option<String>,

    /// Allow insecure server connections
    #[arg(short = 'k', long = "insecure")]
    pub skip_certificate_validate: bool,
    /// Show document info only
    #[arg(long = "head", short = 'I', group = "http")]
    pub header_option: bool,
    /// Retrieve only the bytes within RANGE
    #[arg(short = 'r', long = "range", value_name = "range")]
    pub range_option: Option<String>,
    /// 设置日志级别：
    /// -v   (debug)
    /// -vv  (trace)
    #[arg(short = 'v', long = "verbose", action = clap::ArgAction::Count)]
    pub verbosity: u8,
    #[arg(long = "http2")]
    pub http2: bool,
    #[arg(long = "http2-prior-knowledge")]
    pub http2_prior_knowledge: bool,
    /// Disable use of proxy
    #[arg(long = "noproxy")]
    pub noproxy: bool,
    /// Show timing information for request phases
    #[arg(long = "time")]
    pub time: bool,
    /// Show TLS handshake information
    #[arg(long = "tls-info")]
    pub tls_info: bool,
    /// Show TLS certificate information
    #[arg(long = "cert-info")]
    pub cert_info: bool,
    /// Show TLS handshake information in JSON format
    #[arg(long = "tls-info-json")]
    pub tls_info_json: bool,
    /// Show TLS certificate information in JSON format
    #[arg(long = "cert-info-json")]
    pub cert_info_json: bool,
    /// Quick command (ping, disk, telnet, ns, whois, and their shorthands)
    #[command(subcommand)]
    pub quick_cmd: Option<QuickCommand>,
}
