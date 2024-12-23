use anyhow::{anyhow, Result};
use glob::glob;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Clone)]
pub struct TranslationFile {
    pub language_code: String,
    pub relative_path: String,
    pub content: HashMap<String, String>,
}

impl TranslationFile {
    pub fn from_path<P: AsRef<Path>>(base_path: P, file_path: P) -> Result<Self> {
        let file_path = file_path.as_ref().canonicalize()?;
        let base_path = base_path.as_ref().canonicalize()?;

        // Get relative path from base_path
        let relative_path = file_path
            .strip_prefix(&base_path)
            .map_err(|_| anyhow!("File path must be under base path"))?
            .to_str()
            .ok_or_else(|| anyhow!("Invalid file path"))?
            .to_string();

        // Extract language code from filename (e.g., "en-US.json" -> "en-US")
        let language_code = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid file name"))?
            .to_string();

        let content = fs::read_to_string(&file_path)?;
        let json: Value = serde_json::from_str(&content)?;
        let flattened = flatten_json(&json);

        Ok(TranslationFile {
            language_code,
            relative_path,
            content: flattened,
        })
    }

    pub fn from_content(
        language_code: String,
        relative_path: String,
        content: HashMap<String, String>,
    ) -> Self {
        TranslationFile {
            language_code,
            relative_path,
            content,
        }
    }
}

pub fn read_translation_files<P: AsRef<Path>>(
    base_path: P,
    include_patterns: &[String],
    exclude_patterns: &[String],
) -> Result<Vec<TranslationFile>> {
    let base_path = base_path.as_ref().canonicalize()?;
    tracing::info!("Reading translations from: {:?}", base_path);
    let mut files = Vec::new();
    let mut included_files = Vec::new();

    // First, collect all files that match include patterns
    for pattern in include_patterns {
        let pattern_path = base_path.join(pattern);
        let pattern_str = pattern_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid pattern path"))?;

        tracing::info!("Searching for pattern: {}", pattern_str);

        for entry in glob(pattern_str)? {
            match entry {
                Ok(path) => {
                    if path.is_file() {
                        included_files.push(path);
                    }
                }
                Err(e) => return Err(anyhow!("Failed to read glob pattern: {}", e)),
            }
        }
    }

    // Then, filter out excluded files
    for file_path in included_files {
        let mut should_include = true;

        for exclude_pattern in exclude_patterns {
            let exclude_path = base_path.join(exclude_pattern);
            let exclude_str = exclude_path
                .to_str()
                .ok_or_else(|| anyhow!("Invalid exclude pattern"))?;

            if let Ok(matches) = glob(exclude_str) {
                for exclude_path in matches.flatten() {
                    if file_path == exclude_path {
                        should_include = false;
                        break;
                    }
                }
            }
        }

        if should_include {
            match TranslationFile::from_path(&base_path, &file_path) {
                Ok(file) => files.push(file),
                Err(e) => tracing::warn!("Failed to read file {:?}: {}", file_path, e),
            }
        }
    }

    Ok(files)
}

fn flatten_json(value: &Value) -> HashMap<String, String> {
    let mut map = HashMap::new();
    flatten_json_inner(value, String::new(), &mut map);
    map
}

pub fn flatten_json_inner(value: &Value, prefix: String, map: &mut HashMap<String, String>) {
    match value {
        Value::Object(obj) => {
            for (key, val) in obj {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json_inner(val, new_prefix, map);
            }
        }
        Value::String(s) => {
            map.insert(prefix, s.clone());
        }
        _ => {
            map.insert(prefix, value.to_string());
        }
    }
}

/// Compare two translation files and return the missing keys from base translation
pub fn get_missing_keys(
    base: &TranslationFile,
    other: &TranslationFile,
) -> HashMap<String, String> {
    let mut missing = HashMap::new();

    for (key, value) in &base.content {
        if !other.content.contains_key(key) {
            missing.insert(key.clone(), value.clone());
        }
    }

    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_flatten_json_simple() {
        let json: Value = serde_json::from_str(r#"{"key": "value"}"#).unwrap();
        let flattened = flatten_json(&json);
        assert_eq!(flattened.get("key").unwrap(), "value");
    }

    #[test]
    fn test_flatten_json_nested() {
        let json: Value =
            serde_json::from_str(r#"{"parent": {"child": "value", "child2": "value2"}}"#).unwrap();
        let flattened = flatten_json(&json);
        assert_eq!(flattened.get("parent.child").unwrap(), "value");
        assert_eq!(flattened.get("parent.child2").unwrap(), "value2");
    }

    #[test]
    fn test_translation_file_from_path() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let file_path = temp_dir.path().join("en-US.json");

        let content = r#"{"key": "value"}"#;
        let mut file = File::create(&file_path)?;
        file.write_all(content.as_bytes())?;

        let translation = TranslationFile::from_path(temp_dir.path(), &file_path)?;
        assert_eq!(translation.language_code, "en-US");
        assert_eq!(translation.relative_path, "en-US.json");
        assert_eq!(translation.content.get("key").unwrap(), "value");

        Ok(())
    }

    #[test]
    fn test_read_translation_files() -> Result<()> {
        let temp_dir = TempDir::new()?;

        // Create test files
        let files = vec![
            ("en-US.json", r#"{"key": "value"}"#),
            ("languages/fr-FR.json", r#"{"key": "valeur"}"#),
            ("temp/es-ES.json", r#"{"key": "valor"}"#),
        ];

        for (path, content) in files {
            let file_path = temp_dir.path().join(path);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = File::create(file_path)?;
            file.write_all(content.as_bytes())?;
        }

        // Test with include and exclude patterns
        let include_patterns = vec!["**/*.json".to_string()];
        let exclude_patterns = vec!["temp/*.json".to_string()];

        let translations =
            read_translation_files(temp_dir.path(), &include_patterns, &exclude_patterns)?;
        assert_eq!(translations.len(), 2); // Should not include es-ES.json

        Ok(())
    }

    #[test]
    fn test_get_missing_keys() {
        let mut base_content = HashMap::new();
        base_content.insert("key1".to_string(), "Value1".to_string());
        base_content.insert("key2".to_string(), "Value2".to_string());
        base_content.insert("detail.label_time".to_string(), "Time".to_string());

        let mut other_content = HashMap::new();
        other_content.insert("key1".to_string(), "值1".to_string());

        let base = TranslationFile::from_content(
            "en-US".to_string(),
            "en-US.json".to_string(),
            base_content,
        );

        let other = TranslationFile::from_content(
            "zh-CN".to_string(),
            "zh-CN.json".to_string(),
            other_content,
        );

        let missing = get_missing_keys(&base, &other);
        assert_eq!(missing.len(), 2);
        assert_eq!(missing.get("key2").unwrap(), "Value2");
        assert_eq!(missing.get("detail.label_time").unwrap(), "Time");
    }
}
