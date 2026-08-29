//! The workload: what is measured, and how one iteration of it is executed.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{Deserialize, Serialize};

use crate::adapter::{BenchResult, NewPost, OrmAdapter};
use crate::fixture::Fixture;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scenario {
    InsertOne,
    InsertMany,
    FindByPk,
    FindByUniqueEmail,
    RecentPosts,
    UpdateOne,
    DeleteOne,
    Count,
    SumViews,
    Transaction,
}

impl Scenario {
    pub const ALL: &'static [Scenario] = &[
        Scenario::InsertOne,
        Scenario::InsertMany,
        Scenario::FindByPk,
        Scenario::FindByUniqueEmail,
        Scenario::RecentPosts,
        Scenario::UpdateOne,
        Scenario::DeleteOne,
        Scenario::Count,
        Scenario::SumViews,
        Scenario::Transaction,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Scenario::InsertOne => "insert_one",
            Scenario::InsertMany => "insert_many",
            Scenario::FindByPk => "find_by_pk",
            Scenario::FindByUniqueEmail => "find_by_unique_email",
            Scenario::RecentPosts => "recent_posts",
            Scenario::UpdateOne => "update_one",
            Scenario::DeleteOne => "delete_one",
            Scenario::Count => "count",
            Scenario::SumViews => "sum_views",
            Scenario::Transaction => "transaction",
        }
    }

    pub fn parse(s: &str) -> Option<Scenario> {
        Scenario::ALL.iter().copied().find(|sc| sc.key() == s)
    }

    pub fn description(self) -> &'static str {
        match self {
            Scenario::InsertOne => "INSERT one user, returning the generated row",
            Scenario::InsertMany => "batch INSERT of N posts in one statement",
            Scenario::FindByPk => "SELECT one user by primary key",
            Scenario::FindByUniqueEmail => "SELECT one user by unique email",
            Scenario::RecentPosts => "SELECT posts of one user, ordered, limited",
            Scenario::UpdateOne => "UPDATE one post's title by primary key",
            Scenario::DeleteOne => "DELETE one post by primary key",
            Scenario::Count => "SELECT COUNT(*) of one user's posts",
            Scenario::SumViews => "SELECT SUM(views) of one user's posts",
            Scenario::Transaction => "one transaction: insert a user and its posts",
        }
    }

    /// Write scenarios mutate the fixture, so they get a freshly seeded
    /// database and are never run concurrently with a read scenario.
    pub fn is_write(self) -> bool {
        matches!(
            self,
            Scenario::InsertOne
                | Scenario::InsertMany
                | Scenario::UpdateOne
                | Scenario::DeleteOne
                | Scenario::Transaction
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    /// Rows per batch in `insert_many`.
    pub batch_size: usize,
    /// Rows requested by `recent_posts`.
    pub page_size: i64,
    /// Posts created inside one `transaction` iteration.
    pub posts_per_transaction: usize,
}

impl Default for ScenarioConfig {
    fn default() -> Self {
        Self {
            batch_size: 100,
            page_size: 20,
            posts_per_transaction: 5,
        }
    }
}

/// Per-run state shared by every task executing a scenario: the fixture ids to
/// touch and the counters that keep generated values unique.
pub struct OpCtx {
    pub fixture: Arc<Fixture>,
    pub config: ScenarioConfig,
    /// Monotonic counter, unique per iteration across all tasks.
    pub seq: AtomicUsize,
    /// Distinguishes rows created by different runs in the same database.
    pub run_tag: String,
}

impl OpCtx {
    pub fn next(&self) -> usize {
        self.seq.fetch_add(1, Ordering::Relaxed)
    }
}

/// Execute one iteration of `scenario` against `adapter`.
pub async fn run_op(
    scenario: Scenario,
    adapter: &Arc<dyn OrmAdapter>,
    ctx: &OpCtx,
) -> BenchResult<()> {
    let i = ctx.next();

    match scenario {
        Scenario::InsertOne => {
            adapter
                .insert_user(
                    format!("insert-{}-{i}@bench.local", ctx.run_tag),
                    format!("user{i}"),
                )
                .await?;
        }
        Scenario::InsertMany => {
            let user_id = ctx.fixture.user(i);
            let rows = (0..ctx.config.batch_size)
                .map(|j| NewPost {
                    user_id,
                    title: format!("batch {i}/{j}"),
                    content: BODY.to_string(),
                    views: (j % 97) as i32,
                })
                .collect();
            adapter.insert_posts_many(rows).await?;
        }
        Scenario::FindByPk => {
            let id = ctx.fixture.user(i);
            let row = adapter.find_user_by_id(id).await?;
            debug_assert!(row.is_some(), "fixture user {id} must exist");
        }
        Scenario::FindByUniqueEmail => {
            let email = ctx.fixture.user_email(i);
            let row = adapter.find_user_by_email(&email).await?;
            debug_assert!(row.is_some(), "fixture user {email} must exist");
        }
        Scenario::RecentPosts => {
            let id = ctx.fixture.user(i);
            adapter.recent_posts(id, ctx.config.page_size).await?;
        }
        Scenario::UpdateOne => {
            let id = ctx.fixture.post(i);
            adapter
                .update_post_title(id, format!("updated {i}"))
                .await?;
        }
        Scenario::DeleteOne => {
            // Each iteration deletes a distinct row, so no iteration is a
            // cheap no-op against an already deleted id.
            let id = ctx.fixture.disposable_post(i);
            adapter.delete_post(id).await?;
        }
        Scenario::Count => {
            let id = ctx.fixture.user(i);
            adapter.count_posts(id).await?;
        }
        Scenario::SumViews => {
            let id = ctx.fixture.user(i);
            adapter.sum_views(id).await?;
        }
        Scenario::Transaction => {
            adapter
                .tx_user_with_posts(
                    format!("tx-{}-{i}@bench.local", ctx.run_tag),
                    ctx.config.posts_per_transaction,
                )
                .await?;
        }
    }

    Ok(())
}

pub const BODY: &str = "Benchmark post body. Long enough to be a realistic TEXT payload rather than a single word, short enough that the network is not the whole measurement.";
