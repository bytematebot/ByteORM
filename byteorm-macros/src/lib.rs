extern crate proc_macro;

mod codegen;
mod parse;
mod types;

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(ByteOrm, attributes(byteorm))]
pub fn derive_byteorm(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let model = parse::parse_model(&input);

    let from_row_impl = codegen::utils::generate_from_row_impl(&model);
    let model_impl = codegen::model::generate_model_impl(&model);
    let query_builder = codegen::query::generate_query_builder_struct(&model);
    let update_builder = codegen::update::generate_update_builder(&model);
    let create_builder = codegen::create::generate_create_builder(&model);
    let delete_builder = codegen::delete::generate_delete_builder(&model);
    let upsert_builder = codegen::upsert::generate_upsert_builder(&model);
    let jsonb_sub_accessors = codegen::jsonb::generate_jsonb_sub_accessors(&model);
    let accessor = codegen::client::generate_accessor(&model);

    let expanded = quote! {
        #from_row_impl
        #model_impl
        #query_builder
        #update_builder
        #create_builder
        #delete_builder
        #upsert_builder
        #(#jsonb_sub_accessors)*
        #accessor
    };

    TokenStream::from(expanded)
}

#[cfg(test)]
mod size_probe {
    use crate::codegen;
    use crate::types::{Field, Model, Modifier};

    fn model() -> Model {
        let mut fields = vec![Field {
            name: "id".to_string(),
            type_name: "BigInt".to_string(),
            modifiers: vec![Modifier::PrimaryKey],
            attributes: vec![],
        }];
        for (name, ty) in [
            ("user_id", "BigInt"),
            ("name", "String"),
            ("slug", "String"),
            ("amount", "Int"),
            ("active", "Boolean"),
            ("created_at", "TimestamptZ"),
            ("updated_at", "TimestamptZ"),
        ] {
            fields.push(Field {
                name: name.to_string(),
                type_name: ty.to_string(),
                modifiers: vec![],
                attributes: vec![],
            });
        }
        Model {
            name: "Probe".to_string(),
            fields,
            computed_fields: vec![],
            table_name: "probe".to_string(),
        }
    }

    #[test]
    fn print_generated_sizes() {
        let m = model();
        let parts = [
            (
                "from_row",
                codegen::utils::generate_from_row_impl(&m).to_string().len(),
            ),
            (
                "model_impl",
                codegen::model::generate_model_impl(&m).to_string().len(),
            ),
            (
                "query",
                codegen::query::generate_query_builder_struct(&m)
                    .to_string()
                    .len(),
            ),
            (
                "update",
                codegen::update::generate_update_builder(&m)
                    .to_string()
                    .len(),
            ),
            (
                "create",
                codegen::create::generate_create_builder(&m)
                    .to_string()
                    .len(),
            ),
            (
                "delete",
                codegen::delete::generate_delete_builder(&m)
                    .to_string()
                    .len(),
            ),
            (
                "upsert",
                codegen::upsert::generate_upsert_builder(&m)
                    .to_string()
                    .len(),
            ),
            (
                "accessor",
                codegen::client::generate_accessor(&m).to_string().len(),
            ),
        ];
        let total: usize = parts.iter().map(|(_, n)| n).sum();
        for (name, n) in parts {
            println!(
                "{name:12} {n:>8} chars  {:>5.1}%",
                100.0 * n as f64 / total as f64
            );
        }
        println!("{:12} {total:>8} chars", "TOTAL");
    }
}
