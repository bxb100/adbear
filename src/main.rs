mod adb_commands;
mod password;
mod scanning;

use mdns_sd::ServiceDaemon;
use regex_lite::Regex;
use std::env;
use std::process::Output;

#[tokio::main]
async fn main() {
    let mdns = ServiceDaemon::new().expect("Failed to create daemon");

    let hostname = env::var("HOSTNAME").unwrap_or("localhost".to_string());
    let identifier = format!("ADBear@{hostname}");
    let password = password::generate();
    fast_qr::QRBuilder::new(format!("WIFI:T:ADB;S:{identifier};P:{password};;"))
        .build()
        .expect("Failed to print QR code")
        .print();

    let info = scanning::find_pairing_service(&mdns, &identifier)
        .await
        .expect("Failed to find pairing service");
    let port = info.get_port();
    let addresses = info.get_addresses_v4();
    let ip = addresses.iter().next().unwrap();
    let guid = match adb_commands::pair(ip, port, &password) {
        Ok(Output { status, stdout, .. }) if status.success() => {
            let result = unsafe { String::from_utf8_unchecked(stdout) };
            println!("{result}");

            let line = result.lines().next().unwrap();
            // eg. Successfully paired to 192.168.1.86:41915 [guid=adb-939AX05XBZ-vWgJpq]
            let line_regex =
                Regex::new(r"Successfully paired to ([^:]*):([0-9]*) \[guid=([^]]*)]").unwrap();
            line_regex.captures(line).and_then(|caps| {
                let guid = caps.get(3)?.as_str().to_owned();
                Some(guid)
            })
        }
        _ => {
            // https://stackoverflow.com/questions/33316006/adb-error-error-protocol-fault-couldnt-read-status-invalid-argument
            panic!("Failed to pair, maybe need restart adb server");
        }
    };

    if let Ok(infos) = scanning::find_connection_service(&mdns, guid).await {
        for entry in infos.iter() {
            let (name, info) = entry.pair();
            println!(
                "Found service: {} at port {}",
                info.get_fullname(),
                info.get_port()
            );

            let port = info.get_port();
            let addresses = info.get_addresses_v4();
            let ip = addresses.iter().next().unwrap();
            let Ok(output) = adb_commands::connect(ip, port) else {
                println!("Failed to connect {name}");
                continue;
            };
            if output.status.success() && !output.stdout.starts_with(b"failed") {
                if let Ok(output) = adb_commands::get_device_name(ip, port) {
                    println!(
                        "Connected to {device_name}",
                        device_name = String::from_utf8_lossy(&output.stdout)
                    );
                    continue;
                }
            }
            println!("Failed to connect {name}");
        }
    }

    mdns.shutdown().unwrap();
}
