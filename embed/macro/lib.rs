#![recursion_limit = "1024"]
#![forbid(unsafe_code)]
#[macro_use]
extern crate quote;
extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use std::{collections::BTreeMap, env, path::Path};
use syn::{parse_macro_input, Data, DeriveInput, Expr, ExprLit, Fields, Lit, Meta, MetaNameValue};

fn embedded(
    ident: &syn::Ident,
    relative_folder_path: Option<&str>,
    absolute_folder_path: String,
    prefix: Option<&str>,
    includes: &[String],
    excludes: &[String],
) -> syn::Result<TokenStream2> {
    extern crate prest_embed_utils;
    use prest_embed_utils::*;

    let mut match_values = BTreeMap::new();
    let mut list_values = Vec::<String>::new();

    let includes: Vec<&str> = includes.iter().map(AsRef::as_ref).collect();
    let excludes: Vec<&str> = excludes.iter().map(AsRef::as_ref).collect();
    for FileEntry {
        rel_path,
        full_canonical_path,
    } in get_files(absolute_folder_path.clone(), &includes, &excludes)
    {
        match_values.insert(
            rel_path.clone(),
            embed_file(
                relative_folder_path.clone(),
                ident,
                &rel_path,
                &full_canonical_path,
            )?,
        );

        list_values.push(if let Some(prefix) = prefix {
            format!("{}{}", prefix, rel_path)
        } else {
            rel_path
        });
    }

    let array_len = list_values.len();

    let not_debug_attr = quote! { #[cfg(any(not(debug_assertions), target_arch = "wasm32"))]};

    let handle_prefix = if let Some(prefix) = prefix {
        quote! {
          let file_path = file_path.strip_prefix(#prefix)?;
        }
    } else {
        TokenStream2::new()
    };
    let match_values = match_values.into_iter().map(|(path, bytes)| {
        quote! {
            (#path, #bytes),
        }
    });
    let value_type = quote! { prest::embed::EmbeddedFile };
    let get_value = quote! {|idx| ENTRIES[idx].1.clone()};

    Ok(quote! {
        #not_debug_attr
        impl #ident {
            /// Get an embedded file and its metadata.
            pub fn get(file_path: &str) -> Option<prest::embed::EmbeddedFile> {
              #handle_prefix
              let key = file_path.replace("\\", "/");
              const ENTRIES: &'static [(&'static str, #value_type)] = &[
                  #(#match_values)*];
              let position = ENTRIES.binary_search_by_key(&key.as_str(), |entry| entry.0);
              position.ok().map(#get_value)

            }

            fn names() -> std::slice::Iter<'static, &'static str> {
                const ITEMS: [&str; #array_len] = [#(#list_values),*];
                ITEMS.iter()
            }

            /// Iterates over the file paths in the folder.
            pub fn iter() -> impl Iterator<Item = std::borrow::Cow<'static, str>> {
                Self::names().map(|x| std::borrow::Cow::from(*x))
            }
        }

        #not_debug_attr
        impl prest::EmbeddedStruct for #ident {
          fn get(file_path: &str) -> Option<prest::embed::EmbeddedFile> {
            #ident::get(file_path)
          }
          fn iter() -> prest::embed::__Filenames {
            prest::embed::__Filenames::Embedded(#ident::names())
          }
        }
    })
}

fn dynamic(
    ident: &syn::Ident,
    folder_path: String,
    prefix: Option<&str>,
    includes: &[String],
    excludes: &[String],
) -> TokenStream2 {
    let (handle_prefix, map_iter) = if let Some(prefix) = prefix {
        (
            quote! { let file_path = file_path.strip_prefix(#prefix)?; },
            quote! { std::borrow::Cow::Owned(format!("{}{}", #prefix, e.rel_path)) },
        )
    } else {
        (
            TokenStream2::new(),
            quote! { std::borrow::Cow::from(e.rel_path) },
        )
    };

    let declare_includes = quote! {
      const INCLUDES: &[&str] = &[#(#includes),*];
    };

    let declare_excludes = quote! {
      const EXCLUDES: &[&str] = &[#(#excludes),*];
    };

    let canonical_folder_path = Path::new(&folder_path)
        .canonicalize()
        .expect("folder path must resolve to an absolute path");
    let canonical_folder_path = canonical_folder_path
        .to_str()
        .expect("absolute folder path must be valid unicode");

    quote! {
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        impl #ident {
            /// Get an embedded file and its metadata.
            pub fn get(file_path: &str) -> Option<prest::embed::EmbeddedFile> {
                #handle_prefix

                #declare_includes
                #declare_excludes

                let rel_file_path = file_path.replace("\\", "/");
                let file_path = std::path::Path::new(#folder_path).join(&rel_file_path);

                // Make sure the path requested does not escape the folder path
                let canonical_file_path = file_path.canonicalize().ok()?;
                if !canonical_file_path.starts_with(#canonical_folder_path) {
                    // Tried to request a path that is not in the embedded folder
                    return None;
                }

                if prest::embed::is_path_included(&rel_file_path, INCLUDES, EXCLUDES) {
                  prest::embed::read_file_from_fs(&canonical_file_path).ok()
                } else {
                  None
                }
            }

            /// Iterates over the file paths in the folder.
            pub fn iter() -> impl Iterator<Item = std::borrow::Cow<'static, str>> {
                use std::path::Path;

                #declare_includes
                #declare_excludes

                prest::embed::get_files(String::from(#folder_path), INCLUDES, EXCLUDES)
                    .map(|e| #map_iter)
            }
        }

        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        impl prest::EmbeddedStruct for #ident {
          fn get(file_path: &str) -> Option<prest::embed::EmbeddedFile> {
            #ident::get(file_path)
          }
          fn iter() -> prest::embed::__Filenames {
            // the return type of iter() is unnamable, so we have to box it
            prest::embed::__Filenames::Dynamic(Box::new(#ident::iter()))
          }
        }
    }
}

fn generate_assets(
    ident: &syn::Ident,
    relative_folder_path: Option<&str>,
    absolute_folder_path: String,
    prefix: Option<String>,
    includes: Vec<String>,
    excludes: Vec<String>,
) -> syn::Result<TokenStream2> {
    let embedded_impl = embedded(
        ident,
        relative_folder_path,
        absolute_folder_path.clone(),
        prefix.as_deref(),
        &includes,
        &excludes,
    );
    let embedded_impl = embedded_impl?;
    let dynamic_impl = dynamic(
        ident,
        absolute_folder_path,
        prefix.as_deref(),
        &includes,
        &excludes,
    );

    Ok(quote! {
        #embedded_impl
        #dynamic_impl
    })
}

fn embed_file(
    _folder_path: Option<&str>,
    _ident: &syn::Ident,
    _rel_path: &str,
    full_canonical_path: &str,
) -> syn::Result<TokenStream2> {
    let file = prest_embed_utils::read_file_from_fs(Path::new(full_canonical_path))
        .expect("File should be readable");
    let hash = file.metadata.sha256_hash();
    let last_modified = match file.metadata.last_modified() {
        Some(last_modified) => quote! { Some(#last_modified) },
        None => quote! { None },
    };
    let mimetype_tokens = {
        let mt = file.metadata.mimetype();
        quote! { , #mt }
    };

    let embedding_code = quote! {
      const BYTES: &'static [u8] = include_bytes!(#full_canonical_path);
    };

    let closure_args = quote! {};
    Ok(quote! {
         #closure_args {
          #embedding_code

          prest::embed::EmbeddedFile {
              data: std::borrow::Cow::Borrowed(&BYTES),
              metadata: prest::embed::EmbeddedFileMetadata::__rust_embed_new([#(#hash),*], #last_modified #mimetype_tokens)
          }
        }
    })
}

/// Find all pairs of the `name = "value"` attribute from the derive input
fn find_attribute_values(ast: &syn::DeriveInput, attr_name: &str) -> Vec<String> {
    ast.attrs
        .iter()
        .filter(|value| value.path().is_ident(attr_name))
        .filter_map(|attr| match &attr.meta {
            Meta::NameValue(MetaNameValue {
                value:
                    Expr::Lit(ExprLit {
                        lit: Lit::Str(val), ..
                    }),
                ..
            }) => Some(val.value()),
            _ => None,
        })
        .collect()
}

fn impl_rust_embed(ast: &syn::DeriveInput) -> syn::Result<TokenStream2> {
    match ast.data {
        Data::Struct(ref data) => match data.fields {
            Fields::Unit => {}
            _ => {
                return Err(syn::Error::new_spanned(
                    ast,
                    "Embed can only be derived for unit structs",
                ))
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                ast,
                "Embed can only be derived for unit structs",
            ))
        }
    };

    let mut folder_paths = find_attribute_values(ast, "folder");
    if folder_paths.len() != 1 {
        return Err(syn::Error::new_spanned(
            ast,
            "Embed must contain one attribute like this #[folder = \"examples/public/\"]",
        ));
    }
    let folder_path = folder_paths.remove(0);

    let prefix = find_attribute_values(ast, "prefix").into_iter().next();
    let includes = find_attribute_values(ast, "include");
    let excludes = find_attribute_values(ast, "exclude");

    let folder_path = shellexpand::full(&folder_path)
        .map_err(|v| syn::Error::new_spanned(ast, v.to_string()))?
        .to_string();

    // Base relative paths on the Cargo.toml location
    let (relative_path, absolute_folder_path) = if Path::new(&folder_path).is_relative() {
        let absolute_path = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
            .join(&folder_path)
            .to_str()
            .unwrap()
            .to_owned();
        (Some(folder_path.clone()), absolute_path)
    } else {
        (None, folder_path)
    };

    if !Path::new(&absolute_folder_path).exists() {
        let message = format!(
            "The embedded folder '{}' does not exist. cwd: '{}'",
            absolute_folder_path,
            std::env::current_dir().unwrap().to_str().unwrap()
        );

        return Err(syn::Error::new_spanned(ast, message));
    };

    generate_assets(
        &ast.ident,
        relative_path.as_deref(),
        absolute_folder_path,
        prefix,
        includes,
        excludes,
    )
}

/// Derive macro that embeds files and provides access to them through the given struct
#[proc_macro_derive(Embed, attributes(folder, prefix, include, exclude))]
pub fn derive_input_object(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);
    match impl_rust_embed(&ast) {
        Ok(ok) => ok.into(),
        Err(e) => e.to_compile_error().into(),
    }
}
