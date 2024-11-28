
mod room;
mod wshandler;
mod client;
mod host;

use actix_cors::Cors;
use actix_identity::IdentityMiddleware;
use actix_session::{config::PersistentSession, storage::CookieSessionStore, SessionMiddleware};
use actix_web::{
    cookie::{time::Duration, Key, SameSite}, http, middleware, web::{self, ServiceConfig}
};
use host::AuthUser;
use room::BingoServer;
use shuttle_actix_web::ShuttleActixWeb;
use shuttle_runtime::SecretStore;
use sqlx::PgPool;
use sqlx::types::Uuid;
use tokio::spawn;
use crate::host::{host_room,start};
use crate::room::RoomCreds;
use crate::client::join;

const FIVE_MINUTES: Duration = Duration::minutes(5);

async fn load_accounts(pool: &sqlx::PgPool, secrets: &SecretStore) {

    let mut count = 0;
    loop
    {
        let lookup = format!("USER_{}_ID", count);
        let id = secrets.get(&lookup);
        let username = secrets.get(&format!("USER_{}_USERNAME", count));
        let password_hash = secrets.get(&format!("USER_{}_TOKEN", count));

        if id.is_none() || username.is_none() || password_hash.is_none() {
            break;
        }
        let username = username.unwrap();
        let id = id.unwrap().parse::<Uuid>();
        if id.is_err(){
            log::error!("Failed to parse UUID for user {}", username);
            count += 1;
            continue;
        }

        let result = sqlx::query("INSERT INTO users (id, username, token) VALUES ($1, $2, $3)")
            .bind(id.unwrap())
            .bind(username.clone())
            .bind(password_hash.unwrap())
            .execute(pool)
            .await;

        match result {
            Ok(_) => log::info!("Added user {} to database", username),
            Err(e) => log::error!("Failed to add user {} to database: {}", username, e),
        }
        count += 1;
    }
}

#[shuttle_runtime::main]
async fn main(
    #[shuttle_shared_db::Postgres] pool: PgPool,
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> ShuttleActixWeb<impl FnOnce(&mut ServiceConfig) + Send + Clone + 'static> {

    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("Failed to run migrations");

        load_accounts(&pool, &secrets).await;


    let rows: Vec<RoomCreds> = sqlx::query_as("SELECT * FROM rooms")
        .fetch_all(&pool)
        .await
        .map_err(|e| shuttle_runtime::Error::from(anyhow::Error::new(e)))?;

    for row in rows {
        println!("{:?}", row);
    }


    let users: Vec<AuthUser> = sqlx::query_as("SELECT * FROM users")
        .fetch_all(&pool)
        .await
        .map_err(|e| shuttle_runtime::Error::from(anyhow::Error::new(e)))?;

    for user in users {
        println!("{:?}", user);
    }

    let secret_key = Key::generate();

    let (mut server, server_tx) = BingoServer::new(pool.clone());
    server.populate_rooms().await;
    let _server = spawn(server.run());

    let config = move |cfg: &mut ServiceConfig| {
        cfg.service(
            web::scope("")
                .app_data(web::Data::new(server_tx.clone()))
                .app_data(web::Data::new(pool.clone()))
                .service(host_room)
                .service(start)
                .service(join)
                .wrap(IdentityMiddleware::default())
                .wrap(
                    SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                        .cookie_name("JSESSIONID".to_owned())
                        .cookie_secure(true)
                        .cookie_http_only(true)
                        .cookie_same_site(SameSite::None)
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
