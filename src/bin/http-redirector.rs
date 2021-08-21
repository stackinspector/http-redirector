use std::{sync::Arc, thread::spawn};
use structopt::StructOpt;
use ureq::get;
use tiny_http::{Header, Response, Server};
use http_redirector::{init, lookup};

#[derive(StructOpt)]
#[structopt(name = "http-redirector")]
struct Args {
    #[structopt(short = "p", long, default_value = "8080")]
    port: u16,
    #[structopt(short = "c", long)]
    config: String,
}

fn main() {
    let args = Args::from_args();
    let config = get(args.config.as_str()).call().unwrap().into_string().unwrap();
    let map = Arc::new(init(config).unwrap());
    let server = Server::http(format!("0.0.0.0:{}", args.port)).unwrap();

    for request in server.incoming_requests() {
        let map_local = map.clone();
        spawn(move || {
            let response = match lookup(request.url(), &map_local) {
                None => Response::empty(404),
                Some(result) => Response::empty(301)
                    .with_header(Header::from_bytes(&b"Location"[..], &result.as_bytes()[..]).unwrap()),
            };
            request.respond(response).unwrap();
        });
    }
}
