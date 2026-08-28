use crate::codegen::utils::{
    generate_arithmetic_methods, generate_filter_methods, generate_select_columns,
    generate_update_set_methods,
};
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_update_builder(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let update_builder_name = format_ident!("{}Update", model.name);
    let table_name = model.name.to_lowercase();

    let where_methods = generate_filter_methods(model, "core.filters()");
    let set_methods = generate_update_set_methods(model, "core");
    let inc_methods = generate_arithmetic_methods(model, "core");
    let select_columns = generate_select_columns(model);

    quote! {
        pub struct #update_builder_name {
            core: __private::UpdateCore,
            pool: ConnectionPool,
            fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<tokio_postgres::Row>, Box<dyn std::error::Error + Send + Sync>>> + Send>>>,
        }

        impl #update_builder_name {
            pub fn new(pool: ConnectionPool) -> Self {
                Self {
                    core: __private::UpdateCore::new(#table_name, #select_columns),
                    pool,
                    fut: None,
                }
            }

            /// Allows the update to run without a WHERE clause, changing every
            /// row in the table.
            pub fn allow_all_rows(mut self) -> Self {
                self.core.allow_all_rows();
                self
            }

            /// Runs the update and returns every row it changed. `.await`
            /// returns only the first one.
            pub async fn all(mut self) -> Result<Vec<#model_name>, Box<dyn std::error::Error + Send + Sync>> {
                let core = std::mem::replace(
                    &mut self.core,
                    __private::UpdateCore::new(#table_name, #select_columns),
                );
                let rows = core.execute(self.pool.clone()).await?;
                Ok(rows.iter().map(|row| #model_name::from_row(row)).collect())
            }

            /// Hands the collected assignments to `upsert_many`.
            pub fn into_core(self) -> __private::UpdateCore {
                self.core
            }

            #(#where_methods)*
            #(#set_methods)*
            #(#inc_methods)*
        }

        impl std::future::Future for #update_builder_name {
            type Output = Result<#model_name, Box<dyn std::error::Error + Send + Sync>>;
            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
                let me = &mut *self;

                if me.fut.is_none() {
                    let core = std::mem::replace(
                        &mut me.core,
                        __private::UpdateCore::new(#table_name, #select_columns),
                    );
                    me.fut = Some(Box::pin(core.execute(me.pool.clone())));
                }

                match me.fut.as_mut().unwrap().as_mut().poll(cx) {
                    std::task::Poll::Ready(Ok(rows)) => std::task::Poll::Ready(
                        match rows.first() {
                            Some(row) => Ok(#model_name::from_row(row)),
                            None => Err("UPDATE matched no rows".into()),
                        }
                    ),
                    std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_update_builder;
    use crate::types::{Field, Model, Modifier};

    fn model() -> Model {
        Model {
            name: "User".to_string(),
            fields: vec![Field {
                name: "id".to_string(),
                type_name: "BigInt".to_string(),
                modifiers: vec![Modifier::PrimaryKey],
                attributes: vec![],
            }],
            computed_fields: vec![],
            table_name: "user".to_string(),
        }
    }

    #[test]
    fn update_without_where_is_refused_unless_opted_in() {
        let code = generate_update_builder(&model()).to_string();

        assert!(code.contains("allow_all_rows"));
    }

    #[test]
    fn awaiting_yields_one_model_while_all_yields_every_changed_row() {
        let code = generate_update_builder(&model()).to_string();

        // query_one turned a multi-row update into an error after the write
        assert!(!code.contains("query_one"));
        assert!(code.contains("type Output = Result < User"));
        assert!(code.contains("pub async fn all"));
    }

    #[test]
    fn statement_assembly_lives_in_the_shared_runtime() {
        let code = generate_update_builder(&model()).to_string();

        assert!(code.contains("__private :: UpdateCore"));
        assert!(!code.contains("UPDATE {} SET"));
    }
}
