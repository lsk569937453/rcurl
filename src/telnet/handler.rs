use crate::cli::app_config::Cli;
use crate::response::res::RcurlResponse;
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use telnet::Telnet;
use tokio::task;

pub async fn telnet_command(
    host: String,
    port: u16,
    _cli: Cli,
) -> Result<RcurlResponse, anyhow::Error> {
    // Run telnet in blocking task since telnet library is synchronous
    task::spawn_blocking(move || run_telnet(host, port)).await?
}

fn run_telnet(host: String, port: u16) -> Result<RcurlResponse, anyhow::Error> {
    // Connect to telnet server
    let addr = format!("{}:{}", host, port);
    println!("Connecting To {}...", host);
    let mut telnet = Telnet::connect(addr.as_str(), 256).map_err(|_e| {
        anyhow!(
            "Could not open connection to the host, on port {}: Connect failed",
            port
        )
    })?;

    println!("Trying {}...", addr);
    println!("Connected to {}.", addr);
    println!("Escape character is '^]'.");
    println!();

    // Create a channel for sending user input to the telnet thread
    let (tx, rx) = mpsc::channel::<Vec<u8>>();

    // Spawn thread to handle stdin and send to telnet via channel
    let stdin_thread = thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin_lock = stdin.lock();
        let mut input = String::new();

        loop {
            input.clear();
            match stdin_lock.read_line(&mut input) {
                Ok(0) => {
                    // EOF - send empty signal to quit
                    let _ = tx.send(vec![]);
                    break;
                }
                Ok(_) => {
                    let trimmed = input.trim();
                    if trimmed == "quit" || trimmed == "exit" {
                        // Send quit signal
                        let _ = tx.send(vec![]);
                        break;
                    } else if !trimmed.is_empty() {
                        // Send user input with CRLF
                        let mut command = trimmed.as_bytes().to_vec();
                        command.extend_from_slice(b"\r\n");
                        if tx.send(command).is_err() {
                            break;
                        }
                    }
                }
                Err(_e) => {
                    // Error reading stdin
                    let _ = tx.send(vec![]);
                    break;
                }
            }
        }
    });

    // Main thread handles telnet I/O
    println!("Type commands and press Enter. Type 'quit' or 'exit' to close.");
    println!();

    loop {
        // Try to read from telnet server (with timeout via try_read pattern)
        match telnet.read() {
            Ok(event) => {
                match event {
                    telnet::Event::Data(data) => {
                        // Server sent data - display it
                        let text = String::from_utf8_lossy(&data);
                        print!("{}", text);
                        io::stdout().flush().ok();
                    }
                    telnet::Event::TimedOut => {
                        // Timeout waiting for data, check for user input
                    }
                    telnet::Event::NoData => {
                        // No data available, check for user input
                    }
                    _ => {}
                }
            }
            Err(_e) => {
                // Connection error
                println!("\nConnection lost.");
                break;
            }
        }

        // Check for user input from channel (non-blocking)
        if let Ok(command) = rx.try_recv() {
            if command.is_empty() {
                // Quit signal received
                println!("\nClosing connection...");
                break;
            }
            // Send user input to telnet server
            telnet.write(&command).ok();
        }

        // Small sleep to prevent busy-waiting
        thread::sleep(Duration::from_millis(10));
    }

    // Wait for stdin thread to finish
    let _ = stdin_thread.join();

    Ok(RcurlResponse::Telnet(()))
}
