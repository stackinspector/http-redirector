use std::{collections::HashMap, io::{self, Write}, fs, path::PathBuf, net::SocketAddr, sync::Arc, borrow::Cow, convert::Infallible};
use serde::Serialize;
use dashmap::DashMap;
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
    scopes: DashMap<String, Scope>,
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

struct LogContext {
    // TODO(actor): let Handle not need Context's generics
    writer: Box<dyn Write + 'static + Send>,
}

impl LogContext {
    // use SyncInitContext leads to unconstrained type parameter
    fn init<W: Write + 'static + Send>(writer: W) -> LogContext {
        LogContext { writer: Box::new(writer) }
    }
}

impl actor_core::Context for LogContext {
    type Req = String;
    type Res = ();
    type Err = anyhow::Error;

    fn exec(&mut self, req: Self::Req) -> Result<Self::Res, Self::Err> {
        let mut line = req.into_bytes();
        line.push(b'\n');
        self.writer.write_all(&line)?;
        Ok(())
    }

    fn close(mut self) -> Result<(), Self::Err> {
        Ok(self.writer.flush()?)
    }
}

pub struct LogCloser {
    // similar problem: LogContext should be public if not wrap
    handle: tokio_actor::Handle<LogContext>,
}

impl LogCloser {
    pub async fn wait_close(self) -> Result<(), anyhow::Error> {
        Ok(self.handle.wait_close().await?)
    }
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
    log_sender: tokio_actor::Handle<LogContext>,
    req_id_header: Option<Arc<str>>,
}

impl Context {
    pub async fn init(
        input: String,
        log_path: Option<PathBuf>,
        req_id_header: Option<String>,
        allow_update: bool,
    ) -> anyhow::Result<(Context, LogCloser)> {
        let req_id_header = req_id_header.map(Arc::from);
        let scopes = DashMap::new();
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
        let log_context = match log_path {
            Some(path) => LogContext::init(fs::OpenOptions::new().write(true).create(true).append(true).open(path)?),
            None => LogContext::init(io::stdout()),
        };
        let log_sender = tokio_actor::spawn(log_context);
        let time = now();
        let event = Event { time, event: InnerEvent::Init {
            ver: env!("CARGO_PKG_VERSION"),
            state: &state,
            req_id_header: req_id_header.as_deref(),
        } };
        // TODO should wait for log writed? (actor's default behavior)
        log_sender.request(serde_json::to_string(&event).unwrap()).await?;
        Ok((Context {
            state_ref: Arc::new(state),
            log_sender: log_sender.clone(),
            req_id_header,
        }, LogCloser {
            handle: log_sender
        }))
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
        let (resp, event) = if state_ref.allow_update && (scope == UPDATE_PATH_STR) {
            let scope = key;
            let result = match state_ref.scopes.get(scope) {
                None => UpdateResult::ScopeNotFound,
                Some(scope_state) => {
                    match get(&scope_state.url).await {
                        Err(error) => UpdateResult::GetConfigError(error.to_string()),
                        Ok(config) => {
                            match init_map(config.as_str()) {
                                None => UpdateResult::ParseConfigError,
                                Some(map) => {
                                    // TODO serialize before insert
                                    let new = Scope {
                                        url: scope_state.url.clone(),
                                        map,
                                    };
                                    // TODO will always locked
                                    let old = state_ref.scopes.insert(scope.to_owned(), new.clone()).unwrap();
                                    UpdateResult::Succeed { new, old }
                                },
                            }
                        },
                    }
                },
            };
            let resp_status = match result {
                UpdateResult::Succeed { .. } => 200,
                UpdateResult::ScopeNotFound => 404,
                _ => 500,
            };
            let resp_body = Body::from(serde_json::to_string(&result).unwrap());
            let resp = resp.status(resp_status).header(header::CONTENT_TYPE, "application/json; charset=utf-8").body(resp_body).unwrap();
            let event = RequestEvent::Update { result };
            (resp, event)
        } else {
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
        log_sender.request(serde_json::to_string(&event).unwrap()).await.unwrap(); // ok?
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
