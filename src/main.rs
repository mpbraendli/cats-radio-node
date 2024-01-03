use std::sync::{Arc, Mutex};
use sqlx::{Connection, SqliteConnection};

mod config;
mod ui;

struct AppState {
    conf : config::Config,
    db : Mutex<SqliteConnection>
}

type SharedState = Arc<Mutex<AppState>>;

#[tokio::main]
async fn main() -> std::io::Result<()> {

    // simple_logger::

    let mut conn = SqliteConnection::connect("sqlite:cats-radio-node.db").await.unwrap();
    sqlx::migrate!()
        .run(&mut conn)
        .await
        .expect("could not run SQLx migrations");

    let conf = config::Config::load().expect("Could not load config");

    let shared_state = Arc::new(Mutex::new(AppState {
        conf,
        db: Mutex::new(conn)
    }));

    ui::serve(3000, shared_state).await;
    Ok(())
}

