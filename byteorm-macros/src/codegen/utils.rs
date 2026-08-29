use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn rust_type_from_schema(type_name: &str, nullable: bool) -> TokenStream {
    let base_type = match type_name {
        "BigInt" => quote! { i64 },
        "Int" => quote! { i32 },
        "String" | "Text" => quote! { String },
        "JsonB" | "Jsonb" => quote! { serde_json::Value },
        "TimestamptZ" | "Timestamp" => quote! { DateTime<Utc> },
        "Date" => quote! { NaiveDate },
        "Boolean" => quote! { bool },
        "Float" => quote! { f64 },
        "Serial" => quote! { i32 },
        "Real" => quote! { f32 },
        _ => {
            let enum_type = format_ident!("{}", type_name);
            quote! { #enum_type }
        }
    };
    if nullable {
        quote! { Option<#base_type> }
    } else {
        base_type
    }
}

pub fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_lowercase().next().unwrap_or(ch));
    }
    result
}

pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

pub fn is_numeric_type(ty: &str) -> bool {
    matches!(ty, "BigInt" | "Int" | "Serial" | "Float" | "Real")
}

pub fn is_builtin_type(ty: &str) -> bool {
    matches!(
        ty,
        "BigInt"
            | "Int"
            | "String"
            | "JsonB"
            | "Jsonb"
            | "TimestamptZ"
            | "Timestamp"
            | "Boolean"
            | "Float"
            | "Serial"
            | "Real"
            | "Text"
            | "Date"
    )
}

/// Wraps a value in the `SqlArg` variant matching the column's type, so
/// ordinary columns skip the box. Nullable columns and unrecognised types
/// keep the boxed fallback.
pub fn sql_arg_expr(type_name: &str, nullable: bool, value: TokenStream) -> TokenStream {
    if nullable {
        return quote! { __private::SqlArg::boxed(#value) };
    }

    match type_name {
        "BigInt" => quote! { __private::SqlArg::I64(#value) },
        "Int" | "Serial" => quote! { __private::SqlArg::I32(#value) },
        "String" | "Text" => quote! { __private::SqlArg::Text(#value) },
        "JsonB" | "Jsonb" => quote! { __private::SqlArg::Json(#value) },
        "TimestamptZ" | "Timestamp" => quote! { __private::SqlArg::Timestamp(#value) },
        "Date" => quote! { __private::SqlArg::Date(#value) },
        "Boolean" => quote! { __private::SqlArg::Bool(#value) },
        "Float" => quote! { __private::SqlArg::F64(#value) },
        "Real" => quote! { __private::SqlArg::F32(#value) },
        _ => quote! { __private::SqlArg::boxed(#value) },
    }
}

pub fn generate_select_columns(model: &Model) -> String {
    model
        .fields
        .iter()
        .map(|field| {
            let col_name = to_snake_case(&field.name);
            if is_builtin_type(&field.type_name) {
                col_name
            } else {
                format!("CAST({} AS TEXT) as {}", col_name, col_name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn generate_from_row_impl(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let field_gets = model.fields.iter().map(|field| {
        let field_name = format_ident!("{}", field.name);
        let col_name = to_snake_case(&field.name);
        quote! { #field_name: row.get(#col_name) }
    });
    quote! {
        impl crate::FromRow for #model_name {
            fn from_row(row: &tokio_postgres::Row) -> Self {
                Self { #(#field_gets),* }
            }
        }
    }
}

/// Emits the model's `ModelMeta` impl: the table, column list and the
/// static tables the shared runtime consults instead of re-deriving them.
pub fn generate_model_meta_impl(model: &Model) -> TokenStream {
    let model_name = format_ident!("{}", model.name);
    let table_name = model.name.to_lowercase();
    let select_columns = generate_select_columns(model);

    let enum_casts: Vec<(String, String)> = model
        .fields
        .iter()
        .filter(|field| !is_builtin_type(&field.type_name))
        .map(|field| (to_snake_case(&field.name), field.type_name.to_lowercase()))
        .collect();

    let required_columns: Vec<String> = model
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

    let pk_columns: Vec<String> = model
        .fields
        .iter()
        .filter(|field| {
            field
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
        })
        .map(|field| to_snake_case(&field.name))
        .collect();

    let columns = sorted_columns(model);
    assert!(
        columns.len() <= 128,
        "model {} has {} columns; the column masks hold at most 128",
        model.name,
        columns.len()
    );

    let column_casts: Vec<TokenStream> = columns
        .iter()
        .map(|column| {
            match enum_casts
                .iter()
                .find(|(name, _)| name == column)
                .map(|(_, cast)| cast)
            {
                Some(cast) => quote! { Some(#cast) },
                None => quote! { None },
            }
        })
        .collect();

    let required_mask = column_mask(&columns, &required_columns);
    let pk_mask = column_mask(&columns, &pk_columns);

    quote! {
        impl crate::ModelMeta for #model_name {
            const TABLE: &'static str = #table_name;
            const SELECT_COLUMNS: &'static str = #select_columns;
            const COLUMNS: &'static [&'static str] = &[#(#columns),*];
            const COLUMN_CASTS: &'static [Option<&'static str>] = &[#(#column_casts),*];
            const REQUIRED_MASK: u128 = #required_mask;
            const PK_MASK: u128 = #pk_mask;
            const PK_COLUMNS: &'static [&'static str] = &[#(#pk_columns),*];
        }
    }
}

/// Every column name, sorted. Builders address a column by its index here,
/// and emitting them in this order keeps insert statements stable without
/// sorting at runtime.
pub fn sorted_columns(model: &Model) -> Vec<String> {
    let mut columns: Vec<String> = model
        .fields
        .iter()
        .map(|field| to_snake_case(&field.name))
        .collect();
    columns.sort();
    columns
}

/// Index of a column in `sorted_columns`.
pub fn column_index(model: &Model, column: &str) -> usize {
    sorted_columns(model)
        .iter()
        .position(|name| name == column)
        .unwrap_or_else(|| panic!("column {} is not part of model {}", column, model.name))
}

fn column_mask(columns: &[String], selected: &[String]) -> u128 {
    selected.iter().fold(0u128, |mask, column| {
        match columns.iter().position(|name| name == column) {
            Some(index) => mask | (1u128 << index),
            None => mask,
        }
    })
}

pub fn pk_args(
    model: &Model,
) -> (
    Vec<proc_macro2::Ident>,
    Vec<TokenStream>,
    Vec<String>,
    Vec<String>,
    Vec<TokenStream>,
) {
    let pk_fields: Vec<_> = model
        .fields
        .iter()
        .filter(|f| {
            f.modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
        })
        .collect();
    let pk_names: Vec<_> = pk_fields
        .iter()
        .map(|pk| format_ident!("{}", to_snake_case(&pk.name)))
        .collect();
    let pk_types: Vec<_> = pk_fields
        .iter()
        .map(|pk| {
            let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
            rust_type_from_schema(&pk.type_name, is_nullable)
        })
        .collect();
    let pk_cols: Vec<_> = pk_fields.iter().map(|pk| to_snake_case(&pk.name)).collect();
    let pk_placeholders: Vec<_> = (1..=pk_fields.len()).map(|i| format!("${}", i)).collect();
    let pk_arg_refs: Vec<_> = pk_fields
        .iter()
        .map(|pk| {
            let name = format_ident!("{}", to_snake_case(&pk.name));
            quote! { &#name }
        })
        .collect();
    (pk_names, pk_types, pk_cols, pk_placeholders, pk_arg_refs)
}

/// Emits `where_*` methods that record predicates on a `__private::Filters`
/// reached through `filters_expr` (e.g. `core.filters()`). The SQL itself is
/// assembled by the shared runtime, not here.
pub fn generate_filter_methods<'a>(
    model: &'a Model,
    filters_expr: &'a str,
) -> impl Iterator<Item = TokenStream> + 'a {
    let filters: TokenStream = filters_expr
        .parse()
        .expect("filters expression must be valid Rust");

    model.fields.iter().flat_map(move |field| {
        let snake = to_snake_case(&field.name);
        let method_name = format_ident!("where_{}", snake);
        let method_in = format_ident!("where_{}_in", snake);
        let is_nullable = field
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Nullable));
        let field_type = rust_type_from_schema(&field.type_name, is_nullable);
        let field_col = snake.clone();
        let filters = filters.clone();
        let arg = sql_arg_expr(&field.type_name, is_nullable, quote! { value });

        let mut methods = vec![
            quote! {
                pub fn #method_name(mut self, value: #field_type) -> Self {
                    self.#filters.push(#field_col, WhereOp::Eq, #arg);
                    self
                }
            },
            quote! {
                pub fn #method_in(mut self, values: Vec<#field_type>) -> Self {
                    let values: Vec<__private::SqlArg> = values
                        .into_iter()
                        .map(|value| #arg)
                        .collect();
                    self.#filters.push_in(#field_col, values);
                    self
                }
            },
        ];

        if is_nullable {
            let method_is_null = format_ident!("where_{}_is_null", snake);
            let method_is_not_null = format_ident!("where_{}_is_not_null", snake);
            let filters_null = filters.clone();
            let filters_not_null = filters.clone();
            methods.push(quote! {
                pub fn #method_is_null(mut self) -> Self {
                    self.#filters_null.push_bare(#field_col, WhereOp::IsNull);
                    self
                }
            });
            methods.push(quote! {
                pub fn #method_is_not_null(mut self) -> Self {
                    self.#filters_not_null.push_bare(#field_col, WhereOp::IsNotNull);
                    self
                }
            });
        }

        if field.type_name == "TimestamptZ" {
            let comparisons = [
                (format_ident!("where_{}_gt", snake), format_ident!("Gt")),
                (format_ident!("where_{}_lt", snake), format_ident!("Lt")),
                (format_ident!("where_{}_gte", snake), format_ident!("Gte")),
                (format_ident!("where_{}_lte", snake), format_ident!("Lte")),
            ];
            methods.extend(comparisons.into_iter().map(|(method, op)| {
                let field_type = field_type.clone();
                let field_col = field_col.clone();
                let filters = filters.clone();
                let arg = arg.clone();
                quote! {
                    pub fn #method(mut self, value: #field_type) -> Self {
                        self.#filters.push(#field_col, WhereOp::#op, #arg);
                        self
                    }
                }
            }));
        }

        methods.into_iter()
    })
}

pub fn generate_create_value_methods<'a>(
    model: &'a Model,
    target: &'a str,
) -> impl Iterator<Item = TokenStream> + 'a {
    let core: TokenStream = if target.is_empty() {
        quote! {}
    } else {
        let path: TokenStream = target.parse().expect("target must be valid Rust");
        quote! { #path. }
    };

    model.fields.iter().map(move |field| {
        let snake = to_snake_case(&field.name);
        let index = column_index(model, &snake);
        let method_name = format_ident!("set_{}", snake);
        let is_nullable = field
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Nullable));
        let field_type = rust_type_from_schema(&field.type_name, is_nullable);
        let core = core.clone();
        let arg = sql_arg_expr(&field.type_name, is_nullable, quote! { value });
        let string_arg = sql_arg_expr(&field.type_name, is_nullable, quote! { value.into() });

        if !is_builtin_type(&field.type_name) {
            quote! {
                pub fn #method_name(mut self, value: #field_type) -> Self {
                    self.#core push_value(#index, __private::SqlArg::Text(value.to_string()));
                    self
                }
            }
        } else if matches!(field.type_name.as_str(), "String" | "Text") {
            quote! {
                pub fn #method_name(mut self, value: impl Into<#field_type>) -> Self {
                    self.#core push_value(#index, #string_arg);
                    self
                }
            }
        } else {
            quote! {
                pub fn #method_name(mut self, value: #field_type) -> Self {
                    self.#core push_value(#index, #arg);
                    self
                }
            }
        }
    })
}

/// Emits `set_*` methods for the update builder, carrying the enum cast that
/// the assignment needs.
pub fn generate_update_set_methods<'a>(
    model: &'a Model,
    target: &'a str,
) -> impl Iterator<Item = TokenStream> + 'a {
    let core: TokenStream = target.parse().expect("target must be valid Rust");

    model.fields.iter().map(move |field| {
        let snake = to_snake_case(&field.name);
        let method_name = format_ident!("set_{}", snake);
        let is_nullable = field
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Nullable));
        let field_type = rust_type_from_schema(&field.type_name, is_nullable);
        let field_col = snake.clone();
        let core = core.clone();
        let arg = sql_arg_expr(&field.type_name, is_nullable, quote! { value });
        let string_arg = sql_arg_expr(&field.type_name, is_nullable, quote! { value.into() });

        if !is_builtin_type(&field.type_name) {
            let cast = field.type_name.to_lowercase();
            quote! {
                pub fn #method_name(mut self, value: #field_type) -> Self {
                    self.#core.push_set(#field_col, Some(#cast), __private::SqlArg::Text(value.to_string()));
                    self
                }
            }
        } else if matches!(field.type_name.as_str(), "String" | "Text") {
            quote! {
                pub fn #method_name(mut self, value: impl Into<#field_type>) -> Self {
                    self.#core.push_set(#field_col, None, #string_arg);
                    self
                }
            }
        } else {
            quote! {
                pub fn #method_name(mut self, value: #field_type) -> Self {
                    self.#core.push_set(#field_col, None, #arg);
                    self
                }
            }
        }
    })
}

/// Emits `inc_*` / `dec_*` / `mul_*` / `div_*` methods for the update builder.
pub fn generate_arithmetic_methods<'a>(
    model: &'a Model,
    target: &'a str,
) -> impl Iterator<Item = TokenStream> + 'a {
    let core: TokenStream = target.parse().expect("target must be valid Rust");

    model
        .fields
        .iter()
        .filter(|f| is_numeric_type(&f.type_name))
        .map(move |field| {
            let field_col = to_snake_case(&field.name);
            let inc_method = format_ident!("inc_{}", field_col);
            let dec_method = format_ident!("dec_{}", field_col);
            let mul_method = format_ident!("mul_{}", field_col);
            let div_method = format_ident!("div_{}", field_col);
            let (c1, c2, c3, c4) = (core.clone(), core.clone(), core.clone(), core.clone());
            let (f1, f2, f3, f4) = (
                field_col.clone(),
                field_col.clone(),
                field_col.clone(),
                field_col.clone(),
            );

            quote! {
                pub fn #inc_method(mut self, amount: i64) -> Self {
                    self.#c1.push_arithmetic(#f1, "inc", amount);
                    self
                }
                pub fn #dec_method(mut self, amount: i64) -> Self {
                    self.#c2.push_arithmetic(#f2, "dec", amount);
                    self
                }
                pub fn #mul_method(mut self, factor: i64) -> Self {
                    self.#c3.push_arithmetic(#f3, "mul", factor);
                    self
                }
                pub fn #div_method(mut self, divisor: i64) -> Self {
                    self.#c4.push_arithmetic(#f4, "div", divisor);
                    self
                }
            }
        })
}
