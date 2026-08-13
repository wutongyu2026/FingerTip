# Release Pipeline — 一次性设置

> 2026-08-13 · v0.6.2 引入

---

## 前置条件（仓库管理员一次性操作）

### 1. GitHub Pages 启用

1. 仓库 `Settings` → `Pages`
2. **Source**: `GitHub Actions`（不要选 `Deploy from a branch`）
3. 保存

这样 `.github/workflows/release-sync.yml` 的 `deploy-pages` job 才能用 OIDC 部署。

### 2. 环境保护（可选但推荐）

1. 仓库 `Settings` → `Environments` → `New environment`
2. 名字：`github-pages`
3. （可选）`Required reviewers`：1 人审批保护

### 3. 仓库可见性

仓库需要是 **Public** 才能用免费 Pages（Private 仓库需要 GitHub Pro）。

---

## 触发流程

### 正常发版

```bash
git tag v0.7.0
git push origin v0.7.0
```

CI 自动：
1. **build-tauri** 矩阵：windows + linux + macos 出 MSI / NSIS / DEB / AppImage / DMG
2. **deploy-pages**：把 `docs/landing.html` + `algorithm-explainer.html` 部署到 Pages
3. **release-summary**：写 GitHub Actions summary

### 测试 release 草稿

CI 的 `tauri-action` 配置 `releaseDraft: true` —— tag push 后**不直接发**到 GitHub Releases，
而是创建 Draft release。仓库管理员 review 草稿 → 人工 publish。

### 手动重跑

仓库 `Actions` → `Release Sync` → `Run workflow`（无需 push 新 tag）。

---

## 环境变量（可选）

发布时若想指向不同资源仓库（如 `wutongyu2026/FingerTip`），设这些 Secrets：

| Secret | 默认值 | 说明 |
|---|---|---|
| `FINGERTIP_DOWNLOAD_URL` | `https://github.com/<org>/FingerTip/releases/latest` | landing.html GitHub 下载按钮 href |
| `FINGERTIP_DOWNLOAD_URL_CN` | (空) | 国内镜像下载按钮 href（空则隐藏镜像按钮） |
| `FINGERTIP_LANDING_PAGE_URL` | `https://<org>.github.io/FingerTip/landing.html` | QR 码落地页 URL |

CI 暂未注入这些（需在 tauri-action `args` 透传或 `RUNNER_ENV`）。
当前优先级低——`default_download_url()` 内置 fallback 足够。

---

## 故障排查

| 症状 | 排查 |
|---|---|
| Pages 部署 404 | 检查 Settings → Pages → Source 是否选 GitHub Actions |
| Windows 构建失败 | `pnpm install --frozen-lockfile` 是否锁文件与 lockfile 同步 |
| Engine pytest 失败 | `python -m pip install -r engine/requirements.txt` 是否成功（无网络：mock 仍能跑） |
| tauri-action 报 `no release assets` | `tauri-apps/tauri-action@v0` 自动匹配 tag；检查 tag 格式 `v*.*.*` |

---

## 已知限制

- **macOS 构建** 需要 Apple Developer 证书签名（当前用 ad-hoc，distribute 模式可能拒装）
- **Linux AppImage** 需要 `appimage-builder` 工具链（已自动）
- **cargo 网络**：国内 runner 拉 crates.io 可能慢；后续可加 cache 镜像
