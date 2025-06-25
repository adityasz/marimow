/// This file was generated using Claude 4.0 Sonnet in Cursor
/// (and was cleaned up by hand afterwards).
///
/// Parts of it may look extremely stupid but does seem to work in
/// convert_test.rs, and that's the only use for this.

use proc_macro::TokenStream;
use quote::quote;
use syn::{LitStr, Token, parse_macro_input};

#[proc_macro]
pub fn generate_file_tests(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as FileTestInput);

    let dir_path = &input.directory;
    let files = &input.files;

    let test_functions = files.iter().map(|file| {
        let filename = &file.value();
        let test_name_ident = syn::Ident::new(
            &format!("test_{}", filename.replace('.', "_")),
            proc_macro2::Span::call_site(),
        );

        quote! {
            #[test]
            fn #test_name_ident() {
                let input_path = ::std::path::PathBuf::from(#dir_path).join(#filename);
                let temp_file = ::tempfile::NamedTempFile::new().unwrap();
                ::marimow::run_convert_command(&input_path, &temp_file.path()).unwrap();
                let output_path = {
                    let stem = input_path.file_stem().unwrap().to_string_lossy();
                    input_path.with_file_name(format!("{}_output.py", stem))
                };
                assert_eq!(
                    std::fs::read_to_string(&temp_file.path()).unwrap(),
                    std::fs::read_to_string(&output_path).unwrap(),
                );
            }
        }
    });

    let expanded = quote! {
        #(#test_functions)*
    };

    TokenStream::from(expanded)
}

struct FileTestInput {
    directory: LitStr,
    files: Vec<LitStr>,
}

impl syn::parse::Parse for FileTestInput {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let directory = input.parse::<LitStr>()?;
        input.parse::<Token![;]>()?;

        let files = syn::punctuated::Punctuated::<LitStr, syn::Token![,]>::parse_terminated(input)?;

        Ok(FileTestInput {
            directory,
            files: files.into_iter().collect(),
        })
    }
}
