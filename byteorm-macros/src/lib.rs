extern crate proc_macro;

mod codegen;
mod parse;
mod types;

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(ByteOrm, attributes(byteorm))]
pub fn derive_byteorm(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let model = parse::parse_model(&input);

    let from_row_impl = codegen::utils::generate_from_row_impl(&model);
    let model_meta_impl = codegen::utils::generate_model_meta_impl(&model);
    let model_impl = codegen::model::generate_model_impl(&model);
    let query_builder = codegen::query::generate_query_builder_struct(&model);
    let mutation_builders = codegen::mutation::generate_mutation_builders(&model);
    let jsonb_sub_accessors = codegen::jsonb::generate_jsonb_sub_accessors(&model);
    let accessor = codegen::client::generate_accessor(&model);

    let expanded = quote! {
        #from_row_impl
        #model_meta_impl
        #model_impl
        #query_builder
        #mutation_builders
        #(#jsonb_sub_accessors)*
        #accessor
    };

    TokenStream::from(expanded)
}
