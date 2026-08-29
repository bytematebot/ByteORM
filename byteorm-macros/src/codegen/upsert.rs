use crate::codegen::utils::{
    column_index, generate_create_value_methods, generate_select_columns, is_numeric_type,
    rust_type_from_schema, sql_arg_expr, to_snake_case,
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
        let index = column_index(model, &field_col);
        let arg = sql_arg_expr(&field.type_name, is_nullable, quote! { value });

        quote! {
            pub fn #method_name(mut self, value: #field_type) -> Self {
                self.core.push_value(#index, #arg);
                self
            }
        }
    });

    let set_methods = generate_create_value_methods(model, "core");

    // Arithmetic on conflict seeds the inserted row with the operand itself,
    // then applies the operator to the existing row on update. The seed has to
    // match the column's own type, not the i64 the API takes.
    let inc_methods = model
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
                    self.core.push_arithmetic(#index, "inc", amount, #inc_seed);
                    self
                }
                pub fn #dec_method(mut self, amount: i64) -> Self {
                    self.core.push_arithmetic(#index, "dec", amount, #dec_seed);
                    self
                }
                pub fn #mul_method(mut self, factor: i64) -> Self {
                    self.core.push_arithmetic(#index, "mul", factor, #mul_seed);
                    self
                }
                pub fn #div_method(mut self, divisor: i64) -> Self {
                    self.core.push_arithmetic(#index, "div", divisor, #div_seed);
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
                        <#model_name as crate::ModelMeta>::COLUMNS,
                        <#model_name as crate::ModelMeta>::COLUMN_CASTS,
                        <#model_name as crate::ModelMeta>::PK_MASK,
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
                            <#model_name as crate::ModelMeta>::COLUMNS,
                            <#model_name as crate::ModelMeta>::COLUMN_CASTS,
                            <#model_name as crate::ModelMeta>::PK_MASK,
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
