use clap::Parser;
#[derive(Parser)]
#[command(author, version, about, long_about)]
pub struct Cli {
    /// The request url,like http://www.google.com
    pub url: String,
    ///  Specify request method to use
    #[arg(short = 'X', long = "request", value_name = "method", group = "http")]
    pub method_option: Option<String>,
    /// HTTP POST data.
    #[arg(short = 'd', long = "data", value_name = "data", group = "http")]
    pub body_option: Option<String>,
    /// Specify multipart MIME data.
    #[arg(
        short = 'F',
        long = "form",
        value_name = "name=content",
        group = "http"
    )]
    pub form_option: Vec<String>,
    /// The http headers.
    #[arg(
        short = 'H',
        long = "header",
        value_name = "header/@file",
        group = "http"
    )]
    pub headers: Vec<String>,
    /// The pem path.
    #[arg(short = 'c', long, group = "http")]
    pub certificate_path_option: Option<String>,
    /// Server user and password
    #[arg(
        short = 'u',
        long = "user",
        group = "http",
        value_name = "user:password"
    )]
    pub authority_option: Option<String>,
    ///  Send User-Agent <name> to server
    #[arg(short = 'A', long = "user-agent", value_name = "name", group = "http")]
    pub user_agent_option: Option<String>,
    /// The Cookie option.
    #[arg(
        short = 'b',
        long = "cookie",
        value_name = "data|filename",
        group = "http"
    )]
    pub cookie_option: Option<String>,
    ///  Referrer URL
    #[arg(short = 'e', long = "referer", value_name = "URL", group = "http")]
    pub refer_option: Option<String>,
    ///  Write to file instead of stdout.
    #[arg(
        global = true,
        long = "output",
        short = 'o',
        value_name = "file",
        default_missing_value = "none",
        group = "http"
    )]
    pub file_path_option: Option<String>,

    ///  Transfer local FILE to destination
    #[arg(long = "upload-file", short = 'T', value_name = "file")]
    pub uploadfile_option: Option<String>,

    /// Allow insecure server connections
    #[arg(short = 'k', long = "insecure")]
    pub skip_certificate_validate: bool,
    /// Show document info only
    #[arg(long = "head", short = 'I', group = "http")]
    pub header_option: bool,
    /// Retrieve only the bytes within RANGE
    #[arg(short = 'r', long = "range", value_name = "range", group = "http")]
    pub range_option: Option<String>,
    ///  Make the operation more talkative
    #[arg(short = 'v', long = "verbose")]
    pub debug: bool,
}
