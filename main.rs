use std::{sync::Arc, path::PathBuf};
use tokio::{spawn, signal, sync::oneshot};
use structopt::StructOpt;
use warp::Filter;
use http_redirector::{init, handle};

#[derive(StructOpt)]
#[structopt(about = concat!(env!("CARGO_PKG_DESCRIPTION"), "\nsee https://github.com/stackinspector/http-redirector"))]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long = "config")]
    url: String,
    #[structopt(short = "l", long, parse(from_os_str))]
    log_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let Args { port, url, log_path } = Args::from_args();

    let (map, log_sender) = init(url, log_path).await;
    let map = Arc::new(map);

    let map_filter = warp::any().map(move || map.clone());
    let log_sender_filter = warp::any().map(move || log_sender.clone());

    let (tx, rx) = oneshot::channel();

    let route = warp::path::param::<String>()
        .and(warp::get())
        .and(warp::addr::remote())
        .and(warp::header::optional::<String>("X-Forward-For"))
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
