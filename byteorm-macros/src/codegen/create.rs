use crate::codegen::utils::{
    generate_create_value_methods, generate_filter_methods, generate_select_columns,
};
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_create_builder(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let create_builder_name = format_ident!("{}Create", model.name);
    let table_name = model.name.to_lowercase();

    let where_methods = generate_filter_methods(model, "core.filters()");
    let set_methods = generate_create_value_methods(model, "core");
    let select_columns = generate_select_columns(model);

    quote! {
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
                        <#model_name as crate::ModelMeta>::REQUIRED_COLUMNS,
                        <#model_name as crate::ModelMeta>::ENUM_CASTS,
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
                            <#model_name as crate::ModelMeta>::REQUIRED_COLUMNS,
                            <#model_name as crate::ModelMeta>::ENUM_CASTS,
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
