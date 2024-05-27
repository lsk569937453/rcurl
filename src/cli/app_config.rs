use clap::Parser;

#[derive(Parser)]
#[command(author, version, about, long_about, arg_required_else_help = true)]
// #[clap(disable_help_flag = true)]
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
    #[arg(
        short = 'b',
        long = "cookie",
        value_name = "data|filename",
        hide_short_help = true
    )]
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
    ///  Make the operation more talkative
    #[arg(short = 'v', long = "verbose")]
    pub debug: bool,
    // /// Print help
    // #[arg(short, long, action = clap::ArgAction::Help)]
    // pub help: bool,
    // /// Print help, including uncommon options
    // #[arg(long, action = clap::ArgAction::Help)]
    // pub help_all: bool,
}

impl Cli {
    pub fn new() -> Self {
        Cli {
            url: String::new(),
            method_option: None,
            body_option: None,
            form_option: Vec::new(),
            headers: Vec::new(),
            certificate_path_option: None,
            authority_option: None,
            user_agent_option: None,
            cookie_option: None,
            refer_option: None,
            file_path_option: None,
            uploadfile_option: None,
            quote_option: None,
            skip_certificate_validate: false,
            header_option: false,
            range_option: None,
            debug: false,
            // help: false,
            // help_all: false,
        }
    }
}
