use std::path::PathBuf;
use tokio::{spawn, signal, sync::oneshot};
use warp::Filter;
use http_redirector::*;

// #[argh(description = r#"concat!(env!("CARGO_PKG_DESCRIPTION"), "\nsee https://github.com/stackinspector/http-redirector")"#)]
#[derive(argh::FromArgs)]
/// A simple http redirection service with access logging based on an input key-link table. see https://github.com/stackinspector/http-redirector
struct Args {
    /// port
    #[argh(option, short = 'p', default = "8080")]
    port: u16,
    /// semicolon-separated list of scope,config
    #[argh(option, short = 'c', long = "configs")]
    input: String,
    /// log path
    #[argh(option, short = 'l')]
    log_path: Option<PathBuf>,
    /// request id header name
    #[argh(option, short = 'h')]
    req_id_header: Option<String>,
    // TODO: cfg
    // /// allow update
    // #[argh(switch, short = 'u')]
    // allow_update: bool,
}

#[tokio::main]
async fn main() {
    let Args { port, input, log_path, req_id_header } = argh::from_env();

    let req_id_header: &'static str = Box::leak(req_id_header.unwrap_or_default().into_boxed_str());

    let (state_ref, log_sender) = init(input, log_path, req_id_header.clone(), false).await.unwrap();
    let (tx, rx) = oneshot::channel();

    let route = warp::get()
        .and(warp::path::param::<String>())
        .and(warp::path::param::<String>())
        .and(warp::addr::remote())
        .and(warp::header::optional::<String>("X-Forwarded-For"))
        .and(warp::header::optional::<String>(req_id_header))
        .and(warp::header::optional::<String>("User-Agent"))
        .and(warp::any().map(move || state_ref.clone()))
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
