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
    let model_name = format_ident!("{}", model.name);
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

    // Only touch updated_at on models that actually have the column.
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

    let (_, _, pk_columns, pk_placeholders, _) = pk_args(model);

    jsonb_fields
        .into_iter()
        .map(|jsonb| {
            let jsonb_name = &jsonb.name;
            let jsonb_snake = to_snake_case(jsonb_name);
            let accessor_struct =
                format_ident!("{}{}Accessor", model.name, capitalize_first(jsonb_name));
            let field_marker =
                format_ident!("{}{}Field", model.name, capitalize_first(jsonb_name));
            let defaults_const = format_ident!("{}_DEFAULTS", jsonb_snake.to_uppercase());

            let json_content = jsonb
                .attributes
                .iter()
                .find(|a| a.name == "jsonb_default")
                .and_then(|a| a.args.as_ref());

            let default_json_init = match json_content {
                Some(content) => quote! {
                    static #defaults_const: Lazy<serde_json::Value> = Lazy::new(|| {
                        serde_json::from_str(#content)
                            .expect(&format!("Failed to parse default JSON for {}.{}", #model_name_str, #jsonb_name))
                    });
                },
                None => quote! {
                    static #defaults_const: Lazy<serde_json::Value> = Lazy::new(|| {
                        serde_json::json!({})
                    });
                },
            };

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
            let set_sql = format!(
                "INSERT INTO {} ({}, {}{}) VALUES ({}, jsonb_set('{{}}'::jsonb, string_to_array(${}, '.')::text[], ${}::jsonb, true){}) \
                 ON CONFLICT ({}) DO UPDATE SET {} = jsonb_set(COALESCE({}.{}, '{{}}'::jsonb), string_to_array(${}, '.')::text[], ${}::jsonb, true){}",
                table_name,
                pk_columns.join(", "),
                jsonb_snake,
                updated_at_column,
                pk_placeholders.join(", "),
                pk_columns.len() + 1,
                pk_columns.len() + 2,
                updated_at_value,
                pk_columns.join(", "),
                jsonb_snake,
                table_name,
                jsonb_snake,
                pk_columns.len() + 1,
                pk_columns.len() + 2,
                updated_at_assignment,
            );

            // Thin per-field wrappers keep the call-site arity of the primary
            // key; everything below them is the shared JsonbAccessor.
            let (pk_params, pk_refs) = if pk_fields.len() == 1 {
                let pk = &pk_fields[0];
                let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
                let pk_type = rust_type_from_schema(&pk.type_name, is_nullable);
                (quote! { id: #pk_type }, quote! { &[&id] })
            } else {
                let params = pk_fields.iter().map(|pk| {
                    let name = format_ident!("{}", to_snake_case(&pk.name));
                    let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
                    let pk_type = rust_type_from_schema(&pk.type_name, is_nullable);
                    quote! { #name: #pk_type }
                });
                let refs = pk_fields.iter().map(|pk| {
                    let name = format_ident!("{}", to_snake_case(&pk.name));
                    quote! { &#name }
                });
                (quote! { #(#params),* }, quote! { &[#(#refs),*] })
            };

            let pk_field_type = if pk_fields.len() == 1 {
                let pk = &pk_fields[0];
                let is_nullable = pk.modifiers.iter().any(|m| matches!(m, Modifier::Nullable));
                rust_type_from_schema(&pk.type_name, is_nullable)
            } else {
                quote! { () }
            };

            quote! {
                #default_json_init

                #[doc(hidden)]
                pub enum #field_marker {}

                impl __private::JsonbField<#model_name> for #field_marker {
                    const SELECT_ONE_SQL: &'static str = #select_one_sql;
                    const SELECT_BY_IDS_SQL: &'static str = #select_by_ids_sql;
                    const SET_SQL: &'static str = #set_sql;

                    fn defaults() -> &'static serde_json::Value {
                        &#defaults_const
                    }
                }

                #[derive(Clone)]
                pub struct #accessor_struct {
                    inner: __private::JsonbAccessor<#model_name, #field_marker>,
                }

                impl std::fmt::Debug for #accessor_struct {
                    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        f.debug_struct(stringify!(#accessor_struct))
                            .field("pool", &"<bb8::Pool>")
                            .finish()
                    }
                }

                impl #accessor_struct {
                    pub fn new(pool: ConnectionPool) -> Self {
                        Self { inner: __private::JsonbAccessor::new(pool) }
                    }

                    pub async fn get_all(&self, #pk_params)
                        -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>>
                    {
                        self.inner.get_all(#pk_refs).await
                    }

                    pub async fn get_all_as<T>(&self, #pk_params)
                        -> Result<T, Box<dyn std::error::Error + Send + Sync>>
                    where
                        T: serde::de::DeserializeOwned,
                    {
                        self.inner.get_all_as(#pk_refs).await
                    }

                    pub async fn get(&self, #pk_params, key: &str)
                        -> Result<String, Box<dyn std::error::Error + Send + Sync>>
                    {
                        self.inner.get(#pk_refs, key).await
                    }

                    pub async fn get_as<T>(&self, #pk_params, key: &str)
                        -> Result<T, Box<dyn std::error::Error + Send + Sync>>
                    where
                        T: serde::de::DeserializeOwned,
                    {
                        self.inner.get_as(#pk_refs, key).await
                    }

                    pub async fn get_or<T>(&self, #pk_params, key: &str, default: T)
                        -> Result<T, Box<dyn std::error::Error + Send + Sync>>
                    where
                        T: serde::de::DeserializeOwned,
                    {
                        self.inner.get_or(#pk_refs, key, default).await
                    }

                    pub async fn has(&self, #pk_params, key: &str)
                        -> Result<bool, Box<dyn std::error::Error + Send + Sync>>
                    {
                        self.inner.has(#pk_refs, key).await
                    }

                    pub async fn set<T>(&self, #pk_params, key: &str, value: T)
                        -> Result<(), Box<dyn std::error::Error + Send + Sync>>
                    where
                        T: serde::Serialize + Send + Sync,
                    {
                        self.inner.set(#pk_refs, key, value).await
                    }

                    pub async fn get_many(&self, #pk_params, keys: &[&str])
                        -> Result<HashMap<String, serde_json::Value>, Box<dyn std::error::Error + Send + Sync>>
                    {
                        self.inner.get_many(#pk_refs, keys).await
                    }

                    pub async fn get_many_as<T>(&self, #pk_params, keys: &[&str])
                        -> Result<HashMap<String, T>, Box<dyn std::error::Error + Send + Sync>>
                    where
                        T: serde::de::DeserializeOwned,
                    {
                        self.inner.get_many_as(#pk_refs, keys).await
                    }

                    pub async fn get_many_ids<T>(&self, ids: &[#pk_field_type], key: &str)
                        -> Result<HashMap<#pk_field_type, T>, Box<dyn std::error::Error + Send + Sync>>
                    where
                        T: serde::de::DeserializeOwned,
                    {
                        self.inner.get_many_ids(ids, key).await
                    }
                }
            }
        })
        .collect()
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
        assert!(with[0].to_string().contains("updated_at = NOW()"));

        let without = generate_jsonb_sub_accessors(&model_with(vec![pk(), jsonb()]));
        let code = without[0].to_string();
        assert!(!code.contains("updated_at"));
        assert!(!code.contains("NOW()"));
    }

    #[test]
    fn behaviour_lives_in_the_shared_accessor() {
        let accessors = generate_jsonb_sub_accessors(&model_with(vec![pk(), jsonb()]));
        let code = accessors[0].to_string();

        assert!(code.contains("__private :: JsonbAccessor"));
        assert!(!code.contains("query_opt"));
    }
}
