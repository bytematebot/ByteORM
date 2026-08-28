use crate::codegen::utils::*;
use crate::types::*;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn generate_jsonb_accessor_fields(
    model: &Model,
) -> Vec<(proc_macro2::Ident, proc_macro2::Ident)> {
    model
        .fields
        .iter()
        .filter(|f| {
            (f.type_name == "JsonB" || f.type_name == "Jsonb")
                && f.attributes.iter().any(|a| a.name == "jsonb_default")
        })
        .map(|f| {
            let field_name = format_ident!("{}", to_snake_case(&f.name));
            let struct_name = format_ident!("{}{}Accessor", model.name, capitalize_first(&f.name));
            (field_name, struct_name)
        })
        .collect()
}

pub fn generate_jsonb_sub_accessors(model: &Model) -> Vec<TokenStream> {
    let model_name_str = &model.name;
    let table_name = &model.table_name;

    let pk_fields: Vec<_> = model
        .fields
        .iter()
        .filter(|f| {
            f.modifiers
                .iter()
                .any(|m| matches!(m, Modifier::PrimaryKey))
        })
        .collect();

    let has_updated_at = model
        .fields
        .iter()
        .any(|f| to_snake_case(&f.name) == "updated_at");
    let updated_at_column = if has_updated_at { ", updated_at" } else { "" };
    let updated_at_value = if has_updated_at { ", NOW()" } else { "" };
    let updated_at_assignment = if has_updated_at {
        ", updated_at = NOW()"
    } else {
        ""
    };

    let jsonb_fields: Vec<_> = model
        .fields
        .iter()
        .filter(|f| {
            (f.type_name == "JsonB" || f.type_name == "Jsonb")
                && f.attributes.iter().any(|a| a.name == "jsonb_default")
        })
        .collect();

    jsonb_fields.into_iter().map(|jsonb| {
        let jsonb_name = &jsonb.name;
        let jsonb_snake = to_snake_case(jsonb_name);
        let sub_accessor_struct = format_ident!("{}{}Accessor", model.name, capitalize_first(jsonb_name));
        let defaults_const = format_ident!("{}_DEFAULTS", jsonb_snake.to_uppercase());

        let json_content = jsonb.attributes.iter()
            .find(|a| a.name == "jsonb_default")
            .and_then(|a| a.args.as_ref());

        let default_json_init = if let Some(content) = json_content {
            quote! {
                static #defaults_const: Lazy<serde_json::Value> = Lazy::new(|| {
                    serde_json::from_str(#content)
                        .expect(&format!("Failed to parse default JSON for {}.{}", #model_name_str, #jsonb_name))
                });
            }
        } else {
            quote! {
                static #defaults_const: Lazy<serde_json::Value> = Lazy::new(|| {
                    serde_json::json!({})
                });
            }
        };

        let (pk_params, pk_args_for_set) = if pk_fields.len() == 1 {
            let pk = &pk_fields[0];
            let is_pk_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
            let pk_type = rust_type_from_schema(&pk.type_name, is_pk_nullable);
            (quote! { id: #pk_type }, vec![quote! { &id }])
        } else {
            let params = pk_fields.iter().map(|pk| {
                let param_name = format_ident!("{}", to_snake_case(&pk.name));
                let is_pk_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
                let pk_type = rust_type_from_schema(&pk.type_name, is_pk_nullable);
                quote! { #param_name: #pk_type }
            });
            let set_args = pk_fields.iter().map(|pk| {
                let param_name = format_ident!("{}", to_snake_case(&pk.name));
                quote! { &#param_name }
            });
            (quote! { #(#params),* }, set_args.collect::<Vec<_>>())
        };

        let pk_field_type = if pk_fields.len() == 1 {
            let pk = &pk_fields[0];
            let is_pk_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
            rust_type_from_schema(&pk.type_name, is_pk_nullable)
        } else {
            quote! { () }
        };

        let pk_args_clone = if pk_fields.len() == 1 {
            quote! { id }
        } else {
            let args = pk_fields.iter().map(|pk| {
                let param_name = format_ident!("{}", to_snake_case(&pk.name));
                quote! { #param_name }
            });
            quote! { #(#args),* }
        };

        let (_, _, pk_columns, pk_placeholders, _) = pk_args(model);
        let insert_pk_part = pk_columns.join(", ");
        let insert_values_part = pk_placeholders.join(", ");
        let conflict_clause = pk_columns.join(", ");
        let key_placeholder = format!("${}", pk_columns.len() + 1);
        let value_placeholder = format!("${}", pk_columns.len() + 2);

        let pk_where_sql = pk_columns
            .iter()
            .zip(pk_placeholders.iter())
            .map(|(col, placeholder)| format!("{} = {}", col, placeholder))
            .collect::<Vec<_>>()
            .join(" AND ");
        let select_one_sql = format!(
            "SELECT {} FROM {} WHERE {}",
            jsonb_snake, table_name, pk_where_sql
        );
        let select_by_ids_sql = match pk_columns.first() {
            Some(pk_col) => format!(
                "SELECT {}, {} FROM {} WHERE {} = ANY($1)",
                pk_col, jsonb_snake, table_name, pk_col
            ),
            None => String::new(),
        };

        quote! {
            #default_json_init

            #[derive(Clone)]
            pub struct #sub_accessor_struct {
                pool: ConnectionPool,
            }

            impl std::fmt::Debug for #sub_accessor_struct {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.debug_struct(stringify!(#sub_accessor_struct))
                        .field("pool", &"<bb8::Pool>")
                        .finish()
                }
            }

            impl #sub_accessor_struct {
                pub fn new(pool: ConnectionPool) -> Self {
                    Self { pool }
                }

                /// Reads just the JSONB column instead of the whole row.
                /// A missing row and a NULL column both come back as None,
                /// which callers resolve to the compiled-in defaults.
                async fn fetch_jsonb(&self, #pk_params)
                    -> Result<Option<serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
                {
                    let client = self.pool.get().await
                        .map_err(|e| format!("Failed to get database connection from pool: {}", e))?;

                    let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                        vec![#(#pk_args_for_set as &(dyn tokio_postgres::types::ToSql + Sync)),*];
                    debug::log_query(#select_one_sql, params.len());

                    let row = client.query_opt(#select_one_sql, &params[..]).await?;
                    Ok(row.and_then(|row| row.get::<_, Option<serde_json::Value>>(0)))
                }

                pub async fn get_all(&self, #pk_params)
                    -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>
                {
                    Ok(self.fetch_jsonb(#pk_args_clone).await?
                        .unwrap_or_else(|| #defaults_const.clone()))
                }

                pub async fn get_all_as<T>(&self, #pk_params)
                    -> Result<T, Box<dyn std::error::Error + Send + Sync>>
                where
                    T: serde::de::DeserializeOwned,
                {
                    let value = self.get_all(#pk_args_clone).await?;
                    Ok(serde_json::from_value(value)?)
                }

                pub async fn get(&self, #pk_params, key: &str)
                    -> Result<String, Box<dyn std::error::Error + Send + Sync>>
                {
                    match self.fetch_jsonb(#pk_args_clone).await? {
                        Some(value) => value.get_string(key)
                            .or_else(|_| #defaults_const.get_string(key)),
                        None => #defaults_const.get_string(key),
                    }
                }

                pub async fn get_as<T>(&self, #pk_params, key: &str)
                    -> Result<T, Box<dyn std::error::Error + Send + Sync>>
                where
                    T: serde::de::DeserializeOwned,
                {
                    match self.fetch_jsonb(#pk_args_clone).await? {
                        Some(value) => value.get_value(key)
                            .or_else(|_| #defaults_const.get_value(key)),
                        None => #defaults_const.get_value(key),
                    }
                }

                pub async fn get_or<T>(&self, #pk_params, key: &str, default: T)
                    -> Result<T, Box<dyn std::error::Error + Send + Sync>>
                where
                    T: serde::de::DeserializeOwned,
                {
                    match self.get_as(#pk_args_clone, key).await {
                        Ok(value) => Ok(value),
                        Err(_) => Ok(default),
                    }
                }

                pub async fn has(&self, #pk_params, key: &str)
                    -> Result<bool, Box<dyn std::error::Error + Send + Sync>>
                {
                    match self.fetch_jsonb(#pk_args_clone).await? {
                        Some(value) => Ok(value.has_key(key) || #defaults_const.has_key(key)),
                        None => Ok(#defaults_const.has_key(key)),
                    }
                }

                pub async fn set<T>(&self, #pk_params, key: &str, value: T)
                    -> Result<(), Box<dyn std::error::Error + Send + Sync>>
                where
                    T: serde::Serialize + Send + Sync,
                {
                    let value_json = serde_json::to_value(&value)
                        .map_err(|e| format!("Failed to serialize value for key '{}': {}", key, e))?;

                    let sql = format!(
                        "INSERT INTO {} ({}, {}{}) VALUES ({}, jsonb_set('{{}}'::jsonb, string_to_array({}, '.')::text[], {}::jsonb, true){}) \
                         ON CONFLICT ({}) DO UPDATE SET {} = jsonb_set(COALESCE({}.{}, '{{}}'::jsonb), string_to_array({}, '.')::text[], {}::jsonb, true){}",
                        #table_name,
                        #insert_pk_part,
                        #jsonb_snake,
                        #updated_at_column,
                        #insert_values_part,
                        #key_placeholder,
                        #value_placeholder,
                        #updated_at_value,
                        #conflict_clause,
                        #jsonb_snake,
                        #table_name,
                        #jsonb_snake,
                        #key_placeholder,
                        #value_placeholder,
                        #updated_at_assignment,
                    );

                    let client = self.pool.get().await
                        .map_err(|e| format!("Failed to get database connection from pool: {}", e))?;

                    let params = vec![#(#pk_args_for_set),*, &key as &(dyn tokio_postgres::types::ToSql + Sync), &value_json as &(dyn tokio_postgres::types::ToSql + Sync)];
                    debug::log_query(&sql, params.len());

                    client.execute(
                        &sql,
                        &params[..]
                    ).await
                        .map_err(|e| format!("Database error setting key '{}': {} (SQL: {}, value: {:?})", key, e, sql, value_json))?;

                    Ok(())
                }

                pub async fn get_many(
                    &self, #pk_params, keys: &[&str]
                ) -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
                {
                    let stored = self.fetch_jsonb(#pk_args_clone).await?;

                    let mut out = HashMap::new();
                    for &key in keys {
                        if let Some(value) = stored.as_ref().and_then(|stored| stored.get(key)) {
                            out.insert(key.to_string(), value.clone());
                        } else if let Some(value) = #defaults_const.get(key) {
                            out.insert(key.to_string(), value.clone());
                        }
                    }
                    Ok(out)
                }

                pub async fn get_many_as<T>(
                    &self, #pk_params, keys: &[&str]
                ) -> Result<HashMap<String, T>, Box<dyn std::error::Error + Send + Sync>>
                where T: serde::de::DeserializeOwned
                {
                    let values = self.get_many(#pk_args_clone, keys).await?;
                    let mut map = HashMap::new();
                    for (k, v) in values {
                        if let Ok(x) = serde_json::from_value::<T>(v) {
                            map.insert(k, x);
                        }
                    }
                    Ok(map)
                }

                pub async fn get_many_ids<T>(
                    &self, ids: &[#pk_field_type], key: &str
                ) -> Result<HashMap<#pk_field_type, T>, Box<dyn std::error::Error + Send + Sync>>
                where T: serde::de::DeserializeOwned
                {
                    if ids.is_empty() {
                        return Ok(HashMap::new());
                    }

                    let client = self.pool.get().await
                        .map_err(|e| format!("Failed to get database connection from pool: {}", e))?;

                    let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                        vec![&ids as &(dyn tokio_postgres::types::ToSql + Sync)];
                    debug::log_query(#select_by_ids_sql, params.len());

                    let rows = client.query(#select_by_ids_sql, &params[..]).await?;

                    let mut map = HashMap::new();
                    for row in rows {
                        let id: #pk_field_type = row.get(0);
                        let stored: Option<serde_json::Value> = row.get(1);
                        let value = stored
                            .as_ref()
                            .and_then(|stored| stored.get_value::<T>(key).ok())
                            .or_else(|| #defaults_const.get_value::<T>(key).ok());
                        if let Some(value) = value {
                            map.insert(id, value);
                        }
                    }
                    Ok(map)
                }
            }
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::generate_jsonb_sub_accessors;
    use crate::types::{Attribute, Field, Model, Modifier};

    fn model_with(fields: Vec<Field>) -> Model {
        Model {
            name: "Settings".to_string(),
            fields,
            computed_fields: vec![],
            table_name: "settings".to_string(),
        }
    }

    fn pk() -> Field {
        Field {
            name: "id".to_string(),
            type_name: "BigInt".to_string(),
            modifiers: vec![Modifier::PrimaryKey],
            attributes: vec![],
        }
    }

    fn jsonb() -> Field {
        Field {
            name: "data".to_string(),
            type_name: "JsonB".to_string(),
            modifiers: vec![],
            attributes: vec![Attribute {
                name: "jsonb_default".to_string(),
                args: Some("{}".to_string()),
            }],
        }
    }

    fn updated_at() -> Field {
        Field {
            name: "updatedAt".to_string(),
            type_name: "TimestamptZ".to_string(),
            modifiers: vec![],
            attributes: vec![],
        }
    }

    #[test]
    fn reads_select_only_the_jsonb_column() {
        let accessors =
            generate_jsonb_sub_accessors(&model_with(vec![pk(), jsonb(), updated_at()]));
        let code = accessors[0].to_string();

        assert!(code.contains("\"SELECT data FROM settings WHERE id = $1\""));
        assert!(code.contains("\"SELECT id, data FROM settings WHERE id = ANY($1)\""));
        assert!(!code.contains("SettingsQuery"));
    }

    #[test]
    fn set_touches_updated_at_only_when_the_column_exists() {
        let with = generate_jsonb_sub_accessors(&model_with(vec![pk(), jsonb(), updated_at()]));
        let code = with[0].to_string();
        assert!(code.contains("\", updated_at\""));
        assert!(code.contains("\", NOW()\""));
        assert!(code.contains("\", updated_at = NOW()\""));

        let without = generate_jsonb_sub_accessors(&model_with(vec![pk(), jsonb()]));
        let code = without[0].to_string();
        assert!(!code.contains("updated_at"));
        assert!(!code.contains("NOW()"));
    }
}
