use axum::body::{Body, HttpBody};
use axum::extract::State;
use axum::http::{Request, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::{delete, get, post, put};
use axum::{Router, serve};
use dotenvy::dotenv;
use sqlx::PgPool;
use std::{env, net::SocketAddr, sync::Arc};
use tera::Tera;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{filter::EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

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

async fn strip_trailing_slash(req: Request<Body>, next: Next) -> impl IntoResponse {
    let uri = req.uri();
    let path = uri.path();

    if path.len() > 1 && path.ends_with('/') {
        // remove the trailing slash and redirect
        let new_path = path.trim_end_matches('/');
        let new_uri_string = if let Some(query) = uri.query() {
            format!("{}?{}", new_path, query)
        } else {
            new_path.to_string()
        };

        // Use a permanent redirect
        return Redirect::permanent(&new_uri_string).into_response();
    }

    // If no trailing slash, just continue
    next.run(req).await
}

// Auth guard for web routes
async fn auth(
    State(state): State<Arc<AppState>>,
    req: axum::http::Request<Body>,
    next: Next,
) -> impl IntoResponse {
    let base_path = &state.base_path;
    let login_path = format!("{}/web/login", base_path);

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

    let api_routes = Router::new()
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

    // Routes that require authentication
    let protected_web_routes = Router::new()
        .route("/", get(web_handlers::root_handler))
        .route("/logout", get(web_handlers::logout_handler))
        .route(
            "/categories/add",
            get(web_handlers::show_add_category_form).post(web_handlers::add_category_handler),
        )
        .route(
            "/items/add",
            get(web_handlers::show_add_item_form).post(web_handlers::add_item_handler),
        )
        .route(
            "/items/edit/{id}",
            get(web_handlers::show_edit_item_form).post(web_handlers::edit_item_handler),
        )
        .route(
            "/items/delete/{id}",
            post(web_handlers::delete_item_handler),
        )
        .route("/items/use/{id}", post(web_handlers::use_item_handler))
        .route(
            "/items/purchase/{id}",
            post(web_handlers::purchase_item_handler),
        )
        .layer(middleware::from_fn_with_state(shared_state.clone(), auth));

    // Public routes that do not require authentication
    let public_web_routes = Router::new()
        .route(
            "/signup",
            get(web_handlers::show_signup_form).post(web_handlers::signup_handler),
        )
        .route(
            "/login",
            get(web_handlers::show_login_form).post(web_handlers::login_handler),
        );

    let web_routes = Router::new()
        .merge(protected_web_routes)
        .merge(public_web_routes);

    let nested = env::var("RUN_ON_SUBPATH").unwrap_or_else(|_| "false".to_string()) == "true";

    let app = if nested {
        Router::new().nest(
            "/inventory",
            Router::new()
                .route("/", get(|| async { Redirect::permanent("/inventory/web") }))
                .nest("/web", web_routes)
                .nest("/api", api_routes)
                .nest_service("/static", static_service),
        )
    } else {
        Router::new()
            .route("/", get(|| async { Redirect::permanent("/web") }))
            .nest("/web", web_routes)
            .nest("/api", api_routes)
            .nest_service("/static", static_service)
    }
    .route("/health", get(health_check))
    .with_state(shared_state)
    .fallback(|| async { (StatusCode::NOT_FOUND, "Route Not Found") })
    .layer(TraceLayer::new_for_http())
    .layer(middleware::from_fn(strip_trailing_slash));

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
    if !nested {
        tracing::info!("listening on {}", addr);
    } else {
        tracing::info!("listening on {}/inventory", addr);
    }
    serve(listener, app).await?;

    Ok(())
}
