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
                    self.core().push_order(#expression.to_string(), "ASC");
                    self
                }
                pub fn #desc_method(mut self) -> Self {
                    self.core().push_order(#expression.to_string(), "DESC");
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
                    self.core().filters().push(#expression, WhereOp::Gt, __private::SqlArg::boxed(value));
                    self
                }
                pub fn #method_lt<V>(mut self, value: V) -> Self
                where V: tokio_postgres::types::ToSql + Sync + Send + 'static
                {
                    self.core().filters().push(#expression, WhereOp::Lt, __private::SqlArg::boxed(value));
                    self
                }
                pub fn #method_eq<V>(mut self, value: V) -> Self
                where V: tokio_postgres::types::ToSql + Sync + Send + 'static
                {
                    self.core().filters().push(#expression, WhereOp::Eq, __private::SqlArg::boxed(value));
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
                    self.core().push_include(format!(
                        "(SELECT row_to_json(r) FROM {} r WHERE r.{} = t.{}) as {}",
                        #target_table, #target_col, #self_col, #relation_name
                    ));
                    self
                }
            }
        })
        .collect()
}

pub fn generate_query_builder_struct(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let builder_name = format_ident!("{}Query", model.name);
    let where_builder_name = format_ident!("{}WhereBuilder", model.name);

    let where_methods: Vec<_> = generate_filter_methods(model, "core().filters()").collect();
    let computed_where_methods = generate_computed_where_methods(model);
    let order_by_methods = generate_order_by_methods(model);
    let include_methods = generate_include_methods(model);

    quote! {
        pub type #where_builder_name = __private::Query<#model_name, __private::WhereOnly>;
        pub type #builder_name = __private::Query<#model_name, __private::Executable>;

        // Conditions and ordering read the same on a bare where-builder and on
        // an executable query, so they are written once for both states.
        impl<S> __private::Query<#model_name, S> {
            #(#where_methods)*
            #(#computed_where_methods)*
            #(#order_by_methods)*
        }

        impl __private::Query<#model_name, __private::Executable> {
            #(#include_methods)*
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
    fn both_builder_states_share_one_set_of_methods() {
        let code = generate_query_builder_struct(&model()).to_string();

        // one impl block carries the conditions for both states
        assert_eq!(code.matches("where_id").count(), 2); // where_id and where_id_in
        assert!(code.contains("impl < S > __private :: Query < Post , S >"));
        assert!(code.contains("pub type PostWhereBuilder"));
        assert!(!code.contains("SELECT {} FROM {}"));
    }
}
