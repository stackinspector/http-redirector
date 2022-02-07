use std::{sync::Arc, net::SocketAddr, path::PathBuf};
use tokio::{spawn, signal, sync::oneshot};
use structopt::StructOpt;
use warp::{Filter, http::Response, hyper::Body};
use http_redirector::*;

#[derive(StructOpt)]
#[structopt(about = concat!(env!("CARGO_PKG_DESCRIPTION"), "\nsee https://github.com/stackinspector/http-redirector"))]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long = "config")]
    url: String,
    #[structopt(short = "l", long, parse(from_os_str))]
    log_path: Option<PathBuf>,
    #[structopt(short = "x", long = "prefix")]
    prefix: String,
}

async fn handle(
    key: String, ip: Option<SocketAddr>, xff: Option<String>, map: Arc<Map>, log_sender: Sender
) -> Result<impl warp::Reply, warp::Rejection> {
    let time = now();
    let mut from = Vec::new();
    if let Some(ip) = ip {
        from.push(ip.to_string())
    };
    if let Some(xff) = xff {
        for _ip in xff.split(',') {
            from.push(_ip.to_owned())
        }
    };
    let result = map.get(&key);
    let hit = result.is_some();
    log_sender.send(Event { time, from, key, hit }).unwrap();
    Ok(match result {
        None => Response::builder().status(404).body(Body::empty()).unwrap(),
        Some(val) => Response::builder().status(307).header("Location", val).body(Body::empty()).unwrap(),
    })
}

#[tokio::main]
async fn main() {
    let Args { port, url, log_path, prefix } = Args::from_args();

    let (map, log_sender) = init(url, log_path).await;
    let map = Arc::new(map);

    let map_filter = warp::any().map(move || map.clone());
    let log_sender_filter = warp::any().map(move || log_sender.clone());

    let (tx, rx) = oneshot::channel();

    let route = warp::path(prefix)
        .and(warp::path::param::<String>())
        .and(warp::get())
        .and(warp::addr::remote())
        .and(warp::header::optional::<String>("X-Forwarded-For"))
        .and(map_filter.clone())
        .and(log_sender_filter.clone())
        .and_then(handle);

    let (_addr, server) = warp::serve(route).bind_with_graceful_shutdown(
        ([0, 0, 0, 0], port),
        async { rx.await.unwrap(); }
    );

    spawn(server);

    signal::ctrl_c().await.unwrap();
    tx.send(()).unwrap();
}
