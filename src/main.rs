
mod room;
mod wshandler;
mod client;
mod host;

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_session::{config::PersistentSession, storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::{time::Duration, Key}, http, middleware, web::{self, ServiceConfig}
};
use room::BingoServer;
use shuttle_actix_web::ShuttleActixWeb;
use tokio::spawn;


use crate::host::{host_room,start};


use crate::client::join;


const FIVE_MINUTES: Duration = Duration::minutes(5);







#[shuttle_runtime::main]
async fn main() -> ShuttleActixWeb<impl FnOnce(&mut ServiceConfig) + Send + Clone + 'static> {
    let secret_key = Key::generate();


    let (server, server_tx) = BingoServer::new();
    let server = spawn(server.run());

    let config = move |cfg: &mut ServiceConfig| {
        cfg.service(
            web::scope("")
                .app_data(web::Data::new(server_tx.clone()))
                .service(host_room)
                .service(start)
                .service(join)
                .wrap(IdentityMiddleware::default())
                .wrap(
                    SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                        .cookie_name("JSESSIONID".to_owned())
                        .cookie_secure(false)
                        .cookie_http_only(true)
                        .session_lifecycle(PersistentSession::default().session_ttl(FIVE_MINUTES))
                        .build(),
                )
                .wrap(middleware::NormalizePath::trim())
                .wrap(middleware::Logger::default())
                .wrap(
                    Cors::default()
                        .allowed_origin("http://127.0.0.1:5500") // Replace with your allowed origin
                        .allowed_origin("http://10.0.0.199:5500") // Replace with your allowed origin
                        .allowed_origin("https://web2098.github.io") // Replace with your allowed origin
                        .allowed_methods(vec!["GET", "POST"])
                        .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
                        .allowed_header(http::header::CONTENT_TYPE)
                        .supports_credentials()
                        .max_age(3600),
                ),
        );
    };

    Ok(config.into())
}
