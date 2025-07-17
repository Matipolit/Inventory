use crate::AppState;
use crate::db::get_all_categories;
use crate::models::{
    CategoryWithItems, CreateCategoryPayload, GroupedItems, Item, PurchaseItemPayload,
};
use crate::{
    db::{self as db_queries},
    errors::AppError,
    models::{
        CreateAccountPayload, CreateItemPayload, LoginPayload, Notification, UpdateItemPayload,
    },
};
use axum::debug_handler;
use axum::extract::Query;
use axum::{
    extract::{Form, Path, State},
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use bcrypt::{DEFAULT_COST, hash, verify};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tera::Context;

pub fn get_text_color_for_bg(hex_color: &str) -> String {
    let hex_color = hex_color.trim_start_matches('#');
    if hex_color.len() != 6 {
        return "#000000".to_string(); // Default to black for invalid colors
    }

    let r = u8::from_str_radix(&hex_color[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&hex_color[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&hex_color[4..6], 16).unwrap_or(0);

    // Formula for perceived brightness
    let brightness = ((r as u32 * 299) + (g as u32 * 587) + (b as u32 * 114)) / 1000;

    if brightness > 150 {
        // Threshold can be adjusted
        "#000000".to_string() // Black text for light backgrounds
    } else {
        "#FFFFFF".to_string() // White text for dark backgrounds
    }
}

// Helper to check and prepare notifications
async fn get_notifications(pool: &PgPool, user_id: i32) -> Vec<Notification> {
    match db_queries::get_items_to_restock(pool, user_id).await {
        Ok(items_to_restock) => items_to_restock
            .into_iter()
            .map(|item| Notification {
                item_name: item.name.clone(),
                message: format!(
                    "Aktualna ilość: {}, próg uzupełnienia: {}. Proszę uzupełnij!",
                    item.quantity, item.restock_threshold
                ),
            })
            .collect(),
        Err(e) => {
            tracing::error!("Failed to get items to restock: {:?}", e);
            vec![] // Return empty on error
        }
    }
}

pub async fn root_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    let group_by_category: bool = jar
        .get("group")
        .and_then(|g| g.value().parse().ok())
        .unwrap_or(true);

    let user = db_queries::get_user_by_id(&state.db_pool, user_id).await?;
    let items = db_queries::get_all_items(&state.db_pool, user_id).await?;
    let categories = get_all_categories(&state.db_pool, user_id).await?;
    let notifications = get_notifications(&state.db_pool, user_id).await;

    let mut context = Context::new();
    context.insert("notifications", &notifications);
    context.insert("user", &user);
    context.insert("group_by_category", &group_by_category);
    context.insert("categories", &categories);
    context.insert("base_path", &state.base_path);
    context.insert("item_amount", &items.len());

    if group_by_category {
        let mut categorized_map: HashMap<i32, CategoryWithItems> = HashMap::new();
        // Use the already fetched categories
        for category in &categories {
            let text_color = get_text_color_for_bg(&category.color);
            categorized_map.insert(
                category.id,
                CategoryWithItems {
                    id: category.id,
                    name: category.name.clone(),
                    color: category.color.clone(),
                    text_color,
                    items: vec![],
                },
            );
        }

        let mut uncategorized_items: Vec<Item> = vec![];

        for item in items {
            if let Some(ref category) = item.category {
                if let Some(cat_with_items) = categorized_map.get_mut(&category.id) {
                    cat_with_items.items.push(item);
                }
            } else {
                uncategorized_items.push(item);
            }
        }

        let mut categorized_items: Vec<CategoryWithItems> = categorized_map.into_values().collect();
        categorized_items.sort_by(|a, b| a.name.cmp(&b.name));

        let grouped_items = GroupedItems {
            categorized: categorized_items,
            uncategorized: uncategorized_items,
        };
        context.insert("grouped_items", &grouped_items);
    } else {
        context.insert("items", &items);
    }

    let rendered = state.tera.render("index.html", &context)?;
    Ok(Html(rendered))
}

pub async fn show_add_item_form(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    let user = db_queries::get_user_by_id(&state.db_pool, user_id).await?;
    let notifications = get_notifications(&state.db_pool, user_id).await;
    let categories = get_all_categories(&state.db_pool, user_id).await?;
    let mut context = Context::new();
    context.insert("notifications", &notifications);
    context.insert("categories", &categories);
    context.insert("base_path", &state.base_path);
    context.insert("user", &user);
    let rendered = state.tera.render("add_item.html", &context)?;
    Ok(Html(rendered))
}

pub async fn add_item_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(payload): Form<CreateItemPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    db_queries::create_item(&state.db_pool, user_id, payload).await?;
    let redirect_url = format!("{}/web", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}

pub async fn add_category_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(payload): Form<CreateCategoryPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    db_queries::create_category(&state.db_pool, user_id, payload).await?;
    let redirect_url = format!("{}/web", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}

pub async fn show_add_category_form(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    let user = db_queries::get_user_by_id(&state.db_pool, user_id).await?;
    let notifications = get_notifications(&state.db_pool, user_id).await;
    let mut context = Context::new();
    context.insert("notifications", &notifications);
    context.insert("base_path", &state.base_path);
    context.insert("user", &user);
    let rendered = state.tera.render("add_category.html", &context)?;
    Ok(Html(rendered))
}

/// GET /signup
pub async fn show_signup_form(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut context = Context::new();
    context.insert("base_path", &state.base_path);
    let rendered = state.tera.render("signup.html", &context)?;
    Ok(Html(rendered))
}

/// POST /signup
pub async fn signup_handler(
    State(state): State<Arc<AppState>>,
    Form(payload): Form<CreateAccountPayload>,
) -> Result<impl IntoResponse, AppError> {
    let hashed_password_string = hash(&payload.password, DEFAULT_COST)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?;
    db_queries::create_account(
        &state.db_pool,
        payload.name.as_str(),
        payload.email.as_str(),
        &hashed_password_string,
    )
    .await?;
    let redirect_url = format!("{}/web/login", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}

/// GET /login
pub async fn show_login_form(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, AppError> {
    let mut context = Context::new();
    context.insert("base_path", &state.base_path);
    let rendered = state.tera.render("login.html", &context)?;
    Ok(Html(rendered))
}

/// POST /login
#[debug_handler]
pub async fn login_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Form(payload): Form<LoginPayload>,
) -> Result<impl IntoResponse, AppError> {
    let acct = db_queries::get_account_by_email(&state.db_pool, &payload.email)
        .await?
        .ok_or(AppError::BadRequest("Nieprawidłowe dane logowania".into()))?;
    if verify(&payload.password, &acct.password)
        .map_err(|e| AppError::InternalServerError(e.to_string()))?
    {
        let session_cookie = Cookie::build(("session", acct.id.to_string()))
            .path("/")
            .http_only(true);
        // .secure(true) // Uncomment if served over HTTPS
        let jar = jar.add(session_cookie);
        let redirect_url = format!("{}/web", &state.base_path);
        Ok((jar, Redirect::to(&redirect_url)))
    } else {
        Err(AppError::BadRequest("Nieprawidłowe dane logowania".into()))
    }
}

/// GET /logout
pub async fn logout_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
) -> Result<(CookieJar, Redirect), AppError> {
    // Remove the cookie by setting its path and making it expire.
    // axum-extra's `remove` method sets Max-Age=0 and clears the value.
    // Ensure the path matches the one used during cookie creation.
    let jar = jar.remove(Cookie::build("session").path("/").build());
    let redirect_url = format!("{}/web/login", &state.base_path);
    Ok((jar, Redirect::to(&redirect_url)))
}

pub async fn show_edit_item_form(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    let user = db_queries::get_user_by_id(&state.db_pool, user_id).await?;
    let item = db_queries::get_item_by_id(&state.db_pool, user_id, item_id)
        .await?
        .ok_or(AppError::ItemNotFound)?;
    let notifications = get_notifications(&state.db_pool, user_id).await;
    let categories = get_all_categories(&state.db_pool, user_id).await?;
    let mut context = Context::new();
    context.insert("item", &item);
    context.insert("notifications", &notifications);
    context.insert("categories", &categories);
    context.insert("selected_category", &item.category.map(|c| c.id));
    context.insert("base_path", &state.base_path);
    context.insert("user", &user);
    let rendered = state.tera.render("edit_item.html", &context)?;
    Ok(Html(rendered))
}

pub async fn edit_item_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
    Form(payload): Form<UpdateItemPayload>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!("UpdateItemPayload: {:?}", payload);
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    db_queries::update_item(&state.db_pool, user_id, item_id, payload).await?;
    let redirect_url = format!("{}/web", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}

pub async fn purchase_item_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
    Form(payload): Form<PurchaseItemPayload>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    db_queries::purchase_item(&state.db_pool, user_id, item_id, payload).await?;
    let redirect_url = format!("{}/web", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}

pub async fn use_item_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    db_queries::use_item(&state.db_pool, user_id, item_id).await?;
    let redirect_url = format!("{}/web", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}

pub async fn delete_item_handler(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    Path(item_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let user_id: i32 = jar
        .get("session")
        .and_then(|c| c.value().parse().ok())
        .ok_or_else(|| AppError::BadRequest("Authentication required".into()))?;

    let affected_rows = db_queries::delete_item(&state.db_pool, user_id, item_id).await?;
    if affected_rows == 0 {
        return Err(AppError::ItemNotFound);
    }
    let redirect_url = format!("{}/web", &state.base_path);
    Ok(Redirect::to(&redirect_url))
}
