-- Seed data for the loadr SQL protocol example (PostgreSQL).
CREATE TABLE IF NOT EXISTS products (
    id    SERIAL PRIMARY KEY,
    name  TEXT    NOT NULL,
    price NUMERIC NOT NULL,
    stock INTEGER NOT NULL DEFAULT 0
);

INSERT INTO products (name, price, stock) VALUES
    ('Widget',  9.99,  100),
    ('Gadget',  19.99, 50),
    ('Gizmo',   4.50,  250),
    ('Doohickey', 14.0, 0),
    ('Thingamajig', 99.99, 7);
