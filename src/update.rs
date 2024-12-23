use anyhow::Result;
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, AUTHORIZATION, USER_AGENT};
use reqwest::Client;
use semver::Version;
use serde::Deserialize;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_LATEST_RELEASE: &str =
    "https://api.github.com/repos/lexiaoyao20/i18n-app/releases/latest";
const GITHUB_TOKEN: &str = "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";
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct GithubRelease {
    pub tag_name: String,
    pub html_url: String,
    pub assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct GithubAsset {
    pub name: String,
    pub browser_download_url: String,
}

fn create_client() -> Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github.v3+json"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static("i18n-app"));

    // 添加固定的 GitHub Token
    if let Ok(auth_value) = HeaderValue::from_str(&format!("Bearer {}", GITHUB_TOKEN)) {
        headers.insert(AUTHORIZATION, auth_value);
        tracing::debug!("Using GitHub token for authentication");
    }

    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .timeout(std::time::Duration::from_secs(10))
        .build()?)
}

async fn check_rate_limit(client: &reqwest::Client) -> Result<()> {
    let response = client
        .get("https://api.github.com/rate_limit")
        .send()
        .await?;

    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let limit = response
        .headers()
        .get("x-ratelimit-limit")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(60);

    tracing::debug!("GitHub API rate limit: {}/{} remaining", remaining, limit);

    if remaining < 10 {
        tracing::warn!("GitHub API 调用次数即将用尽，剩余：{}/{}", remaining, limit);
    }

    Ok(())
}

pub async fn check_update() -> Result<Option<GithubRelease>> {
    match check_update_with_retry().await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::warn!("检查更新失败: {}", e);
            Ok(None)
        }
    }
}

async fn check_update_with_retry() -> Result<Option<GithubRelease>> {
    let client = create_client()?;
    let mut last_error = None;

    for retry in 0..MAX_RETRIES {
        if retry > 0 {
            tokio::time::sleep(RETRY_DELAY).await;
        }

        match check_update_internal(&client).await {
            Ok(release) => return Ok(release),
            Err(e) => {
                tracing::warn!("第 {} 次检查更新失败: {}", retry + 1, e);
                last_error = Some(e);

                // 检查是否是频率限制导致的错误
                if let Ok(()) = check_rate_limit(&client).await {
                    continue;
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("检查更新失败")))
}

async fn check_update_internal(client: &reqwest::Client) -> Result<Option<GithubRelease>> {
    let current = Version::parse(CURRENT_VERSION)?;

    let latest: GithubRelease = client
        .get(GITHUB_LATEST_RELEASE)
        .send()
        .await?
        .json()
        .await?;

    let latest_version = Version::parse(latest.tag_name.trim_start_matches('v'))?;

    if latest_version > current {
        Ok(Some(latest))
    } else {
        Ok(None)
    }
}

async fn download_file(client: &Client, url: &str, description: &str) -> Result<Vec<u8>> {
    let res = client.get(url).send().await?;
    let total_size = res.content_length().unwrap_or(0);

    // 创建进度条
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")?
            .progress_chars("#>-"),
    );
    pb.set_message(description.to_string());

    // 下载文件并更新进度
    let mut downloaded: u64 = 0;
    let mut bytes = Vec::new();
    let mut stream = res.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
        bytes.extend_from_slice(&chunk);
    }

    pb.finish_with_message(format!("{} 下载完成", description));
    Ok(bytes)
}

pub async fn update() -> Result<bool> {
    match update_internal().await {
        Ok(updated) => Ok(updated),
        Err(e) => {
            tracing::error!("更新失败: {}", e);
            Ok(false)
        }
    }
}

async fn update_internal() -> Result<bool> {
    if let Some(release) = check_update().await? {
        tracing::info!("发现新版本 {}，正在更新...", release.tag_name);

        let client = create_client()?;

        // 下载安装脚本
        let install_script = download_file(
            &client,
            "https://github.com/lexiaoyao20/i18n-app/raw/main/install.sh",
            "下载安装脚本",
        )
        .await?;

        // 创建临时文件来存储安装脚本
        let mut temp_file = tempfile::NamedTempFile::new()?;
        std::io::Write::write_all(&mut temp_file, &install_script)?;

        // 设置脚本文件为可执行
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = temp_file.as_file().metadata()?.permissions();
            perms.set_mode(0o755);
            temp_file.as_file().set_permissions(perms)?;
        }

        // 执行安装脚本
        let status = std::process::Command::new("/bin/bash")
            .arg(temp_file.path())
            .status()?;

        if status.success() {
            tracing::info!("更新成功！请重新运行程序。");
            Ok(true)
        } else {
            anyhow::bail!("更新失败，请手动更新");
        }
    } else {
        tracing::info!("当前已是最新版本");
        Ok(false)
    }
}
