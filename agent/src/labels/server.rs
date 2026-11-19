use super::{
    backend::{platform_backend, PrinterBackend},
    config::{valid_name, LabelSettings},
    jobs::{ApiError, HttpDocuments, Jobs},
};
use anyhow::{ensure, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, net::Ipv4Addr, sync::Arc, time::Duration};
use subtle::ConstantTimeEq;
use tokio::{io::{AsyncRead, AsyncWrite}, net::TcpListener, sync::Semaphore, time::timeout};
use tokio_tungstenite::{
    accept_hdr_async_with_config,
    tungstenite::{handshake::server::{Request, Response}, protocol::WebSocketConfig, Message},
    WebSocketStream,
};

pub const PAIRING_PORT: u16 = 9718;
const PAIRING_HOST: &str = "127.0.0.1:9718";
const PAIRING_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Pairing(BTreeMap<String, String>);

impl Pairing {
    pub fn new(settings: &LabelSettings) -> Self {
        Self(settings.trusted_origins.iter().map(|origin| {
            (origin.clone(), uuid::Uuid::new_v4().simple().to_string())
        }).collect())
    }

    pub fn local_tokens(&self) -> BTreeMap<String, String> {
        self.0.clone()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Auth {
    #[serde(rename = "type")]
    kind: String,
    token: String,
}

async fn authorize<Stream: AsyncRead + AsyncWrite + Unpin>(stream: Stream, pairing: &Pairing)
    -> Result<(WebSocketStream<Stream>, String)> {
    let mut origin = None;
    let config = WebSocketConfig {
        max_message_size: Some(16 * 1024),
        max_frame_size: Some(16 * 1024),
        max_write_buffer_size: 1024 * 1024,
        ..Default::default()
    };
    let upgraded = accept_hdr_async_with_config(stream, |request: &Request, response: Response| {
        let origins = request.headers().get_all("origin");
        let mut values = origins.iter();
        let candidate = values.next().and_then(|value| value.to_str().ok());
        let host_values = request.headers().get_all("host");
        let mut hosts = host_values.iter();
        let host = hosts.next().and_then(|value| value.to_str().ok());
        if request.uri().path_and_query().map(|part| part.as_str()) == Some("/labels")
            && host == Some(PAIRING_HOST) && hosts.next().is_none() && values.next().is_none() {
            if let Some(candidate) = candidate.filter(|candidate| pairing.0.contains_key(*candidate)) {
                origin = Some(candidate.to_owned());
                return Ok(response);
            }
        }
        Err(Response::builder().status(403).body(None).expect("valid HTTP rejection"))
    }, Some(config));
    let mut socket = timeout(PAIRING_TIMEOUT, upgraded).await?
        .map_err(|_| anyhow::anyhow!("WebSocket upgrade rejected"))?;
    let origin = origin.context("Missing trusted origin")?;
    let expected = pairing.0.get(&origin).context("Unknown origin")?;
    let first = timeout(PAIRING_TIMEOUT, socket.next()).await?;
    let Some(Ok(Message::Text(text))) = first else { anyhow::bail!("Pairing is required"); };
    let auth: Auth = serde_json::from_str(&text).map_err(|_| anyhow::anyhow!("Pairing is required"))?;
    ensure!(auth.kind == "auth" && auth.token.as_bytes().ct_eq(expected.as_bytes()).into(),
        "Pairing rejected");
    Ok((socket, origin))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RequestMessage {
    #[serde(rename = "type")]
    kind: String,
    request_id: String,
    #[serde(default)]
    payload: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileLookup {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct JobLookup {
    job_id: String,
}

fn payload<Payload: DeserializeOwned>(value: Value) -> std::result::Result<Payload, ApiError> {
    serde_json::from_value(value).map_err(|_| ApiError::new("INVALID_PAYLOAD", "Invalid operation payload"))
}

async fn dispatch(request: RequestMessage, origin: &str, settings: &LabelSettings,
    jobs: &Jobs, backend: &dyn PrinterBackend) -> std::result::Result<Value, ApiError> {
    if !valid_name(&request.request_id) {
        return Err(ApiError::new("INVALID_REQUEST_ID", "Request ID must be 1–64 ASCII letters, digits, underscores or hyphens"));
    }
    match request.kind.as_str() {
        "agent.status" => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "protocol": 1,
            "platform": std::env::consts::OS,
            "sumatra": backend.status().await,
        })),
        "printers.list" => backend.printers().await.map(|names| json!(names))
            .map_err(|error| ApiError::new("PRINTER_DISCOVERY_FAILED", error.to_string())),
        "profiles.list" => Ok(json!(settings.profiles)),
        "profiles.get" => {
            let lookup: ProfileLookup = payload(request.payload)?;
            settings.profile(&lookup.name).map(|profile| json!(profile))
                .map_err(|error| ApiError::new("INVALID_PROFILE", error.to_string()))
        }
        "jobs.submit" => jobs.submit(origin, &request.request_id, payload(request.payload)?)
            .map(|job| json!(job)),
        "jobs.status" => {
            let lookup: JobLookup = payload(request.payload)?;
            jobs.status(origin, &lookup.job_id).map(|job| json!(job))
        }
        "jobs.cancel" => {
            let lookup: JobLookup = payload(request.payload)?;
            jobs.cancel(origin, &lookup.job_id).map(|job| json!(job))
        }
        _ => Err(ApiError::new("UNKNOWN_OPERATION", "Unsupported label API operation")),
    }
}

async fn reply<Stream: AsyncRead + AsyncWrite + Unpin>(socket: &mut WebSocketStream<Stream>, response: Value)
    -> Result<()> {
    timeout(Duration::from_secs(5), socket.send(Message::Text(response.to_string()))).await??;
    Ok(())
}

async fn connection<Stream: AsyncRead + AsyncWrite + Unpin>(stream: Stream, pairing: &Pairing,
    settings: &LabelSettings, jobs: &Jobs, backend: &dyn PrinterBackend) -> Result<()> {
    let (mut socket, origin) = authorize(stream, pairing).await?;
    reply(&mut socket, json!({ "type": "ready", "protocol": 1 })).await?;
    while let Some(message) = timeout(Duration::from_secs(15 * 60), socket.next()).await? {
        match message? {
            Message::Text(text) => {
                let response = match serde_json::from_str::<RequestMessage>(&text) {
                    Ok(request) => {
                        let id = valid_name(&request.request_id).then(|| request.request_id.clone());
                        match dispatch(request, &origin, settings, jobs, backend).await {
                            Ok(result) => json!({"type": "response", "requestId": id, "ok": true, "result": result}),
                            Err(error) => json!({"type": "response", "requestId": id, "ok": false, "error": error}),
                        }
                    }
                    Err(_) => json!({"type": "response", "requestId": null, "ok": false,
                        "error": {"code": "INVALID_REQUEST", "message": "Invalid label API envelope"}}),
                };
                reply(&mut socket, response).await?;
            }
            Message::Ping(_) => timeout(Duration::from_secs(5), socket.flush()).await??,
            Message::Close(_) => break,
            Message::Binary(_) => anyhow::bail!("Binary requests are not supported"),
            _ => {}
        }
    }
    Ok(())
}

pub async fn serve(settings: Arc<LabelSettings>, pairing: Arc<Pairing>, ready: impl FnOnce()) -> Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, PAIRING_PORT)).await
        .context("Cannot bind 127.0.0.1:9718; quit any existing PrintBridge instance")?;
    let backend = platform_backend();
    let directory = LabelSettings::directory()?;
    let jobs = Jobs::open(&directory, settings.clone(), backend.clone(), Arc::new(HttpDocuments))
        .context("Cannot open local label job history")?;
    let connections = Arc::new(Semaphore::new(16));
    ready();
    loop {
        let (stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() {
            continue;
        }
        let Ok(permit) = connections.clone().try_acquire_owned() else { continue; };
        let (settings, pairing, jobs, backend) = (settings.clone(), pairing.clone(), jobs.clone(), backend.clone());
        tokio::spawn(async move {
            let _permit = permit;
            let _ = connection(stream, &pairing, &settings, &jobs, backend.as_ref()).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::{client_async, tungstenite::client::IntoClientRequest};

    const TRUSTED: &str = "https://stockman.example.com";

    async fn attempt(origin: Option<&str>, endpoint: &str, token: Option<&str>) -> bool {
        let settings = LabelSettings { trusted_origins: vec![TRUSTED.into()], ..Default::default() };
        let pairing = Pairing::new(&settings);
        let (client, server) = tokio::io::duplex(4096);
        let mut request = endpoint.into_client_request().unwrap();
        if let Some(origin) = origin {
            request.headers_mut().insert("origin", origin.parse().unwrap());
        }
        let frame = match token {
            Some("valid") => json!({ "type": "auth", "token": pairing.0[TRUSTED] }),
            Some(wrong) => json!({ "type": "auth", "token": wrong }),
            None => json!({ "type": "printers.list", "requestId": "job" }),
        };
        let (server_result, _) = tokio::join!(authorize(server, &pairing), async {
            if let Ok((mut socket, _)) = client_async(request, client).await {
                socket.send(Message::Text(frame.to_string())).await.unwrap();
            }
        });
        server_result.is_ok()
    }

    #[tokio::test]
    async fn handshake_requires_loopback_host_exact_origin_and_path() {
        let endpoint = "ws://127.0.0.1:9718/labels";
        assert!(attempt(Some(TRUSTED), endpoint, Some("valid")).await);
        assert!(!attempt(None, endpoint, Some("valid")).await);
        assert!(!attempt(Some("null"), endpoint, Some("valid")).await);
        assert!(!attempt(Some("https://else.example.com"), endpoint, Some("valid")).await);
        assert!(!attempt(Some(TRUSTED), "ws://localhost:9718/labels", Some("valid")).await);
        assert!(!attempt(Some(TRUSTED), "ws://127.0.0.1:9718/", Some("valid")).await);
    }

    #[tokio::test]
    async fn trusted_origin_alone_cannot_use_api() {
        let endpoint = "ws://127.0.0.1:9718/labels";
        assert!(!attempt(Some(TRUSTED), endpoint, None).await);
        assert!(!attempt(Some(TRUSTED), endpoint, Some("wrong")).await);
    }
}
