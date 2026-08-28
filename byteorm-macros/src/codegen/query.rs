use crate::codegen::utils::*;
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Emits `order_by_*_asc` / `_desc` for every column and computed field.
fn generate_order_by_methods(model: &Model) -> Vec<TokenStream> {
    let column_orders = model.fields.iter().map(|field| {
        let snake = to_snake_case(&field.name);
        (snake.clone(), snake)
    });
    let computed_orders = model.computed_fields.iter().map(|computed| {
        let snake = to_snake_case(&computed.name);
        (snake, format!("({})", computed.expression))
    });

    column_orders
        .chain(computed_orders)
        .map(|(name, expression)| {
            let asc_method = format_ident!("order_by_{}_asc", name);
            let desc_method = format_ident!("order_by_{}_desc", name);
            quote! {
                pub fn #asc_method(mut self) -> Self {
                    self.core.push_order(#expression.to_string(), "ASC");
                    self
                }
                pub fn #desc_method(mut self) -> Self {
                    self.core.push_order(#expression.to_string(), "DESC");
                    self
                }
            }
        })
        .collect()
}

/// Emits comparisons against a computed field's SQL expression.
fn generate_computed_where_methods(model: &Model) -> Vec<TokenStream> {
    model
        .computed_fields
        .iter()
        .map(|computed| {
            let snake = to_snake_case(&computed.name);
            let method_gt = format_ident!("where_{}_gt", snake);
            let method_lt = format_ident!("where_{}_lt", snake);
            let method_eq = format_ident!("where_{}_eq", snake);
            let expression = format!("({})", computed.expression);

            quote! {
                pub fn #method_gt<V>(mut self, value: V) -> Self
                where V: tokio_postgres::types::ToSql + Sync + Send + 'static
                {
                    self.core.filters().push(#expression, WhereOp::Gt, Box::new(value));
                    self
                }
                pub fn #method_lt<V>(mut self, value: V) -> Self
                where V: tokio_postgres::types::ToSql + Sync + Send + 'static
                {
                    self.core.filters().push(#expression, WhereOp::Lt, Box::new(value));
                    self
                }
                pub fn #method_eq<V>(mut self, value: V) -> Self
                where V: tokio_postgres::types::ToSql + Sync + Send + 'static
                {
                    self.core.filters().push(#expression, WhereOp::Eq, Box::new(value));
                    self
                }
            }
        })
        .collect()
}

/// Emits `include_*` methods for foreign keys, used by the JSON reads.
fn generate_include_methods(model: &Model) -> Vec<TokenStream> {
    model
        .fields
        .iter()
        .filter_map(|field| {
            field.modifiers.iter().find_map(|m| {
                if let Modifier::ForeignKey {
                    model: target_model,
                    field: target_field,
                    ..
                } = m
                {
                    Some((field, target_model, target_field))
                } else {
                    None
                }
            })
        })
        .map(|(field, target_model, target_field)| {
            let relation_name = to_snake_case(target_model);
            let method_name = format_ident!("include_{}", relation_name);
            let target_table = target_model.to_lowercase();
            let self_col = to_snake_case(&field.name);
            let target_col = target_field.clone().unwrap_or_else(|| "id".to_string());

            quote! {
                pub fn #method_name(mut self) -> Self {
                    self.core.push_include(format!(
                        "(SELECT row_to_json(r) FROM {} r WHERE r.{} = t.{}) as {}",
                        #target_table, #target_col, #self_col, #relation_name
                    ));
                    self
                }
            }
        })
        .collect()
}

/// Paging and raw-clause methods, identical on both builders.
fn generate_shared_methods() -> TokenStream {
    quote! {
        pub fn where_raw(
            mut self,
            clause: impl Into<String>,
            params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>,
        ) -> Self {
            self.core.filters().push_raw(clause.into(), params);
            self
        }

        pub fn limit(mut self, limit: usize) -> Self {
            self.core.set_limit(limit);
            self
        }

        pub fn offset(mut self, offset: usize) -> Self {
            self.core.set_offset(offset);
            self
        }
    }
}

pub fn generate_query_builder_struct(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let builder_name = format_ident!("{}Query", model.name);
    let where_builder_name = format_ident!("{}WhereBuilder", model.name);
    let table_name = model.name.to_lowercase();
    let select_columns = generate_select_columns(model);

    let where_methods: Vec<_> = generate_filter_methods(model, "core.filters()").collect();
    let computed_where_methods = generate_computed_where_methods(model);
    let order_by_methods = generate_order_by_methods(model);
    let include_methods = generate_include_methods(model);
    let shared_methods = generate_shared_methods();

    quote! {
        pub struct #where_builder_name {
            core: __private::QueryCore,
        }

        impl #where_builder_name {
            pub fn new() -> Self {
                Self { core: __private::QueryCore::new(#table_name, #select_columns) }
            }

            #(#where_methods)*
            #(#computed_where_methods)*
            #(#order_by_methods)*
            #shared_methods
        }

        pub struct #builder_name {
            core: __private::QueryCore,
            pool: ConnectionPool,
            fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<tokio_postgres::Row>, Box<dyn std::error::Error + Send + Sync>>> + Send>>>,
        }

        impl #builder_name {
            pub fn new(pool: ConnectionPool) -> Self {
                Self {
                    core: __private::QueryCore::new(#table_name, #select_columns),
                    pool,
                    fut: None,
                }
            }

            pub fn from_builder(pool: ConnectionPool, builder: #where_builder_name) -> Self {
                Self { core: builder.core, pool, fut: None }
            }

            #(#where_methods)*
            #(#computed_where_methods)*
            #(#order_by_methods)*
            #(#include_methods)*
            #shared_methods

            fn take_core(&mut self) -> __private::QueryCore {
                std::mem::replace(
                    &mut self.core,
                    __private::QueryCore::new(#table_name, #select_columns),
                )
            }

            pub async fn find_many_json(mut self)
                -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
            {
                let core = self.take_core();
                core.fetch_json(self.pool.clone()).await
            }

            pub async fn find_first_json(self)
                -> Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
            {
                let result = self.limit(1).find_many_json().await?;
                Ok(result.into_iter().next())
            }

            pub async fn first(self)
                -> Result<Option<#model_name>, Box<dyn std::error::Error + Send + Sync>>
            {
                let result = self.limit(1).await?;
                Ok(result.into_iter().next())
            }

            pub async fn count(mut self)
                -> Result<i64, Box<dyn std::error::Error + Send + Sync>>
            {
                let core = self.take_core();
                core.count(self.pool.clone()).await
            }

            pub async fn aggregate<T>(mut self, field: &str, func: &str)
                -> Result<Option<T>, Box<dyn std::error::Error + Send + Sync>>
            where
                T: for<'a> tokio_postgres::types::FromSql<'a>,
            {
                let core = self.take_core();
                let row = core
                    .scalar(self.pool.clone(), format!("{}({})", func.to_uppercase(), field))
                    .await?;
                Ok(row.get(0))
            }

            pub async fn sum<T>(mut self, field: &str)
                -> Result<T, Box<dyn std::error::Error + Send + Sync>>
            where
                T: for<'a> tokio_postgres::types::FromSql<'a> + Default + tokio_postgres::types::ToSql + Sync,
            {
                let default_value = T::default();
                let mut core = self.take_core();
                let expression = format!(
                    "COALESCE(SUM({}), ${})",
                    field,
                    core.next_placeholder()
                );
                let (sql, params) = core.build_scalar(&expression);

                let client = self.pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                let mut refs = __private::as_sql_refs(&params);
                refs.push(&default_value);
                debug::log_query(&sql, refs.len());
                let row = client.query_one(&sql, &refs[..]).await?;
                Ok(row.get(0))
            }

            pub async fn sum_cast_i64(mut self, field: &str)
                -> Result<i64, Box<dyn std::error::Error + Send + Sync>>
            {
                let core = self.take_core();
                core.sum_cast_i64(self.pool.clone(), field).await
            }

            pub async fn avg<T>(self, field: &str)
                -> Result<Option<T>, Box<dyn std::error::Error + Send + Sync>>
            where
                T: for<'a> tokio_postgres::types::FromSql<'a>,
            {
                self.aggregate(field, "AVG").await
            }

            pub async fn min<T>(self, field: &str)
                -> Result<Option<T>, Box<dyn std::error::Error + Send + Sync>>
            where
                T: for<'a> tokio_postgres::types::FromSql<'a>,
            {
                self.aggregate(field, "MIN").await
            }

            pub async fn max<T>(self, field: &str)
                -> Result<Option<T>, Box<dyn std::error::Error + Send + Sync>>
            where
                T: for<'a> tokio_postgres::types::FromSql<'a>,
            {
                self.aggregate(field, "MAX").await
            }
        }

        impl Future for #builder_name {
            type Output = Result<Vec<#model_name>, Box<dyn std::error::Error + Send + Sync>>;

            fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let me = &mut *self;

                if me.fut.is_none() {
                    let core = me.take_core();
                    me.fut = Some(Box::pin(core.fetch(me.pool.clone())));
                }

                match me.fut.as_mut().unwrap().as_mut().poll(cx) {
                    Poll::Ready(Ok(rows)) => Poll::Ready(Ok(
                        rows.iter().map(|row| #model_name::from_row(row)).collect()
                    )),
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                    Poll::Pending => Poll::Pending,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_query_builder_struct;
    use crate::types::{Field, Model, Modifier};

    fn model() -> Model {
        Model {
            name: "Post".to_string(),
            fields: vec![Field {
                name: "id".to_string(),
                type_name: "BigInt".to_string(),
                modifiers: vec![Modifier::PrimaryKey],
                attributes: vec![],
            }],
            computed_fields: vec![],
            table_name: "post".to_string(),
        }
    }

    #[test]
    fn statement_assembly_lives_in_the_shared_runtime() {
        let code = generate_query_builder_struct(&model()).to_string();

        assert!(code.contains("__private :: QueryCore"));
        assert!(!code.contains("SELECT {} FROM {}"));
        assert!(!code.contains("ORDER BY"));
    }
}
