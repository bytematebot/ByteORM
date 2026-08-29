use crate::Schema;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashMap;
use std::fs;

pub mod jsonb;
pub mod utils;

pub use jsonb::*;
pub use utils::*;

pub fn generate_rust_code(schema: &Schema) -> HashMap<String, String> {
    let mut files = HashMap::new();
    let mut jsonb_defaults = HashMap::new();
    for model in &schema.models {
        for field in &model.fields {
            if let Some(path) = field.get_jsonb_default_path() {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        jsonb_defaults.insert((model.name.clone(), field.name.clone()), content);
                    }
                    Err(e) => {
                        eprintln!("Warning: Could not read default file '{}': {}", path, e);
                    }
                }
            }
        }
    }

    // Generate Enums
    let enums_code = if !schema.enums.is_empty() {
        let enums = schema.enums.iter().map(|e| {
            let name = term_ident(&e.name);
            let _name_str = &e.name;
            let variants = e.values.iter().map(|v| {
                let v_ident = term_ident(v);
                quote! { #v_ident }
            });
            let display_impl = {
                let match_arms = e.values.iter().map(|v| {
                    let v_ident = term_ident(v);
                    let v_str = v;
                    quote! { Self::#v_ident => write!(f, #v_str) }
                });
                quote! {
                    impl std::fmt::Display for #name {
                        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                            match self {
                                #(#match_arms),*
                            }
                        }
                    }
                }
            };
            let from_str_impl = {
                let match_arms = e.values.iter().map(|v| {
                    let v_str = v;
                    let v_ident = term_ident(v);
                    quote! { #v_str => Ok(Self::#v_ident) }
                });
                let match_arms_vec: Vec<_> = match_arms.collect();
                let all_arms = if match_arms_vec.is_empty() {
                    quote! {
                        _ => Err(format!("Unknown variant: {}", s))
                    }
                } else {
                    let last = &match_arms_vec[match_arms_vec.len() - 1];
                    let rest = &match_arms_vec[..match_arms_vec.len() - 1];
                    quote! {
                        #(#rest,)*
                        #last,
                        _ => Err(format!("Unknown variant: {}", s))
                    }
                };
                quote! {
                    impl std::str::FromStr for #name {
                        type Err = String;
                        fn from_str(s: &str) -> Result<Self, Self::Err> {
                            match s {
                                #all_arms
                            }
                        }
                    }
                }
            };
            let to_sql_impl = quote! {
                impl tokio_postgres::types::ToSql for #name {
                    fn to_sql(&self, ty: &tokio_postgres::types::Type, out: &mut tokio_postgres::types::private::BytesMut) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
                        let s = self.to_string();
                        s.to_sql(ty, out)
                    }

                    fn accepts(ty: &tokio_postgres::types::Type) -> bool {
                        matches!(*ty, tokio_postgres::types::Type::TEXT)
                    }

                    fn to_sql_checked(&self, ty: &tokio_postgres::types::Type, out: &mut tokio_postgres::types::private::BytesMut) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
                        if !Self::accepts(ty) {
                            return Err(format!("unsupported type {:?}", ty).into());
                        }
                        self.to_sql(ty, out)
                    }
                }
            };
            let from_sql_impl = quote! {
                impl<'a> tokio_postgres::types::FromSql<'a> for #name {
                    fn from_sql(ty: &tokio_postgres::types::Type, raw: &'a [u8]) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
                        let s = String::from_sql(ty, raw)?;
                        s.parse().map_err(|e| format!("Failed to parse {}: {}", stringify!(#name), e).into())
                    }

                    fn accepts(ty: &tokio_postgres::types::Type) -> bool {
                        matches!(*ty, tokio_postgres::types::Type::TEXT)
                    }
                }
            };
            quote! {
                #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
                #[allow(non_camel_case_types)]
                pub enum #name {
                    #(#variants),*
                }

                #display_impl
                #from_str_impl
                #to_sql_impl
                #from_sql_impl
            }
        });
        let code = quote! {
            use serde::{Deserialize, Serialize};
            #(#enums)*
        };
        pretty_print(&code)
    } else {
        String::new()
    };
    files.insert("src/enums.rs".to_string(), enums_code);

    // Generate Models
    let mut model_mods = Vec::new();
    for model in &schema.models {
        let model_name_snake_str = to_snake_case(&model.name);
        let model_name_snake = format_ident!("{}", model_name_snake_str);
        model_mods.push(quote! {
            pub mod #model_name_snake;
            pub use #model_name_snake::*;
        });

        let model_code = generate_derive_model(model, &jsonb_defaults);

        files.insert(
            format!("src/models/{}.rs", model_name_snake_str),
            pretty_print(&model_code),
        );
    }

    let models_mod_code = quote! {
        #(#model_mods)*
    };
    files.insert(
        "src/models/mod.rs".to_string(),
        pretty_print(&models_mod_code),
    );

    // Generate lib.rs
    let model_accessors = schema.models.iter().map(|model| {
        let accessor_name = format_ident!("{}", to_snake_case(&model.name));
        let model_name_snake = format_ident!("{}", to_snake_case(&model.name));
        let accessor_struct = format_ident!("{}Accessor", model.name);
        quote! { pub #accessor_name: models::#model_name_snake::#accessor_struct }
    });

    let accessor_inits = schema.models.iter().map(|model| {
        let accessor_name = format_ident!("{}", to_snake_case(&model.name));
        let model_name_snake = format_ident!("{}", to_snake_case(&model.name));
        let accessor_struct = format_ident!("{}Accessor", model.name);
        quote! { #accessor_name: models::#model_name_snake::#accessor_struct::new(pool.clone()) }
    });

    let debug_accessor_fields = schema.models.iter().map(|model| {
        let accessor_name = to_snake_case(&model.name);
        let accessor_name_ident = format_ident!("{}", accessor_name);
        quote! { .field(#accessor_name, &self.#accessor_name_ident) }
    });

    let accessor_inits_clone = schema.models.iter().map(|model| {
        let accessor_name = format_ident!("{}", to_snake_case(&model.name));
        let model_name_snake = format_ident!("{}", to_snake_case(&model.name));
        let accessor_struct = format_ident!("{}Accessor", model.name);
        quote! { #accessor_name: models::#model_name_snake::#accessor_struct::new(pool.clone()) }
    });

    // ON CONFLICT targets are tuples of columns; 64 is far past any real
    // composite key but costs nothing beyond this one generated block.
    let tuple_conflict_impls: Vec<TokenStream> = (2..=64usize)
        .map(|arity| {
            let types = (0..arity).map(|_| quote! { ConflictField<M> });
            let names: Vec<_> = (0..arity).map(|i| format_ident!("f{}", i)).collect();
            quote! {
                impl<M> ConflictTarget<M> for (#(#types),*) {
                    fn columns(self) -> Vec<&'static str> {
                        let (#(#names),*) = self;
                        vec![#(#names.name()),*]
                    }
                }
            }
        })
        .collect();
    let tuple_conflict_impls = quote! { #(#tuple_conflict_impls)* };

    let jsonb_ext = generate_jsonb_ext();

    let lib_code = quote! {
        #![allow(unused_imports)]

        use serde::{Deserialize, Serialize};
        use chrono::{DateTime, Utc};
        use tokio_postgres::{Client as PgClient, NoTls, Error};
        use std::sync::Arc;
        use once_cell::sync::Lazy;
        use std::collections::HashMap;
        use futures_util::task::Context;
        use std::pin::Pin;
        use futures_util::task::Poll;

        pub mod enums;
        pub mod models;
        pub use models::*;
        pub use enums::*;
        pub use tokio_postgres;

        pub trait FromRow {
            fn from_row(row: &tokio_postgres::Row) -> Self;
        }

        /// Everything the shared runtime needs to know about a model. Each
        /// generated model implements this once; the builders are generic
        /// over it, so their bodies exist a single time in the crate.
        pub trait ModelMeta: FromRow + Sized {
            const TABLE: &'static str;
            const SELECT_COLUMNS: &'static str;
            /// Every column, sorted, so a builder can address one by index
            /// and emit them in a stable order without sorting at runtime.
            const COLUMNS: &'static [&'static str];
            /// Enum cast per column, parallel to `COLUMNS`.
            const COLUMN_CASTS: &'static [Option<&'static str>];
            /// Bit per column in `COLUMNS`: set where an insert must provide
            /// a value.
            const REQUIRED_MASK: u128;
            /// Bit per column in `COLUMNS`: set for primary key columns.
            const PK_MASK: u128;
            const PK_COLUMNS: &'static [&'static str];
        }

        #[derive(Debug)]
        pub enum ByteOrmError {
            Pool(String),
            Query(tokio_postgres::Error),
            MissingField(String),
            NotFound(String),
            Validation(String),
            Serialization(String),
        }

        impl std::fmt::Display for ByteOrmError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    ByteOrmError::Pool(msg) => write!(f, "Pool error: {}", msg),
                    ByteOrmError::Query(e) => write!(f, "Query error: {}", e),
                    ByteOrmError::MissingField(field) => write!(f, "Missing required field: {}", field),
                    ByteOrmError::NotFound(msg) => write!(f, "Not found: {}", msg),
                    ByteOrmError::Validation(msg) => write!(f, "Validation error: {}", msg),
                    ByteOrmError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
                }
            }
        }

        impl std::error::Error for ByteOrmError {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                match self {
                    ByteOrmError::Query(e) => Some(e),
                    _ => None,
                }
            }
        }

        impl From<tokio_postgres::Error> for ByteOrmError {
            fn from(e: tokio_postgres::Error) -> Self {
                ByteOrmError::Query(e)
            }
        }

        impl From<serde_json::Error> for ByteOrmError {
            fn from(e: serde_json::Error) -> Self {
                ByteOrmError::Serialization(e.to_string())
            }
        }

        pub fn expect_keys<T: Copy>(
            map: &std::collections::HashMap<String, T>,
            keys: &[&str]
        ) -> Result<Vec<T>, &'static str> {
            keys.iter()
                .map(|k| map.get(*k).copied().ok_or("missing key"))
                .collect()
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum WhereOp {
            Eq,
            Gt,
            Lt,
            Gte,
            Lte,
            In,
            IsNull,
            IsNotNull,
        }

        impl WhereOp {
            pub fn symbol(self) -> &'static str {
                match self {
                    WhereOp::Eq => "=",
                    WhereOp::Gt => ">",
                    WhereOp::Lt => "<",
                    WhereOp::Gte => ">=",
                    WhereOp::Lte => "<=",
                    WhereOp::In => "IN",
                    WhereOp::IsNull => "IS NULL",
                    WhereOp::IsNotNull => "IS NOT NULL",
                }
            }
        }

        /// A single WHERE condition. `args` indexes the builder's argument
        /// vector, so SQL text and placeholder numbers are produced only by
        /// `render_where_predicates`, never by the builder methods.
        #[derive(Debug, Clone)]
        pub struct WherePredicate {
            pub column: &'static str,
            pub op: WhereOp,
            pub args: std::ops::Range<usize>,
        }

        impl WherePredicate {
            pub fn new(column: &'static str, op: WhereOp, args: std::ops::Range<usize>) -> Self {
                Self { column, op, args }
            }
        }

        /// Renders predicates into SQL, numbering placeholders from
        /// `next_param`. Returns the clauses and the next free placeholder
        /// number, so callers that emit parameters before the WHERE clause
        /// (UPDATE ... SET) can pass their own offset in.
        pub fn render_where_predicates(
            predicates: &[WherePredicate],
            next_param: usize,
        ) -> (Vec<String>, usize) {
            let mut next_param = next_param;
            let mut clauses = Vec::with_capacity(predicates.len());
            for predicate in predicates {
                let count = predicate.args.len();
                let clause = match predicate.op {
                    WhereOp::IsNull | WhereOp::IsNotNull => {
                        format!("{} {}", predicate.column, predicate.op.symbol())
                    }
                    WhereOp::In => {
                        if count == 0 {
                            // `IN ()` is not valid SQL; an empty set matches nothing.
                            "1 = 0".to_string()
                        } else {
                            let placeholders: Vec<String> = (next_param..next_param + count)
                                .map(|i| format!("${}", i))
                                .collect();
                            next_param += count;
                            format!("{} IN ({})", predicate.column, placeholders.join(", "))
                        }
                    }
                    op => {
                        let clause = format!("{} {} ${}", predicate.column, op.symbol(), next_param);
                        next_param += count;
                        clause
                    }
                };
                clauses.push(clause);
            }
            (clauses, next_param)
        }

        /// Shared statement machinery. Generated builders are thin wrappers
        /// over these types, so the SQL assembly and execution code is
        /// compiled once for the whole crate rather than once per model.
        #[doc(hidden)]
        pub mod __private {
            use super::{ConnectionPool, FromRow, JsonbExt, ModelMeta, WhereOp, WherePredicate, debug, render_where_predicates};

            /// A bound parameter. Common column types are stored inline;
            /// anything else (nullable columns, `where_raw`, enums the
            /// generator does not recognise) falls back to a box.
            #[derive(Debug)]
            pub enum SqlArg {
                I32(i32),
                I64(i64),
                F32(f32),
                F64(f64),
                Bool(bool),
                Text(String),
                Json(serde_json::Value),
                Timestamp(chrono::DateTime<chrono::Utc>),
                Date(chrono::NaiveDate),
                Boxed(Box<dyn tokio_postgres::types::ToSql + Sync + Send>),
            }

            impl SqlArg {
                pub fn boxed<T>(value: T) -> Self
                where
                    T: tokio_postgres::types::ToSql + Sync + Send + 'static,
                {
                    SqlArg::Boxed(Box::new(value))
                }
            }

            impl tokio_postgres::types::ToSql for SqlArg {
                fn to_sql(
                    &self,
                    ty: &tokio_postgres::types::Type,
                    out: &mut bytes::BytesMut,
                ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
                {
                    match self {
                        SqlArg::I32(value) => value.to_sql(ty, out),
                        SqlArg::I64(value) => value.to_sql(ty, out),
                        SqlArg::F32(value) => value.to_sql(ty, out),
                        SqlArg::F64(value) => value.to_sql(ty, out),
                        SqlArg::Bool(value) => value.to_sql(ty, out),
                        SqlArg::Text(value) => value.to_sql(ty, out),
                        SqlArg::Json(value) => value.to_sql(ty, out),
                        SqlArg::Timestamp(value) => value.to_sql(ty, out),
                        SqlArg::Date(value) => value.to_sql(ty, out),
                        SqlArg::Boxed(value) => value.to_sql_checked(ty, out),
                    }
                }

                fn accepts(_ty: &tokio_postgres::types::Type) -> bool {
                    // The variant decides; `to_sql_checked` does the real check.
                    true
                }

                fn to_sql_checked(
                    &self,
                    ty: &tokio_postgres::types::Type,
                    out: &mut bytes::BytesMut,
                ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
                {
                    match self {
                        SqlArg::I32(value) => value.to_sql_checked(ty, out),
                        SqlArg::I64(value) => value.to_sql_checked(ty, out),
                        SqlArg::F32(value) => value.to_sql_checked(ty, out),
                        SqlArg::F64(value) => value.to_sql_checked(ty, out),
                        SqlArg::Bool(value) => value.to_sql_checked(ty, out),
                        SqlArg::Text(value) => value.to_sql_checked(ty, out),
                        SqlArg::Json(value) => value.to_sql_checked(ty, out),
                        SqlArg::Timestamp(value) => value.to_sql_checked(ty, out),
                        SqlArg::Date(value) => value.to_sql_checked(ty, out),
                        SqlArg::Boxed(value) => value.to_sql_checked(ty, out),
                    }
                }
            }

            impl From<i32> for SqlArg {
                fn from(value: i32) -> Self {
                    SqlArg::I32(value)
                }
            }

            impl From<i64> for SqlArg {
                fn from(value: i64) -> Self {
                    SqlArg::I64(value)
                }
            }

            impl From<f32> for SqlArg {
                fn from(value: f32) -> Self {
                    SqlArg::F32(value)
                }
            }

            impl From<f64> for SqlArg {
                fn from(value: f64) -> Self {
                    SqlArg::F64(value)
                }
            }

            impl From<bool> for SqlArg {
                fn from(value: bool) -> Self {
                    SqlArg::Bool(value)
                }
            }

            impl From<String> for SqlArg {
                fn from(value: String) -> Self {
                    SqlArg::Text(value)
                }
            }

            impl From<serde_json::Value> for SqlArg {
                fn from(value: serde_json::Value) -> Self {
                    SqlArg::Json(value)
                }
            }

            impl From<chrono::DateTime<chrono::Utc>> for SqlArg {
                fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
                    SqlArg::Timestamp(value)
                }
            }

            impl From<chrono::NaiveDate> for SqlArg {
                fn from(value: chrono::NaiveDate) -> Self {
                    SqlArg::Date(value)
                }
            }

            impl<T> From<Box<T>> for SqlArg
            where
                T: tokio_postgres::types::ToSql + Sync + Send + 'static,
            {
                fn from(value: Box<T>) -> Self {
                    SqlArg::Boxed(value)
                }
            }
            pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

            /// One WHERE condition: either a structured predicate or a raw
            /// clause supplied by the caller.
            enum FilterEntry {
                Predicate(WherePredicate),
                Raw { clause: String, args: std::ops::Range<usize> },
            }

            /// Rewrites `$1..$n` in a caller-supplied clause so they continue
            /// from `offset` instead of restarting at one.
            fn shift_placeholders(clause: &str, offset: usize) -> String {
                let mut out = String::with_capacity(clause.len());
                let mut chars = clause.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch != '$' {
                        out.push(ch);
                        continue;
                    }
                    let mut digits = String::new();
                    while let Some(digit) = chars.peek() {
                        if digit.is_ascii_digit() {
                            digits.push(*digit);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    match digits.parse::<usize>() {
                        Ok(n) if n > 0 => out.push_str(&format!("${}", offset + n - 1)),
                        _ => {
                            out.push('$');
                            out.push_str(&digits);
                        }
                    }
                }
                out
            }

            /// Collects WHERE conditions and their bound arguments.
            pub struct Filters {
                entries: Vec<FilterEntry>,
                args: Vec<SqlArg>,
            }

            impl Filters {
                pub fn new() -> Self {
                    Self { entries: vec![], args: vec![] }
                }

                pub fn is_empty(&self) -> bool {
                    self.entries.is_empty()
                }

                /// Placeholder number the next argument would take.
                pub fn next_placeholder(&self) -> usize {
                    self.args.len() + 1
                }

                /// Records a predicate binding one argument.
                pub fn push(&mut self, column: &'static str, op: WhereOp, value: SqlArg) {
                    let start = self.args.len();
                    self.args.push(value);
                    self.entries.push(FilterEntry::Predicate(
                        WherePredicate::new(column, op, start..self.args.len())
                    ));
                }

                /// Records a predicate binding no arguments (IS NULL, IS NOT NULL).
                pub fn push_bare(&mut self, column: &'static str, op: WhereOp) {
                    let at = self.args.len();
                    self.entries.push(FilterEntry::Predicate(
                        WherePredicate::new(column, op, at..at)
                    ));
                }

                /// Records an IN predicate over any number of arguments.
                pub fn push_in(&mut self, column: &'static str, values: Vec<SqlArg>) {
                    let start = self.args.len();
                    self.args.extend(values);
                    self.entries.push(FilterEntry::Predicate(
                        WherePredicate::new(column, WhereOp::In, start..self.args.len())
                    ));
                }

                /// Records a raw clause written against its own `$1..$n`.
                pub fn push_raw(&mut self, clause: String, values: Vec<SqlArg>) {
                    let start = self.args.len();
                    self.args.extend(values);
                    self.entries.push(FilterEntry::Raw { clause, args: start..self.args.len() });
                }

                /// Appends " WHERE ..." to `sql` and moves the arguments into
                /// `params`, numbering placeholders from `params.len() + 1`.
                pub fn append_to(&mut self, sql: &mut String, params: &mut Vec<SqlArg>) {
                    if self.entries.is_empty() {
                        return;
                    }

                    let mut next = params.len() + 1;
                    let mut clauses: Vec<String> = Vec::with_capacity(self.entries.len());
                    for entry in self.entries.drain(..) {
                        match entry {
                            FilterEntry::Predicate(predicate) => {
                                let (rendered, after) =
                                    render_where_predicates(std::slice::from_ref(&predicate), next);
                                next = after;
                                clauses.extend(rendered);
                            }
                            FilterEntry::Raw { clause, args } => {
                                clauses.push(shift_placeholders(&clause, next));
                                next += args.len();
                            }
                        }
                    }

                    sql.push_str(" WHERE ");
                    sql.push_str(&clauses.join(" AND "));
                    params.append(&mut self.args);
                }
            }

            /// Borrows owned arguments as the reference slice tokio-postgres wants.
            pub fn as_sql_refs(params: &[SqlArg]) -> Vec<&(dyn tokio_postgres::types::ToSql + Sync)> {
                params
                    .iter()
                    .map(|param| param as &(dyn tokio_postgres::types::ToSql + Sync))
                    .collect()
            }

            /// A column assignment: the column, an optional enum cast, and
            /// where its bound argument sits in `set_args`.
            pub struct Assignment {
                pub column: &'static str,
                pub cast: Option<&'static str>,
            }

            /// An in-place arithmetic update such as `amount = amount + $1`.
            pub struct Arithmetic {
                pub column: &'static str,
                pub op: &'static str,
                pub value: i64,
            }

            pub struct UpdateCore {
                table: &'static str,
                select_columns: &'static str,
                assignments: Vec<Assignment>,
                set_args: Vec<SqlArg>,
                arithmetic: Vec<Arithmetic>,
                filters: Filters,
                allow_all_rows: bool,
            }

            impl UpdateCore {
                pub fn new(table: &'static str, select_columns: &'static str) -> Self {
                    Self {
                        table,
                        select_columns,
                        assignments: vec![],
                        set_args: vec![],
                        arithmetic: vec![],
                        filters: Filters::new(),
                        allow_all_rows: false,
                    }
                }

                pub fn filters(&mut self) -> &mut Filters {
                    &mut self.filters
                }

                pub fn push_set(&mut self, column: &'static str, cast: Option<&'static str>, value: SqlArg) {
                    self.assignments.push(Assignment { column, cast });
                    self.set_args.push(value);
                }

                pub fn push_arithmetic(&mut self, column: &'static str, op: &'static str, value: i64) {
                    self.arithmetic.push(Arithmetic { column, op, value });
                }

                pub fn allow_all_rows(&mut self) {
                    self.allow_all_rows = true;
                }

                pub fn has_changes(&self) -> bool {
                    !self.assignments.is_empty() || !self.arithmetic.is_empty()
                }

                /// Consumes the core into the pieces `upsert_many` needs.
                pub fn into_parts(self) -> (Vec<Assignment>, Vec<SqlArg>, Vec<Arithmetic>, Filters) {
                    (self.assignments, self.set_args, self.arithmetic, self.filters)
                }

                pub fn build(&mut self) -> Result<(String, Vec<SqlArg>), BoxError> {
                    if !self.has_changes() {
                        return Err("No fields to update".into());
                    }
                    if self.filters.is_empty() && !self.allow_all_rows {
                        return Err("UPDATE without WHERE clause is not allowed; call allow_all_rows() to update every row".into());
                    }

                    let mut clauses: Vec<String> = Vec::with_capacity(
                        self.assignments.len() + self.arithmetic.len()
                    );
                    let mut params: Vec<SqlArg> = vec![];

                    for (assignment, value) in self.assignments.drain(..).zip(self.set_args.drain(..)) {
                        let placeholder = params.len() + 1;
                        match assignment.cast {
                            Some(cast) => clauses.push(format!(
                                "{} = ${}::TEXT::{}",
                                assignment.column, placeholder, cast
                            )),
                            None => clauses.push(format!("{} = ${}", assignment.column, placeholder)),
                        }
                        params.push(value);
                    }

                    for arithmetic in self.arithmetic.drain(..) {
                        let symbol = match arithmetic.op {
                            "inc" => "+",
                            "dec" => "-",
                            "mul" => "*",
                            "div" => "/",
                            _ => continue,
                        };
                        // The operand is an i64 from the builder API, never
                        // caller text, and inlining it keeps the column's own
                        // type from clashing with a bound i64.
                        clauses.push(format!(
                            "{} = {} {} {}",
                            arithmetic.column, arithmetic.column, symbol, arithmetic.value
                        ));
                    }

                    let mut sql = format!("UPDATE {} SET {}", self.table, clauses.join(", "));
                    self.filters.append_to(&mut sql, &mut params);
                    sql.push_str(&format!(" RETURNING {}", self.select_columns));
                    Ok((sql, params))
                }

                pub async fn execute(mut self, pool: ConnectionPool)
                    -> Result<Vec<tokio_postgres::Row>, BoxError>
                {
                    let (sql, params) = self.build()?;
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    Ok(client.query(&sql, &refs[..]).await?)
                }
            }

            /// Column values addressed by their index in `ModelMeta::COLUMNS`,
            /// which keeps insert building free of hashing and sorting.
            pub struct ColumnValues {
                columns: &'static [&'static str],
                casts: &'static [Option<&'static str>],
                values: Vec<Option<SqlArg>>,
                present: u128,
            }

            impl ColumnValues {
                pub fn new(
                    columns: &'static [&'static str],
                    casts: &'static [Option<&'static str>],
                ) -> Self {
                    let mut values = Vec::with_capacity(columns.len());
                    values.resize_with(columns.len(), || None);
                    Self { columns, casts, values, present: 0 }
                }

                pub fn set(&mut self, index: usize, value: SqlArg) {
                    self.values[index] = Some(value);
                    self.present |= 1u128 << index;
                }

                pub fn present(&self) -> u128 {
                    self.present
                }

                pub fn is_empty(&self) -> bool {
                    self.present == 0
                }

                pub fn contains(&self, index: usize) -> bool {
                    self.present & (1u128 << index) != 0
                }

                pub fn column(&self, index: usize) -> &'static str {
                    self.columns[index]
                }

                /// Names the first column required by `mask` but not set.
                pub fn first_missing(&self, mask: u128) -> Option<&'static str> {
                    let missing = mask & !self.present;
                    if missing == 0 {
                        return None;
                    }
                    Some(self.columns[missing.trailing_zeros() as usize])
                }

                /// Emits `$n` placeholders and moves the values into `params`,
                /// walking the columns in their sorted order.
                pub fn take_into(
                    &mut self,
                    params: &mut Vec<SqlArg>,
                ) -> (Vec<&'static str>, Vec<String>) {
                    let count = self.present.count_ones() as usize;
                    let mut columns = Vec::with_capacity(count);
                    let mut placeholders = Vec::with_capacity(count);

                    for index in 0..self.columns.len() {
                        let Some(value) = self.values[index].take() else {
                            continue;
                        };
                        let placeholder = params.len() + 1;
                        match self.casts[index] {
                            Some(cast) => {
                                placeholders.push(format!("${}::TEXT::{}", placeholder, cast))
                            }
                            None => placeholders.push(format!("${}", placeholder)),
                        }
                        columns.push(self.columns[index]);
                        params.push(value);
                    }

                    self.present = 0;
                    (columns, placeholders)
                }
            }

            pub struct CreateCore {
                table: &'static str,
                select_columns: &'static str,
                required_mask: u128,
                values: ColumnValues,
                filters: Filters,
            }

            impl CreateCore {
                pub fn new(
                    table: &'static str,
                    select_columns: &'static str,
                    columns: &'static [&'static str],
                    casts: &'static [Option<&'static str>],
                    required_mask: u128,
                ) -> Self {
                    Self {
                        table,
                        select_columns,
                        required_mask,
                        values: ColumnValues::new(columns, casts),
                        filters: Filters::new(),
                    }
                }

                pub fn filters(&mut self) -> &mut Filters {
                    &mut self.filters
                }

                pub fn push_value(&mut self, index: usize, value: SqlArg) {
                    self.values.set(index, value);
                }

                /// Consumes the core into the pieces `upsert_many` needs.
                pub fn into_parts(self) -> (ColumnValues, Filters) {
                    (self.values, self.filters)
                }

                pub fn check_required(&self) -> Result<(), BoxError> {
                    if let Some(column) = self.values.first_missing(self.required_mask) {
                        return Err(format!("Missing required field: {}", column).into());
                    }
                    if self.values.is_empty() && self.required_mask != 0 {
                        return Err("No fields to create".into());
                    }
                    Ok(())
                }

                pub fn build(&mut self) -> Result<(String, Vec<SqlArg>), BoxError> {
                    self.check_required()?;

                    let mut params: Vec<SqlArg> = vec![];
                    let (columns, placeholders) = self.values.take_into(&mut params);

                    let sql = format!(
                        "INSERT INTO {} ({}) VALUES ({}) RETURNING {}",
                        self.table,
                        columns.join(", "),
                        placeholders.join(", "),
                        self.select_columns
                    );
                    Ok((sql, params))
                }

                /// Runs the pre-insert uniqueness check the builder's WHERE
                /// clause asks for, then the insert itself.
                pub async fn execute(mut self, pool: ConnectionPool)
                    -> Result<tokio_postgres::Row, BoxError>
                {
                    self.check_required()?;

                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;

                    if !self.filters.is_empty() {
                        let mut sql = format!("SELECT COUNT(*) FROM {}", self.table);
                        let mut params: Vec<SqlArg> = vec![];
                        self.filters.append_to(&mut sql, &mut params);
                        debug::log_query(&sql, params.len());
                        let refs = as_sql_refs(&params);
                        let row = client.query_one(&sql, &refs[..]).await?;
                        let count: i64 = row.get(0);
                        if count > 0 {
                            return Err("Record already exists".into());
                        }
                    }

                    let (sql, params) = self.build()?;
                    debug::log_query(&sql, params.len());
                    let refs = as_sql_refs(&params);
                    Ok(client.query_one(&sql, &refs[..]).await?)
                }
            }

            pub async fn insert_many(
                pool: ConnectionPool,
                table: &'static str,
                records: Vec<std::collections::HashMap<
                    &'static str,
                    Box<dyn tokio_postgres::types::ToSql + Sync + Send>,
                >>,
            ) -> Result<u64, BoxError> {
                // `create_many` takes boxed values as part of its public
                // signature; they ride along as the boxed SqlArg variant.
                let records: Vec<std::collections::HashMap<&'static str, SqlArg>> = records
                    .into_iter()
                    .map(|record| {
                        record
                            .into_iter()
                            .map(|(column, value)| (column, SqlArg::Boxed(value)))
                            .collect()
                    })
                    .collect();

                if records.is_empty() {
                    return Ok(0);
                }

                let mut columns: Vec<&'static str> = records[0].keys().copied().collect();
                columns.sort();
                if columns.is_empty() {
                    return Err("Cannot create records without any columns".into());
                }

                let mut tuples: Vec<String> = Vec::with_capacity(records.len());
                let mut params: Vec<SqlArg> = Vec::new();

                for mut record in records {
                    let mut placeholders: Vec<String> = Vec::with_capacity(columns.len());
                    for column in &columns {
                        let value = record.remove(*column).ok_or_else(|| {
                            format!("Record is missing column `{}` present in the first record", column)
                        })?;
                        placeholders.push(format!("${}", params.len() + 1));
                        params.push(value);
                    }
                    if let Some(extra) = record.keys().next() {
                        return Err(format!(
                            "Record has column `{}` that is not present in the first record",
                            extra
                        )
                        .into());
                    }
                    tuples.push(format!("({})", placeholders.join(", ")));
                }

                let sql = format!(
                    "INSERT INTO {} ({}) VALUES {}",
                    table,
                    columns.join(", "),
                    tuples.join(", ")
                );

                debug::log_query(&sql, params.len());
                let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                let refs = as_sql_refs(&params);
                Ok(client.execute(&sql, &refs[..]).await?)
            }

            /// Accumulates the rows of an `INSERT ... ON CONFLICT` batch.
            /// Every record must set the same columns, which is what makes a
            /// single multi-row statement possible.
            pub struct UpsertBatch {
                table: &'static str,
                conflict_columns: Vec<&'static str>,
                conflict_mask: u128,
                insert_mask: Option<u128>,
                update_mask: Option<u128>,
                insert_columns: Vec<&'static str>,
                update_columns: Vec<&'static str>,
                tuples: Vec<String>,
                params: Vec<SqlArg>,
            }

            impl UpsertBatch {
                pub fn new(
                    table: &'static str,
                    columns: &'static [&'static str],
                    conflict_columns: Vec<&'static str>,
                ) -> Result<Self, BoxError> {
                    if conflict_columns.is_empty() {
                        return Err("Conflict target cannot be empty".into());
                    }
                    let mut seen = std::collections::HashSet::new();
                    let mut conflict_mask = 0u128;
                    for column in &conflict_columns {
                        if !seen.insert(*column) {
                            return Err(format!("Duplicate conflict column: {}", column).into());
                        }
                        match columns.iter().position(|name| name == column) {
                            Some(index) => conflict_mask |= 1u128 << index,
                            None => return Err(format!("Unknown conflict column: {}", column).into()),
                        }
                    }
                    Ok(Self {
                        table,
                        conflict_columns,
                        conflict_mask,
                        insert_mask: None,
                        update_mask: None,
                        insert_columns: vec![],
                        update_columns: vec![],
                        tuples: vec![],
                        params: vec![],
                    })
                }

                pub fn push(&mut self, create: CreateCore, update: UpdateCore) -> Result<(), BoxError> {
                    create.check_required()?;
                    let (mut values, create_filters) = create.into_parts();
                    let (assignments, set_args, arithmetic, update_filters) = update.into_parts();

                    if !create_filters.is_empty() {
                        return Err("upsert_many does not support create where clauses".into());
                    }
                    if !update_filters.is_empty() {
                        return Err("upsert_many does not support update where clauses".into());
                    }
                    if !arithmetic.is_empty() {
                        return Err("upsert_many does not support increment operations".into());
                    }
                    if assignments.len() != set_args.len() {
                        return Err("Invalid update builder state".into());
                    }
                    if values.is_empty() {
                        return Err("No fields to upsert".into());
                    }

                    let present = values.present();
                    if let Some(column) = values.first_missing(self.conflict_mask) {
                        return Err(
                            format!("Missing conflict field in create builder: {}", column).into()
                        );
                    }

                    match self.insert_mask {
                        Some(expected) if expected != present => {
                            return Err("upsert_many requires the same create columns for every record".into());
                        }
                        Some(_) => {}
                        None => self.insert_mask = Some(present),
                    }

                    let mut seen = std::collections::HashSet::new();
                    let mut update_mask = 0u128;
                    for assignment in assignments {
                        if !seen.insert(assignment.column) {
                            return Err(format!(
                                "Duplicate update field in upsert_many: {}",
                                assignment.column
                            )
                            .into());
                        }
                        match (0..u128::BITS as usize).find(|index| {
                            present & (1u128 << index) != 0
                                && values.column(*index) == assignment.column
                        }) {
                            Some(index) => update_mask |= 1u128 << index,
                            None => {
                                return Err(format!(
                                    "Update field '{}' must also be set in create builder",
                                    assignment.column
                                )
                                .into())
                            }
                        }
                    }

                    match self.update_mask {
                        Some(expected) if expected != update_mask => {
                            return Err("upsert_many requires the same update columns for every record".into());
                        }
                        Some(_) => {}
                        None => {
                            self.update_mask = Some(update_mask);
                            self.update_columns = (0..u128::BITS as usize)
                                .filter(|index| update_mask & (1u128 << index) != 0)
                                .map(|index| values.column(index))
                                .collect();
                        }
                    }

                    let (columns, placeholders) = values.take_into(&mut self.params);
                    if self.insert_columns.is_empty() {
                        self.insert_columns = columns;
                    }
                    self.tuples.push(format!("({})", placeholders.join(", ")));
                    Ok(())
                }

                pub async fn execute(self, pool: ConnectionPool) -> Result<u64, BoxError> {
                    if self.insert_columns.is_empty() {
                        return Err("No fields to upsert".into());
                    }
                    let conflict_clause = self.conflict_columns.join(", ");

                    let sql = if self.update_columns.is_empty() {
                        format!(
                            "INSERT INTO {} ({}) VALUES {} ON CONFLICT ({}) DO NOTHING",
                            self.table,
                            self.insert_columns.join(", "),
                            self.tuples.join(", "),
                            conflict_clause
                        )
                    } else {
                        let assignments: Vec<String> = self
                            .update_columns
                            .iter()
                            .map(|column| format!("{} = EXCLUDED.{}", column, column))
                            .collect();
                        format!(
                            "INSERT INTO {} ({}) VALUES {} ON CONFLICT ({}) DO UPDATE SET {}",
                            self.table,
                            self.insert_columns.join(", "),
                            self.tuples.join(", "),
                            conflict_clause,
                            assignments.join(", ")
                        )
                    };

                    debug::log_query(&sql, self.params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&self.params);
                    Ok(client.execute(&sql, &refs[..]).await?)
                }
            }

            pub struct UpsertCore {
                table: &'static str,
                select_columns: &'static str,
                pk_mask: u128,
                values: ColumnValues,
                arithmetic: Vec<(usize, &'static str, i64)>,
            }

            impl UpsertCore {
                pub fn new(
                    table: &'static str,
                    select_columns: &'static str,
                    columns: &'static [&'static str],
                    casts: &'static [Option<&'static str>],
                    pk_mask: u128,
                ) -> Self {
                    Self {
                        table,
                        select_columns,
                        pk_mask,
                        values: ColumnValues::new(columns, casts),
                        arithmetic: vec![],
                    }
                }

                pub fn push_value(&mut self, index: usize, value: SqlArg) {
                    self.values.set(index, value);
                }

                /// The inserted row carries the operand; the operator is
                /// applied to the existing row on conflict.
                pub fn push_arithmetic(&mut self, index: usize, op: &'static str, value: i64, seed: SqlArg) {
                    self.arithmetic.push((index, op, value));
                    self.values.set(index, seed);
                }

                pub fn build(&mut self) -> Result<(String, Vec<SqlArg>), BoxError> {
                    if let Some(column) = self.values.first_missing(self.pk_mask) {
                        return Err(format!("Missing primary key field: {}", column).into());
                    }
                    if self.values.is_empty() {
                        return Err("No fields to upsert".into());
                    }

                    let pk_columns: Vec<&'static str> = (0..u128::BITS as usize)
                        .filter(|index| self.pk_mask & (1u128 << index) != 0)
                        .map(|index| self.values.column(index))
                        .collect();

                    let arithmetic: Vec<(&'static str, &'static str, i64)> = self
                        .arithmetic
                        .drain(..)
                        .map(|(index, op, value)| (self.values.column(index), op, value))
                        .collect();

                    let mut params: Vec<SqlArg> = vec![];
                    let (columns, placeholders) = self.values.take_into(&mut params);

                    let update_columns: Vec<&'static str> = columns
                        .iter()
                        .copied()
                        .filter(|column| !pk_columns.contains(column))
                        .collect();

                    let conflict_clause = pk_columns.join(", ");
                    let sql = if update_columns.is_empty() && arithmetic.is_empty() {
                        format!(
                            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT ({}) DO NOTHING RETURNING {}",
                            self.table,
                            columns.join(", "),
                            placeholders.join(", "),
                            conflict_clause,
                            self.select_columns
                        )
                    } else {
                        let assignments: Vec<String> = update_columns
                            .iter()
                            .map(|column| {
                                match arithmetic.iter().find(|(name, _, _)| name == column) {
                                    Some((_, op, value)) => {
                                        let symbol = match *op {
                                            "inc" => "+",
                                            "dec" => "-",
                                            "mul" => "*",
                                            "div" => "/",
                                            _ => return format!("{} = EXCLUDED.{}", column, column),
                                        };
                                        let value = if *op == "dec" { value.abs() } else { *value };
                                        format!(
                                            "{} = COALESCE({}.{}, 0) {} {}",
                                            column, self.table, column, symbol, value
                                        )
                                    }
                                    None => format!("{} = EXCLUDED.{}", column, column),
                                }
                            })
                            .collect();

                        format!(
                            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT ({}) DO UPDATE SET {} RETURNING {}",
                            self.table,
                            columns.join(", "),
                            placeholders.join(", "),
                            conflict_clause,
                            assignments.join(", "),
                            self.select_columns
                        )
                    };

                    Ok((sql, params))
                }

                pub async fn execute(mut self, pool: ConnectionPool)
                    -> Result<tokio_postgres::Row, BoxError>
                {
                    let (sql, params) = self.build()?;
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    Ok(client.query_one(&sql, &refs[..]).await?)
                }
            }

            pub struct QueryCore {
                table: &'static str,
                select_columns: &'static str,
                filters: Filters,
                order_by: Vec<(String, &'static str)>,
                limit: Option<usize>,
                offset: Option<usize>,
                includes: Vec<String>,
            }

            impl QueryCore {
                pub fn new(table: &'static str, select_columns: &'static str) -> Self {
                    Self {
                        table,
                        select_columns,
                        filters: Filters::new(),
                        order_by: vec![],
                        limit: None,
                        offset: None,
                        includes: vec![],
                    }
                }

                pub fn filters(&mut self) -> &mut Filters {
                    &mut self.filters
                }

                pub fn next_placeholder(&self) -> usize {
                    self.filters.next_placeholder()
                }

                pub fn push_order(&mut self, expression: String, direction: &'static str) {
                    self.order_by.push((expression, direction));
                }

                pub fn set_limit(&mut self, limit: usize) {
                    self.limit = Some(limit);
                }

                pub fn set_offset(&mut self, offset: usize) {
                    self.offset = Some(offset);
                }

                pub fn push_include(&mut self, subquery: String) {
                    self.includes.push(subquery);
                }

                fn append_tail(&mut self, sql: &mut String) {
                    if !self.order_by.is_empty() {
                        let clauses: Vec<String> = self
                            .order_by
                            .iter()
                            .map(|(expression, direction)| format!("{} {}", expression, direction))
                            .collect();
                        sql.push_str(" ORDER BY ");
                        sql.push_str(&clauses.join(", "));
                    }
                    if let Some(limit) = self.limit {
                        sql.push_str(&format!(" LIMIT {}", limit));
                    }
                    if let Some(offset) = self.offset {
                        sql.push_str(&format!(" OFFSET {}", offset));
                    }
                }

                pub fn build_select(&mut self) -> (String, Vec<SqlArg>) {
                    let mut sql = format!("SELECT {} FROM {}", self.select_columns, self.table);
                    let mut params: Vec<SqlArg> = vec![];
                    self.filters.append_to(&mut sql, &mut params);
                    self.append_tail(&mut sql);
                    (sql, params)
                }

                /// Builds `SELECT <expression> FROM <table> [WHERE ...]` for
                /// counts and aggregates, which ignore ordering and paging.
                pub fn build_scalar(&mut self, expression: &str) -> (String, Vec<SqlArg>) {
                    let mut sql = format!("SELECT {} FROM {}", expression, self.table);
                    let mut params: Vec<SqlArg> = vec![];
                    self.filters.append_to(&mut sql, &mut params);
                    (sql, params)
                }

                pub async fn fetch(mut self, pool: ConnectionPool)
                    -> Result<Vec<tokio_postgres::Row>, BoxError>
                {
                    let (sql, params) = self.build_select();
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    Ok(client.query(&sql, &refs[..]).await?)
                }

                /// Wraps the select in `row_to_json`, adding any included
                /// relations as sub-selects.
                pub async fn fetch_json(mut self, pool: ConnectionPool)
                    -> Result<Vec<serde_json::Value>, BoxError>
                {
                    let includes = std::mem::take(&mut self.includes);
                    let mut inner = format!("SELECT * FROM {}", self.table);
                    let mut params: Vec<SqlArg> = vec![];
                    self.filters.append_to(&mut inner, &mut params);
                    self.append_tail(&mut inner);

                    let mut outer_select = "t.*".to_string();
                    if !includes.is_empty() {
                        outer_select.push_str(", ");
                        outer_select.push_str(&includes.join(", "));
                    }

                    let sql = format!(
                        "SELECT row_to_json(root) FROM (SELECT {} FROM ({}) t) root",
                        outer_select, inner
                    );

                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    let rows = client.query(&sql, &refs[..]).await?;
                    Ok(rows.into_iter().map(|row| row.get(0)).collect())
                }

                pub async fn count(mut self, pool: ConnectionPool) -> Result<i64, BoxError> {
                    let (sql, params) = self.build_scalar("COUNT(*)");
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    let row = client.query_one(&sql, &refs[..]).await?;
                    Ok(row.get(0))
                }

                /// Runs a one-cell select and hands back the row, leaving the
                /// typed `row.get(0)` to the caller.
                pub async fn scalar(mut self, pool: ConnectionPool, expression: String)
                    -> Result<tokio_postgres::Row, BoxError>
                {
                    let (sql, params) = self.build_scalar(&expression);
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    Ok(client.query_one(&sql, &refs[..]).await?)
                }

                pub async fn sum_cast_i64(mut self, pool: ConnectionPool, field: &str)
                    -> Result<i64, BoxError>
                {
                    let placeholder = self.next_placeholder();
                    let expression = format!(
                        "COALESCE(CAST(SUM({}) AS BIGINT), ${})",
                        field, placeholder
                    );
                    let (sql, mut params) = self.build_scalar(&expression);
                    params.push(SqlArg::I64(0));
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    let row = client.query_one(&sql, &refs[..]).await?;
                    Ok(row.get(0))
                }
            }

            /// Query builder state: collects conditions but cannot run.
            pub struct WhereOnly;

            /// Query builder state: holds the pool and the in-flight future.
            pub struct Executable {
                pool: ConnectionPool,
                fut: Option<std::pin::Pin<Box<dyn std::future::Future<
                    Output = Result<Vec<tokio_postgres::Row>, BoxError>
                > + Send>>>,
            }

            /// One query builder for every model and both states. Per-model
            /// code only adds the typed `where_*` and `order_by_*` methods.
            pub struct Query<M, S> {
                core: QueryCore,
                state: S,
                model: std::marker::PhantomData<fn() -> M>,
            }

            impl<M, S> Query<M, S> {
                pub fn core(&mut self) -> &mut QueryCore {
                    &mut self.core
                }

                pub fn limit(mut self, limit: usize) -> Self {
                    self.core.set_limit(limit);
                    self
                }

                pub fn offset(mut self, offset: usize) -> Self {
                    self.core.set_offset(offset);
                    self
                }

                /// Takes boxed values as part of its public signature; they
                /// ride along as the boxed SqlArg variant.
                pub fn where_raw(
                    mut self,
                    clause: impl Into<String>,
                    params: Vec<Box<dyn tokio_postgres::types::ToSql + Sync + Send>>,
                ) -> Self {
                    let params = params.into_iter().map(SqlArg::Boxed).collect();
                    self.core.filters().push_raw(clause.into(), params);
                    self
                }
            }

            impl<M: ModelMeta> Query<M, WhereOnly> {
                pub fn new() -> Self {
                    Self {
                        core: QueryCore::new(M::TABLE, M::SELECT_COLUMNS),
                        state: WhereOnly,
                        model: std::marker::PhantomData,
                    }
                }
            }

            impl<M: ModelMeta> Default for Query<M, WhereOnly> {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl<M: ModelMeta> Query<M, Executable> {
                pub fn new(pool: ConnectionPool) -> Self {
                    Self {
                        core: QueryCore::new(M::TABLE, M::SELECT_COLUMNS),
                        state: Executable { pool, fut: None },
                        model: std::marker::PhantomData,
                    }
                }

                pub fn from_builder(pool: ConnectionPool, builder: Query<M, WhereOnly>) -> Self {
                    Self {
                        core: builder.core,
                        state: Executable { pool, fut: None },
                        model: std::marker::PhantomData,
                    }
                }

                fn take_core(&mut self) -> QueryCore {
                    std::mem::replace(&mut self.core, QueryCore::new(M::TABLE, M::SELECT_COLUMNS))
                }

                pub async fn find_many_json(mut self) -> Result<Vec<serde_json::Value>, BoxError> {
                    let core = self.take_core();
                    core.fetch_json(self.state.pool.clone()).await
                }

                pub async fn find_first_json(self) -> Result<Option<serde_json::Value>, BoxError> {
                    let result = self.limit(1).find_many_json().await?;
                    Ok(result.into_iter().next())
                }

                pub async fn first(self) -> Result<Option<M>, BoxError> {
                    let result = self.limit(1).await?;
                    Ok(result.into_iter().next())
                }

                pub async fn count(mut self) -> Result<i64, BoxError> {
                    let core = self.take_core();
                    core.count(self.state.pool.clone()).await
                }

                pub async fn aggregate<T>(mut self, field: &str, func: &str) -> Result<Option<T>, BoxError>
                where
                    T: for<'a> tokio_postgres::types::FromSql<'a>,
                {
                    let core = self.take_core();
                    let row = core
                        .scalar(self.state.pool.clone(), format!("{}({})", func.to_uppercase(), field))
                        .await?;
                    Ok(row.get(0))
                }

                pub async fn sum<T>(mut self, field: &str) -> Result<T, BoxError>
                where
                    T: for<'a> tokio_postgres::types::FromSql<'a>
                        + Default
                        + tokio_postgres::types::ToSql
                        + Sync,
                {
                    let default_value = T::default();
                    let mut core = self.take_core();
                    let expression = format!("COALESCE(SUM({}), ${})", field, core.next_placeholder());
                    let (sql, params) = core.build_scalar(&expression);

                    let client = self.state.pool.get().await
                        .map_err(|_| "Failed to get connection from pool")?;
                    let mut refs = as_sql_refs(&params);
                    refs.push(&default_value);
                    debug::log_query(&sql, refs.len());
                    let row = client.query_one(&sql, &refs[..]).await?;
                    Ok(row.get(0))
                }

                pub async fn sum_cast_i64(mut self, field: &str) -> Result<i64, BoxError> {
                    let core = self.take_core();
                    core.sum_cast_i64(self.state.pool.clone(), field).await
                }

                pub async fn avg<T>(self, field: &str) -> Result<Option<T>, BoxError>
                where
                    T: for<'a> tokio_postgres::types::FromSql<'a>,
                {
                    self.aggregate(field, "AVG").await
                }

                pub async fn min<T>(self, field: &str) -> Result<Option<T>, BoxError>
                where
                    T: for<'a> tokio_postgres::types::FromSql<'a>,
                {
                    self.aggregate(field, "MIN").await
                }

                pub async fn max<T>(self, field: &str) -> Result<Option<T>, BoxError>
                where
                    T: for<'a> tokio_postgres::types::FromSql<'a>,
                {
                    self.aggregate(field, "MAX").await
                }
            }

            impl<M: ModelMeta> std::future::Future for Query<M, Executable> {
                type Output = Result<Vec<M>, BoxError>;

                fn poll(
                    mut self: std::pin::Pin<&mut Self>,
                    cx: &mut std::task::Context<'_>,
                ) -> std::task::Poll<Self::Output> {
                    let me = &mut *self;

                    if me.state.fut.is_none() {
                        let core = std::mem::replace(
                            &mut me.core,
                            QueryCore::new(M::TABLE, M::SELECT_COLUMNS),
                        );
                        me.state.fut = Some(Box::pin(core.fetch(me.state.pool.clone())));
                    }

                    match me.state.fut.as_mut().unwrap().as_mut().poll(cx) {
                        std::task::Poll::Ready(Ok(rows)) => std::task::Poll::Ready(Ok(
                            rows.iter().map(|row| M::from_row(row)).collect()
                        )),
                        std::task::Poll::Ready(Err(e)) => std::task::Poll::Ready(Err(e)),
                        std::task::Poll::Pending => std::task::Poll::Pending,
                    }
                }
            }

            /// A column usable as an `ON CONFLICT` target.
            pub struct ConflictField<M> {
                name: &'static str,
                model: std::marker::PhantomData<fn() -> M>,
            }

            impl<M> Clone for ConflictField<M> {
                fn clone(&self) -> Self {
                    *self
                }
            }

            impl<M> Copy for ConflictField<M> {}

            impl<M> ConflictField<M> {
                pub fn new(name: &'static str) -> Self {
                    Self { name, model: std::marker::PhantomData }
                }

                pub fn name(self) -> &'static str {
                    self.name
                }
            }

            /// One column or a tuple of them. Implemented here for every
            /// arity once, instead of per model.
            pub trait ConflictTarget<M> {
                fn columns(self) -> Vec<&'static str>;
            }

            impl<M> ConflictTarget<M> for ConflictField<M> {
                fn columns(self) -> Vec<&'static str> {
                    vec![self.name()]
                }
            }

            #tuple_conflict_impls

            /// Describes one JSONB column with a compiled-in default
            /// document. The statements are built by the generator; the
            /// behaviour around them lives here, once.
            pub trait JsonbField<M> {
                /// `SELECT <column> FROM <table> WHERE <pk> = $1 [...]`
                const SELECT_ONE_SQL: &'static str;
                /// `SELECT <pk>, <column> FROM <table> WHERE <pk> = ANY($1)`
                const SELECT_BY_IDS_SQL: &'static str;
                /// Upsert of a single key inside the document.
                const SET_SQL: &'static str;

                fn defaults() -> &'static serde_json::Value;
            }

            /// Reads and writes single keys inside a JSONB column, falling
            /// back to the compiled-in defaults.
            pub struct JsonbAccessor<M, F> {
                pool: ConnectionPool,
                field: std::marker::PhantomData<fn() -> (M, F)>,
            }

            impl<M, F> Clone for JsonbAccessor<M, F> {
                fn clone(&self) -> Self {
                    Self { pool: self.pool.clone(), field: std::marker::PhantomData }
                }
            }

            impl<M, F: JsonbField<M>> JsonbAccessor<M, F> {
                pub fn new(pool: ConnectionPool) -> Self {
                    Self { pool, field: std::marker::PhantomData }
                }

                /// Reads just the JSONB column instead of the whole row.
                /// A missing row and a NULL column both come back as None,
                /// which callers resolve to the compiled-in defaults.
                async fn fetch(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)])
                    -> Result<Option<serde_json::Value>, BoxError>
                {
                    let client = self.pool.get().await
                        .map_err(|e| format!("Failed to get database connection from pool: {}", e))?;
                    debug::log_query(F::SELECT_ONE_SQL, keys.len());
                    let row = client.query_opt(F::SELECT_ONE_SQL, keys).await?;
                    Ok(row.and_then(|row| row.get::<_, Option<serde_json::Value>>(0)))
                }

                pub async fn get_all(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)])
                    -> Result<serde_json::Value, BoxError>
                {
                    Ok(self.fetch(keys).await?.unwrap_or_else(|| F::defaults().clone()))
                }

                pub async fn get_all_as<T>(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)])
                    -> Result<T, BoxError>
                where
                    T: serde::de::DeserializeOwned,
                {
                    Ok(serde_json::from_value(self.get_all(keys).await?)?)
                }

                pub async fn get(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)], key: &str)
                    -> Result<String, BoxError>
                {
                    match self.fetch(keys).await? {
                        Some(value) => value.get_string(key).or_else(|_| F::defaults().get_string(key)),
                        None => F::defaults().get_string(key),
                    }
                }

                pub async fn get_as<T>(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)], key: &str)
                    -> Result<T, BoxError>
                where
                    T: serde::de::DeserializeOwned,
                {
                    match self.fetch(keys).await? {
                        Some(value) => value.get_value(key).or_else(|_| F::defaults().get_value(key)),
                        None => F::defaults().get_value(key),
                    }
                }

                pub async fn get_or<T>(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)], key: &str, default: T)
                    -> Result<T, BoxError>
                where
                    T: serde::de::DeserializeOwned,
                {
                    match self.get_as(keys, key).await {
                        Ok(value) => Ok(value),
                        Err(_) => Ok(default),
                    }
                }

                pub async fn has(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)], key: &str)
                    -> Result<bool, BoxError>
                {
                    match self.fetch(keys).await? {
                        Some(value) => Ok(value.has_key(key) || F::defaults().has_key(key)),
                        None => Ok(F::defaults().has_key(key)),
                    }
                }

                pub async fn set<T>(&self, keys: &[&(dyn tokio_postgres::types::ToSql + Sync)], key: &str, value: T)
                    -> Result<(), BoxError>
                where
                    T: serde::Serialize + Send + Sync,
                {
                    let value_json = serde_json::to_value(&value)
                        .map_err(|e| format!("Failed to serialize value for key '{}': {}", key, e))?;

                    let client = self.pool.get().await
                        .map_err(|e| format!("Failed to get database connection from pool: {}", e))?;

                    let mut params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = keys.to_vec();
                    params.push(&key);
                    params.push(&value_json);
                    debug::log_query(F::SET_SQL, params.len());

                    client.execute(F::SET_SQL, &params[..]).await.map_err(|e| {
                        format!(
                            "Database error setting key '{}': {} (SQL: {}, value: {:?})",
                            key, e, F::SET_SQL, value_json
                        )
                    })?;
                    Ok(())
                }

                pub async fn get_many(
                    &self,
                    keys: &[&(dyn tokio_postgres::types::ToSql + Sync)],
                    wanted: &[&str],
                ) -> Result<std::collections::HashMap<String, serde_json::Value>, BoxError> {
                    let stored = self.fetch(keys).await?;

                    let mut out = std::collections::HashMap::new();
                    for &key in wanted {
                        if let Some(value) = stored.as_ref().and_then(|stored| stored.get(key)) {
                            out.insert(key.to_string(), value.clone());
                        } else if let Some(value) = F::defaults().get(key) {
                            out.insert(key.to_string(), value.clone());
                        }
                    }
                    Ok(out)
                }

                pub async fn get_many_as<T>(
                    &self,
                    keys: &[&(dyn tokio_postgres::types::ToSql + Sync)],
                    wanted: &[&str],
                ) -> Result<std::collections::HashMap<String, T>, BoxError>
                where
                    T: serde::de::DeserializeOwned,
                {
                    let values = self.get_many(keys, wanted).await?;
                    let mut out = std::collections::HashMap::new();
                    for (key, value) in values {
                        if let Ok(parsed) = serde_json::from_value::<T>(value) {
                            out.insert(key, parsed);
                        }
                    }
                    Ok(out)
                }

                pub async fn get_many_ids<K, T>(&self, ids: &[K], key: &str)
                    -> Result<std::collections::HashMap<K, T>, BoxError>
                where
                    K: tokio_postgres::types::ToSql
                        + for<'a> tokio_postgres::types::FromSql<'a>
                        + std::hash::Hash
                        + Eq
                        + Sync,
                    T: serde::de::DeserializeOwned,
                {
                    if ids.is_empty() {
                        return Ok(std::collections::HashMap::new());
                    }

                    let client = self.pool.get().await
                        .map_err(|e| format!("Failed to get database connection from pool: {}", e))?;

                    let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = vec![&ids];
                    debug::log_query(F::SELECT_BY_IDS_SQL, params.len());
                    let rows = client.query(F::SELECT_BY_IDS_SQL, &params[..]).await?;

                    let mut out = std::collections::HashMap::new();
                    for row in rows {
                        let id: K = row.get(0);
                        let stored: Option<serde_json::Value> = row.get(1);
                        let value = stored
                            .as_ref()
                            .and_then(|stored| stored.get_value::<T>(key).ok())
                            .or_else(|| F::defaults().get_value::<T>(key).ok());
                        if let Some(value) = value {
                            out.insert(id, value);
                        }
                    }
                    Ok(out)
                }
            }

            pub struct DeleteCore {
                table: &'static str,
                filters: Filters,
            }

            impl DeleteCore {
                pub fn new(table: &'static str) -> Self {
                    Self { table, filters: Filters::new() }
                }

                pub fn filters(&mut self) -> &mut Filters {
                    &mut self.filters
                }

                pub fn build(&mut self) -> Result<(String, Vec<SqlArg>), BoxError> {
                    if self.filters.is_empty() {
                        return Err("DELETE without WHERE clause is not allowed".into());
                    }
                    let mut sql = format!("DELETE FROM {}", self.table);
                    let mut params: Vec<SqlArg> = vec![];
                    self.filters.append_to(&mut sql, &mut params);
                    Ok((sql, params))
                }

                pub async fn execute(mut self, pool: ConnectionPool) -> Result<u64, BoxError> {
                    let (sql, params) = self.build()?;
                    debug::log_query(&sql, params.len());
                    let client = pool.get().await.map_err(|_| "Failed to get connection from pool")?;
                    let refs = as_sql_refs(&params);
                    Ok(client.execute(&sql, &refs[..]).await?)
                }
            }
        }

        pub mod debug {
            use std::sync::atomic::{AtomicBool, Ordering};

            static DEBUG_ENABLED: AtomicBool = AtomicBool::new(false);

            pub fn enable_debug() {
                DEBUG_ENABLED.store(true, Ordering::Relaxed);
            }

            pub fn disable_debug() {
                DEBUG_ENABLED.store(false, Ordering::Relaxed);
            }

            pub fn is_debug_enabled() -> bool {
                DEBUG_ENABLED.load(Ordering::Relaxed)
            }

            pub fn log_query(sql: &str, params_count: usize) {
                if is_debug_enabled() {
                    eprintln!("[ByteORM Debug] Executing SQL: {}", sql);
                    eprintln!("[ByteORM Debug] Parameters count: {}", params_count);
                }
            }

            pub fn log_result(operation: &str, rows_affected: u64) {
                if is_debug_enabled() {
                    eprintln!("[ByteORM Debug] {} - Rows affected: {}", operation, rows_affected);
                }
            }

            pub fn log_error(operation: &str, error: &str) {
                if is_debug_enabled() {
                    eprintln!("[ByteORM Debug] Error in {}: {}", operation, error);
                }
            }
        }

        #jsonb_ext

        /// Enum to support both TLS and NoTLS connection pools
        #[derive(Clone)]
        pub enum ConnectionPool {
            Tls(Arc<bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres_rustls::MakeRustlsConnect>>>),
            NoTls(Arc<bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>>),
            Pinned(Arc<tokio_postgres::Client>),
        }

        impl ConnectionPool {
            pub async fn get(&self) -> Result<PooledClient, tokio_postgres::Error> {
                match self {
                    ConnectionPool::Tls(pool) => {
                        let conn = pool.get().await.map_err(|_| tokio_postgres::Error::__private_api_timeout())?;
                        Ok(PooledClient::Tls(conn))
                    }
                    ConnectionPool::NoTls(pool) => {
                        let conn = pool.get().await.map_err(|_| tokio_postgres::Error::__private_api_timeout())?;
                        Ok(PooledClient::NoTls(conn))
                    }
                    ConnectionPool::Pinned(client) => {
                        Ok(PooledClient::Pinned(client.clone()))
                    }
                }
            }
        }

        /// Wrapper for pooled connections that works with both TLS and NoTLS
        pub enum PooledClient<'a> {
            Tls(bb8::PooledConnection<'a, bb8_postgres::PostgresConnectionManager<tokio_postgres_rustls::MakeRustlsConnect>>),
            NoTls(bb8::PooledConnection<'a, bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>),
            Pinned(Arc<tokio_postgres::Client>),
        }

        impl<'a> PooledClient<'a> {
            pub async fn query(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<Vec<tokio_postgres::Row>, tokio_postgres::Error> {
                match self {
                    PooledClient::Tls(c) => c.query(sql, params).await,
                    PooledClient::NoTls(c) => c.query(sql, params).await,
                    PooledClient::Pinned(c) => c.query(sql, params).await,
                }
            }

            pub async fn query_one(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<tokio_postgres::Row, tokio_postgres::Error> {
                match self {
                    PooledClient::Tls(c) => c.query_one(sql, params).await,
                    PooledClient::NoTls(c) => c.query_one(sql, params).await,
                    PooledClient::Pinned(c) => c.query_one(sql, params).await,
                }
            }

            pub async fn query_opt(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<Option<tokio_postgres::Row>, tokio_postgres::Error> {
                match self {
                    PooledClient::Tls(c) => c.query_opt(sql, params).await,
                    PooledClient::NoTls(c) => c.query_opt(sql, params).await,
                    PooledClient::Pinned(c) => c.query_opt(sql, params).await,
                }
            }

            pub async fn execute(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<u64, tokio_postgres::Error> {
                match self {
                    PooledClient::Tls(c) => c.execute(sql, params).await,
                    PooledClient::NoTls(c) => c.execute(sql, params).await,
                    PooledClient::Pinned(c) => c.execute(sql, params).await,
                }
            }
        }

        #[derive(Clone)]
        pub struct Client {
            pool: ConnectionPool,
            connection_string: Option<String>,
            #(#model_accessors),*
        }
        impl std::fmt::Debug for Client {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("Client")
                    .field("pool", &"<ConnectionPool>")
                    #(#debug_accessor_fields)*
                    .finish()
            }
        }
        impl Client {
            pub async fn new(connection_string: &str) -> Result<Self, Error> {
                let is_local = connection_string.contains("localhost") || connection_string.contains("127.0.0.1");
                let requires_ssl = connection_string.contains("sslmode=require") || connection_string.contains("sslmode=verify");

                let pool = if is_local && !requires_ssl {
                    let manager = bb8_postgres::PostgresConnectionManager::new_from_stringlike(
                        connection_string,
                        tokio_postgres::NoTls,
                    )?;
                    let pool = bb8::Pool::builder()
                        .max_size(20)
                        .build(manager)
                        .await?;
                    ConnectionPool::NoTls(Arc::new(pool))
                } else {
                    let root_store = rustls::RootCertStore {
                        roots: webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect(),
                    };
                    let tls_config = rustls::ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                    let tls = tokio_postgres_rustls::MakeRustlsConnect::new(tls_config);
                    let manager = bb8_postgres::PostgresConnectionManager::new_from_stringlike(
                        connection_string,
                        tls,
                    )?;
                    let pool = bb8::Pool::builder()
                        .max_size(20)
                        .build(manager)
                        .await?;
                    ConnectionPool::Tls(Arc::new(pool))
                };

                Ok(Self {
                    pool: pool.clone(),
                    connection_string: Some(connection_string.to_string()),
                    #(#accessor_inits),*
                })
            }

            pub fn from_pool(pool: ConnectionPool) -> Self {
                Self {
                    pool: pool.clone(),
                    connection_string: None,
                    #(#accessor_inits_clone),*
                }
            }

            pub async fn get_client(&self) -> Result<PooledClient<'_>, Error> {
                self.pool.get().await
            }

            pub fn pool(&self) -> &ConnectionPool { &self.pool }

            pub async fn transaction<F, T, E>(&self, f: F) -> Result<T, E>
            where
                F: FnOnce(Transaction<'_>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send + '_>> + Send,
                E: From<tokio_postgres::Error> + Send,
                T: Send,
            {
                let client = self.pool.get().await.map_err(|e| E::from(e))?;
                match client {
                    PooledClient::Tls(mut c) => {
                        let tx = c.transaction().await?;
                        let transaction = Transaction { inner: tx };
                        let result = f(transaction).await;
                        result
                    }
                    PooledClient::NoTls(mut c) => {
                        let tx = c.transaction().await?;
                        let transaction = Transaction { inner: tx };
                        let result = f(transaction).await;
                        result
                    }
                    PooledClient::Pinned(_) => {
                        Err(E::from(tokio_postgres::Error::__private_api_timeout()))
                    }
                }
            }

            pub async fn begin(&self) -> Result<TxClient, Error> {
                let conn_str = self.connection_string.as_deref()
                    .ok_or_else(|| tokio_postgres::Error::__private_api_timeout())?;
                let is_local = conn_str.contains("localhost") || conn_str.contains("127.0.0.1");
                let requires_ssl = conn_str.contains("sslmode=require") || conn_str.contains("sslmode=verify");
                let client = if is_local && !requires_ssl {
                    let (client, connection) = tokio_postgres::connect(conn_str, tokio_postgres::NoTls).await?;
                    tokio::spawn(async move {
                        if let Err(e) = connection.await {
                            eprintln!("Transaction connection error: {}", e);
                        }
                    });
                    client
                } else {
                    let root_store = rustls::RootCertStore {
                        roots: webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect(),
                    };
                    let tls_config = rustls::ClientConfig::builder()
                        .with_root_certificates(root_store)
                        .with_no_client_auth();
                    let tls = tokio_postgres_rustls::MakeRustlsConnect::new(tls_config);
                    let (client, connection) = tokio_postgres::connect(conn_str, tls).await?;
                    tokio::spawn(async move {
                        if let Err(e) = connection.await {
                            eprintln!("Transaction connection error: {}", e);
                        }
                    });
                    client
                };
                client.execute("BEGIN", &[]).await?;
                let pinned = Arc::new(client);
                let pool = ConnectionPool::Pinned(pinned.clone());
                let inner = Self::from_pool(pool);
                Ok(TxClient { inner, pinned })
            }

            pub async fn execute_raw(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<u64, Error> {
                let client = self.pool.get().await?;
                client.execute(sql, params).await
            }

            pub async fn query_raw(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<Vec<tokio_postgres::Row>, Error> {
                let client = self.pool.get().await?;
                client.query(sql, params).await
            }
        }

        pub struct TxClient {
            pub inner: Client,
            pinned: Arc<tokio_postgres::Client>,
        }

        impl TxClient {
            pub async fn commit(self) -> Result<(), Error> {
                self.pinned.execute("COMMIT", &[]).await?;
                Ok(())
            }

            pub async fn rollback(self) -> Result<(), Error> {
                self.pinned.execute("ROLLBACK", &[]).await?;
                Ok(())
            }
        }

        impl std::ops::Deref for TxClient {
            type Target = Client;
            fn deref(&self) -> &Self::Target {
                &self.inner
            }
        }

        pub struct Transaction<'a> {
            inner: tokio_postgres::Transaction<'a>,
        }

        impl<'a> Transaction<'a> {
            pub async fn execute(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<u64, Error> {
                self.inner.execute(sql, params).await
            }

            pub async fn query(&self, sql: &str, params: &[&(dyn tokio_postgres::types::ToSql + Sync)]) -> Result<Vec<tokio_postgres::Row>, Error> {
                self.inner.query(sql, params).await
            }

            pub async fn commit(self) -> Result<(), Error> {
                self.inner.commit().await
            }

            pub async fn rollback(self) -> Result<(), Error> {
                self.inner.rollback().await
            }
        }
    };

    files.insert("src/lib.rs".to_string(), pretty_print(&lib_code));

    files
}

fn term_ident(s: &str) -> proc_macro2::Ident {
    quote::format_ident!("{}", s)
}

fn pretty_print(code: &TokenStream) -> String {
    match syn::parse2::<syn::File>(code.clone()) {
        Ok(file) => prettyplease::unparse(&file),
        Err(e) => {
            eprintln!("ERROR parsing generated code: {}", e);
            eprintln!("Generated code:\n{}", code);
            panic!("Failed to parse generated Rust code");
        }
    }
}

fn generate_derive_model(
    model: &crate::Model,
    jsonb_defaults: &HashMap<(String, String), String>,
) -> TokenStream {
    use crate::Modifier;
    let model_name = format_ident!("{}", model.name);
    let table_name = model.name.to_lowercase();

    let computed_attrs: Vec<_> = model
        .computed_fields
        .iter()
        .map(|cf| {
            let cf_name = &cf.name;
            let cf_expr = &cf.expression;
            quote! { #[byteorm(computed(name = #cf_name, expr = #cf_expr))] }
        })
        .collect();

    let fields: Vec<_> = model
        .fields
        .iter()
        .map(|field| {
            let field_name = format_ident!("{}", field.name);
            let is_nullable = field
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Nullable));
            let field_type = rust_type_from_schema(&field.type_name, is_nullable);

            let mut attrs = Vec::new();

            if field
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
            {
                attrs.push(quote! { #[byteorm(pk)] });
            }
            if field.type_name == "Serial" {
                attrs.push(quote! { #[byteorm(serial)] });
            }
            if field
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Unique))
            {
                attrs.push(quote! { #[byteorm(unique)] });
            }
            if field.modifiers.iter().any(|m| matches!(m, Modifier::Index)) {
                attrs.push(quote! { #[byteorm(index)] });
            }
            for m in &field.modifiers {
                if let Modifier::ForeignKey {
                    model: fk_model,
                    field: fk_field,
                    ..
                } = m
                {
                    if let Some(fk_f) = fk_field {
                        attrs.push(quote! { #[byteorm(fk(model = #fk_model, field = #fk_f))] });
                    } else {
                        attrs.push(quote! { #[byteorm(fk(model = #fk_model))] });
                    }
                }
            }
            if !is_builtin_type(&field.type_name) {
                let enum_name = &field.type_name;
                attrs.push(quote! { #[byteorm(enum_type = #enum_name)] });
            }
            if let Some(path) = field.get_jsonb_default_path() {
                if let Some(content) = jsonb_defaults.get(&(model.name.clone(), field.name.clone()))
                {
                    attrs.push(quote! { #[byteorm(jsonb_default = #content)] });
                } else {
                    attrs.push(quote! { #[byteorm(jsonb_default = #path)] });
                }
            } else if field.type_name == "JsonB" || field.type_name == "Jsonb" {
                if let Some(default_val) = field.get_default_value() {
                    attrs.push(quote! { #[byteorm(jsonb_default = #default_val)] });
                }
            }
            if let Some(sql_default) = field.sql_default_literal() {
                attrs.push(quote! { #[byteorm(sql_default = #sql_default)] });
            }

            quote! {
                #(#attrs)*
                pub #field_name: #field_type
            }
        })
        .collect();

    quote! {
        // Which of these a model needs depends on its fields; the shared
        // runtime covers the rest.
        #![allow(unused_imports)]

        use serde::{Deserialize, Serialize};
        use chrono::{DateTime, NaiveDate, Utc};
        use std::sync::Arc;
        use std::collections::HashMap;
        use once_cell::sync::Lazy;
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use crate::{ByteOrmError, ConnectionPool, FromRow, PooledClient, debug, expect_keys, render_where_predicates, JsonbExt, WhereOp, WherePredicate, __private};
        use crate::enums::*;
        use byteorm_macros::ByteOrm;

        #[derive(ByteOrm, Debug, Clone, Serialize, Deserialize)]
        #[byteorm(table = #table_name)]
        #(#computed_attrs)*
        pub struct #model_name {
            #(#fields),*
        }
    }
}
