"""Regenerate synthetic format fixtures with Python 3 and FFmpeg (not needed by tests)."""

import math
from pathlib import Path
import struct
import subprocess
import wave

ROOT = Path(__file__).resolve().parent
SOURCE = ROOT / "stereo.wav"
with wave.open(str(SOURCE), "wb") as output:
    output.setparams((2, 2, 48_000, 0, "NONE", "not compressed"))
    output.writeframes(b"".join(
        struct.pack("<hh", *(
            round(12_000 * math.sin(2 * math.pi * frequency * frame / 48_000))
            for frequency in (440, 880)
        ))
        for frame in range(12_000)
    ))

CASES = {
    "stereo.aac": ["-c:a", "aac", "-f", "adts"],
    "stereo.m4a": ["-c:a", "aac"],
    "mono.m4a": ["-c:a", "aac", "-ac", "1", "-ar", "44100"],
    "short.m4a": ["-c:a", "aac", "-t", "0.01"],
    "alac.m4a": ["-c:a", "alac"],
    "fragmented.mp4": ["-c:a", "aac", "-movflags", "frag_keyframe+empty_moov"],
    "stereo.mp2": ["-c:a", "mp2", "-b:a", "192k"],
    "stereo.aiff": ["-c:a", "pcm_s16be"],
    "stereo.caf": ["-c:a", "alac"],
    "pcm.caf": ["-c:a", "pcm_s16le"],
    "stereo.webm": ["-c:a", "vorbis", "-strict", "experimental"],
    "stereo.mka": ["-c:a", "flac"],
    "adpcm.wav": ["-c:a", "adpcm_ima_wav"],
    "flac.ogg": ["-c:a", "flac"],
}
for filename, options in CASES.items():
    subprocess.run([
        "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
        "-i", str(SOURCE), "-map_metadata", "-1", *options, str(ROOT / filename),
    ], check=True)

for name, include_audio in [("video-first.mp4", True), ("video-only.mp4", False)]:
    subprocess.run([
        "ffmpeg", "-nostdin", "-hide_banner", "-loglevel", "error", "-y",
        "-f", "lavfi", "-i", "color=black:size=16x16:rate=20:duration=0.25",
        "-i", str(SOURCE), "-map", "0:v",
        *(["-map", "1:a", "-c:a", "aac"] if include_audio else []),
        "-c:v", "mpeg4", "-map_metadata", "-1", str(ROOT / name),
    ], check=True)

# MPEG-1 Layer I: mono, 256 kbit/s, 44.1 kHz, no CRC. Zero bit allocations encode silence.
mp1_frame = bytes.fromhex("ffff80c0") + bytes(272)
(ROOT / "silence.mp1").write_bytes(mp1_frame * 30)
