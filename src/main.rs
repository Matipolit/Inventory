use axum::body::{Body, HttpBody};
use axum::extract::State;
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::{delete, get, post, put};
use axum::{Router, serve};
use axum_extra::extract::cookie::CookieJar;
use dotenvy::dotenv;
use sqlx::PgPool;
use std::{env, net::SocketAddr, sync::Arc};
use tera::Tera;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

// enable cookies for session management
use tower::ServiceBuilder;

mod db;
mod errors;
mod handlers;
mod models;

use handlers::{api_handlers, web_handlers};

#[derive(Clone)]
pub struct AppState {
    pub tera: Arc<Tera>,
    pub db_pool: PgPool,
    pub base_path: String,
}

// Auth guard for web routes
async fn auth(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let path = req.uri().path();
    let base_path = &state.base_path;
    let login_path = format!("{}/web/login", base_path);
    let signup_path = format!("{}/web/signup", base_path);
    let static_path = format!("{}/static", base_path);

    // allow access to login and signup without auth
    if path == "/"
        || path.starts_with(&login_path)
        || path.starts_with(&signup_path)
        || path.starts_with(&static_path)
    {
        next.run(req).await
    } else {
        let is_auth = req
            .headers()
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .map_or(false, |s| s.contains("session="));
        if is_auth {
            next.run(req).await
        } else {
            Redirect::to(&login_path).into_response()
        }
    }
}

async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "household_inventory=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let tera = Tera::new("templates/**/*")?;
    let db_pool = db::create_pool().await?;

    let run_on_subpath =
        env::var("RUN_ON_SUBPATH").unwrap_or_else(|_| "false".to_string()) == "true";
    let base_path = if run_on_subpath {
        "/inventory".to_string()
    } else {
        "".to_string()
    };

    let shared_state = Arc::new(AppState {
        tera: Arc::new(tera),
        db_pool,
        base_path,
    });

    let static_service = ServeDir::new("static");

    let api_router = Router::new()
        .route(
            "/items",
            get(api_handlers::list_items_api).post(api_handlers::create_item_api),
        )
        .route(
            "/items/{id}",
            get(api_handlers::get_item_api)
                .put(api_handlers::update_item_api)
                .delete(api_handlers::delete_item_api),
        )
        .route("/notifications", get(api_handlers::get_notifications_api));

    // protected web routes
    let web = Router::new()
        .route("/", get(web_handlers::root_handler))
        .route("/web", get(web_handlers::root_handler))
        .route(
            "/web/signup",
            get(web_handlers::show_signup_form).post(web_handlers::signup_handler),
        )
        .route(
            "/web/login",
            get(web_handlers::show_login_form).post(web_handlers::login_handler),
        )
        .route("/web/logout", get(web_handlers::logout_handler))
        .route(
            "/web/categories/add",
            get(web_handlers::show_add_category_form).post(web_handlers::add_category_handler),
        )
        .route(
            "/web/items/add",
            get(web_handlers::show_add_item_form).post(web_handlers::add_item_handler),
        )
        .route(
            "/web/items/edit/{id}",
            get(web_handlers::show_edit_item_form).post(web_handlers::edit_item_handler),
        )
        .route(
            "/web/items/delete/{id}",
            post(web_handlers::delete_item_handler),
        )
        .route("/web/items/use/{id}", post(web_handlers::use_item_handler))
        .route(
            "/web/items/purchase/{id}",
            post(web_handlers::purchase_item_handler),
        );

    let app = if env::var("RUN_ON_SUBPATH").unwrap_or_else(|_| "false".to_string()) == "true" {
        Router::new().nest(
            "/inventory",
            Router::new()
                .merge(web)
                .nest("/api", api_router)
                .nest_service("/static", static_service)
                .layer(middleware::from_fn_with_state(shared_state.clone(), auth)),
        )
    } else {
        Router::new()
            .merge(web)
            .nest("/api", api_router)
            .nest_service("/static", static_service)
            .layer(middleware::from_fn_with_state(shared_state.clone(), auth))
    }
    .route("/health", get(health_check))
    .with_state(shared_state)
    .layer(ServiceBuilder::new().layer(TraceLayer::new_for_http()))
    .fallback(|| async { (StatusCode::NOT_FOUND, "Route Not Found") });

    let port: u16 = env::var("APP_PORT")
        .unwrap_or_else(|_| "3000".into())
        .parse()?;
    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let listener = TcpListener::bind(addr).await?;

    println!(
        r#"
        __________               _______ _      ________________ _______
        \__   __( (    /|\     /(  ____ ( (    /\__   __(  ___  (  ____ |\     /|
           ) (  |  \  ( | )   ( | (    \|  \  ( |  ) (  | (   ) | (    )( \   / )
           | |  |   \ | | |   | | (__   |   \ | |  | |  | |   | | (____)|\ (_) /
           | |  | (\ \) ( (   ) |  __)  | (\ \) |  | |  | |   | |     __) \   /
           | |  | | \   |\ \_/ /| (     | | \   |  | |  | |   | | (\ (     ) (
        ___) (__| )  \  | \   / | (____/| )  \  |  | |  | (___) | ) \ \__  | |
        \_______|/    )_)  \_/  (_______|/    )_)  )_(  (_______|/   \__/  \_/


        "#
    );
    tracing::info!("listening on {}", addr);
    serve(listener, app).await?;

    Ok(())
}
