use proc_macro::TokenStream;
use quote::quote;
use syn::{Attribute, Data, DataEnum, DeriveInput, Fields, Ident, LitStr, Variant, parse_macro_input};

#[proc_macro_derive(BuiltinCommandGroup, attributes(command))]
pub fn derive_builtin_command_group(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	match expand_builtin_command_group(&input) {
		Ok(tokens) => tokens.into(),
		Err(err) => err.into_compile_error().into(),
	}
}

#[proc_macro_derive(BuiltinCommandRoot, attributes(command))]
pub fn derive_builtin_command_root(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	match expand_builtin_command_root(&input) {
		Ok(tokens) => tokens.into(),
		Err(err) => err.into_compile_error().into(),
	}
}

fn expand_builtin_command_group(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
	let enum_name = &input.ident;
	let data = expect_enum(&input.data)?;
	let variants = data.variants.iter().collect::<Vec<_>>();

	let all_variants = variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		quote! { #enum_name::#variant_name }
	});
	let segment_arms = variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let segment = command_segment_for_variant(variant);
		quote! { Self::#variant_name => #segment }
	});
	let description_arms = variants
		.iter()
		.map(|variant| {
			let variant_name = &variant.ident;
			let description = command_description_for_variant(variant)?;
			Ok(quote! { Self::#variant_name => #description })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let arg_kind_arms = variants
		.iter()
		.map(|variant| {
			let variant_name = &variant.ident;
			let arg_kind = command_arg_kind_for_variant(variant)?;
			Ok(quote! { Self::#variant_name => #arg_kind })
		})
		.collect::<syn::Result<Vec<_>>>()?;

	Ok(quote! {
		impl crate::command::BuiltinCommandGroupMeta for #enum_name {
			fn command_segment(self) -> &'static str {
				match self {
					#(#segment_arms,)*
				}
			}

			fn description(self) -> &'static str {
				match self {
					#(#description_arms,)*
				}
			}

			fn arg_kind(self) -> crate::command::CommandArgKind {
				match self {
					#(#arg_kind_arms,)*
				}
			}

			fn all_commands() -> &'static [Self] {
				static ALL: &[#enum_name] = &[#(#all_variants,)*];
				ALL
			}
		}
	})
}

fn expand_builtin_command_root(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
	let enum_name = &input.ident;
	let data = expect_enum(&input.data)?;
	let variants = data.variants.iter().collect::<Vec<_>>();

	let category_arms = variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		quote! { Self::#variant_name(..) => crate::command::BuiltinCommandCategory::#variant_name }
	});
	let description_arms = variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let field_ty = expect_single_field_type(variant)?;
		Ok(quote! { Self::#variant_name(inner) => <#field_ty as crate::command::BuiltinCommandGroupMeta>::description(inner) })
	});
	let description_arms = description_arms.collect::<syn::Result<Vec<_>>>()?;
	let arg_kind_arms = variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let field_ty = expect_single_field_type(variant)?;
		Ok(quote! { Self::#variant_name(inner) => <#field_ty as crate::command::BuiltinCommandGroupMeta>::arg_kind(inner) })
	});
	let arg_kind_arms = arg_kind_arms.collect::<syn::Result<Vec<_>>>()?;

	let all_segments = variants
		.iter()
		.map(|variant| {
			let variant_name = &variant.ident;
			let field_ty = expect_single_field_type(variant)?;
			Ok(quote! {
				<#field_ty as crate::command::BuiltinCommandGroupMeta>::all_commands()
					.iter()
					.copied()
					.map(Self::#variant_name)
			})
		})
		.collect::<syn::Result<Vec<_>>>()?;

	let from_id_arms = variants
		.iter()
		.map(|variant| {
			let variant_name = &variant.ident;
			let field_ty = expect_single_field_type(variant)?;
			let namespace = command_namespace_for_root_variant(variant)?;
			let prefix = if namespace.is_empty() { "core".to_string() } else { format!("core.{}", namespace) };
			Ok(quote! {
				for candidate in <#field_ty as crate::command::BuiltinCommandGroupMeta>::all_commands() {
					if id == format!("{}.{}", #prefix, candidate.command_segment()) {
						return Some(Self::#variant_name(candidate.to_owned()));
					}
				}
			})
		})
		.collect::<syn::Result<Vec<_>>>()?;

	let display_id_arms = variants
		.iter()
		.map(|variant| {
			let variant_name = &variant.ident;
			let namespace = command_namespace_for_root_variant(variant)?;
			let prefix = if namespace.is_empty() { "core".to_string() } else { format!("core.{}", namespace) };
			Ok(quote! {
				Self::#variant_name(inner) => {
					format!("{}.{}", #prefix, inner.command_segment())
				}
			})
		})
		.collect::<syn::Result<Vec<_>>>()?;

	Ok(quote! {
		impl crate::command::BuiltinCommandRootMeta for #enum_name {
			fn id(self) -> String {
				match self {
					#(#display_id_arms,)*
				}
			}

			fn category(self) -> crate::command::BuiltinCommandCategory {
				match self {
					#(#category_arms,)*
				}
			}

			fn description(self) -> &'static str {
				match self {
					#(#description_arms,)*
				}
			}

			fn arg_kind(self) -> crate::command::CommandArgKind {
				match self {
					#(#arg_kind_arms,)*
				}
			}

			fn all_commands() -> Vec<Self> {
				let mut commands = Vec::new();
				#(commands.extend(#all_segments);)*
				commands
			}

			fn from_id(id: &str) -> Option<Self> {
				#(#from_id_arms)*
				None
			}
		}
	})
}

fn expect_enum(data: &Data) -> syn::Result<&DataEnum> {
	match data {
		Data::Enum(data) => Ok(data),
		_ => Err(syn::Error::new(proc_macro2::Span::call_site(), "derive only supports enums")),
	}
}

fn expect_single_field_type(variant: &Variant) -> syn::Result<&syn::Type> {
	match &variant.fields {
		Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
			Ok(&fields.unnamed.first().expect("checked len").ty)
		}
		_ => Err(syn::Error::new_spanned(variant, "root enum variants must be single-field tuple variants")),
	}
}

fn command_segment_for_variant(variant: &Variant) -> String {
	variant.ident.to_string().chars().enumerate().fold(String::new(), |mut acc, (idx, ch)| {
		if ch.is_uppercase() {
			if idx > 0 {
				acc.push('_');
			}
			acc.extend(ch.to_lowercase());
		} else {
			acc.push(ch);
		}
		acc
	})
}

fn command_description_for_variant(variant: &Variant) -> syn::Result<String> {
	let docs = doc_lines(&variant.attrs);
	if docs.is_empty() {
		return Err(syn::Error::new_spanned(variant, "builtin command variants require a doc comment"));
	}
	Ok(docs.join(" ").trim().to_string())
}

fn command_arg_kind_for_variant(variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	for attr in &variant.attrs {
		if !attr.path().is_ident("command") {
			continue;
		}
		let mut parsed = None;
		attr.parse_nested_meta(|meta| {
			if meta.path.is_ident("arg") || meta.path.is_ident("arg_kind") {
				let value = meta.value()?;
				let ident: Ident = value.parse()?;
				parsed = Some(quote! { crate::command::CommandArgKind::#ident });
			}
			Ok(())
		})?;
		if let Some(arg_kind) = parsed {
			return Ok(arg_kind);
		}
	}
	Ok(quote! { crate::command::CommandArgKind::None })
}

fn command_namespace_for_root_variant(variant: &Variant) -> syn::Result<String> {
	for attr in &variant.attrs {
		if !attr.path().is_ident("command") {
			continue;
		}
		let mut namespace = None;
		attr.parse_nested_meta(|meta| {
			if meta.path.is_ident("namespace") {
				let value = meta.value()?;
				let text: LitStr = value.parse()?;
				namespace = Some(text.value());
			}
			Ok(())
		})?;
		if let Some(namespace) = namespace {
			return Ok(namespace);
		}
	}
	Ok(command_segment_for_variant(variant))
}

fn doc_lines(attrs: &[Attribute]) -> Vec<String> {
	let mut docs = Vec::new();
	for attr in attrs {
		if !attr.path().is_ident("doc") {
			continue;
		}
		let syn::Meta::NameValue(name_value) = &attr.meta else {
			continue;
		};
		let syn::Expr::Lit(expr_lit) = &name_value.value else {
			continue;
		};
		let syn::Lit::Str(lit) = &expr_lit.lit else {
			continue;
		};
		let line = lit.value().trim().to_string();
		if !line.is_empty() {
			docs.push(line);
		}
	}
	docs
}
