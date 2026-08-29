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

#[cfg(test)]
mod tests {
    use super::rust_type_from_schema;

    #[test]
    fn maps_text_fields_to_string() {
        assert_eq!(rust_type_from_schema("Text", false).to_string(), "String");
        assert_eq!(
            rust_type_from_schema("Text", true).to_string(),
            "Option < String >"
        );
    }
}
