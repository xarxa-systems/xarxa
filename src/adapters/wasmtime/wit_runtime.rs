/*
 * Project: Xarxa — Durable WASM Workflow Orchestrator
 * Copyright (c) 2025 Xarxa Systems
 *
 * Xarxa is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).
 * See the LICENSE file in the project root for the full license text.
 *
 * Commercial licensing (MIT / proprietary) is available.
 * Contact: contact@xarxa.io
 *
 * SPDX-License-Identifier: AGPL-3.0-or-later
 */

use wasmtime::*;
use wasmtime::component::{Component, Linker};
use wasmtime_wasi::p2::{WasiCtx, WasiCtxBuilder, WasiView, IoView};
use wasmtime_wasi::ResourceTable;

use std::collections::HashMap;
use anyhow::{Result, Context, bail};
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::{Value as JsonValue, json};
use tracing::info;

use crate::core::ports::wit::Controller;
use crate::core::ports::wit::exports::xarxa::engine::worker_handler::History;
use crate::core::ports::wit::xarxa::engine::engine_types::{Kvpair, Value};

struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
    limits: StoreLimits,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl IoView for HostState  {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

pub struct LoadedWitPlugin {
    component: Component,
    // info: PluginInfo,
    loaded_at: std::time::SystemTime,
    execution_count: u64,
}

pub struct WitPluginRuntime {
    engine: Engine,
    pub(crate) plugins: Arc<RwLock<HashMap<String, LoadedWitPlugin>>>,
}

impl WitPluginRuntime {
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        config.async_support(false); // Synchronous operation for simplicity
        
        let engine = Engine::new(&config)
            .context("Failed to create WASM engine with component model support")?;

        let plugins = Arc::new(RwLock::new(HashMap::new()));
        
        info!("🚀 Initialized WIT Plugin Manager");

        Ok(WitPluginRuntime {
            engine,
            plugins,
        })
    }

    pub async fn load_wit_plugin(&self, name: &str, wasm_bytes: &[u8]) -> Result<()> {
        info!("📦 Loading WIT plugin: {}", name);

        if name.is_empty() {
            bail!("Plugin name cannot be empty");
        }
        
        if wasm_bytes.is_empty() {
            bail!("WASM bytes cannot be empty for plugin '{}'", name);
        }
        
        // Create WIT component
        let component = Component::new(&self.engine, wasm_bytes)
            .with_context(|| format!("Failed to create WIT component for plugin '{}'", name))?;
        
        // Get plugin information
        // let info = self.extract_wit_plugin_info(&component, name).await?;
        // TODO: It should me information about pipelines, activities, etc.
        
        let loaded_plugin = LoadedWitPlugin {
            component,
            // info: info.clone(),
            loaded_at: std::time::SystemTime::now(),
            execution_count: 0,
        };

        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(name) {
                info!("⚠️  Plugin '{}' is already loaded, replacing...", name);
                // todo: to think how to handle it. 
            }
        }
        
        let mut plugins = self.plugins.write().await;
        plugins.insert(name.to_string(), loaded_plugin);
        
        Ok(())
    }

    pub async fn execute_wit_function( &self, plugin_name: &str, function_name: &str, params: JsonValue) -> Result<JsonValue> {
        let mut plugins = self.plugins.write().await;
        
        let plugin = plugins.get_mut(plugin_name)
            .with_context(|| format!("WIT Plugin '{}' not found", plugin_name))?;
        
        // Increment execution counter
        plugin.execution_count += 1;
        let execution_count = plugin.execution_count;
        
        info!("🚀 Executing WIT function: {}.{} (execution #{})", 
              plugin_name, function_name, execution_count);
        
        // Execute the function
        let result = self.execute_wit_component_function(&plugin.component, function_name, &params).await?;
        
        info!("✅ WIT function completed: {}.{}", plugin_name, function_name);
        Ok(result)
    }

    async fn execute_wit_component_function(
        &self,
        component: &Component,
        function_name: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        let mut store = self.create_store()?;
        
        // Create linker and add WASI
        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        
        // Create component instance
        let instance = Controller::instantiate(&mut store, component, &linker)?;
        let worker_handler = instance.xarxa_engine_worker_handler();
        
        let result = match function_name {
            "start-workflow" => {
                let workflow_name = params.get("workflow_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                
                let input = self.json_to_kvpairs(params.get("input").unwrap_or(&json!([])))?;
                
                match worker_handler.call_start_workflow(&mut store, workflow_name, &input) {
                    Ok(Ok(workflow_run)) => {
                        json!({
                            "success": true,
                            "result": format!("Workflow started successfully"),
                            "workflow_run": format!("{:?}", workflow_run),
                            "run_id": format!("run_{}", chrono::Utc::now().timestamp())
                        })
                    }
                    Ok(Err(error)) => {
                        json!({
                            "success": false,
                            "error": format!("Workflow error: {:?}", error)
                        })
                    }
                    Err(e) => {
                        json!({
                            "success": false,
                            "error": format!("Runtime error: {}", e)
                        })
                    }
                }
            }
            "continue-workflow" => {
                let run_id = params.get("run_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0); // todo handle err
                
                let history = History{ tasks_result:  vec![]};
                
                match worker_handler.call_continue_workflow(&mut store, run_id, &history) {
                    Ok(Ok(result)) => {
                        json!({
                            "success": true,
                            "result": result,
                            "run_id": run_id
                        })
                    }
                    Ok(Err(error)) => {
                        json!({
                            "success": false,
                            "error": format!("Workflow error: {:?}", error)
                        })
                    }
                    Err(e) => {
                        json!({
                            "success": false,
                            "error": format!("Runtime error: {}", e)
                        })
                    }
                }
            }
            "execute-activity" => {
                let activity_name = params.get("activity_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                
                let input = self.json_to_kvpairs(params.get("input").unwrap_or(&json!([])))?;
                
                match worker_handler.call_execute_activity(&mut store, activity_name, &input) {
                    Ok(Ok(result)) => {
                        json!({
                            "success": true,
                            "result": result,
                            "activity": activity_name
                        })
                    }
                    Ok(Err(error)) => {
                        json!({
                            "success": false,
                            "error": error,
                            "activity": activity_name
                        })
                    }
                    Err(e) => {
                        json!({
                            "success": false,
                            "error": format!("Runtime error: {}", e)
                        })
                    }
                }
            }
            // "signal-workflow" => {
            //     let run_id = params.get("run_id")
            //         .and_then(|v| v.as_u64())
            //         .unwrap_or(0);
                
            //     // Create signal from parameters
            //     let signal_data = params.get("signal").unwrap_or(&json!({}));
                
            //     #[warn(unreachable_code)]
            //     let signal = engine::api::engine_types::Signal {
            //         // name: signal_data.get("name")
            //         //     .and_then(|v| v.as_str())
            //         //     .unwrap_or("default")
            //         //     .to_string(),
            //         // payload: signal_data.get("payload")
            //         //     .and_then(|v| v.as_str())
            //         //     .unwrap_or("{}")
            //         //     .to_string(),
                    
            //         payload: todo!(),
            //     };
                
            //     match worker_handler.call_signal_workflow(&mut store, run_id, &signal) {
            //         Ok(Ok(result)) => {
            //             json!({
            //                 "success": true,
            //                 "result": result,
            //                 "run_id": run_id,
            //                 "signal": signal.payload,
            //             })
            //         }
            //         Ok(Err(error)) => {
            //             json!({
            //                 "success": false,
            //                 "error": format!("Workflow error: {:?}", error)
            //             })
            //         }
            //         Err(e) => {
            //             json!({
            //                 "success": false,
            //                 "error": format!("Runtime error: {}", e)
            //             })
            //         }
            //     }
            // }
            "cancel-workflow" => {
                let run_id = params.get("run_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0); // todo handle with err
                
                match worker_handler.call_cancel_workflow(&mut store, run_id) {
                    Ok(Ok(result)) => {
                        json!({
                            "success": true,
                            "result": result,
                            "run_id": run_id,
                            "action": "cancelled"
                        })
                    }
                    Ok(Err(error)) => {
                        json!({
                            "success": false,
                            "error": format!("Workflow error: {:?}", error)
                        })
                    }
                    Err(e) => {
                        json!({
                            "success": false,
                            "error": format!("Runtime error: {}", e)
                        })
                    }
                }
            }
            _ => {
                json!({
                    "success": false,
                    "error": format!("Unknown WIT function: {}", function_name)
                })
            }
        };
        
        Ok(result)
    }

    pub async fn remove_plugin(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut plugins = self.plugins.write().await;
        
        if plugins.remove(name).is_some() {
            info!("Removed plugin: {}", name);
            Ok(())
        } else {
            Err(format!("Plugin '{}' not found", name).into())
        }
    }

    pub async fn reload_plugin(&self, name: &str, wasm_bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        // Remove the old plugin
        self.remove_plugin(name).await.ok(); // Ignore error if plugin didn't exist
        
        // Load the new one
        self.load_wit_plugin(name, wasm_bytes).await?;
        
        info!("Reloaded plugin: {}", name);
        Ok(())
    }

    pub async fn list_plugin_names(&self) -> Vec<String> {
        let plugins = self.plugins.read().await;
        plugins.keys().cloned().collect()
    }

    fn create_store(&self) -> Result<Store<HostState>> {
        let wasi = WasiCtxBuilder::new()
            .inherit_stdio()
            .build();
            
        let host_state = HostState {
            ctx: wasi,
            table: ResourceTable::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(1 << 28) // 256 MB
                .instances(10)
                .build(),
        };
        
        let mut store = Store::new(&self.engine, host_state);
        store.limiter(|state| &mut state.limits);
        
        Ok(store)
    }

    fn json_to_kvpairs(&self, params: &JsonValue) -> Result<Vec<Kvpair>> {
        let mut kvpairs = Vec::new();
        
        match params {
            JsonValue::Array(arr) => {
                // If already an array of kvpairs
                for item in arr {
                    if let Some(obj) = item.as_object() {
                        if let (Some(key), Some(value)) = (obj.get("key"), obj.get("value")) {
                            kvpairs.push(Kvpair {
                                key: key.as_str().unwrap_or("").to_string(),
                                value: Value::Str(value.as_str().unwrap_or("").to_string()),
                            });
                        }
                    }
                }
            }
            JsonValue::Object(obj) => {
                // If regular object - convert to kvpairs
                for (key, value) in obj {
                    let value_str = match value {
                        JsonValue::String(s) => s.clone(),
                        JsonValue::Number(n) => n.to_string(),
                        JsonValue::Bool(b) => b.to_string(),
                        _ => serde_json::to_string(value)?,
                    };
                    
                    kvpairs.push(Kvpair {
                        key: key.clone(),
                        value: Value::Str(value_str),
                    });
                }
            }
            _ => {
                // For other types create empty array
            }
        }
        
        Ok(kvpairs)
    }
}