mod adb_commands;
mod password;
mod scanning;

use adb_commands::ConnectOutcome;
use mdns_sd::{ResolvedService, ServiceDaemon};
use std::sync::mpsc;
use tokio::select;

#[tokio::main]
async fn main() {
    let skip_pair = std::env::args().find(|arg| arg == "--skip").is_some();

    let mdns = ServiceDaemon::new().expect("Failed to create mDNS daemon");

    if skip_pair {
        println!("Skipping pairing phase, connect all possible devices");
    } else {
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".to_string());
        let identifier = format!("ADBear@{hostname}");
        let password = password::generate();

        fast_qr::QRBuilder::new(format!("WIFI:T:ADB;S:{identifier};P:{password};;"))
            .build()
            .expect("Failed to print QR code")
            .print();
        println!("Scan this QR code on your phone: Settings → Developer options → Wireless debugging → Pair device with QR code");

        // --- Pairing phase ---
        match scanning::find_pairing_service(&mdns, &identifier).await {
            Ok(info) => {
                let Some(ip) = scanning::pick_best_ipv4(info.get_addresses_v4()) else {
                    eprintln!("Error: paired device has no IPv4 address");
                    let _ = mdns.shutdown();
                    std::process::exit(1);
                };
                let port = info.get_port();

                println!("Pairing with {ip}:{port}…");
                match adb_commands::pair(ip, port, &password) {
                    Ok(output) if output.status.success() => {
                        println!("Pairing successful.");
                    }
                    Ok(output) => {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        eprintln!(
                            "Pairing failed (exit {}): {}",
                            output.status.code().unwrap_or(-1),
                            stderr.trim()
                        );
                        let _ = mdns.shutdown();
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("Failed to run adb pair: {e}");
                        let _ = mdns.shutdown();
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                // Not fatal: device may already be paired.
                eprintln!(
                    "Pairing service not found ({e}); skipping pairing (device may already be paired)."
                );
            }
        }
    }

    // --- Connection phase ---
    let (tx, rx) = mpsc::channel::<ResolvedService>();
    let scanner = scanning::find_connection_service(&mdns, tx);

    let mut logic = tokio::task::spawn_blocking(move || {
        let mut last_error = None;
        while let Ok(info) = rx.recv() {
            let Some(ip) = scanning::pick_best_ipv4(info.get_addresses_v4()) else {
                last_error = Some("connection service has no IPv4 address".to_string());
                continue;
            };
            let port = info.get_port();

            println!("Connecting to {ip}:{port}…");
            match adb_commands::connect(ip, port) {
                Ok(output) => match adb_commands::parse_connect_output(&output) {
                    ConnectOutcome::Connected | ConnectOutcome::AlreadyConnected => {
                        let device_name = adb_commands::get_device_name(ip, port)
                            .ok()
                            .filter(|o| o.status.success())
                            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| format!("{ip}:{port}"));
                        println!("Connected to {device_name}");
                        return Ok(());
                    }
                    ConnectOutcome::Failed(msg) => {
                        eprintln!("Connection failed: {msg}");
                        last_error = Some(msg);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to run adb connect: {e}");
                    last_error = Some(e.to_string());
                }
            }
        }

        Err(last_error.unwrap_or_else(|| "connection service not found".to_string()))
    });

    let exit_code = select! {
        result = &mut logic => {
            match result {
                Ok(Ok(())) => 0,
                Ok(Err(e)) => {
                    eprintln!("Connection failed: {e}");
                    1
                }
                Err(e) => {
                    eprintln!("Connection task failed: {e}");
                    1
                }
            }
        }
        result = scanner => {
            if let Err(e) = result {
                eprintln!("Connection service scan stopped: {e}");
            }

            match logic.await {
                Ok(Ok(())) => 0,
                Ok(Err(e)) => {
                    eprintln!("Connection failed: {e}");
                    1
                }
                Err(e) => {
                    eprintln!("Connection task failed: {e}");
                    1
                }
            }
        }
    };

    let _ = mdns.shutdown();
    std::process::exit(exit_code);
}
