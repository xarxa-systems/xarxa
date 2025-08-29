use notify::{Watcher, RecursiveMode, Result as NotifyResult, Event, EventKind};
use tokio::sync::{mpsc, oneshot};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, error, warn, debug};
use anyhow::{Result, Context, bail, ensure};

use crate::adapters::wasmtime::wit_runtime::WitPluginRuntime;

#[derive(Debug)]
pub enum PluginEvent {
    Added(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
}

pub struct PluginAutoLoader {
    workflow_manager: Arc<WitPluginRuntime>,
    shutdown_rx: oneshot::Receiver<()>,
    plugins_dir: PathBuf,
    watcher: Option<notify::RecommendedWatcher>,
}

impl PluginAutoLoader {
    pub fn new(workflow_manager: Arc<WitPluginRuntime>, shutdown_rx: oneshot::Receiver<()>, plugins_dir: PathBuf) -> Self {
        Self {
            workflow_manager,
            shutdown_rx,
            plugins_dir,
            watcher: None,
        }
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("🚀 Starting plugin auto-loader for directory: {:?}", self.plugins_dir);
        
        self.ensure_plugins_directory().await?;
        self.load_existing_plugins().await?;
        self.start_file_watcher().await?;
        
        info!("✅ Plugin auto-loader started successfully");
        Ok(())
    }

    async fn ensure_plugins_directory(&self) -> Result<()> {
        if !self.plugins_dir.exists() {
            tokio::fs::create_dir_all(&self.plugins_dir).await
                .with_context(|| format!("Failed to create plugins directory: {:?}", self.plugins_dir))?;
            info!("📁 Created plugins directory: {:?}", self.plugins_dir);
        } else {
            info!("📁 Using existing plugins directory: {:?}", self.plugins_dir);
        }
        
        let metadata = tokio::fs::metadata(&self.plugins_dir).await
            .context("Failed to read plugins directory metadata")?;
            
        if !metadata.is_dir() {
            bail!("Plugins path is not a directory: {:?}", self.plugins_dir);
        }
        
        info!("📁 Directory permissions: readonly={}", metadata.permissions().readonly());
        Ok(())
    }

      async fn load_existing_plugins(&self) -> Result<()> {
        info!("📦 Loading existing plugins from: {:?}", self.plugins_dir);
        
        let mut entries = tokio::fs::read_dir(&self.plugins_dir).await
            .with_context(|| format!("Failed to read plugins directory: {:?}", self.plugins_dir))?;
        
        let mut loaded_count = 0;
        let mut failed_count = 0;
        
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            debug!("🔍 Checking file: {:?}", path);
            
            if self.is_wasm_file(&path) {
                info!("🔄 Loading existing plugin: {:?}", path);
                match self.load_plugin(&path).await {
                    Ok(_) => {
                        loaded_count += 1;
                        info!("✅ Successfully loaded existing plugin: {:?}", path);
                    }
                    Err(e) => {
                        failed_count += 1;
                        error!("❌ Failed to load existing plugin {:?}: {}", path, e);
                    }
                }
            } else {
                debug!("⏭️  Skipping non-WASM file: {:?}", path);
            }
        }
        
        info!("📊 Plugin loading summary: {} loaded, {} failed", loaded_count, failed_count);
        Ok(())
    }

    async fn start_file_watcher(&mut self) -> Result<()> {
        let (tx, mut rx) = mpsc::unbounded_channel::<PluginEvent>();
        
        let canonical_plugins_dir = self.plugins_dir.canonicalize()
            .context("Failed to canonicalize plugins directory")?;
        
        info!("👀 Creating file watcher for: {:?}", canonical_plugins_dir);
        
        // Create file watcher
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            debug!("📡 File system event received: {:?}", res);
            match res {
                Ok(event) => {
                    debug!("📡 Processing event: {:?}", event);

                    if let Err(e) = Self::handle_fs_event(event, &tx, &canonical_plugins_dir.clone()) {
                        error!("❌ Error handling filesystem event: {}", e);
                    }
                }

                Err(e) => error!("❌ File watcher error: {:?}", e),
            }
            }).context("Failed to create file watcher")?;
        
        // Start watching the directory
        watcher.watch(&self.plugins_dir, RecursiveMode::NonRecursive)
            .with_context(|| format!("Failed to start watching directory: {:?}", self.plugins_dir))?;
        
        self.watcher = Some(watcher);
        
        info!("✅ File watcher started successfully");
        

        // tokio::spawn(async move {
        //     info!("🔄 Starting plugin event processor task");

        //     while let Some(event) = rx.recv().await {
        //         info!("📨 Received plugin event: {:?}", event);

        //         if let Err(e) = Self::process_plugin_event(event, &plugin_manager).await {
        //             error!("❌ Error processing plugin event: {}", e);
        //         }
        //     }

        //     warn!("⚠️  Plugin event processor task ended");
        // });
        info!("🔄 Starting plugin event processor");

        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(event) => {
                            info!("📨 Received plugin event: {:?}", event);

                            if let Err(e) = Self::process_plugin_event(event, &self.workflow_manager).await {
                                error!("❌ Error processing plugin event: {}", e);
                            }
                        }
                        None => {
                            warn!("📨 Plugin event channel closed");
                            break;
                        }
                    }
                }

                _ = &mut self.shutdown_rx => {
                    info!("🛑 Received shutdown signal, stopping plugin event processor");
                    break;
                }
            }
        }
        
        info!("✅ Plugin event processor stopped");
        Ok(())
    }

    fn handle_fs_event(
        event: Event,
        tx: &mpsc::UnboundedSender<PluginEvent>,
        plugins_dir: &Path,
    ) -> Result<()> {
        debug!("🔍 Handling filesystem event: {:?}", event);
        
        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    debug!("📝 File created: {:?}", path);
                    
                    if path.parent() == Some(plugins_dir) && Self::is_wasm_file_static(&path) {
                        info!("🆕 New WASM file detected: {:?}", path);

                        tx.send(PluginEvent::Added(path))
                            .context("Failed to send Added event")?;
                    } else {
                        debug!("⏭️  Ignoring created file (not WASM or wrong directory): {:?}", path);
                    }
                }
            }
            EventKind::Modify(_) => {
                for path in event.paths {
                    debug!("✏️  File modified: {:?}", path);
                    
                    if path.parent() == Some(plugins_dir) && Self::is_wasm_file_static(&path) {
                        info!("🔄 WASM file modified: {:?}", path);

                        tx.send(PluginEvent::Modified(path))
                            .context("Failed to send Modified event")?;
                    } else {
                        debug!("⏭️  Ignoring modified file (not WASM or wrong directory): {:?}", path);
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    debug!("🗑️  File removed: {:?}", path);
                    
                    if path.parent() == Some(plugins_dir) && Self::is_wasm_file_static(&path) {
                        info!("❌ WASM file removed: {:?}", path);
                        
                        tx.send(PluginEvent::Removed(path))
                            .context("Failed to send Removed event")?;
                    } else {
                        debug!("⏭️  Ignoring removed file (not WASM or wrong directory): {:?}", path);
                    }
                }
            }
            _ => {
                debug!("⏭️  Ignoring event kind: {:?}", event.kind);
            }
        }
        
        Ok(())
    }

    async fn process_plugin_event(
        event: PluginEvent,
        workflow_manager: &WitPluginRuntime,
    ) -> Result<()> {
        info!("🔄 Processing plugin event: {:?}", event);
        
        match event {
            PluginEvent::Added(path) => {
                info!("🆕 Processing new plugin: {:?}", path);
                
                // wait to make sure that the plugin is saved
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                
                if !path.exists() {
                    warn!("⚠️  Plugin file disappeared: {:?}", path);
                    return Ok(());
                }
                
                match Self::load_plugin_static(workflow_manager, &path).await {
                    Ok(_) => info!("✅ Successfully loaded new plugin: {:?}", path),
                    Err(e) => error!("❌ Failed to load new plugin {:?}: {}", path, e),
                }
            }
            PluginEvent::Modified(path) => {
                info!("🔄 Processing modified plugin: {:?}", path);
                
                // wait to make sure that the plugin is saved right
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                
                if !path.exists() {
                    warn!("⚠️  Modified plugin file disappeared: {:?}", path);
                    return Ok(());
                }
                
                let plugin_name = Self::extract_workflow_name(&path);
                info!("🔄 Reloading plugin: {}", plugin_name);
                
                match Self::load_plugin_static(workflow_manager, &path).await {
                    Ok(_) => info!("✅ Successfully reloaded plugin: {:?}", path),
                    Err(e) => error!("❌ Failed to reload plugin {:?}: {}", path, e),
                }
            }
            PluginEvent::Removed(path) => {
                info!("🗑️  Processing removed plugin: {:?}", path);
                let plugin_name = Self::extract_workflow_name(&path);
                
                // TODO: Implement plugin removal from PluginManager
                warn!("🚧 Plugin removal not implemented yet: {}", plugin_name);
                // Здесь нужно добавить метод в WitPluginRuntime для удаления плагинов
                // workflow_manager.unload_plugin(&plugin_name).await?;
            }
        }
        
        Ok(())
    }

    async fn load_plugin(&self, path: &Path) -> Result<()> {
        Self::load_plugin_static(&self.workflow_manager, path).await
    }

    async fn load_plugin_static(
        workflow_manager: &WitPluginRuntime,
        path: &Path,
    ) -> Result<()> {
        let plugin_name = Self::extract_workflow_name(path);
        
        info!("📦 Loading plugin '{}' from: {:?}", plugin_name, path);
        
        let wasm_bytes = tokio::fs::read(path).await
            .with_context(|| format!("Failed to read plugin file: {:?}", path))?;
        
        ensure!(!wasm_bytes.is_empty(), "Plugin file is empty: {:?}", path);
        
        // check WASM magic number
        if wasm_bytes.len() >= 4 {
            let magic = &wasm_bytes[0..4];
            ensure!(magic == b"\0asm", "Invalid WASM file format: {:?}", path);
        }
        
        // calculate hash
        let digest = md5::compute(&wasm_bytes);
        info!("🔐 WASM MD5: {:x} (size: {} bytes)", digest, wasm_bytes.len());
        
        // load plugin
        workflow_manager.load_wit_plugin(&plugin_name, &wasm_bytes).await
            .with_context(|| format!("Failed to load plugin '{}' from {:?}", plugin_name, path))?;
        
        info!("✅ Plugin '{}' loaded successfully", plugin_name);
        Ok(())
    }

    fn extract_workflow_name(path: &Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    fn is_wasm_file(&self, path: &Path) -> bool {
        Self::is_wasm_file_static(path)
    }

    fn is_wasm_file_static(path: &Path) -> bool {
        let result = path.is_file() && 
            path.extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_lowercase() == "wasm")
                .unwrap_or(false);
        
        debug!("🔍 Is WASM file? {:?} -> {}", path, result);

        result
    }
}

impl Drop for PluginAutoLoader {
    fn drop(&mut self) {
        if let Some(watcher) = self.watcher.take() {
            info!("🛑 Stopping file watcher");
            drop(watcher);
        }
    }
}