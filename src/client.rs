use actix_web::{web, get, Error, HttpRequest, HttpResponse};
use tokio::task::spawn_local;

use crate::{room::{BingoServerHandle, RoomId, USER_CLIENT}, wshandler::{ws_handler, CommandHandler, ErrorMessage}};


pub async fn client_command_handler(
    room: RoomId,
    server: web::Data<BingoServerHandle>,
    msg: String
) {
    server.update(room, msg, USER_CLIENT).await;
}

fn create_command_handler(
    room: RoomId,
    server: web::Data<BingoServerHandle>
) -> CommandHandler {
    Box::new(move |msg| Box::pin({
    let value = server.clone();
    async move { client_command_handler(room, value, msg).await }
    }))
}

#[get("/join/{room}")]
async fn join(
    req: HttpRequest,
    payload: web::Payload,
    path: web::Path<(RoomId,)>,
    server: web::Data<BingoServerHandle>,
) -> Result<HttpResponse, Error> {
    let  (res, mut session, msg_stream ) = actix_ws::handle(&req, payload)?;

    //Validate that the room exists
    if !server.room_exists(path.0).await {
        log::info!("Room not found {}", path.0);
        let _ = session.text(ErrorMessage::new("Room not found".to_owned()).to_string());
        return Err(actix_web::error::ErrorNotFound("Room not found"));
    }

    log::info!("Client is joining room {}", path.0);
    // spawn websocket handler (and don't await it) so that the response is returned immediately
    spawn_local(ws_handler(
        server.clone(),
        path.0,
        USER_CLIENT,
        create_command_handler(path.0, server),
        session,
        msg_stream,
    ));

    Ok(res)
}
