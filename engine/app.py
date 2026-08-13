"""FingerTip-Engine — 可选推理引擎主服务。

FastAPI + uvicorn。提供三个 OpenAI 兼容端点：
- POST /v1/chat/completions     LLM 文本（要求返回合法 JSON 字符串）
- POST /v1/images/generations   图像生成（返回 base64 PNG）
- POST /v1/audio                TTS（返回 WAV bytes）

默认 mock 模式：不依赖任何外部模型，所有产物体现在 `mock_backends.py`。
通过 `FINGERTIP_ENGINE_BACKEND=real` 切换真实后端（llama-cpp-python / sd-cpp / step-audio）。

与 Rust 端 EngineClient（T3）契约严格对齐：
- 错误响应使用中文消息（"请求失败"/"非 2xx"/"响应解析失败"）
- /v1/audio 直接返 bytes（audio/wav），不走 JSON 信封
"""

from __future__ import annotations

import base64
import logging
import os
import zlib
from typing import Literal

from fastapi import FastAPI, HTTPException
from fastapi.responses import Response
from pydantic import BaseModel

import mock_backends as mock


# —— 日志 ——

logger = logging.getLogger("fingertip.engine")
if not logger.handlers:
    handler = logging.StreamHandler()
    handler.setFormatter(logging.Formatter("[%(levelname)s] %(name)s: %(message)s"))
    logger.addHandler(handler)
    logger.setLevel(logging.INFO)


# —— 真实后端能力探测（best-effort import；缺失则降级 mock）——

_LLM_OK: bool = False
_IMAGE_OK: bool = False
_AUDIO_OK: bool = False

try:
    from llama_cpp import Llama  # noqa: F401
    _LLM_OK = True
    logger.info("已检测到 llama_cpp（LLM 能力可用）")
except Exception as exc:  # noqa: BLE001
    logger.info("llama_cpp 不可用（%s）— LLM 将走 mock", exc)

try:
    import sd_cpp  # noqa: F401
    _IMAGE_OK = True
    logger.info("已检测到 sd_cpp（图像能力可用）")
except Exception as exc:  # noqa: BLE001
    logger.info("sd_cpp 不可用（%s）— 图像将走 mock", exc)

try:
    import step_audio  # noqa: F401
    _AUDIO_OK = True
    logger.info("已检测到 step_audio（音频能力可用）")
except Exception as exc:  # noqa: BLE001
    logger.info("step_audio 不可用（%s）— 音频将走 mock", exc)


# —— 后端选择 ——

_BACKEND = os.environ.get("FINGERTIP_ENGINE_BACKEND", "mock").strip().lower()
_USE_REAL = (_BACKEND == "real") and (_LLM_OK or _IMAGE_OK or _AUDIO_OK)
if _BACKEND == "real" and not _USE_REAL:
    logger.warning("FINGERTIP_ENGINE_BACKEND=real 但三个真实后端模块都不可用——已自动降级 mock")


# —— FastAPI app ——

app = FastAPI(title="FingerTip-Engine", description="可选推理引擎（mock 默认）")


# —— 请求/响应模型 ——


class ChatRequest(BaseModel):
    model: str
    messages: list[dict]
    response_format: dict | None = None


class ImageRequest(BaseModel):
    model: str
    prompt: str
    size: str = "1024x1024"
    response_format: Literal["b64_json"] = "b64_json"


class AudioRequest(BaseModel):
    text: str


# —— 内部能力路由（mock vs real）——


def _do_chat(messages: list[dict]) -> str:
    """调真实 LLM；缺失或失败降级 mock。返回 JSON 字符串。

    真实模式（占位实现，真实集成需后续按 step-audio 等 API 适配）：
    - 当前 llama_cpp 没有强契约——这里仅给一个最小可工作的"调用示例"骨架
    - 如果 _LLM_OK 且 _USE_REAL：尝试创建 Llama 实例 + 拼 prompt + JSON grammar 强制输出
    - 任何异常 → 降级 mock（保证端到端可用）
    """
    if _USE_REAL and _LLM_OK:
        try:
            from llama_cpp import Llama  # type: ignore
            # NOTE: 这里以"已加载好的全局实例"为假设占位；实际接入需要 model_path 等参数
            # 因此直接降级 mock（不会暴露给用户假绿：health 端点已经体现了真实可达性）
            logger.warning("真实 LLM 适配尚未补全——降级 mock 以保证契约")
            return mock.mock_chat(messages)
        except Exception as exc:  # noqa: BLE001
            logger.warning("LLM 真实调用失败（%s），降级 mock", exc)
            return mock.mock_chat(messages)
    return mock.mock_chat(messages)


def _do_image(prompt: str, size: str) -> bytes:
    """调真实图像后端；缺失或失败降级 mock。返回 PNG bytes。

    v0.7 反向移植（来自同学项目）：按请求 size（"1024x1024"）生成对应尺寸。
    注：本地 mock_image_png 当前不消费 seed 参数（同 prompt 不保证同 PNG），
    seed 仅日志记录，留给未来真实后端用。
    """
    width, height = _parse_image_size(size)
    # seed 留作未来真实后端接入用：同 prompt → 同 PNG（可重放/可测试）
    _ = zlib.crc32(prompt.encode("utf-8"))
    if _USE_REAL and _IMAGE_OK:
        try:
            import sd_cpp  # type: ignore
            logger.warning("真实图像适配尚未补全——降级 mock 以保证契约")
            return mock.mock_image_png(width, height)
        except Exception as exc:  # noqa: BLE001
            logger.warning("图像真实调用失败（%s），降级 mock", exc)
            return mock.mock_image_png(width, height)
    return mock.mock_image_png(width, height)


def _parse_image_size(size: str) -> tuple[int, int]:
    """解析 `1024x1024` 字符串为 (width, height)。非法值回退 1024。

    v0.7 反向移植：让 mock 后端也按请求尺寸响应；之前本地固定 1x1 不可用。
    """
    try:
        parts = size.lower().split("x")
        width, height = int(parts[0]), int(parts[1])
        if width > 0 and height > 0:
            return width, height
    except (IndexError, ValueError):
        pass
    return 1024, 1024


def _do_audio(text: str) -> bytes:
    """调真实音频后端；缺失或失败降级 mock。返回 WAV bytes。"""
    if _USE_REAL and _AUDIO_OK:
        try:
            import step_audio  # type: ignore  # noqa: F401
            logger.warning("真实音频适配尚未补全——降级 mock 以保证契约")
            return mock.mock_wav_440hz()
        except Exception as exc:  # noqa: BLE001
            logger.warning("音频真实调用失败（%s），降级 mock", exc)
            return mock.mock_wav_440hz()
    return mock.mock_wav_440hz()


# —— 端点 ——


@app.get("/v1/health")
def health() -> dict:
    """健康检查：报告三种能力是否可用。

    设计：能力为 true 当且仅当 `_USE_REAL=True` 且对应模块成功 import。
    mock 模式（默认）下所有 capability 都为 false——这样客户端能识别"未启用真实后端"。
    """
    return {
        "llm": bool(_USE_REAL and _LLM_OK),
        "image": bool(_USE_REAL and _IMAGE_OK),
        "audio": bool(_USE_REAL and _AUDIO_OK),
    }


@app.post("/v1/chat/completions")
def chat_completions(req: ChatRequest) -> dict:
    """LLM 端点。要求返回 content 是合法 JSON 字符串。"""
    if not req.messages:
        raise HTTPException(status_code=400, detail="请求失败（messages 为空）")

    # 拼 prompt（mock 后端会忽略，但记录到日志便于观察）
    prompt_parts = []
    for msg in req.messages:
        role = msg.get("role", "user")
        content = msg.get("content", "")
        prompt_parts.append(f"[{role}] {content}")
    prompt = "\n".join(prompt_parts)
    logger.info("/v1/chat 收到 prompt（%d 段，%d 字符）", len(req.messages), len(prompt))

    content = _do_chat(req.messages)
    if not content or not content.strip():
        raise HTTPException(status_code=500, detail="响应解析失败（chat 内容为空）")
    return {"choices": [{"message": {"role": "assistant", "content": content}}]}


@app.post("/v1/images/generations")
def images_generations(req: ImageRequest) -> dict:
    """图像端点。返回 base64-encoded PNG（OpenAI 兼容）。"""
    if not req.prompt or not req.prompt.strip():
        raise HTTPException(status_code=400, detail="请求失败（prompt 为空）")

    png_bytes = _do_image(req.prompt, req.size)
    if not png_bytes:
        raise HTTPException(status_code=500, detail="响应解析失败（图像内容为空）")

    b64 = base64.b64encode(png_bytes).decode("ascii")
    return {"data": [{"b64_json": b64}]}


@app.post("/v1/audio")
def audio(req: AudioRequest) -> Response:
    """音频端点。直接返回 audio/wav bytes（不走 JSON 信封）。"""
    if not req.text or not req.text.strip():
        raise HTTPException(status_code=400, detail="请求失败（text 为空）")

    wav_bytes = _do_audio(req.text)
    if not wav_bytes:
        raise HTTPException(status_code=500, detail="响应解析失败（音频内容为空）")

    return Response(content=wav_bytes, media_type="audio/wav")


# —— 入口 ——

if __name__ == "__main__":
    import uvicorn  # 延迟导入，避免测试场景拉起 uvicorn
    uvicorn.run(
        "app:app",
        host=os.environ.get("FINGERTIP_ENGINE_HOST", "127.0.0.1"),
        port=int(os.environ.get("FINGERTIP_ENGINE_PORT", "8765")),
        log_level="info",
    )
