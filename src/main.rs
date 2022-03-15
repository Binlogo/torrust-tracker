use std::net::SocketAddr;
use log::{info};
use torrust_tracker::{http_api_server, Configuration, TorrentTracker, UdpServer, HttpTrackerConfig, UdpTrackerConfig, HttpApiConfig, logging};
use std::sync::Arc;
use tokio::task::JoinHandle;
use torrust_tracker::torrust_http_tracker::server::HttpServer;

#[tokio::main]
async fn main() {
    let config = match Configuration::load_from_file() {
        Ok(config) => Arc::new(config),
        Err(error) => {
            panic!("{}", error)
        }
    };

    logging::setup_logging(&config);

    // the singleton torrent tracker that gets passed to the HTTP and UDP server
    let tracker = Arc::new(TorrentTracker::new(config.clone()));

    // Load torrents if enabled
    if config.persistence {
        load_torrents_into_memory(tracker.clone()).await;
    }

    // start torrent cleanup job (periodically removes old peers)
    let _torrent_cleanup_job = start_torrent_cleanup_job(config.clone(), tracker.clone()).unwrap();

    // start HTTP API server
    if config.http_api.enabled {
        let _api_server = start_api_server(&config.http_api, tracker.clone());
    }

    // start UDP tracker if enabled
    if config.udp_tracker.enabled {
        let _udp_server = start_udp_tracker_server(&config.udp_tracker, tracker.clone()).await;
    }

    // start UDP tracker for IPv6 if enabled
    if config.udp_tracker_ipv6.enabled {
        let _udp_server_ipv6 = start_udp_ipv6_tracker_server(&config.udp_tracker_ipv6, tracker.clone()).await;
    }

    // start HTTP tracker if enabled
    if config.http_tracker.enabled {
        let _http_server = start_http_tracker_server(&config.http_tracker, tracker.clone());
    }

    // start HTTPS tracker if enabled
    if config.http_tracker.ssl_enabled {
        let _http_ssl_server = start_http_ssl_tracker_server(&config.http_tracker, tracker.clone());
    }

    //start HTTP tracker for IPv6 if enabled
    if config.http_tracker_ipv6.enabled {
        let _http_server_ipv6 = start_http_tracker_server(&config.http_tracker_ipv6, tracker.clone());
    }

    // start HTTPS tracker for IPv6 if enabled
    if config.http_tracker_ipv6.ssl_enabled {
        let _http_ssl_server_ipv6 = start_http_ssl_tracker_server(&config.http_tracker_ipv6, tracker.clone());
    }

    // handle the signals here
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::select! {
        _ = ctrl_c => { info!("Torrust shutting down..") }
    }

    // Save torrents if enabled
    if config.persistence {
        save_torrents_into_memory(tracker.clone()).await;
    }
}

async fn load_torrents_into_memory(tracker: Arc<TorrentTracker>) {
    info!("Loading torrents from SQL into memory...");
    let _ = tracker.load_torrents(tracker.clone()).await;
    info!("Torrents loaded");
}

async fn save_torrents_into_memory(tracker: Arc<TorrentTracker>) {
    info!("Saving torrents into SQL from memory...");
    let _ = tracker.save_torrents(tracker.clone()).await;
    info!("Torrents saved");
}

fn start_torrent_cleanup_job(config: Arc<Configuration>, tracker: Arc<TorrentTracker>) -> Option<JoinHandle<()>> {
    let weak_tracker = std::sync::Arc::downgrade(&tracker);
    let interval = config.cleanup_interval.unwrap_or(600);

    return Some(tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(interval);
        let mut interval = tokio::time::interval(interval);
        interval.tick().await; // first tick is immediate...
        // periodically call tracker.cleanup_torrents()
        loop {
            interval.tick().await;
            if let Some(tracker) = weak_tracker.upgrade() {
                tracker.cleanup_torrents().await;
            } else {
                break;
            }
        }
    }))
}

fn start_api_server(config: &HttpApiConfig, tracker: Arc<TorrentTracker>) -> JoinHandle<()> {
    info!("Starting HTTP API server on: {}", config.bind_address);
    let bind_addr = config.bind_address.parse::<std::net::SocketAddr>().unwrap();

    tokio::spawn(async move {
        let server = http_api_server::build_server(tracker);
        server.bind(bind_addr).await;
    })
}

fn start_http_tracker_server(config: &HttpTrackerConfig, tracker: Arc<TorrentTracker>) -> JoinHandle<()> {
    let http_tracker = HttpServer::new(tracker);
    let bind_addr = config.bind_address.parse::<SocketAddr>().unwrap();

    tokio::spawn(async move {
        // run with tls if ssl_enabled and cert and key path are set
        info!("Starting HTTP server on: {}", bind_addr);
        http_tracker.start(bind_addr).await;
    })
}

fn start_http_ssl_tracker_server(config: &HttpTrackerConfig, tracker: Arc<TorrentTracker>) -> JoinHandle<()> {
    let http_tracker = HttpServer::new(tracker);
    let ssl_bind_addr = config.ssl_bind_address.parse::<SocketAddr>().unwrap();
    let ssl_cert_path = config.ssl_cert_path.clone();
    let ssl_key_path = config.ssl_key_path.clone();


    tokio::spawn(async move {
        // run with tls if ssl_enabled and cert and key path are set
        if ssl_cert_path.is_some() && ssl_key_path.is_some() {
            info!("Starting HTTPS server on: {} (TLS)", ssl_bind_addr);
            http_tracker.start_tls(ssl_bind_addr, ssl_cert_path.as_ref().unwrap(), ssl_key_path.as_ref().unwrap()).await;
        }
    })
}

async fn start_udp_tracker_server(config: &UdpTrackerConfig, tracker: Arc<TorrentTracker>) -> JoinHandle<()> {
    let udp_server = UdpServer::new(tracker).await.unwrap_or_else(|e| {
        panic!("Could not start UDP server: {}", e);
    });

    info!("Starting UDP server on: {}", config.bind_address);
    tokio::spawn(async move {
        udp_server.start().await;
    })
}

async fn start_udp_ipv6_tracker_server(config: &UdpTrackerConfig, tracker: Arc<TorrentTracker>) -> JoinHandle<()> {
    let udp_server = UdpServer::new_ipv6(tracker).await.unwrap_or_else(|e| {
        panic!("Could not start UDP server (IPv6): {}", e);
    });

    info!("Starting UDP server on: {}", config.bind_address);
    tokio::spawn(async move {
        udp_server.start().await;
    })
}
