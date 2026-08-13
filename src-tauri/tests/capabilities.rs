//! v0.3.7 R5 + P0-A followup: capability 校验
//!
//! 验证意图：
//! 阻止 downloadWavFromPath / 任何后续 readFile 误用 ——
//! capabilities/default.json 必须包含 plugin-fs 的 read-file 权限。
//!
//! 历史教训：v0.3.7 R5 改造 useTonePlayback 后，Artworks.onDownloadMusic
//! 改用 plugin-fs.readFile 读 backend music_wav_path 文件，但忘了在
//! capabilities/default.json 加 `fs:allow-read-file` —— 桌面端调 readFile
//! 直接被 capability 拒绝，catch 块只 console.warn → 用户看不到任何文件。
//!
//! 这种错误的特征：单测 + e2e (web 模式) 都不会失败（web 没 capability 校验），
//! 必须真实桌面端才能发现。本测试锁住"权限列表必须含 read-file"防止回归。

#[test]
fn capabilities_default_includes_read_file_permission() {
    let caps = include_str!("../capabilities/default.json");
    assert!(
        caps.contains("\"fs:allow-read-file\""),
        "capabilities/default.json 必须包含 fs:allow-read-file —— Artworks.onDownloadMusic 依赖 readFile(wav_path)。"
    );
}

#[test]
fn capabilities_default_includes_write_file_permission() {
    // 锁住 write-file 权限，确保 ensureDefaultDir / writeFile 不会被回归破坏。
    let caps = include_str!("../capabilities/default.json");
    assert!(
        caps.contains("\"fs:allow-write-file\""),
        "capabilities/default.json 必须包含 fs:allow-write-file —— downloadBlob 写盘路径依赖。"
    );
}

#[test]
fn capabilities_default_includes_appdata_scope() {
    // 锁住 fs:scope 包含 $APPDATA/** —— backend 生成的 wav/png 在
    // app_data_dir/downloads/{date}/ 下，readFile 路径必须在此 scope 内。
    let caps = include_str!("../capabilities/default.json");
    assert!(
        caps.contains("$APPDATA"),
        "fs:scope 必须包含 $APPDATA 路径（backend music_wav_path 在 app_data_dir 下）"
    );
}
