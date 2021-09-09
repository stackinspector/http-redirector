use std::{sync::Arc, convert::Infallible, net::SocketAddr};
use structopt::StructOpt;
use reqwest::{get as http_get};
use hyper::{Method, Body, Request, Response, Server};
use hyper::service::{make_service_fn, service_fn};
use http_redirector::{Map, init, lookup};

#[derive(StructOpt)]
#[structopt(name = "http-redirector")]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long)]
    config: String,
}

async fn handler(req: Request<Body>, map: Arc<Map>) -> Result<Response<Body>, Infallible> {
    Ok(match *req.method() {
        Method::GET | Method::HEAD => match lookup(req.uri().path(), &map) {
            None => Response::builder().status(404).body(Body::empty()).unwrap(),
            Some(result) => Response::builder().status(307).header("Location", result).body(Body::empty()).unwrap(),
        },
        _ => Response::builder().status(400).body(Body::empty()).unwrap(),
    })
}

#[tokio::main]
async fn main() {
    let args = Args::from_args();
    let config = http_get(args.config).await.unwrap().text().await.unwrap();
    let map = Arc::new(init(config).unwrap());

    let service = make_service_fn(move |_conn| {
        let map_local = map.clone();
        async move { Ok::<_, Infallible>(service_fn(move |req| handler(req, map_local.clone()))) }
    });
    let server = Server::bind(&SocketAddr::from(([127, 0, 0, 1], args.port))).serve(service);
    server.await.unwrap();
}
