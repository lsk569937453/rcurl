# Rcurl

![Crates.io Version](https://img.shields.io/crates/v/rcurl.svg)![License](https://img.shields.io/crates/l/rcurl.svg)

`rcurl` is a simple, powerful command-line tool for transferring data with URLs, inspired by cURL, and built in Rust.

## Features

- Support for HTTP/1.1, HTTP/2, and HTTP/2 with prior knowledge.
- Customizable HTTP methods (GET, POST, etc.).
- Sending custom request body and form data.
- Custom headers, user-agent, cookies, and referrer.
- User authentication.
- Support for PEM certificates and insecure connections.
- File downloads and uploads.
- Byte range support.
- Verbose mode for debugging.

## Installation on Linux (Quick Start)

For Linux users, the quickest way to get started is by downloading the pre-compiled binary directly from GitHub Releases. This method does not require you to have the Rust toolchain installed.

### Download the Latest Release

```
curl -L -o rcurl https://github.com/lsk569937453/rcurl/releases/download/v0.0.27/rcurl-x86_64-unknown-linux-gnu
chmod +x ./rcurl
```

## Installation

Ensure you have Rust and Cargo installed. You can install `rcurl` by cloning this repository and building it with Cargo:

```bash
git clone https://github.com/lsk569937453/rcurl.git
cd rcurl
cargo install --path .
```

## Usage

### Examples

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

### Options

Below is a complete list of available command-line options:

| Short | Long                        | Argument          | Description                                |
| :---- | :-------------------------- | :---------------- | :----------------------------------------- |
|       |                             | `url`             | The request URL (required).                |
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
| `-v`  | `--verbose`                 |                   | Make the operation more talkative.         |
|       | `--http2`                   |                   | Use HTTP/2.                                |
|       | `--http2-prior-knowledge`   |                   | Use HTTP/2 with prior knowledge.           |
| `-h`  | `--help`                    |                   | Print help information.                    |
| `-V`  | `--version`                 |                   | Print version information.                 |

## Contributing

Contributions are welcome! Feel free to fork the repository, make your changes, and submit a pull request.

## License

This project is licensed under the [Apache License](LICENSE).
