use std::path::PathBuf;
use tokio::{spawn, signal, sync::oneshot};
use structopt::StructOpt;
use warp::Filter;
use http_redirector::*;

#[derive(StructOpt)]
#[structopt(about = concat!(env!("CARGO_PKG_DESCRIPTION"), "\nsee https://github.com/stackinspector/http-redirector"))]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long = "configs")]
    input: String,
    #[structopt(short = "l", long, parse(from_os_str))]
    log_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let Args { port, input, log_path } = Args::from_args();
    let (wrapped_state, log_sender) = init(input, log_path).await.unwrap();
    let (tx, rx) = oneshot::channel();

    let route = warp::get()
        .and(warp::path::param::<String>())
        .and(warp::path::param::<String>())
        .and(warp::addr::remote())
        .and(warp::header::optional::<String>("X-Forwarded-For"))
        .and(warp::any().map(move || wrapped_state.clone()))
        .and(warp::any().map(move || log_sender.clone()))
        .then(handle);

    let (_addr, server) = warp::serve(route).bind_with_graceful_shutdown(
        ([0, 0, 0, 0], port),
        async { rx.await.unwrap(); }
    );

    spawn(server);

    signal::ctrl_c().await.unwrap();
    tx.send(()).unwrap();
}
