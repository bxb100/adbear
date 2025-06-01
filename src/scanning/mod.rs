use anyhow::anyhow;
use dashmap::DashMap;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::time::Duration;
use tokio::time::{sleep, timeout};

const MDNS_SCAN_TYPE: &str = "_adb-tls-connect._tcp.local.";
const MDNS_PAIRING_TYPE: &str = "_adb-tls-pairing._tcp.local.";

async fn find_mdns_service(
    mdns: &ServiceDaemon,
    service_type: &str,
    is_match: impl Fn(&ServiceInfo) -> bool,
) -> Option<ServiceInfo> {
    let receiver = mdns.browse(service_type).expect("Failed to browse");

    while let Ok(event) = receiver.recv_async().await {
        if let ServiceEvent::ServiceResolved(info) = event {
            if is_match(&info) {
                return Some(info);
            }
        }
    }
    None
}

pub async fn find_pairing_service(
    mdns: &ServiceDaemon,
    identifier: &str,
) -> anyhow::Result<ServiceInfo> {
    let service_type = MDNS_PAIRING_TYPE.to_string();

    match timeout(
        Duration::from_secs(30),
        find_mdns_service(mdns, MDNS_PAIRING_TYPE, |info| {
            info.get_fullname() == format!("{}.{service_type}", identifier)
        }),
    )
    .await
    {
        Ok(Some(info)) => Ok(info),
        Ok(None) => Err(anyhow!("Device not found")),
        Err(_) => Err(anyhow!("Timeout")),
    }
}

pub async fn find_connection_service(
    mdns: &ServiceDaemon,
    guid: Option<String>,
) -> anyhow::Result<DashMap<String, ServiceInfo>> {
    let map = DashMap::<String, ServiceInfo>::new();
    let task = async {
        let receiver = mdns.browse(MDNS_SCAN_TYPE).expect("Failed to browse");
        while let Ok(event) = receiver.recv_async().await {
            if let ServiceEvent::ServiceResolved(info) = event {
                let name = info.get_fullname();
                if let Some(ref guid) = guid {
                    if !name.starts_with(guid) {
                        continue;
                    }
                }
                if name.ends_with(MDNS_SCAN_TYPE) {
                    map.insert(info.get_fullname().to_owned(), info);
                }
            }
        }
    };

    tokio::select!(
        _ = sleep(Duration::from_secs(3)) => {}
        _ = task => {}
    );

    Ok(map)
}
