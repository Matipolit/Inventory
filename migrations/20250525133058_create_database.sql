-- Add migration script here

CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    password VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE categories (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    color VARCHAR(7) NOT NULL,
    user_id INTEGER,
    CONSTRAINT fk_user_categories
        FOREIGN KEY(user_id)
            REFERENCES users(id)
);

CREATE TABLE items (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 0,
    restock_threshold INTEGER NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    user_id INTEGER,
    category_id INTEGER,
    CONSTRAINT fk_user_items
        FOREIGN KEY(user_id)
            REFERENCES users(id),
    CONSTRAINT fk_category_items
        FOREIGN KEY(category_id)
            REFERENCES categories(id)
);

CREATE UNIQUE INDEX idx_items_account_id_name ON items (user_id, name);

-- Optional: Trigger to update updated_at timestamp automatically
CREATE OR REPLACE FUNCTION trigger_set_timestamp()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER set_timestamp
BEFORE UPDATE ON items
FOR EACH ROW
EXECUTE PROCEDURE trigger_set_timestamp();

