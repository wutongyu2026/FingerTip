"""FingerTip-Engine 端到端契约测试。

所有用例基于 FastAPI TestClient（不需要起 uvicorn）。
覆盖：
- /v1/health 三能力字段齐全且为 bool
- /v1/chat/completions 返回的 content 是合法 JSON 字符串
- /v1/images/generations 返回 base64 编码的合法 PNG（magic bytes 校验）
- /v1/audio 返回合法 WAV（wave 模块能成功解析）
- /v1/chat 缺 messages 字段返 4xx（中文错误路径）
- health 字段数与 Rust 路由表三能力对齐
"""

from __future__ import annotations

import base64
import io
import json
import struct
import wave

import pytest
from fastapi.testclient import TestClient

from app import app


@pytest.fixture
def client() -> TestClient:
    return TestClient(app)


# —— 健康端点 ——


def test_health_reports_three_capabilities(client: TestClient) -> None:
    r = client.get("/v1/health")
    assert r.status_code == 200
    body = r.json()
    assert set(body.keys()) == {"llm", "image", "audio"}, f"health 字段异常: {body}"
    assert all(isinstance(v, bool) for v in body.values()), f"health 值必须为 bool: {body}"


def test_health_capability_count_matches_routing_table(client: TestClient) -> None:
    """llm/image/audio 三能力与 Rust 端 CapabilityMode × 路由契约一致。"""
    r = client.get("/v1/health")
    assert r.status_code == 200
    body = r.json()
    assert len(body) == 3, f"健康端点应返恰好 3 个字段，实际: {body}"


# —— LLM 端点 ——


def test_chat_returns_valid_json_content(client: TestClient) -> None:
    r = client.post(
        "/v1/chat/completions",
        json={
            "model": "fingertip-llm",
            "messages": [
                {"role": "system", "content": "..."},
                {"role": "user", "content": "..."},
            ],
            "response_format": {"type": "json_object"},
        },
    )
    assert r.status_code == 200, f"LLM 端点非 2xx: {r.status_code} {r.text}"
    body = r.json()
    assert "choices" in body, f"响应缺少 choices: {body}"
    assert len(body["choices"]) >= 1, f"choices 应至少 1 项: {body}"
    content = body["choices"][0]["message"]["content"]
    assert isinstance(content, str), f"content 必须为字符串: {type(content)}"

    # content 必须是合法 JSON
    parsed = json.loads(content)
    assert "music_description" in parsed, f"JSON 缺 music_description: {parsed}"
    assert "image_description" in parsed, f"JSON 缺 image_description: {parsed}"
    assert "sentence" in parsed, f"JSON 缺 sentence: {parsed}"


def test_chat_errors_with_clear_message_on_bad_request(client: TestClient) -> None:
    """缺 messages 字段应返 4xx（FastAPI/Pydantic 422 或 app 层 400 都算符合契约）。"""
    r = client.post("/v1/chat/completions", json={"model": "x"})
    assert r.status_code in (400, 422), f"期望 4xx，实际: {r.status_code} {r.text}"


def test_chat_with_empty_messages_array_returns_400(client: TestClient) -> None:
    """空 messages 数组走 app 层 400（中文错误）。"""
    r = client.post(
        "/v1/chat/completions",
        json={"model": "x", "messages": []},
    )
    assert r.status_code == 400, f"空 messages 应返 400，实际: {r.status_code}"
    body = r.json()
    # 中文错误消息（与 Rust 客户端对齐）
    detail = body.get("detail", "")
    assert "请求失败" in detail or "messages" in detail.lower(), f"错误消息不符契约: {body}"


# —— 图像端点 ——


def test_image_returns_valid_png_b64(client: TestClient) -> None:
    r = client.post(
        "/v1/images/generations",
        json={
            "model": "fingertip-image",
            "prompt": "abstract",
            "size": "1024x1024",
            "response_format": "b64_json",
        },
    )
    assert r.status_code == 200, f"图像端点非 2xx: {r.status_code} {r.text}"
    body = r.json()
    assert "data" in body, f"响应缺少 data: {body}"
    assert len(body["data"]) >= 1, f"data 应至少 1 项: {body}"

    b64 = body["data"][0]["b64_json"]
    raw = base64.b64decode(b64)
    # PNG signature (8 字节)
    assert raw[:8] == b"\x89PNG\r\n\x1a\n", f"非合法 PNG 签名: {raw[:8]!r}"


def test_image_with_empty_prompt_returns_400(client: TestClient) -> None:
    r = client.post(
        "/v1/images/generations",
        json={"model": "x", "prompt": "", "response_format": "b64_json"},
    )
    assert r.status_code == 400, f"空 prompt 应返 400，实际: {r.status_code}"


# —— 音频端点 ——


def test_audio_returns_legal_wav(client: TestClient) -> None:
    r = client.post("/v1/audio", json={"text": "calm piano"})
    assert r.status_code == 200, f"音频端点非 2xx: {r.status_code} {r.text}"
    # 端点直接返 audio/wav bytes，不是 JSON
    assert r.headers["content-type"].startswith("audio/wav"), (
        f"Content-Type 应为 audio/wav，实际: {r.headers.get('content-type')!r}"
    )

    raw = r.content
    assert len(raw) > 44, f"WAV 文件过短: {len(raw)} 字节"

    # RIFF / WAVE / fmt / data 块校验
    assert raw[:4] == b"RIFF", f"缺 RIFF 头: {raw[:4]!r}"
    assert raw[8:12] == b"WAVE", f"缺 WAVE 标记: {raw[8:12]!r}"
    assert raw[12:16] == b"fmt ", f"缺 fmt 标记: {raw[12:16]!r}"

    fmt_size = struct.unpack("<I", raw[16:20])[0]
    assert fmt_size == 16, f"fmt 块大小应为 16，实际: {fmt_size}"

    audio_format = struct.unpack("<H", raw[20:22])[0]
    assert audio_format == 1, f"audio_format 应为 1 (PCM)，实际: {audio_format}"

    channels = struct.unpack("<H", raw[22:24])[0]
    assert channels == 1, f"channels 应为 1 (mono)，实际: {channels}"

    bps = struct.unpack("<H", raw[34:36])[0]
    assert bps == 16, f"bits_per_sample 应为 16，实际: {bps}"

    assert raw[36:40] == b"data", f"缺 data 块标记: {raw[36:40]!r}"

    # 用 Python wave 模块能成功解析（最终验证）
    with wave.open(io.BytesIO(raw), "rb") as w:
        n_channels = w.getnchannels()
        sample_width = w.getsampwidth()
        framerate = w.getframerate()
        n_frames = w.getnframes()

        assert n_channels == 1, f"wave 模块读取 channels 应为 1，实际: {n_channels}"
        assert sample_width == 2, f"wave 模块读取 sample_width 应为 2 (16-bit)，实际: {sample_width}"
        assert framerate > 0, f"wave 模块读取 framerate 应 > 0，实际: {framerate}"
        assert n_frames > 0, f"wave 模块读取 n_frames 应 > 0，实际: {n_frames}"


def test_audio_with_empty_text_returns_400(client: TestClient) -> None:
    r = client.post("/v1/audio", json={"text": ""})
    assert r.status_code == 400, f"空 text 应返 400，实际: {r.status_code}"


# —— v0.7: image size 反向移植测试 ——


def test_image_with_custom_size_returns_png_of_that_size(client: TestClient) -> None:
    """v0.7 反向移植：size 参数应真实生效（之前 mock_image_png 不接 size，固定 1x1）。"""
    r = client.post(
        "/v1/images/generations",
        json={
            "model": "fingertip-image",
            "prompt": "size-test",
            "size": "512x256",
            "response_format": "b64_json",
        },
    )
    assert r.status_code == 200, f"非 2xx: {r.status_code} {r.text}"
    raw = base64.b64decode(r.json()["data"][0]["b64_json"])
    # PNG signature + IHDR length/type 校验
    assert raw[:8] == b"\x89PNG\r\n\x1a\n", f"非合法 PNG 签名: {raw[:8]!r}"
    # IHDR 起始位置 8，4 字节长度(13) + 4 字节类型('IHDR')
    ihdr = raw[8:8 + 4 + 4]
    assert ihdr[4:8] == b"IHDR", f"IHDR 块标识缺失: {ihdr!r}"
    width = int.from_bytes(raw[16:20], "big")
    height = int.from_bytes(raw[20:24], "big")
    assert width == 512, f"width 应为 512，实际 {width}"
    assert height == 256, f"height 应为 256，实际 {height}"


def test_image_with_garbage_size_falls_back_to_default(client: TestClient) -> None:
    """非法 size（如 "x"/"abc"/"0x0"）应回退到默认 1024x1024，不 panic。"""
    r = client.post(
        "/v1/images/generations",
        json={
            "model": "fingertip-image",
            "prompt": "fallback-test",
            "size": "garbage",
            "response_format": "b64_json",
        },
    )
    assert r.status_code == 200, f"非 2xx: {r.status_code} {r.text}"
    raw = base64.b64decode(r.json()["data"][0]["b64_json"])
    width = int.from_bytes(raw[16:20], "big")
    height = int.from_bytes(raw[20:24], "big")
    # 回退 1024x1024
    assert width == 1024, f"width 应回退 1024，实际 {width}"
    assert height == 1024, f"height 应回退 1024，实际 {height}"
