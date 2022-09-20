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
    #[cfg(feature = "req-id")]
    #[argh(option)]
    req_id_header: String,
}

#[tokio::main]
async fn main() {
    let Args { port, input, log_path, #[cfg(feature = "req-id")] req_id_header } = argh::from_env();

    #[cfg(feature = "req-id")]
    let maybe_req_id_header = Some(req_id_header.clone());
    #[cfg(not(feature = "req-id"))]
    let maybe_req_id_header = None;

    let (wrapped_state, log_sender) = init(input, log_path, maybe_req_id_header).await.unwrap();
    let (tx, rx) = oneshot::channel();

    let route = warp::get()
        .and(warp::path::param::<String>())
        .and(warp::path::param::<String>())
        .and(warp::addr::remote())
        .and(warp::header::optional::<String>("X-Forwarded-For"));

    #[cfg(feature = "req-id")]
    let req_id_header: &'static str = Box::leak(req_id_header.into_boxed_str());
    #[cfg(feature = "req-id")]
    let route = route.and(warp::header::optional::<String>(req_id_header));

    let route = route
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
