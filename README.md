# Clash Verger for Android

Android adaptation of Clash Verge Rev, built with Tauri 2, React, TypeScript, Rust, Kotlin/Android, and Mihomo.

> Upstream / original project: [clash-verge-rev/clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev)

## Languages / 使用语言 / 使用言語

- **TypeScript + React**: mobile UI and frontend state.
- **Rust + Tauri 2**: Android backend commands, profile/config handling, Mihomo process control, logs, and local API bridge.
- **Kotlin / Android**: Android shell, app integration, permissions, and mobile runtime layer.
- **YAML / JSON**: Mihomo runtime configuration, profiles, and app settings.

## 中文

这是基于 [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev) 的 Android 移植适配版本。

本项目优先面向 Android 手机端，不以桌面三端兼容为目标。桌面专属能力会被移除、隐藏或替换为 Android 可用的实现。当前核心目标是让 Clash Verge Rev 的前端体验、Profiles、代理组、规则、连接、日志、Mihomo API、配置管理和 Android 本地运行能力在手机端可用。

### Android 适配内容

- 使用 Tauri 2 Android 作为应用壳。
- 使用 React + TypeScript 构建移动端界面。
- 使用 Rust 后端管理配置、订阅、runtime YAML、Mihomo 启停、日志和 API 转发。
- 内置 Android arm64 Mihomo 核心。
- 面向 Android 调整系统代理入口、局域网连接地址显示、日志处理和手机 UI。
- 移除或隐藏 Android 不适用的桌面功能。

### 开发命令

```bash
npm install
npm run tauri android dev
```

### 构建前端

```bash
npm run build
```

### Android Rust 检查

```bash
cargo check --target aarch64-linux-android
```

## English

This repository is an Android adaptation of [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev).

The project targets Android phones first. It is not intended to preserve full desktop cross-platform behavior. Desktop-only features are removed, hidden, stubbed, or replaced with Android-friendly behavior. The goal is to bring the Clash Verge Rev frontend experience, profiles, proxy groups, rules, connections, logs, Mihomo API, configuration management, and local Android runtime into a usable mobile app.

### Android Scope

- Tauri 2 Android application shell.
- React + TypeScript mobile frontend.
- Rust backend for profiles, runtime YAML, Mihomo lifecycle, logs, and local API bridging.
- Bundled Android arm64 Mihomo core.
- Android-specific system proxy entry, LAN connection display, log handling, and mobile UI adjustments.
- Desktop-only features are removed or hidden when they do not apply to Android.

### Development

```bash
npm install
npm run tauri android dev
```

### Frontend Build

```bash
npm run build
```

### Android Rust Check

```bash
cargo check --target aarch64-linux-android
```

## 日本語

このリポジトリは [Clash Verge Rev](https://github.com/clash-verge-rev/clash-verge-rev) を Android 向けに移植・調整したプロジェクトです。

本プロジェクトは Android スマートフォンを優先対象としています。デスクトップ版の三端末互換を維持することは目的ではありません。デスクトップ専用機能は、Android で利用できる形に置き換えるか、削除・非表示・スタブ化します。Clash Verge Rev のフロントエンド体験、Profiles、プロキシグループ、ルール、接続、ログ、Mihomo API、設定管理、Android ローカル実行環境をスマートフォンで利用できるようにすることが目的です。

### Android 対応内容

- Tauri 2 Android アプリケーションシェル。
- React + TypeScript によるモバイル UI。
- Rust バックエンドによる Profiles、runtime YAML、Mihomo の起動停止、ログ、ローカル API ブリッジ。
- Android arm64 Mihomo コアを同梱。
- Android 向けのシステムプロキシ入口、LAN 接続アドレス表示、ログ処理、モバイル UI 調整。
- Android で利用できないデスクトップ専用機能は削除または非表示。

### 開発

```bash
npm install
npm run tauri android dev
```

### フロントエンドビルド

```bash
npm run build
```

### Android Rust チェック

```bash
cargo check --target aarch64-linux-android
```

## Credits

This project is based on and adapted from:

- [clash-verge-rev/clash-verge-rev](https://github.com/clash-verge-rev/clash-verge-rev)

Thanks to the original Clash Verge Rev authors and contributors.
