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

    let jsonb_ext = generate_jsonb_ext();

    let lib_code = quote! {
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
            use super::{ConnectionPool, WhereOp, WherePredicate, debug, render_where_predicates};

            pub type SqlArg = Box<dyn tokio_postgres::types::ToSql + Sync + Send>;
            pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

            /// Collects WHERE predicates and their bound arguments.
            pub struct Filters {
                predicates: Vec<WherePredicate>,
                args: Vec<SqlArg>,
            }

            impl Filters {
                pub fn new() -> Self {
                    Self { predicates: vec![], args: vec![] }
                }

                pub fn is_empty(&self) -> bool {
                    self.predicates.is_empty()
                }

                /// Records a predicate binding one argument.
                pub fn push(&mut self, column: &'static str, op: WhereOp, value: SqlArg) {
                    let start = self.args.len();
                    self.args.push(value);
                    self.predicates.push(WherePredicate::new(column, op, start..self.args.len()));
                }

                /// Records a predicate binding no arguments (IS NULL, IS NOT NULL).
                pub fn push_bare(&mut self, column: &'static str, op: WhereOp) {
                    let at = self.args.len();
                    self.predicates.push(WherePredicate::new(column, op, at..at));
                }

                /// Records an IN predicate over any number of arguments.
                pub fn push_in(&mut self, column: &'static str, values: Vec<SqlArg>) {
                    let start = self.args.len();
                    self.args.extend(values);
                    self.predicates.push(WherePredicate::new(column, WhereOp::In, start..self.args.len()));
                }

                /// Appends " WHERE ..." to `sql` and moves the arguments into
                /// `params`, numbering placeholders from `params.len() + 1`.
                pub fn append_to(&mut self, sql: &mut String, params: &mut Vec<SqlArg>) {
                    if self.predicates.is_empty() {
                        return;
                    }
                    let (clauses, _) = render_where_predicates(&self.predicates, params.len() + 1);
                    sql.push_str(" WHERE ");
                    sql.push_str(&clauses.join(" AND "));
                    params.append(&mut self.args);
                }
            }

            /// Borrows owned arguments as the reference slice tokio-postgres wants.
            pub fn as_sql_refs(params: &[SqlArg]) -> Vec<&(dyn tokio_postgres::types::ToSql + Sync)> {
                params
                    .iter()
                    .map(|param| param.as_ref() as &(dyn tokio_postgres::types::ToSql + Sync))
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
                        clauses.push(format!(
                            "{} = {} {} ${}",
                            arithmetic.column, arithmetic.column, symbol, params.len() + 1
                        ));
                        params.push(Box::new(arithmetic.value));
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

            pub struct CreateCore {
                table: &'static str,
                select_columns: &'static str,
                required: &'static [&'static str],
                casts: &'static [(&'static str, &'static str)],
                values: std::collections::HashMap<&'static str, SqlArg>,
                filters: Filters,
            }

            impl CreateCore {
                pub fn new(
                    table: &'static str,
                    select_columns: &'static str,
                    required: &'static [&'static str],
                    casts: &'static [(&'static str, &'static str)],
                ) -> Self {
                    Self {
                        table,
                        select_columns,
                        required,
                        casts,
                        values: std::collections::HashMap::new(),
                        filters: Filters::new(),
                    }
                }

                pub fn filters(&mut self) -> &mut Filters {
                    &mut self.filters
                }

                pub fn push_value(&mut self, column: &'static str, value: SqlArg) {
                    self.values.insert(column, value);
                }

                /// Consumes the core into the pieces `upsert_many` needs.
                pub fn into_parts(self) -> (std::collections::HashMap<&'static str, SqlArg>, Filters) {
                    (self.values, self.filters)
                }

                pub fn cast_for(casts: &[(&'static str, &'static str)], column: &str) -> Option<&'static str> {
                    casts
                        .iter()
                        .find(|(name, _)| *name == column)
                        .map(|(_, cast)| *cast)
                }

                pub fn check_required(&self) -> Result<(), BoxError> {
                    for column in self.required {
                        if !self.values.contains_key(column) {
                            return Err(format!("Missing required field: {}", column).into());
                        }
                    }
                    if self.values.is_empty() && !self.required.is_empty() {
                        return Err("No fields to create".into());
                    }
                    Ok(())
                }

                pub fn build(&mut self) -> Result<(String, Vec<SqlArg>), BoxError> {
                    self.check_required()?;

                    let mut columns: Vec<&'static str> = self.values.keys().copied().collect();
                    columns.sort();

                    let mut placeholders: Vec<String> = Vec::with_capacity(columns.len());
                    let mut params: Vec<SqlArg> = Vec::with_capacity(columns.len());
                    for column in &columns {
                        let placeholder = params.len() + 1;
                        match Self::cast_for(self.casts, column) {
                            Some(cast) => placeholders.push(format!("${}::TEXT::{}", placeholder, cast)),
                            None => placeholders.push(format!("${}", placeholder)),
                        }
                        params.push(self.values.remove(column).expect("column was just listed"));
                    }

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
