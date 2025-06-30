use crate::{
    db::{self as db_queries},
    errors::AppError,
    models::{CreateItemPayload, Item, Notification, PurchaseItemPayload, UpdateItemPayload},
};
use axum::{
    Json,
    extract::{Json as AxumJson, Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use axum_extra::extract::CookieJar;
use sqlx::PgPool;

use crate::AppState;
use std::sync::Arc;

// Helper to check and prepare notifications for API
async fn get_api_notifications(pool: &PgPool, user_id: i32) -> Vec<Notification> {
    match db_queries::get_items_to_restock(pool, user_id).await {
        Ok(items_to_restock) => items_to_restock
            .into_iter()
            .map(|item| Notification {
                item_name: item.name.clone(),
                message: format!(
                    "Item '{}' needs restocking. Current: {}, Threshold: {}.",
                    item.name, item.quantity, item.restock_threshold
                ),
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to get items to restock for API: {:?}", e);
            vec![]
        }
    }
}

pub async fn list_items_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let items = db_queries::get_all_items(&app_state.db_pool, user_id).await?;
    Ok(Json(items))
}

pub async fn get_item_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let item = db_queries::get_item_by_id(&app_state.db_pool, user_id, item_id)
        .await?
        .ok_or(AppError::ItemNotFound)?;
    Ok(Json(item))
}

pub async fn create_item_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
    AxumJson(payload): AxumJson<CreateItemPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let item = db_queries::create_item(&app_state.db_pool, user_id, payload).await?;
    let notifications = get_api_notifications(&app_state.db_pool, user_id).await;
    Ok((StatusCode::CREATED, Json(item)))
}

pub async fn update_item_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
    AxumJson(payload): AxumJson<UpdateItemPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let item = db_queries::update_item(&app_state.db_pool, user_id, item_id, payload)
        .await?
        .ok_or(AppError::ItemNotFound)?;
    let notifications = get_api_notifications(&app_state.db_pool, user_id).await;
    Ok(Json(item))
}

pub async fn use_item_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let item = db_queries::use_item(&app_state.db_pool, user_id, item_id).await?;
    let notifications = get_api_notifications(&app_state.db_pool, user_id).await;
    Ok(Json(item))
}

pub async fn purchase_item_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
    AxumJson(payload): AxumJson<PurchaseItemPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let item = db_queries::purchase_item(&app_state.db_pool, user_id, item_id, payload)
        .await?
        .ok_or(AppError::ItemNotFound)?;
    let notifications = get_api_notifications(&app_state.db_pool, user_id).await;
    Ok(Json(item))
}

pub async fn delete_item_api(
    State(app_state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let affected_rows = db_queries::delete_item(&app_state.db_pool, user_id, item_id).await?;
    if affected_rows == 0 {
        return Err(AppError::ItemNotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_notifications_api(
    State(app_state): State<Arc<AppState>>,

    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or(AppError::BadRequest("Authentication required".into()))?;
    let notifications = get_api_notifications(&app_state.db_pool, user_id).await;
    Ok(Json(notifications))
}
