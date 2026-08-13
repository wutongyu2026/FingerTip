# FingerTip-Engine

> 可选插件 — FingerTip 桌面端的独立 Python 推理服务
>
> 默认 **mock 模式** 即可让端到端流程跑通；真实模型为可选升级路径。

## 这是什么

T10 是 FingerTip 的"可选插件"，**独立进程** 通过 HTTP 与 App 侧的 `EngineClient`（T3）通信。当用户在 App 的 Settings 中启用了"推理引擎"并指向本服务（默认 `http://127.0.0.1:8765`），编排器会按 `CapabilityMode × 路由表` 决定何时调用哪个端点。

> mock 模式下 EngineClient 仍调用本服务，但收到的产物是固定的；引擎"始终在线"但产物确定性，方便测试、演示和 CI。

## 端点契约（与 Rust EngineClient 对齐）

| 方法 | 路径 | 请求体 | 响应 |
| --- | --- | --- | --- |
| `GET`  | `/v1/health` | — | `{"llm": bool, "image": bool, "audio": bool}` |
| `POST` | `/v1/chat/completions` | `{model, messages, response_format?}` | `{"choices":[{"message":{"content":"<json 字符串>"}}]}` |
| `POST` | `/v1/images/generations` | `{model, prompt, size, response_format:"b64_json"}` | `{"data":[{"b64_json":"<base64 PNG>"}]}` |
| `POST` | `/v1/audio` | `{text}` | `audio/wav` bytes |

错误格式：FastAPI 标准 `{"detail":"..."}`，detail 用**中文**，与 Rust 客户端翻译层对齐：
- `请求失败（messages 为空）` —— `400`
- `请求失败（prompt 为空）` —— `400`
- `请求失败（text 为空）` —— `400`
- `响应解析失败（…内容为空）` —— `500`

## 安装

要求 Python 3.10+。推荐使用虚拟环境：

```bash
cd engine
python -m venv venv
# Windows
venv\Scripts\activate
# macOS/Linux
source venv/bin/activate

pip install -r requirements.txt
```

> requirements.txt 仅含 mock 模式所需的最小依赖。
> 真实后端（llama-cpp-python / sd-cpp / step-audio）按需单独安装，**不**强制进 requirements。

## 启动

### Mock 模式（默认，推荐第一次跑通流程用）

```bash
python app.py
# 等价于：uvicorn app:app --host 127.0.0.1 --port 8765
```

服务起在 `http://127.0.0.1:8765`。打开任意浏览器访问 `http://127.0.0.1:8765/docs` 可看 FastAPI 自带 Swagger UI。

### 真实模式

```bash
pip install llama-cpp-python        # LLM（GGUF 模型）
pip install sd-cpp                  # 图像（stable diffusion）
pip install step-audio              # 音频

FINGERTIP_ENGINE_BACKEND=real python app.py
```

环境变量：

| 名称 | 默认 | 说明 |
| --- | --- | --- |
| `FINGERTIP_ENGINE_BACKEND` | `mock` | `mock` 或 `real`。real 模式下三个真实后端模块至少一个能 import 才生效，否则自动降级 mock 并 `log::warning` |
| `FINGERTIP_ENGINE_HOST` | `127.0.0.1` | 监听地址 |
| `FINGERTIP_ENGINE_PORT` | `8765` | 监听端口（与 Rust 端 EngineClient 默认值一致） |

> 健康端点语义：当且仅当 `BACKEND=real` **且** 对应能力模块成功 import 时，该 capability 才为 `true`。
> mock 模式下三个 capability 全为 `false`——客户端由此判断"未启用真实后端"。

## 测试

```bash
cd engine
python -m pytest -v
```

测试用例基于 `fastapi.testclient.TestClient`，**无需** 起 uvicorn 真实服务，CI 友好。当前覆盖：

- `/v1/health` 三字段齐全且为 bool
- `/v1/health` 字段数=3（与 Rust 三能力路由表对齐）
- `/v1/chat/completions` 返回合法 JSON 字符串（含 `music_description` / `image_description` / `sentence` 三键）
- `/v1/chat/completions` 缺 `messages` 返 4xx
- `/v1/chat/completions` 空 `messages` 数组返 400 含中文 `请求失败`
- `/v1/images/generations` base64 解码后是合法 PNG（8 字节签名校验）
- `/v1/images/generations` 空 `prompt` 返 400
- `/v1/audio` 返回的 bytes 是合法 WAV（RIFF/WAVE/fmt/data 头校验 + wave 模块能解析）
- `/v1/audio` 空 `text` 返 400

## 文件结构

```
engine/
├── app.py                # FastAPI 主服务（含 mock/real 分流）
├── mock_backends.py      # mock 后端（固定 chat JSON / 1x1 PNG / 440Hz WAV）
├── requirements.txt      # 依赖清单（不含真实后端）
├── .gitignore            # Python 缓存 / 虚拟环境 / .env / 模型权重
└── tests/
    └── test_app.py       # 9 个 pytest 用例
```

## Mock 产物（确定性）

| 端点 | 产物 |
| --- | --- |
| `/v1/chat/completions` | 固定 JSON 字符串：`{"music_description":"calm piano with rain ambience","image_description":"orange abstract with swirling shapes","sentence":"A quiet day of focus"}` |
| `/v1/images/generations` | 1x1 红色像素 PNG（8 字节签名 + IHDR + IDAT + IEND） |
| `/v1/audio` | 0.5s 440Hz 正弦波 WAV（PCM16 mono 44.1kHz） |

> 这些 mock 产物与 Rust 端 wav_analysis / 编排器契约一致，端到端测试友好。

## MiniMax 占位说明

> 此处用于汇总 MiniMax 相关说明（如未来接入多模态）：待用户接入 MiniMax 真实模型时，会在 `app.py` 中以新的 `_do_<capability>` 函数接管 `_USE_REAL` 分支。当前不接真实集成；如需扩展，请保留 mock fallback 路径以保证降级可用。

## 已知边界 / 后续工作

- 真实 llama-cpp-python / sd-cpp / step-audio 适配骨架已留位（`app.py` 中 `_do_chat/_do_image/_do_audio` 的 `_USE_REAL` 分支），但具体 GGUF 模型路径 / JSON grammar / sd prompt 工程参数由用户环境决定——当前默认降级 mock 并 `log::warning`。
- 与 Rust EngineClient 的端点契约已对齐；任何契约变更需先回到 `src-tauri/src/model/cloud.rs` 与本服务同步。
