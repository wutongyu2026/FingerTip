"""Mock 后端（FingerTip-Engine 默认推理实现）。

提供三类推理的固定产物，所有产物都满足 Rust 端 EngineClient 与编排器的硬契约：
- LLM 产物：固定 JSON 字符串，含 music_description / image_description / sentence
- 图像产物：合法 1x1 PNG（8 字节签名 + IHDR + IDAT + IEND）
- 音频产物：合法 PCM16 mono 0.5s 440Hz WAV（保证 wav_analysis 能解析）

mock 模式让用户在没装真实模型的环境下也能跑通端到端流程。
"""

from __future__ import annotations

import base64
import math
import struct
import zlib
from typing import Iterable, List


# —— LLM Mock ——

# 与 Rust 端编排器契约一致：固定 JSON 结构，便于 deterministic 测试。
# v0.8: 升级到 6 字段（music_description / image_description / sentence /
#        english_sentence / theme_explanation / funny_summary），对齐 MiniMax 契约。
MOCK_CHAT_JSON: str = (
    '{"music_description":"calm piano with rain ambience",'
    '"image_description":"orange abstract with swirling shapes",'
    '"sentence":"A quiet day of focus",'
    '"english_sentence":"A quiet day of focus, one keystroke at a time.",'
    '"theme_explanation":"反复斟酌推敲，今天的主题是 REWRITE",'
    '"funny_summary":"今天键盘敲了 449 下，退格键拿下 95 次高光——修改比打字还勤，属实是纠结型输出选手。"}'
)


def mock_chat(messages: List[dict]) -> str:
    """Mock LLM：忽略 messages 内容，返回固定 JSON 字符串。

    返回值是**字符串**，调用方负责 JSON 解析。LLM 端点契约要求 `content` 是 JSON 字符串。
    """
    return MOCK_CHAT_JSON


# —— 图像 Mock ——


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    """构造一个 PNG chunk：length(4 BE) + tag(4) + data + crc32(4 BE)。"""
    length = struct.pack(">I", len(data))
    crc = struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
    return length + tag + data + crc


def mock_image_png(width: int = 1, height: int = 1) -> bytes:
    """构造合法 PNG bytes（默认 1x1 RGB）。

    通过 PNG signature + IHDR + IDAT + IEND 四块构造，确保 PNG 解码器都能识别。
    用 8-bit RGB 模式（IHDR color_type=2），最小化复杂度。
    """
    # PNG signature: 8 字节
    sig = b"\x89PNG\r\n\x1a\n"

    # IHDR: 13 字节内容（宽/高 4 字节 + bit_depth + color_type + 3 保留字节）
    ihdr_data = struct.pack(
        ">IIBBBBB",
        width, height,  # 宽度、高度
        8,              # bit depth
        2,              # color type: 2 = RGB
        0, 0, 0,        # compression/filter/interlace (默认)
    )
    ihdr = _png_chunk(b"IHDR", ihdr_data)

    # IDAT: 1 行像素（filter byte=0 + RGB），zlib 压缩
    raw_pixels = b"\x00" + (b"\xff\x00\x00" * width)  # 红色像素 + 过滤字节
    idat_data = zlib.compress(raw_pixels)
    idat = _png_chunk(b"IDAT", idat_data)

    # IEND: 空数据
    iend = _png_chunk(b"IEND", b"")

    return sig + ihdr + idat + iend


def mock_image_b64(width: int = 1, height: int = 1) -> str:
    """返回 base64-encoded PNG 字符串（HTTP 契约要求）。"""
    return base64.b64encode(mock_image_png(width, height)).decode("ascii")


# —— 音频 Mock ——


def _wav_pcm16_mono(samples: Iterable[int], sample_rate: int = 44100) -> bytes:
    """把 PCM16 mono 采样序列包装成合法 WAV bytes。

    WAV 布局：RIFF 头 + fmt chunk（16 字节 PCM 标准）+ data chunk。
    所有字段用 little-endian，校验 wave 模块能成功打开。
    """
    pcm_bytes = b"".join(struct.pack("<h", max(-32768, min(32767, s))) for s in samples)

    # fmt chunk: tag + size(16) + audio_format(1=PCM) + channels(1) + sample_rate + byte_rate + block_align + bits_per_sample
    fmt = struct.pack(
        "<4sIHHIIHH",
        b"fmt ",
        16,              # PCM 标准 fmt 块大小
        1,               # audio_format: 1 = PCM
        1,               # channels: 1 = mono
        sample_rate,
        sample_rate * 2, # byte_rate = sample_rate * channels * bits_per_sample/8
        2,               # block_align = channels * bits_per_sample/8
        16,              # bits_per_sample
    )

    data_chunk = b"data" + struct.pack("<I", len(pcm_bytes)) + pcm_bytes

    # RIFF 头：magic + 总大小 + WAVE
    riff = b"RIFF" + struct.pack("<I", 4 + len(fmt) + len(data_chunk)) + b"WAVE"

    return riff + fmt + data_chunk


def mock_wav_440hz(duration_ms: int = 500, sample_rate: int = 44100, frequency: float = 440.0,
                   amplitude: float = 0.5) -> bytes:
    """生成 0.5s 440Hz 正弦波 WAV（PCM16 mono）。

    amplitude ∈ [0, 1]，转换为 int16 时乘 32767 并截断。
    """
    n_samples = sample_rate * duration_ms // 1000
    peak = amplitude * 32767

    def gen() -> Iterable[int]:
        for i in range(n_samples):
            t = i / sample_rate
            yield int(peak * math.sin(2 * math.pi * frequency * t))

    return _wav_pcm16_mono(gen(), sample_rate=sample_rate)


# —— 健康检查辅助 ——


def mock_capability_summary() -> dict:
    """Mock 模式下 health 端点返的固定形态——供测试断言用。

    注意：app.py 中实际 health 端点返回真实能力探测结果，
    这里只是 mock 自检。
    """
    return {"llm": True, "image": True, "audio": True}
