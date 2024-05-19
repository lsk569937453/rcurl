use clap::Parser;
#[derive(Parser)]
#[command(author, version, about, long_about)]
pub struct Cli {
    /// The request url,like http://www.google.com
    pub url: String,
    /// The http method,like GET,POST,etc.
    #[arg(short = 'X', long, value_name = "HTTP Method")]
    pub method_option: Option<String>,
    /// The body of the http request.
    #[arg(short = 'd', long)]
    pub body_option: Option<String>,
    /// The form data of the http request.
    #[arg(short = 'F', long)]
    pub form_option: Vec<String>,
    /// The http headers.
    #[arg(short = 'H', long)]
    pub headers: Vec<String>,
    /// The pem path.
    #[arg(short = 'c', long)]
    pub certificate_path_option: Option<String>,

    /// The User Agent.
    #[arg(short = 'A', long)]
    pub user_agent_option: Option<String>,
    /// The Cookie option.
    #[arg(short = 'b', long)]
    pub cookie_option: Option<String>,

    /// The downloading file path .
    #[arg(global = true, short = 'o', long, default_missing_value = "none")]
    pub file_path_option: Option<String>,

    /// Skip certificate validation.
    #[arg(short = 'k', long)]
    pub skip_certificate_validate: bool,

    #[arg(long = "head")]
    pub header_option: bool,
    /// Http Range .
    #[arg(short = 'r', long)]
    pub range_option: Option<String>,
    /// The debug switch.
    #[arg(short = 'v', long)]
    pub debug: bool,
}
