

use argon2::{
    password_hash::{
        PasswordHash, PasswordVerifier
    },
    Argon2
};
use actix_identity::Identity;
use actix_web::{
    error, get, web, Error, HttpMessage as _, HttpRequest, HttpResponse, Responder
};
use base64::prelude::*;
use serde::Deserialize;
use sqlx::types::Uuid;
use tokio::task::spawn_local;

use crate::{room::{BingoServerHandle, ConnId, RoomCreds, RoomId, USER_HOST}, wshandler::{ws_handler, CommandHandler}};


#[derive(sqlx::FromRow, serde::Deserialize, Debug)]
pub struct AuthUser{
    id: Uuid,
    username: String,
    token: String,
}

#[derive(serde::Serialize)]
struct HostResult {
    room_id: RoomId,
    room_token: String,
}

impl Responder for HostResult {
    type Body = actix_web::body::BoxBody;

    fn respond_to(self, _: &HttpRequest) -> actix_web::HttpResponse<Self::Body> {
        let body = serde_json::to_string(&self).unwrap();
        actix_web::HttpResponse::Ok()
            .content_type("application/json")
            .body(body)
    }
}


pub fn verify_password(user_token: &str, hash_token: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(&hash_token)?;
    Ok(Argon2::default().verify_password(user_token.as_bytes(), &parsed_hash).is_ok())
}

#[get("/host")]
async fn host_room(
    req: HttpRequest,
    server: web::Data<BingoServerHandle>,
    datebase: web::Data<sqlx::PgPool>,
) -> actix_web::Result<impl Responder> {

    log::info!("Host request");

    //Check for Authorization header and error if not preset
    if !req.headers().contains_key("Authorization") {
        return Err(error::ErrorUnauthorized("Authorization header is required"));
    }
    let auth = req.headers().get("Authorization").unwrap().to_str().unwrap();

    let decoded = BASE64_STANDARD.decode(auth);
    if decoded.is_err() {
        return Err(error::ErrorUnauthorized("Invalid Authorization header, unexpected encoding"));
    }
    let decoded = decoded.unwrap();

    let auth_token: Result<AuthUser, serde_json::Error> = serde_json::from_slice(&decoded);
    if auth_token.is_err() {
        log::warn!("Failed to parse Authorization header: {}", auth_token.unwrap_err());
        return Err(error::ErrorUnauthorized("Invalid Authorization header, unexpected format"));
    }
    let auth_token = auth_token.unwrap();

    log::info!("Host request from {}", auth_token.username);
    // Check if token is valid in the database and matches the user
    // if not return unauthorized
    //Using auth_token.id look up the user in the database
    let user_data: AuthUser = sqlx::query_as("SELECT * FROM users WHERE id = $1")
        .bind(&auth_token.id)
        .fetch_one(&**datebase)
        .await
        .map_err(|_| error::ErrorUnauthorized("Invalid Authorization header, user not found"))?;

    let result = verify_password(&auth_token.token, &user_data.token);
    match result{
        Ok(is_valid) => {
            if !is_valid {
                return Err(error::ErrorUnauthorized("Invalid Authorization header, token does not match"));
            }
        }
        Err(err) => {
            log::warn!("Failed to verify token: {}", err);
            return Err(error::ErrorUnauthorized("Invalid Authorization header, token verification failed"));
        }
    }

    // attach a verified user identity to the active session
    Identity::login(&req.extensions(), auth_token.username.clone()).unwrap();

    // Find if there is still a valid room of the day
    // if there is no room create a new room
    // return room id

    let room: RoomCreds = server.create_room(auth_token.username.clone()).await;
    log::info!("Created a room with id {} and assigned to {}", room.id, room.host);

    Ok(HostResult{room_id: room.id, room_token: room.token})
}


//Create an implementation of the CommandHandler trait for the client_handler

#[derive(serde::Serialize, serde::Deserialize)]
struct ClientMessage{
    r#type: String,
    client_id: ConnId,
}

pub async fn host_command_handler(
    room: RoomId,
    server: web::Data<BingoServerHandle>,
    msg: String
) {
    match serde_json::from_str::<ClientMessage>(&msg) {
        Ok(message) => {
            server.send(room, message.client_id, msg).await;
        }
        Err(_) => {
            server.update(room, msg, USER_HOST).await;
        }
    }
}

fn create_command_handler(
    room: RoomId,
    server: web::Data<BingoServerHandle>
) -> CommandHandler {
    Box::new(move |msg| Box::pin({
    let value = server.clone();
    async move { host_command_handler(room, value, msg).await }
    }))
}

#[derive(Deserialize)]
struct StartQuery {
    room_token: String,
}


#[get("/start/{room}")]
async fn start(
    req: HttpRequest,
    payload: web::Payload,
    user: Option<Identity>,
    path: web::Path<(RoomId,)>,
    query: web::Query<StartQuery>,
    server: web::Data<BingoServerHandle>,
    _datebase: web::Data<sqlx::PgPool>,
) -> Result<HttpResponse, Error> {
    let user_id = if let Some(user) = user {
        user.id().unwrap()
    } else {
        log::warn!("Loging Denied no active session");
        return Err(error::ErrorUnauthorized("Login required using /host endpoint"));
    };

    let (res, session, msg_stream ) = actix_ws::handle(&req, payload)?;

    //Validate that the room exists, and that the requestor has host privileges
    if !server.has_room_host_privileges(path.0, query.room_token.clone()).await {
        log::info!("User {} does not have host privileges for room {} or the room does not exist", user_id, path.0);
        return Err(actix_web::error::ErrorNotFound("Room not found"));
    }

    log::info!("Welcome {} as host for room {}", user_id, path.0);
    // spawn websocket handler (and don't await it) so that the response is returned immediately
    spawn_local(ws_handler(
        server.clone(),
        path.0,
        USER_HOST,
        create_command_handler(path.0, server),
        session,
        msg_stream,
    ));

    Ok(res)
}