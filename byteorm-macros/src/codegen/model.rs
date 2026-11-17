use crate::codegen::utils::*;
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_model_impl(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let pk_fields: Vec<_> = model
        .fields
        .iter()
        .filter(|f| {
            f.modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
        })
        .collect();

    let find_by_id_impl = if pk_fields.is_empty() {
        quote! {}
    } else if pk_fields.len() == 1 {
        let pk = &pk_fields[0];
        let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
        let pk_type = rust_type_from_schema(&pk.type_name, is_nullable);

        quote! {
            pub async fn find_by_id(pool: ConnectionPool, id: #pk_type)
                -> Result<Option<#model_name>, Box<dyn std::error::Error + Send + Sync>>
            {
                __private::find_by_pk::<#model_name>(&pool, &[&id]).await
            }
        }
    } else {
        let (pk_names, pk_types, _, _, pk_arg_refs) = pk_args(model);

        let pk_params = pk_names.iter().zip(pk_types.iter()).map(|(name, typ)| {
            quote! { #name: #typ }
        });

        quote! {
            pub async fn find_by_composite_pk(pool: ConnectionPool, #(#pk_params),*)
                -> Result<Option<#model_name>, Box<dyn std::error::Error + Send + Sync>>
            {
                __private::find_by_pk::<#model_name>(&pool, &[#(#pk_arg_refs),*]).await
            }
        }
    };

    quote! {
        impl #model_name {
            #find_by_id_impl
        }
    }
}
