use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use anyhow::Context;

pub async fn disk_size_command(
    path: String,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    // Use platform-specific commands to get disk information
    #[cfg(target_os = "windows")]
    {
        disk_size_windows(path).await
    }

    #[cfg(not(target_os = "windows"))]
    {
        disk_size_unix(path).await
    }
}

#[cfg(target_os = "windows")]
async fn disk_size_windows(path: String) -> Result<RcurlResponse, anyhow::Error> {
    use std::process::Command;

    let path_abs = std::path::Path::new(&path)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(&path));

    let output = Command::new("wmic")
        .args(&["logicaldisk", "get", "name,size,freespace"])
        .output()
        .context("Failed to execute wmic command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{}", stdout);

    Ok(RcurlResponse::DiskSize(()))
}

#[cfg(not(target_os = "windows"))]
async fn disk_size_unix(path: String) -> Result<RcurlResponse, anyhow::Error> {
    use std::process::Command;

    let output = Command::new("df")
        .arg("-h")
        .arg(&path)
        .output()
        .context("Failed to execute df command")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{}", stdout);

    Ok(RcurlResponse::DiskSize(()))
}
