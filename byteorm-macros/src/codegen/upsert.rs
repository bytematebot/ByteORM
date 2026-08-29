use crate::codegen::utils::{
    generate_create_value_methods, generate_select_columns, is_numeric_type, rust_type_from_schema,
    to_snake_case,
};
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_upsert_builder(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let upsert_builder_name = format_ident!("{}Upsert", model.name);
    let table_name = model.name.to_lowercase();

    let pk_fields: Vec<_> = model
        .fields
        .iter()
        .filter(|f| {
            f.modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
        })
        .collect();

    if pk_fields.is_empty() {
        return quote! {
            pub struct #upsert_builder_name;

            impl #upsert_builder_name {
                pub fn new(_client: Arc<PgClient>) -> Self {
                    Self
                }
            }
        };
    }

    let where_methods = pk_fields.iter().map(|field| {
        let method_name = format_ident!("where_{}", to_snake_case(&field.name));
        let is_nullable = field
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Nullable));
        let field_type = rust_type_from_schema(&field.type_name, is_nullable);
        let field_col = to_snake_case(&field.name);

        quote! {
            pub fn #method_name(mut self, value: #field_type) -> Self {
                self.core.push_value(#field_col, Box::new(value));
                self
            }
        }
    });

    let set_methods = generate_create_value_methods(model, "core");

    // Arithmetic on conflict seeds the inserted row with the operand itself,
    // then applies the operator to the existing row on update.
    let inc_methods = model
        .fields
        .iter()
        .filter(|f| is_numeric_type(&f.type_name))
        .map(|field| {
            let field_col = to_snake_case(&field.name);
            let inc_method = format_ident!("inc_{}", field_col);
            let dec_method = format_ident!("dec_{}", field_col);
            let mul_method = format_ident!("mul_{}", field_col);
            let div_method = format_ident!("div_{}", field_col);
            let (c1, c2, c3, c4) = (
                field_col.clone(),
                field_col.clone(),
                field_col.clone(),
                field_col.clone(),
            );

            quote! {
                pub fn #inc_method(mut self, amount: i64) -> Self {
                    self.core.push_arithmetic(#c1, "inc", amount, Box::new(amount));
                    self
                }
                pub fn #dec_method(mut self, amount: i64) -> Self {
                    self.core.push_arithmetic(#c2, "dec", amount, Box::new(-amount));
                    self
                }
                pub fn #mul_method(mut self, factor: i64) -> Self {
                    self.core.push_arithmetic(#c3, "mul", factor, Box::new(0i64));
                    self
                }
                pub fn #div_method(mut self, divisor: i64) -> Self {
                    self.core.push_arithmetic(#c4, "div", divisor, Box::new(0i64));
                    self
                }
            }
        });

    let select_columns = generate_select_columns(model);

    quote! {
        pub struct #upsert_builder_name {
            core: __private::UpsertCore,
            pool: ConnectionPool,
            fut: Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<tokio_postgres::Row, Box<dyn std::error::Error + Send + Sync>>> + Send>>>,
        }

        impl #upsert_builder_name {
            pub fn new(pool: ConnectionPool) -> Self {
                Self {
                    core: __private::UpsertCore::new(
                        #table_name,
                        #select_columns,
                        <#model_name as crate::ModelMeta>::ENUM_CASTS,
                        <#model_name as crate::ModelMeta>::PK_COLUMNS,
                    ),
                    pool,
                    fut: None,
                }
            }

            #(#where_methods)*
            #(#set_methods)*
            #(#inc_methods)*
        }

        impl std::future::Future for #upsert_builder_name {
            type Output = Result<#model_name, Box<dyn std::error::Error + Send + Sync>>;
            fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
                let me = &mut *self;

                if me.fut.is_none() {
                    let core = std::mem::replace(
                        &mut me.core,
                        __private::UpsertCore::new(
                            #table_name,
                            #select_columns,
                            <#model_name as crate::ModelMeta>::ENUM_CASTS,
                            <#model_name as crate::ModelMeta>::PK_COLUMNS,
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
