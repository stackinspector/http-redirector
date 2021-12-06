use std::{collections::HashMap, sync::Arc};
use tokio::{spawn, signal, sync::oneshot};
use structopt::StructOpt;
use warp::Filter;
use http_redirector::{get, init, open_storage, handle};

#[derive(StructOpt)]
#[structopt(about = concat!(env!("CARGO_PKG_DESCRIPTION"), "\nsee https://github.com/stackinspector/http-redirector"))]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long = "config")]
    url: String,
    #[structopt(short = "l", long)]
    log_path: String,
}

#[tokio::main]
async fn main() {
    let Args { port, url, log_path } = Args::from_args();

    let map = Arc::new({
        let mut _map = HashMap::new();
        init(get(url.as_str()).await.unwrap(), &mut _map).unwrap();
        _map
    });
    let map_filter = warp::any().map(move || map.clone());

    let storage = Arc::new(open_storage(log_path, url));
    let storage_filter = warp::any().map(move || storage.clone());

    let (tx, rx) = oneshot::channel();

    let route = warp::path::param::<String>()
        .and(warp::get())
        .and(warp::addr::remote())
        .and(warp::header::optional::<String>("X-Raw-IP"))
        .and(map_filter.clone())
        .and(storage_filter.clone())
        .and_then(handle);

    let (_addr, server) = warp::serve(route).bind_with_graceful_shutdown(
        ([127, 0, 0, 1], port),
        async { rx.await.ok(); }
    );

    spawn(server);

    signal::ctrl_c().await.unwrap();
    tx.send(()).unwrap();
}
