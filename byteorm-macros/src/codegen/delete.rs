use crate::codegen::utils::generate_filter_methods;
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_delete_builder(model: &Model) -> TokenStream {
    let delete_builder_name = format_ident!("{}Delete", model.name);
    let table_name = model.name.to_lowercase();

    let where_methods = generate_filter_methods(model, "core.filters()");

    quote! {
        pub struct #delete_builder_name {
            core: __private::DeleteCore,
            pool: ConnectionPool,
            fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<u64, Box<dyn std::error::Error + Send + Sync>>> + Send>>>,
        }

        impl #delete_builder_name {
            pub fn new(pool: ConnectionPool) -> Self {
                Self {
                    core: __private::DeleteCore::new(#table_name),
                    pool,
                    fut: None,
                }
            }

            #(#where_methods)*
        }

        impl std::future::Future for #delete_builder_name {
            type Output = Result<u64, Box<dyn std::error::Error + Send + Sync>>;
            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
                let me = &mut *self;

                if me.fut.is_none() {
                    let core = std::mem::replace(&mut me.core, __private::DeleteCore::new(#table_name));
                    me.fut = Some(Box::pin(core.execute(me.pool.clone())));
                }

                me.fut.as_mut().unwrap().as_mut().poll(cx)
            }
        }
    }
}
