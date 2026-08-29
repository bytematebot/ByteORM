use crate::codegen::utils::{
    column_index, generate_create_value_methods, generate_filter_methods, is_numeric_type,
    to_snake_case,
};
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

/// Emits the four mutation builders as aliases of one generic type, with the
/// typed `where_*`, `set_*` and `inc_*` methods written once and shared by
/// every mode that accepts them.
pub fn generate_mutation_builders(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let create_builder = format_ident!("{}Create", model.name);
    let update_builder = format_ident!("{}Update", model.name);
    let delete_builder = format_ident!("{}Delete", model.name);
    let upsert_builder = format_ident!("{}Upsert", model.name);

    let where_methods: Vec<_> = generate_filter_methods(model, "filters()").collect();
    let set_methods: Vec<_> = generate_create_value_methods(model, "").collect();
    let arithmetic_methods = generate_upsert_arithmetic_methods(model);
    let upsert_pk_methods = generate_upsert_pk_methods(model);

    quote! {
        pub type #create_builder = __private::Mutation<#model_name, __private::CreateMode>;
        pub type #update_builder = __private::Mutation<#model_name, __private::UpdateMode>;
        pub type #delete_builder = __private::Mutation<#model_name, __private::DeleteMode>;
        pub type #upsert_builder = __private::Mutation<#model_name, __private::UpsertMode>;

        impl<Mode: __private::AcceptsFilters> __private::Mutation<#model_name, Mode> {
            #(#where_methods)*
        }

        impl<Mode: __private::AcceptsValues> __private::Mutation<#model_name, Mode> {
            #(#set_methods)*
        }

        impl<Mode: __private::AcceptsArithmetic> __private::Mutation<#model_name, Mode> {
            #(#arithmetic_methods)*
        }

        impl __private::Mutation<#model_name, __private::UpsertMode> {
            #(#upsert_pk_methods)*
        }
    }
}

/// `where_<pk>` on an upsert names the conflict row rather than filtering, so
/// it records a column value like `set_<pk>` does.
fn generate_upsert_pk_methods(model: &Model) -> Vec<TokenStream> {
    model
        .fields
        .iter()
        .filter(|field| {
            field
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
        })
        .map(|field| {
            let snake = to_snake_case(&field.name);
            let method_name = format_ident!("where_{}", snake);
            let setter = format_ident!("set_{}", snake);
            let is_nullable = field
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Nullable));
            let field_type =
                crate::codegen::utils::rust_type_from_schema(&field.type_name, is_nullable);

            quote! {
                pub fn #method_name(self, value: #field_type) -> Self {
                    self.#setter(value)
                }
            }
        })
        .collect()
}

/// Arithmetic seeds have to match the column's own type, not the i64 the API
/// takes.
fn generate_upsert_arithmetic_methods(model: &Model) -> Vec<TokenStream> {
    model
        .fields
        .iter()
        .filter(|f| is_numeric_type(&f.type_name))
        .map(|field| {
            let field_col = to_snake_case(&field.name);
            let index = column_index(model, &field_col);
            let inc_method = format_ident!("inc_{}", field_col);
            let dec_method = format_ident!("dec_{}", field_col);
            let mul_method = format_ident!("mul_{}", field_col);
            let div_method = format_ident!("div_{}", field_col);

            let seed = |value: TokenStream| match field.type_name.as_str() {
                "Int" | "Serial" => quote! { __private::SqlArg::I32((#value) as i32) },
                "Float" => quote! { __private::SqlArg::F64((#value) as f64) },
                "Real" => quote! { __private::SqlArg::F32((#value) as f32) },
                _ => quote! { __private::SqlArg::I64(#value) },
            };
            let inc_seed = seed(quote! { amount });
            let dec_seed = seed(quote! { -amount });
            let mul_seed = seed(quote! { 0i64 });
            let div_seed = seed(quote! { 0i64 });

            quote! {
                pub fn #inc_method(mut self, amount: i64) -> Self {
                    self.push_arithmetic(#index, "inc", amount, #inc_seed);
                    self
                }
                pub fn #dec_method(mut self, amount: i64) -> Self {
                    self.push_arithmetic(#index, "dec", amount, #dec_seed);
                    self
                }
                pub fn #mul_method(mut self, factor: i64) -> Self {
                    self.push_arithmetic(#index, "mul", factor, #mul_seed);
                    self
                }
                pub fn #div_method(mut self, divisor: i64) -> Self {
                    self.push_arithmetic(#index, "div", divisor, #div_seed);
                    self
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::generate_mutation_builders;
    use crate::types::{Field, Model, Modifier};

    fn model() -> Model {
        Model {
            name: "User".to_string(),
            fields: vec![
                Field {
                    name: "id".to_string(),
                    type_name: "BigInt".to_string(),
                    modifiers: vec![Modifier::PrimaryKey],
                    attributes: vec![],
                },
                Field {
                    name: "score".to_string(),
                    type_name: "Int".to_string(),
                    modifiers: vec![],
                    attributes: vec![],
                },
            ],
            computed_fields: vec![],
            table_name: "user".to_string(),
        }
    }

    #[test]
    fn all_four_builders_are_one_type() {
        let code = generate_mutation_builders(&model()).to_string();

        for alias in ["UserCreate", "UserUpdate", "UserDelete", "UserUpsert"] {
            assert!(
                code.contains(&format!("pub type {}", alias)),
                "missing {alias}"
            );
        }
        assert!(code.contains("__private :: Mutation"));
    }

    #[test]
    fn each_field_method_is_written_once() {
        let code = generate_mutation_builders(&model()).to_string();

        // one impl block per capability, not one per builder
        assert_eq!(code.matches("pub fn where_score (").count(), 1);
        assert_eq!(code.matches("pub fn set_score (").count(), 1);
        assert_eq!(code.matches("pub fn inc_score (").count(), 1);

        // the primary key keeps its extra upsert spelling, which names the
        // conflict row rather than filtering
        assert_eq!(code.matches("pub fn where_id (").count(), 2);
    }

    #[test]
    fn capabilities_are_gated_by_mode() {
        let code = generate_mutation_builders(&model()).to_string();

        assert!(code.contains("Mode : __private :: AcceptsFilters"));
        assert!(code.contains("Mode : __private :: AcceptsValues"));
        assert!(code.contains("Mode : __private :: AcceptsArithmetic"));
    }

    #[test]
    fn statement_assembly_lives_in_the_shared_runtime() {
        let code = generate_mutation_builders(&model()).to_string();

        assert!(!code.contains("INSERT INTO"));
        assert!(!code.contains("UPDATE"));
        assert!(!code.contains("query_one"));
    }
}
