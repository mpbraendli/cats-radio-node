use std::sync::{Arc, Mutex};
use serde::Deserialize;
use askama::Template;
use axum::{
    extract::State,
    routing::get,
    Router,
    response::Html,
    Form,
};
use sqlx::{Connection, SqliteConnection};
use tower_http::services::ServeDir;

mod config;

struct AppState {
    callsign : String,
    db : Mutex<SqliteConnection>
}

type SharedState = Arc<AppState>;

#[tokio::main]
async fn main() -> std::io::Result<()> {

    // simple_logger::

    let mut conn = SqliteConnection::connect("sqlite:cats-radio-node.db").await.unwrap();
    sqlx::migrate!()
        .run(&mut conn)
        .await
        .expect("could not run SQLx migrations");

    let callsign = "HB9EGM-0".to_owned();

    let shared_state = Arc::new(AppState {
        callsign,
        db: Mutex::new(conn)
    });

    let app = Router::new()
        .route("/", get(dashboard))
        .route("/incoming", get(incoming))
        .route("/send", get(send))
        .route("/settings", get(settings))
        .route("/form", get(show_form).post(accept_form))
        .nest_service("/static", ServeDir::new("static"))
        /* requires tracing and tower, e.g.
         *  tower = { version = "0.4", features = ["util", "timeout"] }
         *  tower-http = { version = "0.5.0", features = ["add-extension", "trace"] }
         *  tracing = "0.1"
         *  tracing-subscriber = { version = "0.3", features = ["env-filter"] }
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(|error: BoxError| async move {
                    if error.is::<tower::timeout::error::Elapsed>() {
                        Ok(StatusCode::REQUEST_TIMEOUT)
                    } else {
                        Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Unhandled internal error: {error}"),
                        ))
                    }
                }))
                .timeout(Duration::from_secs(10))
                .layer(TraceLayer::new_for_http())
                .into_inner(),
        )*/
        .with_state(shared_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

#[derive(PartialEq)]
enum ActivePage {
    Dashboard,
    Incoming,
    Send,
    Settings,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    callsign: String,
}

async fn dashboard(State(state): State<SharedState>) -> DashboardTemplate<'static> {
    DashboardTemplate {
        title: "Dashboard",
        callsign: state.callsign.clone(),
        page: ActivePage::Dashboard,
    }
}

#[derive(Template)]
#[template(path = "incoming.html")]
struct IncomingTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    callsign: String,
}

async fn incoming(State(state): State<SharedState>) -> IncomingTemplate<'static> {
    IncomingTemplate {
        title: "Incoming",
        callsign: state.callsign.clone(),
        page: ActivePage::Incoming,
    }
}

#[derive(Template)]
#[template(path = "send.html")]
struct SendTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    callsign: String,
}

async fn send(State(state): State<SharedState>) -> SendTemplate<'static> {
    SendTemplate {
        title: "Send",
        callsign: state.callsign.clone(),
        page: ActivePage::Send,
    }
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate<'a> {
    title: &'a str,
    page: ActivePage,
    callsign: String,
}

async fn settings(State(state): State<SharedState>) -> SettingsTemplate<'static> {
    SettingsTemplate {
        title: "Settings",
        callsign: state.callsign.clone(),
        page: ActivePage::Settings,
    }
}

async fn show_form() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <html>
            <head></head>
            <body>
                <form action="/" method="post">
                    <label for="name">
                        Enter your name:
                        <input type="text" name="name">
                    </label>

                    <label>
                        Enter your email:
                        <input type="text" name="email">
                    </label>

                    <input type="submit" value="Subscribe!">
                </form>
            </body>
        </html>
        "#,
    )
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct Input {
    name: String,
    email: String,
}

async fn accept_form(Form(input): Form<Input>) {
    dbg!(&input);
}
