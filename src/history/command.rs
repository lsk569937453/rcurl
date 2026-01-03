use crate::cli::app_config::{Cli, QuickCommand};

/// 将 Cli 结构转换为命令字符串
pub fn command_from_cli(cli: &Cli) -> String {
    let mut cmd = String::from("rcurl");

    // Handle quick commands (ping, disk, telnet, etc.)
    if let Some(ref quick_cmd) = cli.quick_cmd {
        match quick_cmd {
            QuickCommand::Ping { target } => {
                cmd.push_str(&format!(" p {}", target));
                return cmd;
            }
            QuickCommand::Disk { target } => {
                cmd.push_str(&format!(" d {}", target));
                return cmd;
            }
            QuickCommand::Telnet { host, port } => {
                cmd.push_str(&format!(" t {} {}", host, port));
                return cmd;
            }
            QuickCommand::Ns { domain } => {
                cmd.push_str(&format!(" ns {}", domain));
                return cmd;
            }
        }
    }

    if let Some(ref method) = cli.method_option {
        cmd.push_str(&format!(" -X {}", method));
    }

    if let Some(ref data) = cli.body_option {
        cmd.push_str(&format!(" -d '{}'", data.replace('\'', "'\\''")));
    }

    for form in &cli.form_option {
        cmd.push_str(&format!(" -F '{}'", form.replace('\'', "'\\''")));
    }

    for header in &cli.headers {
        cmd.push_str(&format!(" -H '{}'", header.replace('\'', "'\\''")));
    }

    if let Some(ref cert) = cli.certificate_path_option {
        cmd.push_str(&format!(" -c '{}'", cert.replace('\'', "'\\''")));
    }

    if let Some(ref user) = cli.authority_option {
        cmd.push_str(&format!(" -u '{}'", user.replace('\'', "'\\''")));
    }

    if let Some(ref ua) = cli.user_agent_option {
        cmd.push_str(&format!(" -A '{}'", ua.replace('\'', "'\\''")));
    }

    if let Some(ref cookie) = cli.cookie_option {
        cmd.push_str(&format!(" -b '{}'", cookie.replace('\'', "'\\''")));
    }

    if let Some(ref referer) = cli.refer_option {
        cmd.push_str(&format!(" -e '{}'", referer.replace('\'', "'\\''")));
    }

    if let Some(ref output) = cli.file_path_option {
        cmd.push_str(&format!(" -o '{}'", output.replace('\'', "'\\''")));
    }

    if let Some(ref upload) = cli.uploadfile_option {
        cmd.push_str(&format!(" -T '{}'", upload.replace('\'', "'\\''")));
    }

    if let Some(ref quote) = cli.quote_option {
        cmd.push_str(&format!(" -Q '{}'", quote.replace('\'', "'\\''")));
    }

    if cli.skip_certificate_validate {
        cmd.push_str(" -k");
    }

    if cli.header_option {
        cmd.push_str(" -I");
    }

    if let Some(ref range) = cli.range_option {
        cmd.push_str(&format!(" -r '{}'", range.replace('\'', "'\\''")));
    }

    for _ in 0..cli.verbosity {
        cmd.push_str(" -v");
    }

    if cli.http2 {
        cmd.push_str(" --http2");
    }

    if cli.http2_prior_knowledge {
        cmd.push_str(" --http2-prior-knowledge");
    }

    if cli.noproxy {
        cmd.push_str(" --noproxy");
    }

    if cli.time {
        cmd.push_str(" --time");
    }

    if let Some(ref url) = cli.url {
        cmd.push(' ');
        cmd.push_str(url);
    }

    cmd
}
