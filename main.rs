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
    /// update key
    #[argh(option, short = 'k')]
    update_key: Option<String>,
    // TODO: cfg
    /// allow update
    #[argh(switch, short = 'u')]
    allow_update: bool,
    /// allow update
    #[argh(switch, short = 'v')]
    return_value: bool,
    /// allow update
    #[argh(switch, short = 'f')]
    no_fill_https: bool,
}

#[tokio::main]
async fn main() {
    let Args { port, input, log_path, req_id_header, update_key, allow_update, return_value, no_fill_https } = argh::from_env();
    let (ctx, log_closer) = Context::init(input, log_path, req_id_header, update_key, allow_update, return_value, no_fill_https).await.unwrap();
    let (tx, rx) = oneshot::channel::<()>();

    let make_service = make_service_fn(move |conn: &AddrStream| {
        let remote_addr = conn.remote_addr();
        let ctx = ctx.clone();
        let service = service_fn(move |req| ctx.clone().handle(remote_addr, req));
        async move { Ok::<_, Infallible>(service) }
    });

    let server_handle = spawn(Server::bind(&([0, 0, 0, 0], port).into())
        .serve(make_service)
        .with_graceful_shutdown(async { rx.await.unwrap(); }));

    signal::ctrl_c().await.unwrap();
    tx.send(()).unwrap();
    tokio::join!(
        async { server_handle.await.unwrap().unwrap() },
        async { log_closer.wait_close().await.unwrap() }
    );
}
