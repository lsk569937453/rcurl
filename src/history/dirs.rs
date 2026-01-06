use std::env;
use std::path::PathBuf;

/// 获取用户目录下的 .rcurl 目录路径
pub fn get_history_dir() -> PathBuf {
    let home = home_dir();
    home.join(".rcurl")
}

fn home_dir() -> PathBuf {
    if cfg!(windows) {
        env::var("USERPROFILE").ok().map(PathBuf::from).or_else(|| {
            env::var("HOMEDRIVE").ok().and_then(|drive| {
                env::var("HOMEPATH").ok().map(|path| {
                    let mut p = PathBuf::from(drive);
                    p.push(path);
                    p
                })
            })
        }).unwrap_or_else(|| PathBuf::from("."))
    } else {
        env::var("HOME").ok().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    }
}
