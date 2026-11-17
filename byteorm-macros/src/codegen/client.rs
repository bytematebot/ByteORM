use crate::codegen::utils::*;
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

struct FieldCategories<'a> {
    numeric_fields: Vec<&'a Field>,
}

impl<'a> FieldCategories<'a> {
    fn from_model(model: &'a Model) -> Self {
        let mut numeric_fields = Vec::new();

        for field in &model.fields {
            if is_numeric_type(&field.type_name) {
                numeric_fields.push(field);
            }
        }

        Self { numeric_fields }
    }
}

fn generate_find_unique(model_name: &proc_macro2::Ident, model: &Model) -> TokenStream {
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
        quote! {}
    } else if pk_fields.len() == 1 {
        let pk = &pk_fields[0];
        let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
        let pk_type = rust_type_from_schema(&pk.type_name, is_nullable);
        quote! {
            pub fn find_unique(&self, id: #pk_type)
                -> impl std::future::Future<Output = Result<Option<#model_name>, Box<dyn std::error::Error + Send + Sync>>>
            {
                #model_name::find_by_id(self.pool().clone(), id)
            }
        }
    } else {
        let pk_params = pk_fields.iter().map(|pk| {
            let name = format_ident!("{}", to_snake_case(&pk.name));
            let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
            let pk_type = rust_type_from_schema(&pk.type_name, is_nullable);
            quote! { #name: #pk_type }
        });

        let pk_args_call = pk_fields.iter().map(|pk| {
            let name = format_ident!("{}", to_snake_case(&pk.name));
            quote! { #name }
        });

        quote! {
            pub fn find_unique(&self, #(#pk_params),*)
                -> impl std::future::Future<Output = Result<Option<#model_name>, Box<dyn std::error::Error + Send + Sync>>>
            {
                #model_name::find_by_composite_pk(self.pool().clone(), #(#pk_args_call),*)
            }
        }
    }
}

fn generate_find_or_create(model_name: &proc_macro2::Ident, model: &Model) -> TokenStream {
    let (pk_names, pk_types, _, _, pk_arg_refs) = pk_args(model);

    if pk_names.is_empty() {
        return quote! {};
    }

    let pk_params = pk_names
        .iter()
        .zip(pk_types.iter())
        .map(|(name, typ)| quote! { #name: #typ });

    quote! {
        pub fn find_or_create(&self, #(#pk_params),*)
            -> impl std::future::Future<Output = Result<#model_name, Box<dyn std::error::Error + Send + Sync>>>
        {
            let pool = self.pool().clone();
            async move {
                __private::find_or_create_by_pk::<#model_name>(&pool, &[#(#pk_arg_refs),*]).await
            }
        }
    }
}

pub fn generate_accessor(model: &Model) -> TokenStream {
    let categories = FieldCategories::from_model(model);
    let model_name = format_ident!("{}", model.name);
    let accessor_struct = format_ident!("{}Accessor", model.name);
    let conflict_field = format_ident!("{}ConflictField", model.name);
    let conflict_selector = format_ident!("{}ConflictSelector", model.name);
    let conflict_target = format_ident!("{}ConflictTarget", model.name);
    let update_builder = format_ident!("{}Update", model.name);
    let create_builder = format_ident!("{}Create", model.name);
    let where_builder = format_ident!("{}WhereBuilder", model.name);
    let find_unique = generate_find_unique(&model_name, model);
    let find_or_create = generate_find_or_create(&model_name, model);

    let conflict_selector_fields = model.fields.iter().map(|field| {
        let field_name = format_ident!("{}", to_snake_case(&field.name));
        quote! { pub #field_name: #conflict_field }
    });

    let conflict_selector_inits = model.fields.iter().map(|field| {
        let field_name = format_ident!("{}", to_snake_case(&field.name));
        let column_name = to_snake_case(&field.name);
        quote! { #field_name: #conflict_field::new(#column_name) }
    });

    let typed_agg_methods = {
        let field_methods = categories.numeric_fields.iter().map(|field| {
            let col_snake = to_snake_case(&field.name);
            let sum_name = format_ident!("sum_{}", col_snake);
            let (ret, call) = match field.type_name.as_str() {
                "Int" | "Serial" => (quote! { i64 }, quote! { self.sum::<F, i64>(f, #col_snake) }),
                "BigInt" => (quote! { i64 }, quote! { self.sum_cast_i64(f, #col_snake) }),
                "Float" => (quote! { f64 }, quote! { self.sum::<F, f64>(f, #col_snake) }),
                "Real" => (quote! { f32 }, quote! { self.sum::<F, f32>(f, #col_snake) }),
                _ => return quote! {},
            };
            quote! {
                pub fn #sum_name<F>(&self, f: F)
                    -> impl std::future::Future<Output = Result<#ret, Box<dyn std::error::Error + Send + Sync>>>
                where
                    F: FnOnce(#where_builder) -> #where_builder,
                {
                    #call
                }
            }
        });
        quote! { #(#field_methods)* }
    };

    let jsonb_fields = crate::codegen::jsonb::generate_jsonb_accessor_fields(model);
    let jsonb_struct_fields: Vec<_> = jsonb_fields
        .iter()
        .map(|(name, typ)| {
            quote! { pub #name: #typ }
        })
        .collect();
    let jsonb_inits: Vec<_> = jsonb_fields
        .iter()
        .map(|(name, typ)| {
            quote! { #name: #typ::new(pool.clone()) }
        })
        .collect();

    quote! {
        pub type #conflict_field = __private::ConflictField<#model_name>;

        // Kept as a named trait so existing signatures still compile; every
        // arity is implemented once in the shared runtime.
        pub trait #conflict_target: __private::ConflictTarget<#model_name> {}
        impl<T: __private::ConflictTarget<#model_name>> #conflict_target for T {}

        #[derive(Clone, Copy)]
        pub struct #conflict_selector {
            #(#conflict_selector_fields),*
        }

        impl #conflict_selector {
            pub fn new() -> Self {
                Self {
                    #(#conflict_selector_inits),*
                }
            }
        }

        #[derive(Clone)]
        pub struct #accessor_struct {
            inner: __private::Accessor<#model_name>,
            #(#jsonb_struct_fields),*
        }

        impl std::ops::Deref for #accessor_struct {
            type Target = __private::Accessor<#model_name>;

            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        impl std::fmt::Debug for #accessor_struct {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!(#accessor_struct))
                    .field("pool", &"<ConnectionPool>")
                    .finish()
            }
        }

        impl #accessor_struct {
            pub fn new(pool: ConnectionPool) -> Self {
                Self { inner: __private::Accessor::new(pool.clone()), #(#jsonb_inits),* }
            }

            #typed_agg_methods

            pub fn upsert_many<T, K, FC, FU>(
                &self,
                records: Vec<T>,
                conflict: impl FnOnce(#conflict_selector) -> K,
                create: FC,
                update: FU,
            ) -> impl std::future::Future<Output = Result<u64, Box<dyn std::error::Error + Send + Sync>>>
            where
                T: Clone,
                K: #conflict_target,
                FC: FnMut(#create_builder, T) -> #create_builder,
                FU: FnMut(#update_builder, T) -> #update_builder,
            {
                self.upsert_many_by(
                    records,
                    move || conflict(#conflict_selector::new()).columns(),
                    create,
                    update,
                )
            }

            #find_unique
            #find_or_create
        }
    }
}
