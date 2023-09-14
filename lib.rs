use std::{collections::HashMap, io::{self, Write}, fs, path::PathBuf, net::SocketAddr, sync::Arc};
use tokio::{spawn, sync::mpsc::{unbounded_channel, UnboundedSender}};
use serde::Serialize;
use warp::{http::{Response, header}, hyper::Body};

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
pub enum InnerEvent {
    Init {
        ver: String,
        state: State,
        req_id_header: &'static str,
    },
    Request {
        from: Vec<String>,
        req_id: Option<String>,
        ua: Option<String>,
        scope: String,
        key: String,
        inner: RequestEvent,
    }
}

#[derive(Serialize, Debug)]
pub struct Event {
    time: u64,
    event: InnerEvent,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "data")]
pub enum UpdateResult {
    Succeed {
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

pub type LogSender = UnboundedSender<Event>;

fn log_thread<W: Write + 'static + Send>(mut writer: W) -> LogSender {
    let (tx, mut rx) = unbounded_channel::<Event>();
    spawn(async move {
        while let Some(event) = rx.recv().await {
            serde_json::to_writer(&mut writer, &event).unwrap();
            writer.write_all(b"\n").unwrap();
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

pub async fn init(
    input: String,
    log_path: Option<PathBuf>,
    req_id_header: &'static str,
    allow_update: bool,
) -> anyhow::Result<(StateRef, LogSender)> {
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
    log_sender.send(Event { time, event: InnerEvent::Init {
        ver: env!("CARGO_PKG_VERSION").to_owned(),
        state: state.clone(),
        req_id_header,
    } })?;
    Ok((Arc::new(state), log_sender))
}

pub async fn handle(
    scope: String,
    key: String,
    ip: Option<SocketAddr>,
    xff: Option<String>,
    ua: Option<String>,
    req_id: Option<String>,
    state_ref: StateRef,
    log_sender: LogSender,
) -> Response<Body> {
    let time = now();
    let resp = Response::builder();
    let mut from = Vec::new();
    if let Some(xff) = xff {
        for _ip in xff.split(',') {
            from.push(_ip.to_owned())
        }
    };
    if let Some(ip) = ip {
        from.push(ip.to_string())
    };
    let (resp, event) = {
        let scope_ref = state_ref.scopes.get(&scope);
        let result = scope_ref.as_deref().and_then(|scope_ref| scope_ref.map.get(&key));
        let hit = result.is_some();
        let resp = match result {
            None => resp.status(404),
            Some(val) => resp.status(307).header(header::LOCATION, val),
        }.body(Body::empty()).unwrap();
        let event = RequestEvent::Get { hit };
        (resp, event)
    };
    log_sender.send(Event { time, event: InnerEvent::Request { from, req_id, scope, key, ua, inner: event } }).unwrap();
    resp
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
