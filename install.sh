#!/bin/bash

set -e

GITHUB_REPO="lexiaoyao20/i18n-app"
BINARY_NAME="i18n-app"

# 检测系统类型和架构
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)
    
    case "$ARCH" in
        x86_64) ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *) echo "不支持的架构: $ARCH"; exit 1 ;;
    esac
    
    case "$OS" in
        linux) OS="linux" ;;
        darwin) OS="darwin" ;;
        *) echo "不支持的操作系统: $OS"; exit 1 ;;
    esac
    
    echo "${OS}-${ARCH}"
}

# 获取最新发布版本
get_latest_version() {
    curl --silent "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" | 
    grep '"tag_name":' | 
    cut -d'"' -f4
}

main() {
    echo "开始安装 ${BINARY_NAME}..."
    
    PLATFORM=$(detect_platform)
    VERSION=$(get_latest_version)
    
    if [ -z "$VERSION" ]; then
        echo "无法获取最新版本"
        exit 1
    fi
    
    DOWNLOAD_URL="https://github.com/${GITHUB_REPO}/releases/download/${VERSION}/${BINARY_NAME}-${PLATFORM}"
    
    echo "下载 ${BINARY_NAME} ${VERSION} for ${PLATFORM}..."
    echo "下载文件：${DOWNLOAD_URL}"
    
    # 创建临时目录
    TMP_DIR=$(mktemp -d)
    curl -sL "$DOWNLOAD_URL" -o "${TMP_DIR}/${BINARY_NAME}"
    
    # 验证下载的文件
    if [ ! -s "${TMP_DIR}/${BINARY_NAME}" ]; then
        echo "错误: 下载失败或文件为空"
        rm -rf "$TMP_DIR"
        exit 1
    fi
    
    file_type=$(file "${TMP_DIR}/${BINARY_NAME}")
    if [[ ! $file_type =~ "executable" ]]; then
        echo "错误: 下载的文件不是可执行文件"
        echo "文件类型: $file_type"
        cat "${TMP_DIR}/${BINARY_NAME}"
        rm -rf "$TMP_DIR"
        exit 1
    fi
    
    chmod +x "${TMP_DIR}/${BINARY_NAME}"
    
    # 移动到 /usr/local/bin
    if [ -w "/usr/local/bin" ]; then
        # 如果本地已存在旧版本，先移除旧版本文件
        if [ -f "/usr/local/bin/${BINARY_NAME}" ]; then
            echo "移除旧版本..."
            rm -f "/usr/local/bin/${BINARY_NAME}"
        fi

        mv "${TMP_DIR}/${BINARY_NAME}" "/usr/local/bin/${BINARY_NAME}"
    else
        # 如果需要 sudo 权限
        if [ -f "/usr/local/bin/${BINARY_NAME}" ]; then
            echo "移除旧版本..."
            sudo rm -f "/usr/local/bin/${BINARY_NAME}"
        fi
        sudo mv "${TMP_DIR}/${BINARY_NAME}" "/usr/local/bin/${BINARY_NAME}"
    fi
    
    rm -rf "$TMP_DIR"
    
    echo "${BINARY_NAME} 已成功安装到 /usr/local/bin/${BINARY_NAME}"
    echo "运行 '${BINARY_NAME} --help' 查看使用说明"
}

main