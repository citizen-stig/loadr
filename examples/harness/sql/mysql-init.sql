-- Seed data for the loadr SQL protocol example (MySQL).
CREATE TABLE IF NOT EXISTS products (
    id    INT AUTO_INCREMENT PRIMARY KEY,
    name  VARCHAR(128)  NOT NULL,
    price DECIMAL(10,2) NOT NULL,
    stock INT           NOT NULL DEFAULT 0
);

INSERT INTO products (name, price, stock) VALUES
    ('Widget',  9.99,  100),
    ('Gadget',  19.99, 50),
    ('Gizmo',   4.50,  250),
    ('Doohickey', 14.0, 0),
    ('Thingamajig', 99.99, 7);
