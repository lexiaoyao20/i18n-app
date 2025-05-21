use anyhow::{ensure, Context, Result};
use std::collections::HashMap;
use std::fs::{self, File};
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

        let mut cached_files: HashMap<String, TranslationFile> = HashMap::new();
        let config_response = api::get_translation_config(&self.config).await?;

        if let Some(files_to_download) = config_response.data.files {
            for file_info in files_to_download {
                if file_info.url.is_empty() {
                    tracing::warn!("No download url found for language: {}", file_info.lang);
                    continue;
                }

                match api::download_translation(&self.config, &file_info.url).await {
                    Ok(raw_content_string) => {
                        let full_json_value: serde_json::Value =
                            serde_json::from_str(&raw_content_string)?;
                        let lang_key = format!("{}/languages", self.config.path_prefix);

                        if let Some(lang_specific_json_value) = full_json_value.get(&lang_key) {
                            let mut flattened = HashMap::new();
                            // 使用提取出的 lang_specific_json_value 进行扁平化
                            flatten_json_inner(
                                lang_specific_json_value,
                                String::new(),
                                &mut flattened,
                            );
                            let flattened_len = flattened.len();

                            if let Some(existing_translation_file) =
                                cached_files.get_mut(&file_info.lang)
                            {
                                existing_translation_file.content.extend(flattened);
                                tracing::debug!(
                                    "Merged {} new keys for language {}",
                                    flattened_len,
                                    file_info.lang
                                );
                            } else {
                                let translation = TranslationFile::from_content(
                                    file_info.lang.clone(),
                                    format!("{}.json", file_info.lang),
                                    flattened,
                                );
                                tracing::debug!(
                                    "Created new translation for language {} with {} keys",
                                    file_info.lang,
                                    translation.content.len()
                                );
                                cached_files.insert(file_info.lang.clone(), translation);
                            }

                            let target_file = cache_dir.join(format!("{}.json", file_info.lang));
                            // 将提取出的 lang_specific_json_value 写入缓存文件
                            std::fs::write(
                                &target_file,
                                serde_json::to_string_pretty(lang_specific_json_value)?,
                            )?;
                            tracing::debug!(
                                "Cached translation for {} to {}",
                                file_info.lang,
                                target_file.display()
                            );
                        } else {
                            tracing::error!(
                                "Key '{}' not found in downloaded content for language: {}. Raw content: {}",
                                lang_key,
                                file_info.lang,
                                raw_content_string
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to download translation for {}: {}",
                            file_info.lang,
                            e
                        );
                    }
                }
            }
        }

        Ok(cached_files)
    }

    pub async fn push_translations(&self, path: Option<String>) -> Result<()> {
        // 1. 读取本地翻译文件
        let (base_path, mut local_translations) = self.read_local_translations(path)?;

        // 2. 找到基准语言翻译
        let base_translation = local_translations
            .iter()
            .find(|t| t.language_code == self.config.base_language)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Base language {} not found in local translations",
                    self.config.base_language
                )
            })?
            .clone();

        // 3. 先处理本地文件的缺失key
        for translation in &mut local_translations {
            // 跳过基准语言
            if translation.language_code == self.config.base_language {
                continue;
            }

            // 获取缺失的键
            let missing_keys = translation::get_missing_keys(&base_translation, translation);
            if !missing_keys.is_empty() {
                tracing::info!(
                    "Found {} missing keys in {} compared to base language {}",
                    missing_keys.len(),
                    translation.language_code,
                    self.config.base_language
                );

                // 将缺失的键添加到翻译文件中
                translation.content.extend(missing_keys.clone());

                // 保存更新后的翻译文件到本地
                let file_path = base_path.join(&translation.relative_path);
                self.save_translation_file(translation, &file_path)?;

                tracing::info!(
                    "Updated local translation file {} with missing keys: {:?}",
                    file_path.display(),
                    missing_keys.keys().collect::<Vec<_>>()
                );
            }
        }

        // 4. 下载当前服务器翻译到缓存
        tracing::info!("Downloading current translations to cache...");
        let cached_translations = match self.download_to_cache().await {
            Ok(translations) => translations,
            Err(e) => {
                tracing::warn!("Failed to download current translations: {}", e);
                HashMap::new()
            }
        };

        // 5. 处理每个翻译文件的上传
        for local_translation in local_translations {
            let lang_code = &local_translation.language_code;
            let full_path = self.get_full_path(&local_translation, &base_path);

            match cached_translations.get(lang_code) {
                None => {
                    // 首次上传，上传全部内容
                    tracing::info!(
                        "First time upload for language {}, uploading all {} keys",
                        lang_code,
                        local_translation.content.len()
                    );
                    self.upload_translation(&local_translation, &full_path)
                        .await?;
                }
                Some(cached_translation) => {
                    let mut need_upload = HashMap::new();

                    // 收集需要上传的键
                    for (key, local_value) in &local_translation.content {
                        match cached_translation.content.get(key) {
                            None => {
                                // 远程没有的键
                                need_upload.insert(key.clone(), local_value.clone());
                                tracing::debug!("New key found: {}", key);
                            }
                            Some(remote_value) if remote_value.trim().is_empty() => {
                                // 远程值为空的键
                                need_upload.insert(key.clone(), local_value.clone());
                                tracing::debug!("Empty value key found: {}", key);
                            }
                            Some(remote_value) if remote_value != local_value => {
                                // 值不同的键（仅记录，不上传）
                                tracing::debug!(
                                    "Different value for key {}: local='{}', remote='{}'",
                                    key,
                                    local_value,
                                    remote_value
                                );
                            }
                            _ => {}
                        }
                    }

                    if !need_upload.is_empty() {
                        tracing::info!(
                            "Uploading {} new/updated keys for language {}",
                            need_upload.len(),
                            lang_code
                        );

                        // 打印要上传的键值对
                        for (key, value) in &need_upload {
                            tracing::info!("  + {}: {}", key, value);
                        }

                        let upload_translation = TranslationFile::from_content(
                            local_translation.language_code.clone(),
                            local_translation.relative_path.clone(),
                            need_upload,
                        );
                        self.upload_translation(&upload_translation, &full_path)
                            .await?;
                    } else {
                        tracing::info!("No new keys to upload for language {}", lang_code);
                    }
                }
            }
        }

        // 6. 清理缓存目录
        let cache_dir = PathBuf::from(".i18n-app").join("cache");
        if cache_dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&cache_dir) {
                tracing::warn!("Failed to clean cache directory: {}", e);
            } else {
                tracing::info!("Cache directory cleaned successfully");
            }
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

    fn get_full_path(&self, translation: &TranslationFile, base_path: &Path) -> String {
        if translation.relative_path.starts_with("fixtures/") {
            translation.relative_path.clone()
        } else {
            format!("{}/{}", base_path.display(), translation.relative_path)
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

        if target_dir.exists() {
            tracing::info!("Cleaning target directory: {}", target_dir.display());
            std::fs::remove_dir_all(&target_dir)?;
        }
        std::fs::create_dir_all(&target_dir)?;

        tracing::info!("Fetching translation configuration...");
        let config_response = api::get_translation_config(&self.config).await?;

        let mut success_count = 0;
        let mut failed_count = 0;

        if let Some(files_to_download) = config_response.data.files {
            for file_info in files_to_download {
                match api::download_translation(&self.config, &file_info.url).await {
                    Ok(raw_content_string) => {
                        let full_json_value: serde_json::Value =
                            serde_json::from_str(&raw_content_string)?;
                        let lang_key = format!("{}/languages", self.config.path_prefix);

                        if let Some(lang_specific_json_value) = full_json_value.get(&lang_key) {
                            let target_file = target_dir.join(format!("{}.json", file_info.lang));

                            // 将提取出的 lang_specific_json_value 写入文件
                            let content_to_write =
                                serde_json::to_string_pretty(lang_specific_json_value)?;
                            std::fs::write(&target_file, content_to_write)?;

                            tracing::info!(
                                "Downloaded translation for {} to {}",
                                file_info.lang,
                                target_file.display()
                            );
                            success_count += 1;
                        } else {
                            tracing::error!(
                                "Key '{}' not found in downloaded content for language: {}. Raw content: {}",
                                lang_key,
                                file_info.lang,
                                raw_content_string
                            );
                            failed_count += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to download translation for {}: {}",
                            file_info.lang,
                            e
                        );
                        failed_count += 1;
                    }
                }
            }
        } else {
            tracing::error!("No files found in the configuration response");
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

    /// 同步翻译文件（从服务器同步到本地）
    pub async fn sync_translations(&self) -> Result<()> {
        tracing::info!("正在下载最新翻译...");
        let config_response = api::get_translation_config(&self.config)
            .await
            .context("获取翻译配置失败")?;

        let files_to_download = config_response
            .data
            .files
            .as_ref()
            .and_then(|files| if files.is_empty() { None } else { Some(files) })
            .with_context(|| {
                format!(
                    "未找到任何翻译文件。系统名称: '{}', 产品代码: '{}'",
                    self.config.sub_system_name, self.config.product_code
                )
            })?;

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

        let mut success_count = 0;
        let mut failed_count = 0;

        for local_file in local_files {
            let lang_code = &local_file.language_code;

            if let Some(remote_file_info) = files_to_download.iter().find(|f| &f.lang == lang_code)
            {
                let target_path = base_path.join(&local_file.relative_path);
                tracing::info!("正在同步 {} 到 {}", lang_code, target_path.display());

                match api::download_translation(&self.config, &remote_file_info.url).await {
                    Ok(raw_content_string) => {
                        let full_json_value: serde_json::Value =
                            serde_json::from_str(&raw_content_string)?;
                        let lang_key = format!("{}/languages", self.config.path_prefix);

                        if let Some(remote_lang_specific_json) = full_json_value.get(&lang_key) {
                            let local_content_string = std::fs::read_to_string(&target_path)
                                .with_context(|| {
                                    format!("读取本地文件 {} 失败", target_path.display())
                                })?;
                            let local_json: serde_json::Value =
                                serde_json::from_str(&local_content_string)?;

                            self.print_json_diff(&local_json, remote_lang_specific_json, lang_code);

                            let merged_content =
                                Self::merge_json_content(&local_json, remote_lang_specific_json);

                            if let Some(parent) = target_path.parent() {
                                std::fs::create_dir_all(parent).with_context(|| {
                                    format!("创建目录 {} 失败", parent.display())
                                })?;
                            }

                            let formatted_json = serde_json::to_string_pretty(&merged_content)?;
                            std::fs::write(&target_path, formatted_json).with_context(|| {
                                format!("写入文件 {} 失败", target_path.display())
                            })?;

                            tracing::info!("成功同步 {}", target_path.display());
                            success_count += 1;
                        } else {
                            tracing::error!(
                                "Key '{}' not found in downloaded content for language: {}. Raw content: {}",
                                lang_key,
                                lang_code,
                                raw_content_string
                            );
                            failed_count += 1;
                        }
                    }
                    Err(e) => {
                        tracing::error!("下载语言 {} 的翻译失败: {}", lang_code, e);
                        failed_count += 1;
                    }
                }
            } else {
                tracing::warn!("未找到语言 {} 的远程翻译，跳过同步", lang_code);
                failed_count += 1;
            }
        }

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

    /// 添加新的辅助方法来保存翻译文件
    fn save_translation_file(&self, translation: &TranslationFile, file_path: &Path) -> Result<()> {
        // 将扁平的键值对转换为嵌套的 JSON 结构
        let mut json_value = serde_json::Map::new();
        for (key, value) in &translation.content {
            let parts: Vec<&str> = key.split('.').collect();
            let mut current = &mut json_value;

            // 创建嵌套结构
            for (i, part) in parts.iter().enumerate() {
                if i == parts.len() - 1 {
                    current.insert(
                        (*part).to_string(),
                        serde_json::Value::String(value.clone()),
                    );
                } else {
                    current = current
                        .entry((*part).to_string())
                        .or_insert(serde_json::Value::Object(serde_json::Map::new()))
                        .as_object_mut()
                        .ok_or_else(|| anyhow::anyhow!("Failed to create nested structure"))?;
                }
            }
        }

        // 创建父目录（如果不存在）
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // 将 JSON 写入文件
        let json_str = serde_json::to_string_pretty(&json_value)?;
        std::fs::write(file_path, json_str)?;

        Ok(())
    }

    // 修改为实例方法
    fn print_json_diff(
        &self,
        local: &serde_json::Value,
        remote: &serde_json::Value,
        lang_code: &str,
    ) {
        let mut local_map = HashMap::new();
        let mut remote_map = HashMap::new();

        // 将 JSON 扁平化以便比较
        flatten_json_inner(local, String::new(), &mut local_map);
        flatten_json_inner(remote, String::new(), &mut remote_map);

        // 找出本地独有的键（将被保留）
        let mut local_only = Vec::new();
        for key in local_map.keys() {
            if !remote_map.contains_key(key) {
                local_only.push(key);
            }
        }

        // 找出远程有但本地没有的键（新增的键）
        let mut remote_only = Vec::new();
        for key in remote_map.keys() {
            if !local_map.contains_key(key) {
                remote_only.push(key);
            }
        }

        // 找出值不同的键（将被更新的键）
        let mut different_values = Vec::new();
        for (key, local_value) in &local_map {
            if let Some(remote_value) = remote_map.get(key) {
                if local_value != remote_value {
                    different_values.push((key, local_value, remote_value));
                }
            }
        }

        // 打印差异信息
        if !local_only.is_empty() {
            tracing::info!("语言 {} 中本地独有的键（将被保留）:", lang_code);
            for key in local_only {
                tracing::info!("  * {}: {}", key, local_map.get(key).unwrap());
            }
        }

        if !remote_only.is_empty() {
            tracing::info!("语言 {} 中新增的键:", lang_code);
            for key in remote_only {
                tracing::info!("  + {}: {}", key, remote_map.get(key).unwrap());
            }
        }

        if !different_values.is_empty() {
            tracing::info!("语言 {} 中将被更新的键:", lang_code);
            for (key, local_value, remote_value) in different_values {
                tracing::info!("  ~ {}", key);
                tracing::info!("    - 当前值: {}", local_value);
                tracing::info!("    + 新值: {}", remote_value);
            }
        }
    }

    // 将方法改为静态方法
    fn merge_json_content(
        local: &serde_json::Value,
        remote: &serde_json::Value,
    ) -> serde_json::Value {
        match (local, remote) {
            (serde_json::Value::Object(local_map), serde_json::Value::Object(remote_map)) => {
                let mut merged = serde_json::Map::new();

                // 处理所有本地键
                for (key, local_value) in local_map {
                    if let Some(remote_value) = remote_map.get(key) {
                        // 如果远程也有这个键
                        match (local_value, remote_value) {
                            (serde_json::Value::Object(_), serde_json::Value::Object(_)) => {
                                // 递归合并对象
                                merged.insert(
                                    key.clone(),
                                    Self::merge_json_content(local_value, remote_value),
                                );
                            }
                            (_, serde_json::Value::String(remote_str)) => {
                                // 如果远程值是字符串
                                if remote_str.trim().is_empty() {
                                    // 如果远程值为空，保留本地值
                                    merged.insert(key.clone(), local_value.clone());
                                    tracing::debug!(
                                        "Keeping local value for empty remote key: {}",
                                        key
                                    );
                                } else {
                                    // 否则使用远程值
                                    merged.insert(key.clone(), remote_value.clone());
                                }
                            }
                            _ => {
                                // 其他情况使用远程值
                                merged.insert(key.clone(), remote_value.clone());
                            }
                        }
                    } else {
                        // 如果远程没有这个键，保留本地值
                        merged.insert(key.clone(), local_value.clone());
                    }
                }

                // 添加远程独有的键
                for (key, remote_value) in remote_map {
                    if !local_map.contains_key(key) {
                        if let serde_json::Value::String(remote_str) = remote_value {
                            if !remote_str.trim().is_empty() {
                                // 只添加非空的远程值
                                merged.insert(key.clone(), remote_value.clone());
                            }
                        } else {
                            merged.insert(key.clone(), remote_value.clone());
                        }
                    }
                }

                serde_json::Value::Object(merged)
            }
            // 如果远程值是空字符串，保留本地值
            (local_value, serde_json::Value::String(remote_str))
                if remote_str.trim().is_empty() =>
            {
                local_value.clone()
            }
            // 其他情况使用远程值
            (_, remote_value) => remote_value.clone(),
        }
    }

    // 改为公共方法
    pub fn init_log_file() -> Result<File> {
        let log_dir = PathBuf::from(".i18n-app");
        fs::create_dir_all(&log_dir)?;
        let log_file = log_dir.join("run.log");
        let file = File::create(log_file)?;
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_service() -> TranslationService {
        let config = Config {
            host: "https://test.com".to_string(),
            sub_system_name: "test".to_string(),
            product_code: "test".to_string(),
            version_no: "1.0.0".to_string(),
            base_language: "en-US".to_string(),
            preview_mode: "1".to_string(),
            path_prefix: "test".to_string(),
            include: vec![],
            exclude: vec![],
        };
        TranslationService::new(config)
    }

    #[test]
    fn test_merge_json_content() {
        // 不需要创建 service 实例
        // 测试场景 1: 基本合并
        let local = json!({
            "common": {
                "time": {
                    "tomorrow": "Tomorrow",
                    "today": "Today"
                }
            }
        });

        let remote = json!({
            "common": {
                "time": {
                    "today": "Today Updated",
                    "yesterday": "Yesterday"
                }
            }
        });

        let merged = TranslationService::merge_json_content(&local, &remote);
        let merged_obj = merged.as_object().unwrap();

        assert!(merged_obj["common"]["time"]["tomorrow"].as_str().unwrap() == "Tomorrow"); // 保留本地独有的键
        assert!(merged_obj["common"]["time"]["today"].as_str().unwrap() == "Today Updated"); // 使用远程的值
        assert!(merged_obj["common"]["time"]["yesterday"].as_str().unwrap() == "Yesterday"); // 添加远程新键

        // 测试场景 2: 嵌套对象合并
        let local = json!({
            "settings": {
                "display": {
                    "theme": "dark",
                    "font": "Arial"
                }
            }
        });

        let remote = json!({
            "settings": {
                "display": {
                    "theme": "light",
                    "size": "large"
                }
            }
        });

        let merged = TranslationService::merge_json_content(&local, &remote);
        let merged_obj = merged.as_object().unwrap();

        assert!(merged_obj["settings"]["display"]["font"].as_str().unwrap() == "Arial"); // 保留本地独有的键
        assert!(merged_obj["settings"]["display"]["theme"].as_str().unwrap() == "light"); // 使用远程的值
        assert!(merged_obj["settings"]["display"]["size"].as_str().unwrap() == "large");
        // 添加远程新键
    }

    #[test]
    fn test_save_translation_file() -> Result<()> {
        let service = create_test_service();
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("test.json");

        let mut content = HashMap::new();
        content.insert("common.time.tomorrow".to_string(), "Tomorrow".to_string());
        content.insert("common.time.today".to_string(), "Today".to_string());

        let translation = TranslationFile {
            language_code: "en-US".to_string(),
            relative_path: "test.json".to_string(),
            content,
        };

        service.save_translation_file(&translation, &file_path)?;

        // 验证保存的文件内容
        let saved_content = std::fs::read_to_string(&file_path)?;
        let saved_json: serde_json::Value = serde_json::from_str(&saved_content)?;

        assert!(saved_json["common"]["time"]["tomorrow"].as_str().unwrap() == "Tomorrow");
        assert!(saved_json["common"]["time"]["today"].as_str().unwrap() == "Today");

        Ok(())
    }

    #[test]
    fn test_print_json_diff() {
        let service = create_test_service();

        let local = json!({
            "common": {
                "time": {
                    "tomorrow": "Tomorrow",
                    "today": "Today"
                }
            }
        });

        let remote = json!({
            "common": {
                "time": {
                    "today": "Today Updated",
                    "yesterday": "Yesterday"
                }
            }
        });

        // 这个测试主要是确保方法不会崩溃，因为它只是打印日志
        service.print_json_diff(&local, &remote, "en-US");
    }

    #[test]
    fn test_init_log_file() -> Result<()> {
        let temp_dir = TempDir::new()?;
        std::env::set_current_dir(temp_dir.path())?;

        TranslationService::init_log_file()?;

        let log_file = temp_dir.path().join(".i18n-app").join("run.log");
        assert!(log_file.exists());

        Ok(())
    }
}
