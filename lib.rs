use std::{collections::HashMap, io::{self, Write}, fs, path::PathBuf, net::SocketAddr, sync::Arc, borrow::Cow, convert::Infallible};
use tokio::{spawn, sync::mpsc::{unbounded_channel, UnboundedSender}};
use serde::Serialize;
use hyper::{http::{Request, Response, header, Method, StatusCode}, Body};

const UPDATE_PATH_STR: &str = "__update__";

fn now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis().try_into().unwrap()
}

fn split_kv<'a, I: Iterator<Item = &'a str>>(mut iter: I) -> Option<(&'a str, &'a str)> {
    let key = iter.next()?;
    let val = iter.next()?;
    if iter.next().is_some() { return None };
    Some((key, val))
}

#[derive(Serialize, Clone, Debug)]
pub struct Scope {
    pub url: String,
    pub map: HashMap<String, String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct State {
    scopes: HashMap<String, Scope>,
    allow_update: bool,
}

pub type StateRef = Arc<State>;

#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum RequestEvent {
    Get {
        hit: bool,
    },
    Update {
        result: UpdateResult,
    },
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum InnerEvent<'a> {
    Init {
        ver: &'a str,
        state: &'a State,
        req_id_header: Option<&'a str>,
    },
    Request {
        from: Vec<Cow<'a, str>>,
        req_id: Option<&'a str>,
        ua: Option<&'a str>,
        scope: &'a str,
        key: &'a str,
        inner: RequestEvent,
    }
}

#[derive(Serialize, Debug)]
pub struct Event<'a> {
    time: u64,
    event: InnerEvent<'a>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum UpdateResult {
    Succeed {
        // TODO &'a when put back update
        new: Scope,
        old: Scope,
    },
    ScopeNotFound,
    GetConfigError(String),
    ParseConfigError,
}

type HttpClient = hyper::Client<hyper_rustls::HttpsConnector<hyper::client::connect::HttpConnector>>;

fn build_http_client() -> HttpClient {
    let connector = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_only()
        .enable_http1()
        .build();
    hyper::Client::builder().build(connector)
}

async fn get(url: &str) -> anyhow::Result<String> {
    if url.starts_with("http") {
        // TODO add to Context when put back update
        let client = build_http_client();
        let resp = client.get(url.parse()?).await?;
        let status = resp.status().as_u16();
        if status == 200 {
            let bytes = hyper::body::to_bytes(resp.into_body()).await?;
            Ok(String::from_utf8(bytes.as_ref().to_vec())?)
        } else {
            Err(anyhow::anyhow!("http request returns status {}", status))
        }
    } else {
        Ok(tokio::fs::read_to_string(url).await?)
    }
}

// TODO tokio-actor = { git = "https://github.com/Berylsoft/actor" }

pub type LogSender = UnboundedSender<String>;

fn log_thread<W: Write + 'static + Send>(mut writer: W) -> LogSender {
    let (tx, mut rx) = unbounded_channel::<String>();
    spawn(async move {
        while let Some(event) = rx.recv().await {
            let mut line = event.into_bytes();
            line.push(b'\n');
            writer.write_all(&line).unwrap();
        }
    });
    tx
}

fn init_map(config: &str) -> Option<HashMap<String, String>> {
    let mut map = HashMap::new();
    for line in config.lines().filter(|s| !s.is_empty()) {
        let (key, val) = split_kv(line.split(' ').filter(|s| !s.is_empty()))?;
        let val = if val.starts_with("http://") {
            val.to_owned()
        } else {
            format!("https://{}", val)
        };
        assert!(matches!(map.insert(key.to_owned(), val), None));
    }
    Some(map)
}

// TODO split(char) or split(str)?

#[derive(Clone)]
pub struct Context {
    state_ref: StateRef,
    log_sender: LogSender,
    req_id_header: Option<Arc<str>>,
}

impl Context {
    pub async fn init(
        input: String,
        log_path: Option<PathBuf>,
        req_id_header: Option<String>,
        allow_update: bool,
    ) -> anyhow::Result<Context> {
        let req_id_header = req_id_header.map(Arc::from);
        let mut scopes = HashMap::new();
        for pair in input.split(';') {
            let (scope_name, url) = split_kv(pair.split(',')).ok_or_else(|| {
                anyhow::anyhow!("parsing input error")
            })?;
            if scope_name == UPDATE_PATH_STR {
                return Err(anyhow::anyhow!("scope name should not be \"{}\"", UPDATE_PATH_STR))
            }
            let config = get(url).await?;
            let map = init_map(config.as_str()).ok_or_else(|| {
                anyhow::anyhow!("parsing config \"{}\" error", url)
            })?;
            scopes.insert(scope_name.to_owned(), Scope { url: url.to_owned(), map });
        }
        let state = State { scopes, allow_update };
        let log_sender = match log_path {
            Some(path) => log_thread(fs::OpenOptions::new().write(true).create(true).append(true).open(path)?),
            None => log_thread(io::stdout()),
        };
        let time = now();
        let event = Event { time, event: InnerEvent::Init {
            ver: env!("CARGO_PKG_VERSION"),
            state: &state,
            req_id_header: req_id_header.as_deref(),
        } };
        log_sender.send(serde_json::to_string(&event).unwrap())?;
        Ok(Context {
            state_ref: Arc::new(state),
            log_sender,
            req_id_header,
        })
    }

    pub async fn handle(self, remote_addr: SocketAddr, req: Request<Body>) -> Result<Response<Body>, Infallible> {
        // TODO if Err is not ! then empty response??
        let Context { state_ref, log_sender, req_id_header } = self;
        let resp = Response::builder();

        macro_rules! err {
            ($status:tt) => {
                return Ok(resp.status(StatusCode::$status).body(Body::empty()).unwrap())
            };
        }

        if req.method() != Method::GET {
            err!(METHOD_NOT_ALLOWED);
        }

        // TODO: record not matched request?
        let Some((scope, key)) = split_kv(req.uri().path().split('/').skip(1).take(2)) else {
            err!(NOT_FOUND);
        };

        macro_rules! header {
            ($key:expr) => {
                match req.headers().get($key) {
                    Some(v) => match v.to_str() {
                        Ok(v) => Some(v),
                        Err(_) => err!(BAD_REQUEST),
                    },
                    None => None,
                }
            };
        }

        let xff = header!("X-Forwarded-For");
        let ua = header!("User-Agent");
        let req_id = match req_id_header {
            Some(k) => header!(k.as_ref()),
            None => None,
        };

        let time = now();
        let mut from = Vec::with_capacity(3);
        if let Some(xff) = xff {
            for _ip in xff.split(',') {
                from.push(Cow::Borrowed(_ip))
            }
        };
        from.push(Cow::Owned(remote_addr.to_string()));
        let (resp, event) = {
            let scope_ref = state_ref.scopes.get(scope);
            let result = scope_ref.as_deref().and_then(|scope_ref| scope_ref.map.get(key));
            let hit = result.is_some();
            let resp = match result {
                None => resp.status(404),
                Some(val) => resp.status(307).header(header::LOCATION, val),
            }.body(Body::empty()).unwrap();
            let event = RequestEvent::Get { hit };
            (resp, event)
        };
        let event = Event { time, event: InnerEvent::Request { from, req_id, scope, key, ua, inner: event } };
        log_sender.send(serde_json::to_string(&event).unwrap()).unwrap();
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE_CONFIG: &str = r#"
rust    www.rust-lang.org/

trpl    doc.rust-lang.org/stable/book/
trpl-cn kaisery.github.io/trpl-zh-cn/
"#;

    #[test]
    fn happypath() {
        let map = init_map(EXAMPLE_CONFIG).unwrap();
        assert_eq!(
            map.get("rust").unwrap().as_str(),
            "https://www.rust-lang.org/"
        );
        assert_eq!(
            map.get("trpl-cn").unwrap().as_str(),
            "https://kaisery.github.io/trpl-zh-cn/"
        );
    }

    #[test]
    fn config_redundant_value() {
        let config = "key val redundance\nkey val";
        matches!(init_map(config), None);
    }

    #[test]
    fn config_lack_value() {
        let config = "key val\nkey";
        matches!(init_map(config), None);
    }
}
