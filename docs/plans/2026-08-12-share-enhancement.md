# 分享链路增强 — 任务清单

> 2026-08-12

---

## ① 落地页 AI 键盘诊断文案

- [ ] `landing.html`：`.funny-card` 样式对齐客户端（橙色渐变 + 左边框），无 payload 默认页加静态示例
- [ ] 后端/客户端不用改（数据链路已通）

## ② 分享海报排版优化

- [ ] `upload.rs` `generate_card_png()`：`CARD_H` 扩到 800，统计面板下方插入 funny_summary 区块，微调各元素位置
- [ ] 生成测试卡片目测比例

## ③ 国内安装包下载适配

- [ ] `upload.rs`：加 `default_download_url_cn()`，`SharePayload` 加 `l`（GitHub 链接）和 `c`（国内镜像）字段
- [ ] `commands.rs`：`generate_share` 构造 `SharePageData` 时传入 `download_url_cn`
- [ ] `landing.html`：`setDownloadLinks()` 删掉 hostname 猜测逻辑，改从 payload 读 `d.l` / `d.c`，下载区双按钮

## ④ 资源仓库域名切换 + 同步

- [ ] 部署时设环境变量 `FINGERTIP_LANDING_PAGE_URL` / `FINGERTIP_DOWNLOAD_URL` / `FINGERTIP_DOWNLOAD_URL_CN`
- [ ] 新增 `.github/workflows/release-sync.yml`：tag push → Tauri build → GitHub Release → Pages 部署

---

## 顺序

1. ④ CI 脚本 + Pages 配置
2. ③ 国内下载（upload.rs → commands.rs → landing.html）
3. ① 落地页 funny 样式
4. ② 海报排版
