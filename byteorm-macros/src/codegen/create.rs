use crate::codegen::utils::{
    generate_create_value_methods, generate_filter_methods, generate_select_columns,
    is_builtin_type, to_snake_case,
};
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_create_builder(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let create_builder_name = format_ident!("{}Create", model.name);
    let table_name = model.name.to_lowercase();

    let required_fields: Vec<String> = model
        .fields
        .iter()
        .filter(|field| {
            !field.attributes.iter().any(|a| a.name == "default")
                && !field
                    .modifiers
                    .iter()
                    .any(|m| matches!(m, Modifier::Nullable))
                && field.type_name != "Serial"
        })
        .map(|field| to_snake_case(&field.name))
        .collect();

    let where_methods = generate_filter_methods(model, "core.filters()");
    let set_methods = generate_create_value_methods(model, "core");
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

    let required_const = format_ident!("{}_REQUIRED_COLUMNS", model.name.to_uppercase());
    let casts_const = format_ident!("{}_ENUM_CASTS", model.name.to_uppercase());

    quote! {
        #[doc(hidden)]
        pub static #required_const: &[&str] = &[#(#required_fields),*];

        #[doc(hidden)]
        pub static #casts_const: &[(&str, &str)] = &[#(#enum_cast_entries),*];

        pub struct #create_builder_name {
            core: __private::CreateCore,
            pool: ConnectionPool,
            fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<tokio_postgres::Row, Box<dyn std::error::Error + Send + Sync>>> + Send>>>,
        }

        impl #create_builder_name {
            pub fn new(pool: ConnectionPool) -> Self {
                Self {
                    core: __private::CreateCore::new(
                        #table_name,
                        #select_columns,
                        #required_const,
                        #casts_const,
                    ),
                    pool,
                    fut: None,
                }
            }

            /// Hands the collected values to `upsert_many`.
            pub fn into_core(self) -> __private::CreateCore {
                self.core
            }

            #(#where_methods)*
            #(#set_methods)*
        }

        impl std::future::Future for #create_builder_name {
            type Output = Result<#model_name, Box<dyn std::error::Error + Send + Sync>>;
            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
                let me = &mut *self;

                if me.fut.is_none() {
                    let core = std::mem::replace(
                        &mut me.core,
                        __private::CreateCore::new(
                            #table_name,
                            #select_columns,
                            #required_const,
                            #casts_const,
                        ),
                    );
                    me.fut = Some(Box::pin(core.execute(me.pool.clone())));
                }

                match me.fut.as_mut().unwrap().as_mut().poll(cx) {
                    std::task::Poll::Ready(Ok(row)) => {
                        std::task::Poll::Ready(Ok(#model_name::from_row(&row)))
                    }
                    std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
                    std::task::Poll::Pending => std::task::Poll::Pending,
                }
            }
        }
    }
}
