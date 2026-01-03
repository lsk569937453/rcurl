use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use telnet::{Action, Event, Telnet};
use tokio::task;

pub async fn telnet_command(
    host: String,
    port: u16,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    task::spawn_blocking(move || run_telnet(host, port)).await?
}

fn run_telnet(host: String, port: u16) -> Result<RcurlResponse, anyhow::Error> {
    let addr = format!("{}:{}", host, port);
    println!("Connecting to {}...", addr);

    let mut telnet = Telnet::connect(addr.as_str(), 256)?;
    println!("Connected. Escape character is '^]'.\n");

    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    // stdin thread
    let stdin_thread = thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let mut input = String::new();

        loop {
            input.clear();
            if stdin.read_line(&mut input).ok() == Some(0) {
                let _ = tx.send(vec![]);
                break;
            }

            let line = input.trim_end_matches(&['\r', '\n'][..]);

            if line == "quit" || line == "exit" {
                let _ = tx.send(vec![]);
                break;
            }

            let mut buf = line.as_bytes().to_vec();
            buf.extend_from_slice(b"\r\n");
            if tx.send(buf).is_err() {
                break;
            }
        }
    });

    println!("Type commands and press Enter.\n");

    // 🔥 正确的 telnet 主循环
    loop {
        // ✅ 非阻塞读取 telnet
        match telnet.read_nonblocking() {
            Ok(event) => match event {
                Event::Data(data) => {
                    print!("{}", String::from_utf8_lossy(&data));
                    io::stdout().flush().ok();
                }

                // Telnet 协商（必须处理）
                Event::Negotiation(action, option) => match action {
                    Action::Do => {
                        let _ = telnet.negotiate(&Action::Wont, option);
                    }
                    Action::Will => {
                        let _ = telnet.negotiate(&Action::Dont, option);
                    }
                    _ => {}
                },

                Event::Subnegotiation(_, _) => {}
                Event::TimedOut | Event::NoData => {}
                Event::UnknownIAC(_) => {}
                Event::Error(e) => {
                    eprintln!("Telnet error: {}", e);
                    break;
                }
            },
            Err(e) => {
                eprintln!("Read error: {}", e);
                break;
            }
        }

        // stdin → telnet
        match rx.try_recv() {
            Ok(buf) => {
                if buf.is_empty() {
                    break;
                }
                let _ = telnet.write(&buf);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => break,
        }

        thread::sleep(Duration::from_millis(10));
    }

    let _ = stdin_thread.join();
    Ok(RcurlResponse::Telnet(()))
}
