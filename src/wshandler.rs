use std::{future::Future, pin::{pin, Pin}, time::{Duration, Instant}};

use actix_web::web;
use actix_ws::AggregatedMessage;
use tokio::{sync::mpsc, time::interval};
use futures_util::future::{select, Either};

use crate::room::{BingoServerHandle, ConnId, RoomId};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);


//Create an interface for command handler that accepts a string message
pub type CommandHandler = Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;


#[derive(Debug, serde::Deserialize)]
pub struct WSMessage{
    r#type: String
}

#[derive(serde::Serialize)]
pub struct IDMessage{
    r#type: String,
    conn_id: ConnId
}

impl IDMessage{
    pub fn new(conn_id: ConnId) -> Self {
        Self{
            r#type: "id".to_string(),
            conn_id
        }
    }
}

#[derive(serde::Serialize)]
pub struct ErrorMessage{
    r#type: String,
    message: String,
}

impl ErrorMessage{
    pub fn new(message: String) -> Self {
        Self{
            r#type: "error".to_string(),
            message
        }
    }

    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

pub async fn ws_handler(
    server: web::Data<BingoServerHandle>,
    room: RoomId,
    user_type: ConnId,
    command_handler: CommandHandler,
    mut session: actix_ws::Session,
    msg_stream: actix_ws::MessageStream)
{
    let mut last_heartbeat = Instant::now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    // unwrap: chat server is not dropped before the HTTP server
    let conn_id = server.connect(room, conn_tx, user_type).await;

    let msg_stream = msg_stream
        .max_frame_size(128 * 1024)
        .aggregate_continuations()
        .max_continuation_size(2 * 1024 * 1024);

    let mut msg_stream = pin!(msg_stream);

    let close_reason = loop {
        let tick = pin!(interval.tick());
        let msg_rx = pin!(conn_rx.recv());
        let stream = pin!(msg_stream.recv());
        let messages = pin!(select(stream, msg_rx));

        match select(messages, tick).await {
            //Client commands
            Either::Left((Either::Left((Some(Ok(msg)), _)), _)) => {
                match msg {
                    AggregatedMessage::Ping(bytes) => {
                        last_heartbeat = Instant::now();
                        // unwrap:
                        session.pong(&bytes).await.unwrap();
                    }

                    AggregatedMessage::Pong(_) => {
                        last_heartbeat = Instant::now();
                    }
                    AggregatedMessage::Close(reason) => break reason,
                    AggregatedMessage::Binary(_bin) => {
                        log::warn!("unexpected binary message");
                    }
                    AggregatedMessage::Text(_text) => {
                        // Check if _text is a request_id message and respond with the appropriate response
                        let message: Result<WSMessage, serde_json::Error> = serde_json::from_str(&_text);
                        if message.is_err() {
                            log::warn!("Invalid message format: {} error {}", _text, message.unwrap_err());
                            continue;
                        }
                        let message = message.unwrap();
                        if message.r#type == "request_id" {
                            let id_message = IDMessage::new(conn_id);
                            let response = serde_json::to_string(&id_message).unwrap();
                            session.text(response).await.unwrap();
                        }
                        else {
                            command_handler(_text.to_string()).await;
                        }

                    }
                }
            }

            // client WebSocket stream error
            Either::Left((Either::Left((Some(Err(_err)), _)), _)) => {
            }

            // client WebSocket stream ended
            Either::Left((Either::Left((None, _)), _)) => break None,

            // room update
            Either::Left((Either::Right((Some(room_update), _)), _)) => {
                session.text(room_update).await.unwrap();
            }

            Either::Left((Either::Right((None, _)), _)) => unreachable!(),

            // heartbeat
            Either::Right((_inst, _)) => {
                // if no heartbeat ping/pong received recently, close the connection
                if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                    break None;
                }

                // send heartbeat ping
                let _ = session.ping(b"").await;
            }
        }
    };

    server.disconnect(room, conn_id, user_type).await;

    // attempt to close connection gracefully
    let _ = session.close(close_reason).await;
}