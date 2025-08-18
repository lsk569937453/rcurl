use clap::Parser;

#[derive(Parser, Default)]
#[command(author, version, about, long_about, arg_required_else_help = true)]
pub struct Cli {
    /// The request url,like http://www.google.com
    pub url: String,
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
}
