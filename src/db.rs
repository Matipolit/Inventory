use crate::{
    handlers::web_handlers::get_text_color_for_bg,
    models::{
        Account, Category, CategoryWithItems, CreateCategoryPayload, CreateItemPayload,
        GroupedItems, Item, PurchaseItemPayload, UpdateItemPayload,
    },
};
use sqlx::{Error as SqlxError, PgPool, postgres::PgPoolOptions, prelude::FromRow};
use std::{collections::HashMap, env};

pub type DBResult<T, E = SqlxError> = Result<T, E>;

pub async fn create_pool() -> Result<PgPool, SqlxError> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
}

// Helper struct for SQLx mapping when category might be NULL
#[derive(FromRow, Debug)]
struct FlatItemRow {
    id: i32,
    name: String,
    quantity: i32,
    restock_threshold: i32,
    created_at: time::OffsetDateTime,
    updated_at: time::OffsetDateTime,
    category_id: Option<i32>,
    category_name: Option<String>,
    category_color: Option<String>,
}

/// Fetches all items for a user and groups them by category.
/// Uncategorized items are returned in a separate list.
pub async fn get_items_grouped_by_category(pool: &PgPool, user_id: i32) -> DBResult<GroupedItems> {
    // The query fetches all items, joining category data if it exists.
    // IMPORTANT: We ORDER BY category_name to ensure items of the same
    // category are processed sequentially, which is efficient for grouping.
    let rows = sqlx::query_as!(
        FlatItemRow,
        r#"
        SELECT
            i.id,
            i.name,
            i.quantity,
            i.restock_threshold,
            i.created_at,
            i.updated_at,
            c.id AS "category_id: Option<i32>",
            c.name AS "category_name: Option<String>",
            c.color AS "category_color: Option<String>"
        FROM items i
        LEFT JOIN categories c ON i.category_id = c.id AND i.user_id = c.user_id
        WHERE i.user_id = $1
        ORDER BY c.name, i.name
        "#,
        user_id
    )
    .fetch_all(pool)
    .await?;

    let mut categorized_map: HashMap<i32, CategoryWithItems> = HashMap::new();
    let mut uncategorized_items: Vec<Item> = Vec::new();

    for row in rows {
        // Determine if there is a category for this row
        let category_data = if let (Some(id), Some(name), Some(color)) =
            (row.category_id, &row.category_name, &row.category_color)
        {
            Some(Category {
                id,
                name: name.clone(),
                color: color.clone(),
            })
        } else {
            None
        };

        // Create the full Item struct
        let item = Item {
            id: row.id,
            name: row.name,
            quantity: row.quantity,
            restock_threshold: row.restock_threshold,
            category: category_data,
            created_at: row.created_at,
            updated_at: row.updated_at,
        };

        // Add the item to the correct group
        if let Some(cat_id) = row.category_id {
            // Use the HashMap's entry API for efficient insertion/update
            let text_color = get_text_color_for_bg(&row.category_color.clone().unwrap());
            let category_group = categorized_map.entry(cat_id).or_insert_with(|| {
                CategoryWithItems {
                    id: cat_id,
                    name: row.category_name.unwrap(), // Safe due to check
                    color: row.category_color.unwrap(), // Safe due to check
                    text_color,
                    items: Vec::new(),
                }
            });
            category_group.items.push(item);
        } else {
            uncategorized_items.push(item);
        }
    }

    // Convert the map's values into the final vector
    let categorized_items = categorized_map.into_values().collect();

    Ok(GroupedItems {
        categorized: categorized_items,
        uncategorized: uncategorized_items,
    })
}

impl From<FlatItemRow> for Item {
    fn from(row: FlatItemRow) -> Self {
        let category = if let (Some(id), Some(name), Some(color)) =
            (row.category_id, row.category_name, row.category_color)
        {
            Some(Category { id, name, color })
        } else {
            None
        };
        Item {
            id: row.id,
            name: row.name,
            quantity: row.quantity,
            restock_threshold: row.restock_threshold,
            category,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

pub async fn get_all_items(pool: &PgPool, user_id: i32) -> DBResult<Vec<Item>> {
    let rows = sqlx::query_as!(
        FlatItemRow,
        r#"
        SELECT
            i.id,
            i.name,
            i.quantity,
            i.restock_threshold,
            i.created_at,
            i.updated_at,
            c.id AS "category_id: Option<i32>",
            c.name AS "category_name: Option<String>",
            c.color AS "category_color: Option<String>"
        FROM items i
        LEFT JOIN categories c ON c.id = i.category_id AND c.user_id = i.user_id
        WHERE i.user_id = $1
        ORDER BY i.name
        "#,
        user_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Item::from).collect())
}

pub async fn get_item_by_id(pool: &PgPool, user_id: i32, item_id: i32) -> DBResult<Option<Item>> {
    let row = sqlx::query_as!(
        FlatItemRow,
        r#"
        SELECT
            i.id,
            i.name,
            i.quantity,
            i.restock_threshold,
            i.created_at,
            i.updated_at,
            c.id AS "category_id: Option<i32>",
            c.name AS "category_name: Option<String>",
            c.color AS "category_color: Option<String>"
        FROM items i
        LEFT JOIN categories c ON c.id = i.category_id AND c.user_id = i.user_id
        WHERE i.user_id = $1 AND i.id = $2
        "#,
        user_id,
        item_id
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Item::from))
}

pub async fn create_item(
    pool: &PgPool,
    user_id: i32,
    payload: CreateItemPayload,
) -> DBResult<Item> {
    let threshold = payload.restock_threshold.unwrap_or(1);

    // Insert the item
    let inserted_item_id: i32 = sqlx::query_scalar!(
        "INSERT INTO items (user_id, name, quantity, restock_threshold, category_id)
         VALUES ($1, $2, $3, $4, $5)
         RETURNING id",
        user_id,
        payload.name,
        payload.quantity,
        threshold,
        payload.category_id // This can be Option<i32>
    )
    .fetch_one(pool)
    .await?;

    // Fetch the newly created item with its category details
    // This ensures the returned Item struct is fully populated.
    get_item_by_id(pool, user_id, inserted_item_id)
        .await
        .and_then(|opt_item| opt_item.ok_or_else(|| SqlxError::RowNotFound)) // Convert Option<Item> to Result<Item, Error>
}

pub async fn update_item(
    pool: &PgPool,
    user_id: i32,
    item_id: i32,
    payload: UpdateItemPayload,
) -> DBResult<Option<Item>> {
    // Fetch current item to know its existing values
    let current_item_row = sqlx::query!(
        "SELECT name, quantity, restock_threshold, category_id FROM items WHERE user_id = $1 AND id = $2",
        user_id,
        item_id
    )
    .fetch_optional(pool)
    .await?;

    if current_item_row.is_none() {
        return Ok(None);
    }
    let current_item_data = current_item_row.unwrap();

    let name = payload.name.unwrap_or(current_item_data.name);
    let quantity = payload.quantity.unwrap_or(current_item_data.quantity);
    let restock_threshold = payload
        .restock_threshold
        .unwrap_or(current_item_data.restock_threshold);

    tracing::info!("Updating item with ID {} for user {}", item_id, user_id);
    tracing::info!(
        "New item details: name={}, quantity={}, restock_threshold={}, category_id={:?}",
        name,
        quantity,
        restock_threshold,
        payload.category_id
    );

    let updated_rows = sqlx::query!(
        "UPDATE items
         SET name = $1, quantity = $2, restock_threshold = $3, category_id = $4, updated_at = NOW()
         WHERE user_id = $5 AND id = $6",
        name,
        quantity,
        restock_threshold,
        payload.category_id, // Use the determined category_id
        user_id,
        item_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    if updated_rows > 0 {
        // Fetch and return the updated item with category details
        get_item_by_id(pool, user_id, item_id).await
    } else {
        Ok(None) // Or an error if an update was expected but didn't happen
    }
}

pub async fn use_item(pool: &PgPool, user_id: i32, item_id: i32) -> DBResult<Option<Item>> {
    // First, get the current quantity to ensure we don't go below 0
    let current_quantity_opt: Option<i32> = sqlx::query_scalar!(
        "SELECT quantity FROM items WHERE user_id = $1 AND id = $2",
        user_id,
        item_id
    )
    .fetch_optional(pool)
    .await?;

    if current_quantity_opt.is_none() {
        return Ok(None); // Item not found
    }
    let current_quantity = current_quantity_opt.unwrap();
    if current_quantity == 0 {
        // Already at 0, no change, just return the item
        return get_item_by_id(pool, user_id, item_id).await;
    }
    let new_quantity = current_quantity - 1;

    let affected_rows = sqlx::query!(
        "UPDATE items SET quantity = $1, updated_at = NOW() WHERE user_id = $2 AND id = $3",
        new_quantity,
        user_id,
        item_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    if affected_rows > 0 {
        get_item_by_id(pool, user_id, item_id).await
    } else {
        // This case should ideally not be reached if the item was found initially
        // but could happen in a race condition if the item is deleted between the select and update.
        Ok(None)
    }
}

pub async fn purchase_item(
    pool: &PgPool,
    user_id: i32,
    item_id: i32,
    payload: PurchaseItemPayload,
) -> DBResult<Option<Item>> {
    if payload.quantity <= 0 {
        // Or return an error like AppError::BadRequest
        return get_item_by_id(pool, user_id, item_id).await; // No change
    }

    let affected_rows = sqlx::query!(
        "UPDATE items SET quantity = quantity + $1, updated_at = NOW() WHERE user_id = $2 AND id = $3",
        payload.quantity, // Use the payload quantity directly
        user_id,
        item_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    if affected_rows > 0 {
        get_item_by_id(pool, user_id, item_id).await
    } else {
        Ok(None) // Item not found or no rows updated
    }
}

pub async fn delete_item(pool: &PgPool, user_id: i32, item_id: i32) -> DBResult<u64> {
    sqlx::query!(
        "DELETE FROM items WHERE user_id = $1 AND id = $2",
        user_id,
        item_id
    )
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}

// For checking items that need restocking
pub async fn get_items_to_restock(pool: &PgPool, user_id: i32) -> DBResult<Vec<Item>> {
    let rows = sqlx::query_as!(
        FlatItemRow,
        r#"
        SELECT
            i.id,
            i.name,
            i.quantity,
            i.restock_threshold,
            i.created_at,
            i.updated_at,
            c.id AS "category_id: Option<i32>",
            c.name AS "category_name: Option<String>",
            c.color AS "category_color: Option<String>"
        FROM items i
        LEFT JOIN categories c ON c.id = i.category_id AND c.user_id = i.user_id
        WHERE i.user_id = $1 AND i.quantity < i.restock_threshold
        ORDER BY i.name
        "#,
        user_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(Item::from).collect())
}

//
// Account management
//

/// Create a new account
pub async fn create_account(
    pool: &PgPool,
    name: &str,
    email: &str,
    hashed_password: &str,
) -> DBResult<Account> {
    sqlx::query_as!(
        Account,
        "INSERT INTO users (name, email, password) VALUES ($1, $2, $3)
         RETURNING id, name, email, password, created_at, updated_at",
        name,
        email,
        hashed_password
    )
    .fetch_one(pool)
    .await
}

/// Fetch an account by email (for login)
pub async fn get_account_by_email(pool: &PgPool, email: &str) -> DBResult<Option<Account>> {
    sqlx::query_as!(
        Account,
        "SELECT id, name, email, password, created_at, updated_at FROM users WHERE email = $1",
        email
    )
    .fetch_optional(pool)
    .await
}

pub async fn get_user_by_id(pool: &PgPool, id: i32) -> DBResult<Option<Account>> {
    sqlx::query_as!(
        Account,
        "SELECT id, name, email, password, created_at, updated_at FROM users WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await
}

// --- Category DB Functions ---
pub async fn create_category(
    pool: &PgPool,
    user_id: i32,
    payload: CreateCategoryPayload,
) -> DBResult<Category> {
    sqlx::query_as!(
        Category,
        "INSERT INTO categories (user_id, name, color) VALUES ($1, $2, $3)
         RETURNING id, name, color", // user_id is not part of Category struct here
        user_id,
        payload.name,
        payload.color
    )
    .fetch_one(pool)
    .await
}

pub async fn get_all_categories(pool: &PgPool, user_id: i32) -> DBResult<Vec<Category>> {
    sqlx::query_as!(
        Category,
        "SELECT id, name, color FROM categories WHERE user_id = $1 ORDER BY name",
        user_id
    )
    .fetch_all(pool)
    .await
}

pub async fn get_category_by_id(
    pool: &PgPool,
    user_id: i32,
    category_id: i32,
) -> DBResult<Option<Category>> {
    sqlx::query_as!(
        Category,
        "SELECT id, name, color FROM categories WHERE user_id = $1 AND id = $2",
        user_id,
        category_id
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_category(
    pool: &PgPool,
    user_id: i32,
    category_id: i32,
    name: Option<String>,
    color: Option<String>,
) -> DBResult<Option<Category>> {
    let current_category = get_category_by_id(pool, user_id, category_id).await?;
    if current_category.is_none() {
        return Ok(None);
    }
    let current_category = current_category.unwrap();

    let name_to_set = name.unwrap_or(current_category.name);
    let color_to_set = color.unwrap_or(current_category.color);

    let affected_rows = sqlx::query!(
        "UPDATE categories SET name = $1, color = $2 WHERE user_id = $3 AND id = $4",
        name_to_set,
        color_to_set,
        user_id,
        category_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    if affected_rows > 0 {
        get_category_by_id(pool, user_id, category_id).await
    } else {
        Ok(None)
    }
}

pub async fn delete_category(pool: &PgPool, user_id: i32, category_id: i32) -> DBResult<u64> {
    // Consider what happens to items in this category based on your ON DELETE constraint.
    // If it's SET NULL, items.category_id will become NULL.
    // If it's CASCADE, items will be deleted.
    // If it's RESTRICT, this will fail if items exist in the category.
    sqlx::query!(
        "DELETE FROM categories WHERE user_id = $1 AND id = $2",
        user_id,
        category_id
    )
    .execute(pool)
    .await
    .map(|r| r.rows_affected())
}
