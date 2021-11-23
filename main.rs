use std::{collections::HashMap, sync::Arc};
use http_redirector::{get, init, open_storage, handle};
use structopt::StructOpt;
use warp::Filter;

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

    warp::serve(
        warp::path::param::<String>()
            .and(warp::get())
            .and(warp::addr::remote())
            .and(warp::header::optional::<String>("X-Raw-IP"))
            .and(map_filter.clone())
            .and(storage_filter.clone())
            .and_then(handle)
    ).run(([127, 0, 0, 1], port)).await;
}
