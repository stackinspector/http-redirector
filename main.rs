use std::{collections::HashMap, sync::Arc};
use http_redirector::{get, init, handle_lookup};
use structopt::StructOpt;
use warp::Filter;

#[derive(StructOpt)]
#[structopt(about = concat!(env!("CARGO_PKG_DESCRIPTION"), "\nsee https://github.com/stackinspector/http-redirector"))]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long = "config")]
    url: String,
}

#[tokio::main]
async fn main() {
    let Args { port, url } = Args::from_args();

    let state = Arc::new({
        let mut map = HashMap::new();
        init(get(url.as_str()).await.unwrap(), &mut map).unwrap();
        map
    });
    let state_filter = warp::any().map(move || state.clone());

    warp::serve(
        warp::path::param::<String>()
            .and(warp::get())
            .and(state_filter.clone())
            .and_then(handle_lookup)
    ).run(([127, 0, 0, 1], port)).await;
}
