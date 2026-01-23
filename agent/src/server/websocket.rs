use crate::config::Settings;
use crate::protocol::{AgentMessage, ClientMessage};
use crate::server::MessageHandler;
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{
        handshake::server::{Request, Response},
        Message,
    },
};

/// WebSocket server for handling SDK connections
pub struct WebSocketServer {
    settings: Arc<Settings>,
    handler: Arc<MessageHandler>,
    shutdown_tx: broadcast::Sender<()>,
}

impl WebSocketServer {
    pub fn new(settings: Arc<Settings>, handler: Arc<MessageHandler>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            settings,
            handler,
            shutdown_tx,
        }
    }

    /// Start the WebSocket server
    pub async fn start(&self) -> Result<()> {
        let addr = format!("{}:{}", self.settings.server.host, self.settings.server.port);
        let listener = TcpListener::bind(&addr).await?;

        log::info!("WebSocket server listening on {}", addr);

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            let settings = self.settings.clone();
                            let handler = self.handler.clone();
                            let mut shutdown = self.shutdown_tx.subscribe();

                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, addr, settings, handler, &mut shutdown).await {
                                    log::error!("Connection error from {}: {}", addr, e);
                                }
                            });
                        }
                        Err(e) => {
                            log::error!("Accept error: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    log::info!("WebSocket server shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Shutdown the server
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    settings: Arc<Settings>,
    handler: Arc<MessageHandler>,
    shutdown: &mut broadcast::Receiver<()>,
) -> Result<()> {
    log::info!("New connection from {}", addr);

    // Track origin for validation
    let mut client_origin: Option<String> = None;

    // Accept WebSocket connection with header validation
    let ws_stream = accept_hdr_async(stream, |req: &Request, response: Response| {
        // Extract and validate Origin header
        if let Some(origin) = req.headers().get("origin") {
            if let Ok(origin_str) = origin.to_str() {
                client_origin = Some(origin_str.to_string());

                if !settings.is_origin_allowed(origin_str) {
                    log::warn!("Rejected connection from origin: {}", origin_str);
                    return Err(Response::builder()
                        .status(403)
                        .body(None)
                        .unwrap());
                }
            }
        }

        // Check auth token if required
        if settings.security.auth_token.is_some() {
            let has_valid_token = req.uri().query()
                .and_then(|q| {
                    q.split('&')
                        .find(|p| p.starts_with("token="))
                        .map(|p| &p[6..])
                })
                .map(|token| settings.check_auth(Some(token)))
                .unwrap_or(false);

            if !has_valid_token {
                log::warn!("Rejected connection: invalid or missing auth token");
                return Err(Response::builder()
                    .status(401)
                    .body(None)
                    .unwrap());
            }
        }

        Ok(response)
    })
    .await?;

    let origin = client_origin.unwrap_or_else(|| "unknown".to_string());
    log::info!("WebSocket connection established from {} (origin: {})", addr, origin);

    let (mut write, mut read) = ws_stream.split();

    // Send welcome message
    let welcome = AgentMessage::Welcome {
        version: env!("CARGO_PKG_VERSION").to_string(),
    };
    let welcome_json = serde_json::to_string(&welcome)?;
    write.send(Message::Text(welcome_json)).await?;

    // Message handling loop
    loop {
        tokio::select! {
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                log::debug!("Received: {:?}", client_msg);

                                let response = handler.handle_message(client_msg).await;
                                let response_json = serde_json::to_string(&response)?;
                                write.send(Message::Text(response_json)).await?;
                            }
                            Err(e) => {
                                log::warn!("Invalid message: {}", e);
                                let error = AgentMessage::error(
                                    "unknown".to_string(),
                                    "INVALID_REQUEST",
                                    &format!("Invalid message format: {}", e),
                                );
                                let error_json = serde_json::to_string(&error)?;
                                write.send(Message::Text(error_json)).await?;
                            }
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        // Handle binary messages (chunked upload data)
                        log::debug!("Received binary data: {} bytes", data.len());
                        // Binary handling could be added for more efficient chunk transfer
                    }
                    Some(Ok(Message::Ping(data))) => {
                        write.send(Message::Pong(data)).await?;
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Ignore pong
                    }
                    Some(Ok(Message::Close(_))) => {
                        log::info!("Client {} disconnected", addr);
                        break;
                    }
                    Some(Ok(Message::Frame(_))) => {
                        // Raw frames, usually not handled directly
                    }
                    Some(Err(e)) => {
                        log::error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        log::info!("Client {} connection closed", addr);
                        break;
                    }
                }
            }
            _ = shutdown.recv() => {
                log::info!("Closing connection to {} due to shutdown", addr);
                let _ = write.send(Message::Close(None)).await;
                break;
            }
        }
    }

    Ok(())
}
