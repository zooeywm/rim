use proc_macro::TokenStream;
use quote::{format_ident, quote};
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

#[proc_macro_derive(PluginCommandSet)]
pub fn derive_plugin_command_set(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	match expand_plugin_command_set(&input) {
		Ok(tokens) => tokens.into(),
		Err(err) => err.into_compile_error().into(),
	}
}

fn expand_builtin_command_group(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
	let enum_name = &input.ident;
	let data = expect_enum(&input.data)?;
	let variants = data.variants.iter().collect::<Vec<_>>();

	let all_variants = variants
		.iter()
		.map(|variant| command_constructor(enum_name, variant))
		.collect::<syn::Result<Vec<_>>>()?;
	let segment_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let segment = command_segment_for_variant(variant);
			Ok(quote! { #pattern => #segment })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let description_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let description = command_description_for_variant(variant)?;
			Ok(quote! { #pattern => #description })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let param_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let params = command_params_for_variant(variant)?;
			Ok(quote! { #pattern => #params })
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

			fn params(self) -> &'static [crate::command::BuiltinCommandParamSpec] {
				match self {
					#(#param_arms,)*
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
	let param_arms = variants.iter().map(|variant| {
		let variant_name = &variant.ident;
		let field_ty = expect_single_field_type(variant)?;
		Ok(quote! { Self::#variant_name(inner) => <#field_ty as crate::command::BuiltinCommandGroupMeta>::params(inner) })
	});
	let param_arms = param_arms.collect::<syn::Result<Vec<_>>>()?;

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

			fn params(self) -> &'static [crate::command::BuiltinCommandParamSpec] {
				match self {
					#(#param_arms,)*
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

fn command_params_for_variant(variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	if let Fields::Named(fields) = &variant.fields {
		let params = fields.named.iter().map(command_param_spec_from_field).collect::<syn::Result<Vec<_>>>()?;
		return Ok(quote! { &[#(#params,)*] });
	}
	if !matches!(variant.fields, Fields::Unit) {
		return Err(syn::Error::new_spanned(
			&variant.fields,
			"builtin command variants must be unit or struct-like with named fields",
		));
	}
	for attr in &variant.attrs {
		if !attr.path().is_ident("command") {
			continue;
		}
		let mut parsed_params = None;
		attr.parse_nested_meta(|meta| {
			if meta.path.is_ident("params") {
				let mut params = Vec::new();
				meta.parse_nested_meta(|param_meta| {
					let name = param_meta.path.get_ident().ok_or_else(|| {
						syn::Error::new_spanned(&param_meta.path, "parameter name must be an identifier")
					})?;
					let value = param_meta.value()?;
					let ident: Ident = value.parse()?;
					params.push(command_param_spec_tokens(name.to_string().as_str(), &ident, false)?);
					Ok(())
				})?;
				parsed_params = Some(quote! { &[#(#params,)*] });
			}
			Ok(())
		})?;
		if let Some(params) = parsed_params {
			return Ok(params);
		}
	}
	Ok(quote! { &[] })
}

fn command_match_pattern(variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	let variant_name = &variant.ident;
	match &variant.fields {
		Fields::Unit => Ok(quote! { Self::#variant_name }),
		Fields::Named(_) => Ok(quote! { Self::#variant_name { .. } }),
		Fields::Unnamed(_) => Err(syn::Error::new_spanned(
			&variant.fields,
			"builtin command variants must be unit or struct-like with named fields",
		)),
	}
}

fn command_constructor(enum_name: &Ident, variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	let variant_name = &variant.ident;
	match &variant.fields {
		Fields::Unit => Ok(quote! { #enum_name::#variant_name }),
		Fields::Named(fields) => {
			let field_values =
				fields.named.iter().map(command_field_initializer).collect::<syn::Result<Vec<_>>>()?;
			Ok(quote! { #enum_name::#variant_name { #(#field_values,)* } })
		}
		Fields::Unnamed(_) => Err(syn::Error::new_spanned(
			&variant.fields,
			"builtin command variants must be unit or struct-like with named fields",
		)),
	}
}

fn command_param_spec_from_field(field: &syn::Field) -> syn::Result<proc_macro2::TokenStream> {
	let name = field
		.ident
		.as_ref()
		.ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))?
		.to_string();
	let (marker_type, optional) = option_inner_type(&field.ty).unwrap_or((&field.ty, false));
	let marker_ident = marker_type_ident(marker_type)?;
	command_param_spec_tokens(name.as_str(), marker_ident, optional)
}

fn command_field_initializer(field: &syn::Field) -> syn::Result<proc_macro2::TokenStream> {
	let field_name =
		field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))?;
	if option_inner_type(&field.ty).is_some() {
		return Ok(quote! { #field_name: None });
	}
	let marker_ident = marker_type_ident(&field.ty)?;
	let initializer = match marker_ident.to_string().as_str() {
		"File" => quote! { crate::command::File },
		"Text" => quote! { crate::command::Text },
		other => {
			return Err(syn::Error::new_spanned(
				&field.ty,
				format!("unsupported command parameter marker type: {}", other),
			));
		}
	};
	Ok(quote! { #field_name: #initializer })
}

fn option_inner_type(ty: &syn::Type) -> Option<(&syn::Type, bool)> {
	let syn::Type::Path(type_path) = ty else {
		return None;
	};
	let segment = type_path.path.segments.last()?;
	if segment.ident != "Option" {
		return None;
	}
	let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
		return None;
	};
	let inner = args.args.first()?;
	let syn::GenericArgument::Type(inner_ty) = inner else {
		return None;
	};
	Some((inner_ty, true))
}

fn marker_type_ident(ty: &syn::Type) -> syn::Result<&Ident> {
	let syn::Type::Path(type_path) = ty else {
		return Err(syn::Error::new_spanned(
			ty,
			"command parameter fields must use File, Text, or Option<T> marker types",
		));
	};
	type_path
		.path
		.segments
		.last()
		.map(|segment| &segment.ident)
		.ok_or_else(|| syn::Error::new_spanned(ty, "unsupported command parameter type"))
}

fn command_param_spec_tokens(
	name: &str,
	ident: &Ident,
	optional: bool,
) -> syn::Result<proc_macro2::TokenStream> {
	let (kind, inferred_optional) = match ident.to_string().as_str() {
		"String" | "Text" => (quote! { crate::command::CommandArgKind::Text }, false),
		"OptionalString" | "OptionalText" => (quote! { crate::command::CommandArgKind::Text }, true),
		"Path" | "File" => (quote! { crate::command::CommandArgKind::File }, false),
		"OptionalPath" | "OptionalFile" => (quote! { crate::command::CommandArgKind::File }, true),
		other => {
			return Err(syn::Error::new_spanned(ident, format!("unsupported command parameter kind: {}", other)));
		}
	};
	let optional = optional || inferred_optional;
	Ok(quote! {
		crate::command::BuiltinCommandParamSpec {
			name: #name,
			kind: #kind,
			optional: #optional,
		}
	})
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

fn expand_plugin_command_set(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
	let enum_name = &input.ident;
	let decoded_enum_name = format_ident!("{}Decoded", enum_name);
	let data = expect_enum(&input.data)?;
	let variants = data.variants.iter().collect::<Vec<_>>();

	let all_variants = variants
		.iter()
		.map(|variant| plugin_command_constructor(enum_name, variant))
		.collect::<syn::Result<Vec<_>>>()?;
	let id_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let command_id = plugin_command_id_for_variant(variant);
			Ok(quote! { #pattern => #command_id })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let name_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let command_name = variant.ident.to_string();
			Ok(quote! { #pattern => #command_name })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let description_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let description = command_description_for_variant(variant)?;
			Ok(quote! { #pattern => #description })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let param_arms = variants
		.iter()
		.map(|variant| {
			let pattern = command_match_pattern(variant)?;
			let params = plugin_command_params_for_variant(variant)?;
			Ok(quote! { #pattern => #params })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let from_id_arms = variants
		.iter()
		.map(|variant| {
			let command_id = plugin_command_id_for_variant(variant);
			let constructor = plugin_command_constructor(enum_name, variant)?;
			Ok(quote! { #command_id => Some(#constructor) })
		})
		.collect::<syn::Result<Vec<_>>>()?;
	let decoded_variants =
		variants.iter().map(|variant| plugin_decoded_variant(variant)).collect::<syn::Result<Vec<_>>>()?;
	let decode_arms = variants
		.iter()
		.map(|variant| plugin_decode_arm(enum_name, &decoded_enum_name, variant))
		.collect::<syn::Result<Vec<_>>>()?;

	Ok(quote! {
		#[derive(Debug, Clone, PartialEq, Eq)]
		pub enum #decoded_enum_name {
			#(#decoded_variants,)*
		}

		impl ::rim_plugin_api::PluginCommandSetMeta for #enum_name {
			fn command_id(self) -> &'static str {
				match self {
					#(#id_arms,)*
				}
			}

			fn command_name(self) -> &'static str {
				match self {
					#(#name_arms,)*
				}
			}

			fn description(self) -> &'static str {
				match self {
					#(#description_arms,)*
				}
			}

			fn params(self) -> &'static [::rim_plugin_api::PluginCommandParamStaticSpec] {
				match self {
					#(#param_arms,)*
				}
			}

			fn all_commands() -> &'static [Self] {
				static ALL: &[#enum_name] = &[#(#all_variants,)*];
				ALL
			}

			fn from_command_id(command_id: &str) -> Option<Self> {
				match command_id {
					#(#from_id_arms,)*
					_ => None,
				}
			}
		}

		impl #enum_name {
			pub fn get(request: &::rim_plugin_api::PluginCommandRequest) -> Option<Self> {
				<Self as ::rim_plugin_api::PluginCommandSetMeta>::from_command_id(request.command_id.as_str())
			}

			pub fn params(
				request: &::rim_plugin_api::PluginCommandRequest,
			) -> Option<::rim_plugin_api::PluginResolvedParams> {
				let command = Self::get(request)?;
				::rim_plugin_api::decode_plugin_params(
					<Self as ::rim_plugin_api::PluginCommandSetMeta>::params(command),
					request,
				)
				.ok()
			}

			pub fn decode(
				request: &::rim_plugin_api::PluginCommandRequest,
			) -> Result<#decoded_enum_name, ::rim_plugin_api::PluginCommandError> {
				let command = Self::get(request).ok_or_else(|| {
					::rim_plugin_api::PluginCommandError::CommandUnavailable {
						command_id: request.command_id.clone(),
					}
				})?;
				let resolved = ::rim_plugin_api::decode_plugin_params(
					<Self as ::rim_plugin_api::PluginCommandSetMeta>::params(command),
					request,
				)?;
				match command {
					#(#decode_arms,)*
				}
			}
		}
	})
}

fn plugin_command_id_for_variant(variant: &Variant) -> String {
	variant.ident.to_string().chars().enumerate().fold(String::new(), |mut acc, (idx, ch)| {
		if ch.is_uppercase() {
			if idx > 0 {
				acc.push('-');
			}
			acc.extend(ch.to_lowercase());
		} else {
			acc.push(ch);
		}
		acc
	})
}

fn plugin_command_params_for_variant(variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	match &variant.fields {
		Fields::Unit => Ok(quote! { &[] }),
		Fields::Named(fields) => {
			let params =
				fields.named.iter().map(plugin_command_param_spec_from_field).collect::<syn::Result<Vec<_>>>()?;
			Ok(quote! { &[#(#params,)*] })
		}
		Fields::Unnamed(_) => Err(syn::Error::new_spanned(
			&variant.fields,
			"plugin command variants must be unit or struct-like with named fields",
		)),
	}
}

fn plugin_decoded_variant(variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	let variant_name = &variant.ident;
	match &variant.fields {
		Fields::Unit => Ok(quote! { #variant_name }),
		Fields::Named(fields) => {
			let decoded_fields = fields.named.iter().map(plugin_decoded_field).collect::<syn::Result<Vec<_>>>()?;
			Ok(quote! { #variant_name { #(#decoded_fields,)* } })
		}
		Fields::Unnamed(_) => Err(syn::Error::new_spanned(
			&variant.fields,
			"plugin command variants must be unit or struct-like with named fields",
		)),
	}
}

fn plugin_decoded_field(field: &syn::Field) -> syn::Result<proc_macro2::TokenStream> {
	let field_name =
		field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))?;
	let ty = plugin_decoded_field_type(&field.ty)?;
	Ok(quote! { #field_name: #ty })
}

fn plugin_decoded_field_type(ty: &syn::Type) -> syn::Result<proc_macro2::TokenStream> {
	let (marker_type, optional) = option_inner_type(ty).unwrap_or((ty, false));
	let marker_ident = marker_type_ident(marker_type)?;
	match (marker_ident.to_string().as_str(), optional) {
		("Text" | "File", true) => Ok(quote! { Option<String> }),
		("Text" | "File", false) => Ok(quote! { String }),
		(other, _) => {
			Err(syn::Error::new_spanned(ty, format!("unsupported plugin command parameter marker type: {}", other)))
		}
	}
}

fn plugin_decode_arm(
	enum_name: &Ident,
	decoded_enum_name: &Ident,
	variant: &Variant,
) -> syn::Result<proc_macro2::TokenStream> {
	let variant_name = &variant.ident;
	match &variant.fields {
		Fields::Unit => Ok(quote! { #enum_name::#variant_name => Ok(#decoded_enum_name::#variant_name) }),
		Fields::Named(fields) => {
			let field_bindings =
				fields.named.iter().map(plugin_decode_field_binding).collect::<syn::Result<Vec<_>>>()?;
			let field_names = fields
				.named
				.iter()
				.map(|field| {
					field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))
				})
				.collect::<syn::Result<Vec<_>>>()?;
			Ok(quote! {
				#enum_name::#variant_name { .. } => {
					#(#field_bindings)*
					Ok(#decoded_enum_name::#variant_name { #(#field_names,)* })
				}
			})
		}
		Fields::Unnamed(_) => Err(syn::Error::new_spanned(
			&variant.fields,
			"plugin command variants must be unit or struct-like with named fields",
		)),
	}
}

fn plugin_decode_field_binding(field: &syn::Field) -> syn::Result<proc_macro2::TokenStream> {
	let field_name =
		field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))?;
	let field_name_str = field_name.to_string();
	let (marker_type, optional) = option_inner_type(&field.ty).unwrap_or((&field.ty, false));
	let marker_ident = marker_type_ident(marker_type)?;
	let getter = match marker_ident.to_string().as_str() {
		"Text" => quote! { get_text },
		"File" => quote! { get_file },
		other => {
			return Err(syn::Error::new_spanned(
				&field.ty,
				format!("unsupported plugin command parameter marker type: {}", other),
			));
		}
	};
	if optional {
		return Ok(quote! {
			let #field_name = resolved.#getter(#field_name_str).map(|value| value.to_string());
		});
	}
	Ok(quote! {
		let #field_name = resolved.#getter(#field_name_str).map(|value| value.to_string()).ok_or_else(|| {
			::rim_plugin_api::PluginCommandError::InvalidRequest {
				message: format!("missing required parameter '{}'", #field_name_str),
			}
		})?;
	})
}

fn plugin_command_constructor(enum_name: &Ident, variant: &Variant) -> syn::Result<proc_macro2::TokenStream> {
	let variant_name = &variant.ident;
	match &variant.fields {
		Fields::Unit => Ok(quote! { #enum_name::#variant_name }),
		Fields::Named(fields) => {
			let field_values =
				fields.named.iter().map(plugin_command_field_initializer).collect::<syn::Result<Vec<_>>>()?;
			Ok(quote! { #enum_name::#variant_name { #(#field_values,)* } })
		}
		Fields::Unnamed(_) => Err(syn::Error::new_spanned(
			&variant.fields,
			"plugin command variants must be unit or struct-like with named fields",
		)),
	}
}

fn plugin_command_param_spec_from_field(field: &syn::Field) -> syn::Result<proc_macro2::TokenStream> {
	let name = field
		.ident
		.as_ref()
		.ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))?
		.to_string();
	let (marker_type, optional) = option_inner_type(&field.ty).unwrap_or((&field.ty, false));
	let marker_ident = marker_type_ident(marker_type)?;
	plugin_command_param_spec_tokens(name.as_str(), marker_ident, optional)
}

fn plugin_command_field_initializer(field: &syn::Field) -> syn::Result<proc_macro2::TokenStream> {
	let field_name =
		field.ident.as_ref().ok_or_else(|| syn::Error::new_spanned(field, "parameter field must be named"))?;
	if option_inner_type(&field.ty).is_some() {
		return Ok(quote! { #field_name: None });
	}
	let marker_ident = marker_type_ident(&field.ty)?;
	let initializer = match marker_ident.to_string().as_str() {
		"File" => quote! { ::rim_plugin_api::File },
		"Text" => quote! { ::rim_plugin_api::Text },
		other => {
			return Err(syn::Error::new_spanned(
				&field.ty,
				format!("unsupported plugin command parameter marker type: {}", other),
			));
		}
	};
	Ok(quote! { #field_name: #initializer })
}

fn plugin_command_param_spec_tokens(
	name: &str,
	ident: &Ident,
	optional: bool,
) -> syn::Result<proc_macro2::TokenStream> {
	let (kind, inferred_optional) = match ident.to_string().as_str() {
		"Text" | "String" => (quote! { ::rim_plugin_api::PluginCommandParamKind::Text }, false),
		"OptionalText" | "OptionalString" => (quote! { ::rim_plugin_api::PluginCommandParamKind::Text }, true),
		"File" | "Path" => (quote! { ::rim_plugin_api::PluginCommandParamKind::File }, false),
		"OptionalFile" | "OptionalPath" => (quote! { ::rim_plugin_api::PluginCommandParamKind::File }, true),
		other => {
			return Err(syn::Error::new_spanned(
				ident,
				format!("unsupported plugin command parameter kind: {}", other),
			));
		}
	};
	let optional = optional || inferred_optional;
	Ok(quote! {
		::rim_plugin_api::PluginCommandParamStaticSpec {
			name: #name,
			kind: #kind,
			optional: #optional,
		}
	})
}
