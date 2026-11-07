//! A WireGuard tunnel inside desk's own process, like a browser's VPN
//! extension: no system VPN is touched, nothing else on the Mac goes through
//! it, no admin rights. Used by the prompt rewriter (prompt_ai.rs) to reach
//! the LLM server from outside its network; it lives only as long as that one rewrite.
//!
//! onetun runs WireGuard and a TCP stack in userspace and forwards a local
//! port (127.0.0.1:<free>) to one host:port behind the tunnel.

use base64::Engine;
use onetun::config::{Config, PortForwardConfig, PortProtocol, PublicKey, StaticSecret};
use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::Arc;

/// The parts of a standard WireGuard `.conf` (as the WireGuard app exports
/// it) that a one-peer client needs.
#[derive(Debug, PartialEq)]
pub struct WgConf {
    pub private_key: [u8; 32],
    pub address: IpAddr,
    pub peer_public_key: [u8; 32],
    pub preshared_key: Option<[u8; 32]>,
    pub endpoint: String,
    pub keepalive: Option<u16>,
    pub mtu: Option<usize>,
}

pub fn parse_conf(text: &str) -> Result<WgConf, String> {
    let mut section = String::new();
    let (mut private_key, mut address, mut mtu) = (None, None, None);
    let (mut peer_public_key, mut preshared_key, mut endpoint, mut keepalive) = (None, None, None, None);
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            section = line.trim_matches(['[', ']']).to_ascii_lowercase();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match (section.as_str(), key.as_str()) {
            ("interface", "privatekey") => private_key = Some(key32(value)?),
            ("interface", "address") => {
                // "10.0.0.2/32, fd00::2/128": the first address is ours.
                let first = value.split(',').next().unwrap_or("").trim();
                let ip = first.split('/').next().unwrap_or("");
                address = Some(ip.parse::<IpAddr>().map_err(|_| format!("zły Address: {first}"))?);
            }
            ("interface", "mtu") => mtu = value.parse().ok(),
            ("peer", "publickey") if peer_public_key.is_none() => peer_public_key = Some(key32(value)?),
            ("peer", "presharedkey") if preshared_key.is_none() => preshared_key = Some(key32(value)?),
            ("peer", "endpoint") if endpoint.is_none() => endpoint = Some(value.to_string()),
            ("peer", "persistentkeepalive") if keepalive.is_none() => keepalive = value.parse().ok(),
            _ => {}
        }
    }
    Ok(WgConf {
        private_key: private_key.ok_or("brak PrivateKey w [Interface]")?,
        address: address.ok_or("brak Address w [Interface]")?,
        peer_public_key: peer_public_key.ok_or("brak PublicKey w [Peer]")?,
        preshared_key,
        endpoint: endpoint.ok_or("brak Endpoint w [Peer]")?,
        keepalive,
        mtu,
    })
}

fn key32(b64: &str) -> Result<[u8; 32], String> {
    base64::engine::general_purpose::STANDARD
        .decode(b64)
        .ok()
        .and_then(|v| <[u8; 32]>::try_from(v).ok())
        .ok_or_else(|| "zły klucz w pliku WireGuard".to_string())
}

/// Starts the tunnel on a background thread and returns the local address
/// that leads to `target` through it. The tunnel ends with the process.
pub fn start(conf: &WgConf, target: SocketAddr) -> Result<SocketAddr, String> {
    let endpoint_addr = conf
        .endpoint
        .to_socket_addrs()
        .ok()
        .and_then(|mut a| a.next())
        .ok_or_else(|| format!("nie mogę znaleźć serwera VPN {}", conf.endpoint))?;
    // A free local port: bind to :0, read it, let onetun take it over. onetun
    // binds by address, so there is a short window where another local
    // process could take the port first; it would then receive one prompt
    // (no keys, the tunnel is not exposed). Accepted for a desktop app: a
    // process already running as a local user can read the prompt anyway.
    let local = TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map_err(|e| format!("port lokalny: {e}"))?;
    let endpoint_bind_addr: SocketAddr = if endpoint_addr.is_ipv4() {
        "0.0.0.0:0".parse().unwrap()
    } else {
        "[::]:0".parse().unwrap()
    };
    let config = Config {
        port_forwards: vec![PortForwardConfig {
            source: local,
            destination: target,
            protocol: PortProtocol::Tcp,
            remote: false,
        }],
        remote_port_forwards: Vec::new(),
        private_key: Arc::new(StaticSecret::from(conf.private_key)),
        endpoint_public_key: Arc::new(PublicKey::from(conf.peer_public_key)),
        preshared_key: conf.preshared_key,
        endpoint_addr,
        endpoint_bind_addr,
        source_peer_ip: conf.address,
        keepalive_seconds: conf.keepalive,
        max_transmission_unit: conf.mtu.unwrap_or(1420),
        log: "error".into(),
        warnings: Vec::new(),
        pcap_file: None,
    };
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => {
                let _ = ready_tx.send(Err(format!("tokio: {e}")));
                return;
            }
        };
        rt.block_on(async move {
            let started = onetun::start_tunnels(config, onetun::events::Bus::default()).await;
            let failed = started.is_err();
            let _ = ready_tx.send(started.map_err(|e| format!("WireGuard: {e:#}")));
            if !failed {
                std::future::pending::<()>().await;
            }
        });
    });
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .map_err(|_| "WireGuard nie wystartował".to_string())??;
    // onetun binds the local port in a task of its own; wait until it listens.
    for _ in 0..40 {
        if std::net::TcpStream::connect_timeout(&local, std::time::Duration::from_millis(100)).is_ok() {
            return Ok(local);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Err("tunel WireGuard nie nasłuchuje".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Throwaway keys generated for this test only.
    const KEY: &str = "YNqHbfBQKaGvzefSSuP6uJcg5Bg3xMrUh0bIQ6tTW0A=";
    const PEER: &str = "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=";

    #[test]
    fn parses_an_exported_wireguard_conf() {
        let text = format!(
            "[Interface]\nPrivateKey = {KEY}\nAddress = 10.8.0.5/32, fd00::5/128\nDNS = 1.1.1.1\nMTU = 1380\n\n\
             [Peer]\n# office\nPublicKey = {PEER}\nAllowedIPs = 10.10.0.0/24\nEndpoint = vpn.example.com:51820\nPersistentKeepalive = 25\n"
        );
        let c = parse_conf(&text).unwrap();
        assert_eq!(c.address, "10.8.0.5".parse::<IpAddr>().unwrap());
        assert_eq!(c.endpoint, "vpn.example.com:51820");
        assert_eq!(c.keepalive, Some(25));
        assert_eq!(c.mtu, Some(1380));
        assert_eq!(c.preshared_key, None);
        assert_eq!(c.private_key, key32(KEY).unwrap());
    }

    #[test]
    fn says_what_is_missing() {
        assert_eq!(parse_conf("[Interface]\nAddress = 10.0.0.2/32\n").unwrap_err(), "brak PrivateKey w [Interface]");
        let no_peer = format!("[Interface]\nPrivateKey = {KEY}\nAddress = 10.0.0.2/32\n");
        assert_eq!(parse_conf(&no_peer).unwrap_err(), "brak PublicKey w [Peer]");
        assert!(parse_conf("[Interface]\nPrivateKey = nope\n").is_err());
    }
}
