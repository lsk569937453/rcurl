# Rcurl

![Crates.io Version](https://img.shields.io/crates/v/rcurl.svg)![License](https://img.shields.io/crates/l/rcurl.svg)

`rcurl` is a simple, powerful command-line tool for transferring data with URLs, inspired by cURL, and built in Rust.

## Features

- Support for HTTP/1.1, HTTP/2, and HTTP/2 with prior knowledge
- Customizable HTTP methods (GET, POST, etc.)
- Sending custom request body and form data
- Custom headers, user-agent, cookies, and referrer
- User authentication
- Support for PEM certificates and insecure connections
- File downloads and uploads
- Byte range support
- Verbose mode for debugging (-v, -vv)
- Request timing breakdown (--time)
- Proxy support (HTTP_PROXY, HTTPS_PROXY, ALL_PROXY, NO_PROXY)
- Quick commands: ping, disk, telnet, DNS lookup, WHOIS
- Interactive mode with request history

## rcurl vs curl

rcurl is a modern reimagining of curl, built with Rust for enhanced safety, better developer experience, and extended functionality.

| Feature | rcurl | curl |
| :--- | :--- | :--- |
| **Language** | Rust (Memory-safe, modern) | C (Legacy, manual memory management) |
| **Core Functionality** | Full curl-compatible HTTP/S/FTP/SFTP | Full protocol support |
| **Interactive Mode** | Request history & replay | Not available |
| **Timing Analysis** | Multi-phase breakdown (DNS, TCP, TLS, etc.) | Basic time_total only |
| **Network Diagnostics** | Built-in ping, DNS, whois, telnet | Requires external tools |
| **Developer Experience** | Colored output, progress bars, cleaner CLI | Traditional Unix style |
| **Quick Commands** | Shorthand aliases (p, d, t, w, ns) | Not available |
| **HTTP/2 Support** | Native support | Native support |
| **Distribution** | Single static binary (cargo/release) | System package / build from source |

**Why choose rcurl?**

- **Modern & Safe**: Built with Rust for memory safety and reliability, eliminating entire classes of security vulnerabilities
- **Better DX**: Interactive mode with request history, colored output, and intuitive shortcuts for daily workflows
- **Enhanced Insights**: Detailed timing breakdown (DNS lookup, TCP handshake, TLS negotiation, transfer) for performance debugging
- **All-in-One**: Network diagnostics (ping, DNS, whois, telnet) integrated—no need to switch tools
- **Drop-in Replacement**: Compatible with curl's most-used options while adding powerful new features

## Installation on Linux (Quick Start)

For Linux users, the quickest way to get started is by downloading the pre-compiled binary directly from GitHub Releases. This method does not require you to have the Rust toolchain installed.

### Download the Latest Release

```
curl -L -o rcurl https://github.com/lsk569937453/rcurl/releases/download/v0.0.27/rcurl-x86_64-unknown-linux-gnu
chmod +x ./rcurl
```

## Installation

### Install via Cargo

If you have Rust and Cargo installed, you can install `rcurl` directly from crates.io:

```bash
cargo install cargo-rcurl
```

### Build from Source

Alternatively, you can install `rcurl` by cloning this repository and building it with Cargo:

```bash
git clone https://github.com/lsk569937453/rcurl.git
cd rcurl
cargo install --path .
```

## Usage

### Examples

#### HTTP/HTTPS Requests

**Make a simple GET request:**

```bash
rcurl http://www.google.com
```

**Download a webpage to a file:**

```bash
rcurl -o google.html http://www.google.com
```

**Send POST form data:**

```bash
rcurl -X POST -d "param1=value1&param2=value2" http://httpbin.org/post
```

**Send JSON data with a custom header:**

```bash
rcurl -X POST -d '{"name":"John Doe"}' -H "Content-Type: application/json" http://httpbin.org/post
```

**View only the response headers (HEAD request):**

```bash
rcurl -I http://www.google.com
```

**Use a custom user-agent:**

```bash
rcurl -A "MyCoolBrowser/1.0" http://httpbin.org/user-agent
```

**Download a specific byte range:**

```bash
rcurl -r 0-1023 http://example.com/file.zip -o partial_file.zip
```

**Show request timing breakdown:**

```bash
rcurl https://example.com --time
```

**Verbose mode for debugging:**

```bash
rcurl -v http://example.com          # Debug level
rcurl -vv https://example.com        # Trace level
```

#### FTP/FTPS/SFTP

**FTP request:**

```bash
rcurl ftp://ftp.example.com
```

**FTPS (FTP over TLS):**

```bash
rcurl ftps://ftp.example.com
```

**SFTP with authentication:**

```bash
rcurl -u user:pass ftp://ftp.example.com
```

#### Quick Commands

**Ping a host:**

```bash
rcurl ping google.com
rcurl p 8.8.8.8          # Shorthand
```

**DNS lookup (like dig):**

```bash
rcurl ns google.com
```

**WHOIS lookup for domain information:**

```bash
rcurl whois google.com
rcurl w example.com      # Shorthand
rcurl whois 8.8.8.8       # IP lookup
```

**Check disk size:**

```bash
rcurl disk .             # Current directory
rcurl d /home            # Specific path (shorthand)
```

**Telnet to a host:port:**

```bash
rcurl telnet example.com 80
rcurl t 192.168.1.1 23   # Shorthand
```

### Options

Below is a complete list of available command-line options:

| Short | Long                        | Argument          | Description                                |
| :---- | :-------------------------- | :---------------- | :----------------------------------------- |
|       |                             | `url`             | The request URL.                           |
| `-X`  | `--request`                 | `<method>`        | Specify request method to use.             |
| `-d`  | `--data`                    | `<data>`          | HTTP POST data.                            |
| `-F`  | `--form`                    | `<name=content>`  | Specify multipart MIME data.               |
| `-H`  | `--header`                  | `<header/@file>`  | The http headers.                          |
| `-c`  | `--certificate-path-option` | `<path>`          | The pem path.                              |
| `-u`  | `--user`                    | `<user:password>` | Server user and password.                  |
| `-A`  | `--user-agent`              | `<name>`          | Send User-Agent <name> to server.          |
| `-b`  | `--cookie`                  | `<data            | filename>`                                 |
| `-e`  | `--referer`                 | `<URL>`           | Referrer URL.                              |
| `-o`  | `--output`                  | `<file>`          | Write to file instead of stdout.           |
| `-T`  | `--upload-file`             | `<file>`          | Transfer local FILE to destination.        |
| `-Q`  | `--quote`                   | `<command>`       | Send command(s) to server before transfer. |
| `-k`  | `--insecure`                |                   | Allow insecure server connections.         |
| `-I`  | `--head`                    |                   | Show document info only.                   |
| `-r`  | `--range`                   | `<range>`         | Retrieve only the bytes within RANGE.      |
| `-v`  | `--verbose`                 |                   | Verbose mode (-v for debug, -vv for trace).|
|       | `--http2`                   |                   | Use HTTP/2.                                |
|       | `--http2-prior-knowledge`   |                   | Use HTTP/2 with prior knowledge.           |
|       | `--noproxy`                 |                   | Disable use of proxy.                      |
|       | `--time`                    |                   | Show timing information for request phases.|
| `-h`  | `--help`                    |                   | Print help information.                    |
| `-V`  | `--version`                 |                   | Print version information.                 |

#### Quick Commands

| Command | Shorthand | Argument | Description                      |
| :------ | :-------- | :------- | :------------------------------- |
| `ping`  | `p`       | `<target>` | Ping a host to check connectivity.|
| `disk`  | `d`       | `<path>`  | Check disk size for a path.      |
| `telnet`| `t`       | `<host> <port>` | Telnet to a host and port.  |
| `ns`    |           | `<domain>`| DNS lookup (like dig).          |
| `whois` | `w`       | `<target>` | WHOIS lookup for domain/IP info. |

### Proxy Support

Set environment variables to use proxy:

```bash
# Unix/Linux/MacOS
export ALL_PROXY=http://127.0.0.1:7890
export HTTPS_PROXY=http://127.0.0.1:7890
export HTTP_PROXY=http://127.0.0.1:7890
export NO_PROXY=example.com,localhost

# Windows CMD
set ALL_PROXY=http://127.0.0.1:7890

# Windows PowerShell
$env:ALL_PROXY='http://127.0.0.1:7890'
```

Disable proxy for a single request:

```bash
rcurl https://example.com --noproxy
```

### Interactive Mode

When no URL or command is provided, `rcurl` enters interactive mode, allowing you to select and execute previous requests from history:

```bash
rcurl
```

## Contributing

Contributions are welcome! Feel free to fork the repository, make your changes, and submit a pull request.

## License

This project is licensed under the [Apache License](LICENSE).
