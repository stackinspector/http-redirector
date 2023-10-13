use std::{path::PathBuf, convert::Infallible};
use hyper::{Server, service::{make_service_fn, service_fn}, server::conn::AddrStream};
use tokio::{spawn, signal, sync::oneshot};
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
    /// allow update
    #[argh(switch, short = 'u')]
    allow_update: bool,
}

#[tokio::main]
async fn main() {
    let Args { port, input, log_path, req_id_header, allow_update } = argh::from_env();
    let (ctx, log_closer) = Context::init(input, log_path, req_id_header, allow_update).await.unwrap();
    let (tx, rx) = oneshot::channel::<()>();

    let make_service = make_service_fn(move |conn: &AddrStream| {
        let remote_addr = conn.remote_addr();
        let ctx = ctx.clone();
        let service = service_fn(move |req| ctx.clone().handle(remote_addr, req));
        async move { Ok::<_, Infallible>(service) }
    });

    let server = Server::bind(&([0, 0, 0, 0], port).into())
        .serve(make_service)
        .with_graceful_shutdown(async { rx.await.unwrap(); });

    spawn(async { server.await.unwrap() });

    signal::ctrl_c().await.unwrap();
    // TODO wait for close finished (actor's and correct behavior)
    tx.send(()).unwrap();
    log_closer.wait_close().await.unwrap();
}
