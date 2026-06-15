// Seed data for the loadr MongoDB plugin example and integration tests.
// Mounted into the mongo:7 container's /docker-entrypoint-initdb.d, which runs
// it once (authenticated as the root user) against the database named by
// MONGO_INITDB_DATABASE (`loadr`).
db = db.getSiblingDB("loadr");

// App user the examples connect as: mongodb://loadr:loadr@host:27017/loadr
db.createUser({
  user: "loadr",
  pwd: "loadr",
  roles: [{ role: "readWrite", db: "loadr" }],
});

db.products.insertMany([
  { name: "Widget", price: 9.99, stock: 100, tags: ["a"] },
  { name: "Gadget", price: 19.99, stock: 50, tags: ["a", "b"] },
  { name: "Gizmo", price: 4.5, stock: 250, tags: ["b"] },
  { name: "Doohickey", price: 14.0, stock: 0, tags: ["c"] },
  { name: "Thingamajig", price: 99.99, stock: 7, tags: ["a", "c"] },
]);

db.products.createIndex({ price: 1 });
