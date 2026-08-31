---
title: Transactions
description: Grouping writes so they land together
order: 3
---

# Transactions

```rust
let tx = client.begin().await?;

tx.user.create(|u| u.set_email("alice@example.com")).await?;
tx.post.create(|p| p.set_title("Hello")).await?;

tx.commit().await?;
```

`begin` returns a client with the same repositories, bound to one connection and one
transaction. Everything you call on it is part of that transaction.

## Rolling back

```rust
tx.rollback().await?;
```

Dropping the transaction without committing rolls it back, so an early `?` cannot leave a
half-finished write behind.

:::warning
Hold a transaction for as short a time as possible. It occupies a pooled connection for its
whole lifetime, and other work waits when the pool runs dry.
:::
