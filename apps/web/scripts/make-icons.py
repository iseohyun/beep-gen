"""icon.svg 와 같은 그림을 PNG(192, 512)로 만든다. 아이콘 디자인을 바꿀 때만 다시 실행한다.

사용법: python apps/web/scripts/make-icons.py   (Pillow 필요)
"""
import os
from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "public")
BLUE = (0x25, 0x63, 0xEB, 255)
SCALE = 4  # 크게 그린 뒤 줄여서 가장자리를 부드럽게 한다.

# (x, y, 너비, 높이, 모서리 반지름, 불투명도) — icon.svg 의 512 기준 좌표
BARS = [(96, 136, 56, 240, 28, 1.0), (200, 216, 40, 80, 20, 0.7),
        (288, 136, 56, 240, 28, 1.0), (392, 216, 40, 80, 20, 0.7)]

for size in (192, 512):
    big = size * SCALE
    k = big / 512
    image = Image.new("RGBA", (big, big), (0, 0, 0, 0))
    ImageDraw.Draw(image).rounded_rectangle((0, 0, big - 1, big - 1), radius=104 * k, fill=BLUE)
    for x, y, w, h, r, alpha in BARS:
        layer = Image.new("RGBA", (big, big), (0, 0, 0, 0))
        ImageDraw.Draw(layer).rounded_rectangle(
            (x * k, y * k, (x + w) * k, (y + h) * k), radius=r * k,
            fill=(255, 255, 255, round(255 * alpha)))
        image = Image.alpha_composite(image, layer)
    image.resize((size, size), Image.LANCZOS).save(os.path.join(OUT, f"icon-{size}.png"))
    print("icon", size)
