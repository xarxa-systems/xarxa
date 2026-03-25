/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, LitStr};

/// Registers a function as a named workflow.
///
/// ```ignore
/// #[workflow("booking")]
/// fn booking_flow(input: BookingInput) -> PipelineState { ... }
/// ```
#[proc_macro_attribute]
pub fn workflow(attr: TokenStream, item: TokenStream) -> TokenStream {
    let workflow_name = parse_macro_input!(attr as LitStr);
    let input_fn = parse_macro_input!(item as ItemFn);

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = fn_name.to_string();

    let inputs = &input_fn.sig.inputs;
    if inputs.len() != 1 {
        return syn::Error::new_spanned(
            &input_fn.sig,
            "#[workflow] function must take exactly one argument (the deserialized input)",
        )
        .to_compile_error()
        .into();
    }

    let input_type = match inputs.first().unwrap() {
        syn::FnArg::Typed(pat_type) => &pat_type.ty,
        _ => {
            return syn::Error::new_spanned(
                inputs,
                "#[workflow] function must not use `self`",
            )
            .to_compile_error()
            .into();
        }
    };

    let registration_ident = quote::format_ident!("__xarxa_register_{}", fn_name_str);

    let expanded = quote! {
        #input_fn

        #[doc(hidden)]
        pub struct #registration_ident;

        ::xarxa_sdk::inventory::submit! {
            ::xarxa_sdk::WorkflowRegistration {
                name: #workflow_name,
                start: |raw_input: &[u8]| -> ::xarxa_sdk::PipelineState {
                    let input: #input_type = ::xarxa_sdk::serde_json::from_slice(raw_input)
                        .unwrap_or_else(|e| panic!(
                            "Failed to deserialize input for workflow '{}': {}",
                            #workflow_name, e
                        ));
                    #fn_name(input)
                },
            }
        }
    };

    expanded.into()
}

/// Generates the full WASM component boilerplate.
///
/// Accepts the path to the WIT directory (relative to the workflow crate):
/// ```ignore
/// // Path to the wit/ directory containing the xarxa:api world
/// xarxa_entry_point!("path/to/wit/orchestrator");
/// ```
///
/// Or with a custom path:
/// ```ignore
/// xarxa_entry_point!("../../wit/orchestrator");
/// ```
#[proc_macro]
pub fn xarxa_entry_point(input: TokenStream) -> TokenStream {
    // Parse optional wit path override; default is relative to the SDK location
    let wit_path: String = if input.is_empty() {
        // Default: user crate is expected to have WIT at ../../wit/orchestrator
        // (e.g. examples/booking-workflow -> root/wit/orchestrator)
        "../../wit/orchestrator".to_string()
    } else {
        match syn::parse::<LitStr>(input) {
            Ok(lit) => lit.value(),
            Err(e) => return e.to_compile_error().into(),
        }
    };

    let expanded = quote! {
        // Generate WIT bindings directly in the user's crate
        ::wit_bindgen::generate!({
            path: #wit_path,
            world: "orchestrator",
        });

        use exports::xarxa::api::workflow_ctrl::{Guest, WorkflowError, WorkflowRun};
        use xarxa::api::engine_types::Code;

        struct __XarxaEngine;

        impl Guest for __XarxaEngine {
            fn start_workflow(
                run_id: u64,
                started_at: u64,
                workflow_name: ::std::string::String,
                input: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<WorkflowRun, WorkflowError> {
                use ::xarxa_sdk::inventory;

                if input.is_empty() {
                    return Err(WorkflowError {
                        err: "Input cannot be empty".to_string(),
                        code: Code::EmptyInput,
                    });
                }

                let reg = inventory::iter::<::xarxa_sdk::WorkflowRegistration>
                    .into_iter()
                    .find(|r| r.name == workflow_name.as_str());

                let reg = match reg {
                    Some(r) => r,
                    None => return Err(WorkflowError {
                        err: ::std::format!("Workflow '{}' not found", workflow_name),
                        code: Code::NotFound,
                    }),
                };

                let mut pipeline = (reg.start)(&input);
                let (_, step_result) = pipeline.run_to_completion();

                if let Some(err) = step_result.error {
                    return Err(WorkflowError { err, code: Code::NotFound });
                }

                let _history = ::xarxa_sdk::WorkflowHistory::new_with_input(
                    workflow_name,
                    input.clone(),
                    &pipeline,
                );

                Ok(WorkflowRun { id: run_id, input, timestamp: started_at })
            }

            fn continue_workflow(
                id: u64,
                state: ::std::vec::Vec<u8>,
            ) -> ::std::result::Result<::std::vec::Vec<u8>, WorkflowError> {
                use ::xarxa_sdk::inventory;

                let history: ::xarxa_sdk::WorkflowHistory =
                    match ::xarxa_sdk::serde_json::from_slice(&state) {
                        Ok(h) => h,
                        Err(e) => return Err(WorkflowError {
                            err: ::std::format!("Failed to deserialize state: {}", e),
                            code: Code::NotFound,
                        }),
                    };

                let reg = inventory::iter::<::xarxa_sdk::WorkflowRegistration>
                    .into_iter()
                    .find(|r| r.name == history.workflow_name.as_str());

                let reg = match reg {
                    Some(r) => r,
                    None => return Err(WorkflowError {
                        err: ::std::format!("Workflow '{}' not found", history.workflow_name),
                        code: Code::NotFound,
                    }),
                };

                let mut pipeline = (reg.start)(&history.serialized_input);
                pipeline.replay_from_history(&history.completed_steps);
                let (_, step_result) = pipeline.run_to_completion();

                if let Some(err) = step_result.error {
                    return Err(WorkflowError { err, code: Code::NotFound });
                }

                let updated = history.advance(step_result.completed_step, step_result.is_done);
                ::xarxa_sdk::serde_json::to_vec(&updated)
                    .map_err(|e| WorkflowError {
                        err: e.to_string(),
                        code: Code::NotFound,
                    })
            }
        }

        export!(__XarxaEngine with_types_in self);
    };

    expanded.into()
}
