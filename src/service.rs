use anyhow::{ensure, Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::{
    api,
    config::Config,
    translation::{self, flatten_json_inner, read_translation_files, TranslationFile},
};

pub struct TranslationService {
    config: Config,
}

impl TranslationService {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    pub async fn download_to_cache(&self) -> Result<HashMap<String, TranslationFile>> {
        let cache_dir = PathBuf::from(".i18n-app").join("cache");
        self.prepare_cache_dir(&cache_dir)?;

        let mut cached_files = HashMap::new();
        let config_response = api::get_translation_config(&self.config).await?;

        if let Some(groups) = config_response.data.file_groups {
            for group in groups {
                if group.file_names.is_empty() {
                    tracing::warn!(
                        "No translation files found for language: {}",
                        group.language_code
                    );
                    continue;
                }

                self.process_language_group(&group, &cache_dir, &mut cached_files)
                    .await?;
            }
        }

        Ok(cached_files)
    }

    pub async fn push_translations(&self, path: Option<String>) -> Result<()> {
        // Download current translations
        tracing::info!("Downloading current translations to cache...");
        let cached_translations = self.download_to_cache().await?;

        // Read local translations
        let (base_path, local_translations) = self.read_local_translations(path)?;

        // Find base language translation
        let base_translation = local_translations
            .iter()
            .find(|t| t.language_code == self.config.base_language)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Base language {} not found in local translations",
                    self.config.base_language
                )
            })?;

        // Clone base_translation for later use
        let base_translation = base_translation.clone();

        // Process each translation
        for local_translation in local_translations {
            if local_translation.language_code == self.config.base_language {
                // Process base language normally
                self.process_translation(&local_translation, &cached_translations, &base_path)
                    .await?;
            } else {
                // For other languages, first complete missing keys
                self.process_non_base_translation(
                    &local_translation,
                    &base_translation,
                    &cached_translations,
                    &base_path,
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn process_language_group(
        &self,
        group: &api::FileGroup,
        cache_dir: &Path,
        cached_files: &mut HashMap<String, TranslationFile>,
    ) -> Result<()> {
        for file_name in &group.file_names {
            match api::download_translation(&self.config, group, file_name).await {
                Ok(content) => {
                    let target_file = cache_dir.join(format!("{}.json", group.language_code));
                    let json_value: serde_json::Value = serde_json::from_str(&content)?;
                    let lang_key = format!("languages/{}.json", group.language_code);

                    if let Some(lang_content) = json_value.get(&lang_key) {
                        let mut content = HashMap::new();
                        flatten_json_inner(lang_content, String::new(), &mut content);

                        let translation = TranslationFile::from_content(
                            group.language_code.clone(),
                            format!("{}.json", group.language_code),
                            content,
                        );

                        cached_files.insert(group.language_code.clone(), translation);

                        let formatted_json = serde_json::to_string_pretty(lang_content)?;
                        std::fs::write(&target_file, formatted_json)?;

                        tracing::info!(
                            "Cached translation for {} to {}",
                            group.language_code,
                            target_file.display()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to download translation for {}: {}",
                        group.language_code,
                        e
                    );
                }
            }
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn process_downloaded_content(
        &self,
        content: &str,
        group: &api::FileGroup,
        cache_dir: &Path,
        cached_files: &mut HashMap<String, TranslationFile>,
    ) -> Result<()> {
        let target_file = cache_dir.join(format!("{}.json", group.language_code));
        let json_value: serde_json::Value = serde_json::from_str(content)?;
        let lang_key = format!("languages/{}.json", group.language_code);

        if let Some(lang_content) = json_value.get(&lang_key) {
            let mut content = HashMap::new();
            flatten_json_inner(lang_content, String::new(), &mut content);

            let translation = TranslationFile::from_content(
                group.language_code.clone(),
                format!("{}.json", group.language_code),
                content,
            );

            cached_files.insert(group.language_code.clone(), translation);

            let formatted_json = serde_json::to_string_pretty(lang_content)?;
            std::fs::write(&target_file, formatted_json)?;

            tracing::info!(
                "Cached translation for {} to {}",
                group.language_code,
                target_file.display()
            );
        }

        Ok(())
    }

    fn prepare_cache_dir(&self, cache_dir: &PathBuf) -> Result<()> {
        if cache_dir.exists() {
            std::fs::remove_dir_all(cache_dir)?;
        }
        std::fs::create_dir_all(cache_dir)?;
        Ok(())
    }

    fn read_local_translations(
        &self,
        path: Option<String>,
    ) -> Result<(PathBuf, Vec<TranslationFile>)> {
        let include_patterns: Vec<String> = if path.is_some() {
            self.config
                .include
                .iter()
                .map(|p| p.replace("fixtures/", ""))
                .collect()
        } else {
            self.config.include.clone()
        };

        let base_path = path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));

        let local_translations =
            read_translation_files(&base_path, &include_patterns, &self.config.exclude)?;

        if local_translations.is_empty() {
            tracing::warn!(
                "No translation files found matching patterns: {:?}",
                include_patterns
            );
        } else {
            tracing::info!("Found {} local translation files", local_translations.len());
        }

        Ok((base_path, local_translations))
    }

    async fn process_translation(
        &self,
        local_translation: &TranslationFile,
        cached_translations: &HashMap<String, TranslationFile>,
        base_path: &Path,
    ) -> Result<()> {
        let lang_code = &local_translation.language_code;
        let full_path = self.get_full_path(local_translation, base_path);

        let diff = self.get_translation_diff(local_translation, cached_translations, lang_code);

        if diff.is_empty() {
            tracing::info!("No changes found for {}", full_path);
            return Ok(());
        }

        self.log_diff_details(&diff, &full_path);

        let diff_translation = TranslationFile::from_content(
            local_translation.language_code.clone(),
            local_translation.relative_path.clone(),
            diff,
        );

        self.upload_translation(&diff_translation, &full_path).await
    }

    fn get_full_path(&self, translation: &TranslationFile, base_path: &Path) -> String {
        if translation.relative_path.starts_with("fixtures/") {
            translation.relative_path.clone()
        } else {
            format!("{}/{}", base_path.display(), translation.relative_path)
        }
    }

    fn get_translation_diff(
        &self,
        local_translation: &TranslationFile,
        cached_translations: &HashMap<String, TranslationFile>,
        lang_code: &str,
    ) -> HashMap<String, String> {
        if let Some(cached_translation) = cached_translations.get(lang_code) {
            crate::translation::get_translation_diff(local_translation, cached_translation)
        } else {
            tracing::info!(
                "No cached translation found for {}, uploading all content",
                lang_code
            );
            local_translation.content.clone()
        }
    }

    fn log_diff_details(&self, diff: &HashMap<String, String>, full_path: &str) {
        tracing::info!("Uploading {} new keys for {}", diff.len(), full_path);
        for (key, value) in diff {
            tracing::info!("  {} = {}", key, value);
        }
    }

    async fn upload_translation(
        &self,
        translation: &TranslationFile,
        full_path: &str,
    ) -> Result<()> {
        if let Err(e) = api::upload_translation(&self.config, translation).await {
            tracing::error!("Failed to push {}: {}", full_path, e);
            Err(e)
        } else {
            tracing::info!("Push {} success 🎉🎉🎉", full_path);
            Ok(())
        }
    }

    pub async fn download_translations(&self, path: Option<String>) -> Result<()> {
        let target_dir = path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".i18n-app").join("preview"));

        // Create or clean the target directory
        if target_dir.exists() {
            tracing::info!("Cleaning target directory: {}", target_dir.display());
            std::fs::remove_dir_all(&target_dir)?;
        }
        std::fs::create_dir_all(&target_dir)?;

        // Get translation configuration
        tracing::info!("Fetching translation configuration...");
        let config_response = api::get_translation_config(&self.config).await?;

        let mut success_count = 0;
        let mut failed_count = 0;

        // Download translations for each language
        if let Some(file_groups) = config_response.data.file_groups {
            for group in file_groups {
                for file_name in &group.file_names {
                    match api::download_translation(&self.config, &group, file_name).await {
                        Ok(content) => {
                            let target_file =
                                target_dir.join(format!("{}.json", group.language_code));
                            // Parse the JSON string to a Value
                            let json_value: serde_json::Value = serde_json::from_str(&content)?;
                            // Extract the specific language content
                            let lang_key = format!("languages/{}.json", group.language_code);
                            if let Some(lang_content) = json_value.get(&lang_key) {
                                // Pretty print the JSON with 2 spaces indentation
                                let formatted_json = serde_json::to_string_pretty(lang_content)?;
                                std::fs::write(&target_file, formatted_json)?;
                                tracing::info!(
                                    "Downloaded translation for {} to {}",
                                    group.language_code,
                                    target_file.display()
                                );
                                success_count += 1;
                            } else {
                                tracing::error!(
                                    "No translation content found for language: {}",
                                    group.language_code
                                );
                                failed_count += 1;
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                "Failed to download translation for {}: {}",
                                group.language_code,
                                e
                            );
                            failed_count += 1;
                        }
                    }
                }
            }
        } else {
            tracing::error!("No file groups found in the configuration response");
            return Ok(());
        }

        tracing::info!(
            "Download completed: {} succeeded, {} failed, {} total",
            success_count,
            failed_count,
            success_count + failed_count
        );

        Ok(())
    }

    async fn process_non_base_translation(
        &self,
        translation: &TranslationFile,
        base_translation: &TranslationFile,
        cached_translations: &HashMap<String, TranslationFile>,
        base_path: &Path,
    ) -> Result<()> {
        let lang_code = &translation.language_code;
        let full_path = self.get_full_path(translation, base_path);

        // Get missing keys from base translation
        let missing_keys = translation::get_missing_keys(base_translation, translation);
        if !missing_keys.is_empty() {
            tracing::info!(
                "Found {} missing keys in {} compared to base language {}",
                missing_keys.len(),
                lang_code,
                self.config.base_language
            );
            for (key, value) in &missing_keys {
                tracing::info!("  {} = {}", key, value);
            }

            // Create a new translation with missing keys
            let missing_translation = TranslationFile::from_content(
                translation.language_code.clone(),
                translation.relative_path.clone(),
                missing_keys,
            );

            // Upload missing keys
            if let Err(e) = api::upload_translation(&self.config, &missing_translation).await {
                tracing::error!("Failed to push missing keys for {}: {}", full_path, e);
                return Err(e);
            }
            tracing::info!("Successfully pushed missing keys for {}", full_path);
        }

        // Process normal differences
        let diff = self.get_translation_diff(translation, cached_translations, lang_code);
        if !diff.is_empty() {
            tracing::info!("Uploading {} new keys for {}", diff.len(), full_path);
            for (key, value) in &diff {
                tracing::info!("  {} = {}", key, value);
            }

            let diff_translation = TranslationFile::from_content(
                translation.language_code.clone(),
                translation.relative_path.clone(),
                diff,
            );

            if let Err(e) = api::upload_translation(&self.config, &diff_translation).await {
                tracing::error!("Failed to push {}: {}", full_path, e);
                return Err(e);
            }
            tracing::info!("Push {} success 🎉🎉🎉", full_path);
        }

        Ok(())
    }

    /// 同步翻译文件（从服务器同步到本地）
    pub async fn sync_translations(&self) -> Result<()> {
        // 1. 下载所有翻译到缓存目录
        tracing::info!("正在下载最新翻译...");
        let config_response = api::get_translation_config(&self.config)
            .await
            .context("获取翻译配置失败")?;

        // 检查 fileGroups 是否为空
        let file_groups = config_response
            .data
            .file_groups
            .as_ref()
            .and_then(|groups| {
                if groups.is_empty() {
                    None
                } else {
                    Some(groups)
                }
            })
            .with_context(|| {
                format!(
                    "未找到任何翻译文件组。系统名称: '{}', 产品代码: '{}'",
                    self.config.sub_system_name, self.config.product_code
                )
            })?;

        let mut cached_files = HashMap::new();

        // 处理每个语言组的翻译
        for group in file_groups {
            if group.file_names.is_empty() {
                tracing::warn!("语言 {} 没有找到任何翻译文件", group.language_code);
                continue;
            }

            if let Err(e) = self
                .process_language_group(
                    group,
                    &PathBuf::from(".i18n-app").join("cache"),
                    &mut cached_files,
                )
                .await
                .with_context(|| format!("处理语言 {} 的翻译失败", group.language_code))
            {
                tracing::error!("{:#}", e);
                continue;
            }
        }

        ensure!(!cached_files.is_empty(), "未能成功下载任何翻译文件");

        // 2. 获取需要同步的本地文件列表
        let (base_path, local_files) = self
            .read_local_translations(None)
            .context("读取本地翻译文件失败")?;

        ensure!(
            !local_files.is_empty(),
            format!(
                "未找到任何本地翻译文件。include 设置: {:?}",
                self.config.include
            )
        );

        // 3. 同步每个文件
        let mut success_count = 0;
        let mut failed_count = 0;

        for local_file in local_files {
            let lang_code = &local_file.language_code;
            if let Some(cached_file) = cached_files.get(lang_code) {
                let target_path = base_path.join(&local_file.relative_path);
                tracing::info!("正在同步 {} 到 {}", lang_code, target_path.display());

                if let Err(e) = self
                    .sync_single_file(cached_file, &target_path)
                    .with_context(|| format!("同步文件 {} 失败", target_path.display()))
                {
                    tracing::error!("{:#}", e);
                    failed_count += 1;
                } else {
                    success_count += 1;
                }
            } else {
                tracing::warn!("未找到语言 {} 的远程翻译，跳过同步", lang_code);
                failed_count += 1;
            }
        }

        // 4. 清理缓存目录
        if let Err(e) = self.cleanup_cache_dir() {
            tracing::warn!("清理缓存目录失败: {:#}", e);
        }

        // 5. 输出最终结果
        ensure!(
            success_count > 0,
            format!(
                "同步失败：成功 {} 个，失败 {} 个",
                success_count, failed_count
            )
        );

        tracing::info!(
            "同步完成: {} 个成功, {} 个失败, 共 {} 个文件",
            success_count,
            failed_count,
            success_count + failed_count
        );

        Ok(())
    }

    /// 同步单个文件
    fn sync_single_file(&self, cached_file: &TranslationFile, target_path: &Path) -> Result<()> {
        // 确保目标目录存在
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建目录 {} 失败", parent.display()))?;
        }

        // 序列化并写入文件
        let json_content =
            serde_json::to_string_pretty(&cached_file.content).context("序列化翻译内容失败")?;

        std::fs::write(target_path, json_content)
            .with_context(|| format!("写入文件 {} 失败", target_path.display()))?;

        tracing::info!("成功同步 {}", target_path.display());
        Ok(())
    }

    /// 清理缓存目录
    fn cleanup_cache_dir(&self) -> Result<()> {
        let cache_dir = PathBuf::from(".i18n-app").join("cache");
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir)
                .with_context(|| format!("删除缓存目录 {} 失败", cache_dir.display()))?;
        }
        Ok(())
    }
}
