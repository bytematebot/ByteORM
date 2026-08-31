---
title: Client
order: 4
pager: false
---

# Using the client

```rust
use byteorm_client::Client;

let client = Client::new("postgres://user:pass@localhost:5432/mydb").await?;
```

`Client::new` opens a connection pool. Create it once and share it; every repository call
borrows a connection from the pool and returns it afterwards.

Each model in your schema becomes a field on the client, named after the model in snake
case:

```rust
client.user    // model User
client.post    // model Post
```

- [CRUD](/client/crud): create, read, update, delete
- [Queries](/client/queries): filters, ordering, paging, aggregates
- [Transactions](/client/transactions)

## Errors

Calls return `Result<_, BoxError>`, so `?` works against any error type in an application
that boxes its errors:

```rust
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
```
