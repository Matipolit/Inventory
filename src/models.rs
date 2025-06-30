use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Category {
    pub id: i32,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Item {
    pub id: i32,
    pub name: String,
    pub quantity: i32,
    pub restock_threshold: i32,
    #[sqlx(flatten)]
    pub category: Option<Category>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize)]
pub struct CategoryWithItems {
    pub id: i32,
    pub name: String,
    pub color: String,
    pub text_color: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Serialize)]
pub struct GroupedItems {
    pub categorized: Vec<CategoryWithItems>,
    pub uncategorized: Vec<Item>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCategoryPayload {
    pub name: String,
    pub color: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateItemPayload {
    pub name: String,
    pub quantity: i32,
    pub restock_threshold: Option<i32>,
    pub category_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateItemPayload {
    pub name: Option<String>,
    pub quantity: Option<i32>,
    pub restock_threshold: Option<i32>,
    pub category_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct PurchaseItemPayload {
    pub quantity: i32,
}

// For notifications
#[derive(Debug, Serialize, Clone)]
pub struct Notification {
    pub item_name: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Account {
    pub id: i32,
    pub name: String,
    pub email: String,
    pub password: String,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct CreateAccountPayload {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct LoginPayload {
    pub email: String,
    pub password: String,
}
