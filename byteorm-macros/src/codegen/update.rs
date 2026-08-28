use crate::codegen::utils::{
    generate_inc_methods, generate_select_columns, generate_set_methods, generate_where_methods,
    is_builtin_type, to_snake_case,
};
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_update_builder(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let update_builder_name = format_ident!("{}Update", model.name);
    let table_name = model.name.to_lowercase();

    let where_methods = generate_where_methods(model, "where_args", "where_predicates");

    let set_methods =
        generate_set_methods(model, false, "", Some("set_args"), Some("set_fragments"));

    let inc_methods = generate_inc_methods(model, "inc_ops", None);

    let select_columns = generate_select_columns(model);

    let enum_cast_entries: Vec<TokenStream> = model
        .fields
        .iter()
        .filter(|field| !is_builtin_type(&field.type_name))
        .map(|field| {
            let col_name = to_snake_case(&field.name);
            let type_name = field.type_name.to_lowercase();
            quote! { (#col_name, #type_name) }
        })
        .collect();

    quote! {
        pub struct #update_builder_name {
            pool: ConnectionPool,
            table: String,
            where_predicates: Vec<WherePredicate>,
            where_args: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>,
            set_fragments: Vec<&'static str>,
            set_args: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>,
            inc_ops: Vec<(&'static str, &'static str, i64)>,
            allow_all_rows: bool,
            fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<#model_name, Box<dyn std::error::Error + Send + Sync>>> + Send>>>,
        }

        unsafe impl Send for #update_builder_name {}

        impl #update_builder_name {
            pub fn new(pool: ConnectionPool) -> Self {
                Self {
                    pool,
                    table: #table_name.to_string(),
                    where_predicates: vec![],
                    where_args: vec![],
                    set_fragments: vec![],
                    set_args: vec![],
                    inc_ops: vec![],
                    allow_all_rows: false,
                    fut: None,
                }
            }

            /// Builds the statement and takes ownership of the bound
            /// parameters. Shared by `.await` and `all()`.
            fn build_statement(&mut self)
                -> Result<(String, Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>), Box<dyn std::error::Error + Send + Sync>>
            {
                if self.set_fragments.is_empty() && self.inc_ops.is_empty() {
                    return Err("No fields to update".into());
                }
                if self.where_predicates.is_empty() && !self.allow_all_rows {
                    return Err("UPDATE without WHERE clause is not allowed; call allow_all_rows() to update every row".into());
                }

                let enum_casts: std::collections::HashMap<&str, &str> = [
                    #(#enum_cast_entries),*
                ].into_iter().collect();

                let mut sql = format!("UPDATE {} SET ", self.table);
                let mut set_clauses: Vec<String> = vec![];
                let mut param_idx = 1;
                for col in self.set_fragments.iter() {
                    if let Some(enum_type) = enum_casts.get(*col) {
                        set_clauses.push(format!("{} = ${}::TEXT::{}", col, param_idx, enum_type));
                    } else {
                        set_clauses.push(format!("{} = ${}", col, param_idx));
                    }
                    param_idx += 1;
                }
                for (field, op, _) in &self.inc_ops {
                    let clause = match *op {
                        "inc" => format!("{} = {} + ${}", field, field, param_idx),
                        "dec" => format!("{} = {} - ${}", field, field, param_idx),
                        "mul" => format!("{} = {} * ${}", field, field, param_idx),
                        "div" => format!("{} = {} / ${}", field, field, param_idx),
                        _ => continue,
                    };
                    set_clauses.push(clause);
                    param_idx += 1;
                }
                sql.push_str(&set_clauses.join(", "));

                let mut all_params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>> = vec![];
                for arg in std::mem::take(&mut self.set_args) {
                    all_params.push(arg);
                }
                for (_, _, val) in &self.inc_ops {
                    all_params.push(Box::new(*val));
                }

                if !self.where_predicates.is_empty() {
                    let (where_clauses, _) = render_where_predicates(&self.where_predicates, param_idx);
                    sql.push_str(" WHERE ");
                    sql.push_str(&where_clauses.join(" AND "));
                    for arg in std::mem::take(&mut self.where_args) {
                        all_params.push(arg);
                    }
                }
                sql.push_str(&format!(" RETURNING {}", #select_columns));

                Ok((sql, all_params))
            }

            /// Runs the update and returns every row it changed. `.await`
            /// returns only the first one.
            pub async fn all(mut self) -> Result<Vec<#model_name>, Box<dyn std::error::Error + Send + Sync>> {
                let (sql, all_params) = self.build_statement()?;
                debug::log_query(&sql, all_params.len());

                let client = self.pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                    all_params.iter().map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
                let rows = client.query(&sql, &params[..]).await?;
                Ok(rows.iter().map(|row| #model_name::from_row(row)).collect())
            }

            /// Allows the update to run without a WHERE clause, changing every
            /// row in the table.
            pub fn allow_all_rows(mut self) -> Self {
                self.allow_all_rows = true;
                self
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
                    let (sql, all_params) = match me.build_statement() {
                        Ok(statement) => statement,
                        Err(e) => return std::task::Poll::Ready(Err(e)),
                    };

                    let pool = me.pool.clone();
                    let fut = async move {
                        debug::log_query(&sql, all_params.len());
                        let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                            all_params.iter().map(|b| b.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync)).collect();
                        let rows = client.query(&sql, &params[..]).await?;
                        match rows.first() {
                            Some(row) => Ok(#model_name::from_row(row)),
                            None => Err::<#model_name, Box<dyn std::error::Error + Send + Sync>>(
                                "UPDATE matched no rows".into()
                            ),
                        }
                    };
                    me.fut = Some(Box::pin(fut));
                }

                me.fut.as_mut().unwrap().as_mut().poll(cx)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_update_builder;
    use crate::types::{Field, Model, Modifier};

    #[test]
    fn update_without_where_is_refused_unless_opted_in() {
        let model = Model {
            name: "User".to_string(),
            fields: vec![Field {
                name: "id".to_string(),
                type_name: "BigInt".to_string(),
                modifiers: vec![Modifier::PrimaryKey],
                attributes: vec![],
            }],
            computed_fields: vec![],
            table_name: "user".to_string(),
        };

        let code = generate_update_builder(&model).to_string();

        assert!(code.contains("allow_all_rows"));
        assert!(code.contains("UPDATE without WHERE clause is not allowed"));
    }

    #[test]
    fn awaiting_yields_one_model_while_all_yields_every_changed_row() {
        let model = Model {
            name: "User".to_string(),
            fields: vec![Field {
                name: "id".to_string(),
                type_name: "BigInt".to_string(),
                modifiers: vec![Modifier::PrimaryKey],
                attributes: vec![],
            }],
            computed_fields: vec![],
            table_name: "user".to_string(),
        };

        let code = generate_update_builder(&model).to_string();

        // query_one turned a multi-row update into an error after the write
        assert!(!code.contains("query_one"));
        assert!(code.contains("type Output = Result < User"));
        assert!(code.contains("pub async fn all"));
    }
}
