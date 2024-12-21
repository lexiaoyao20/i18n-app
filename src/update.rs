use anyhow::Result;
use semver::Version;
use serde::Deserialize;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_LATEST_RELEASE: &str =
    "https://api.github.com/repos/lexiaoyao20/i18n-app/releases/latest";

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

pub async fn check_update() -> Result<Option<GithubRelease>> {
    match check_update_internal().await {
        Ok(result) => Ok(result),
        Err(e) => {
            tracing::warn!("检查更新失败: {}", e);
            Ok(None)
        }
    }
}

async fn check_update_internal() -> Result<Option<GithubRelease>> {
    let current = Version::parse(CURRENT_VERSION)?;
    let client = reqwest::Client::new();

    let latest: GithubRelease = client
        .get(GITHUB_LATEST_RELEASE)
        .header("User-Agent", "i18n-app")
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

        // 首先下载安装脚本
        let install_script =
            reqwest::get("https://github.com/lexiaoyao20/i18n-app/raw/main/install.sh")
                .await?
                .text()
                .await?;

        // 创建临时文件来存储安装脚本
        let mut temp_file = tempfile::NamedTempFile::new()?;
        std::io::Write::write_all(&mut temp_file, install_script.as_bytes())?;

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
