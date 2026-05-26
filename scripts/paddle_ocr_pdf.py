#!/usr/bin/env python3
"""
PaddleOCR PDF OCR 脚本
用法: python paddle_ocr_pdf.py <input_pdf> [output_md]

将扫描版 PDF 转为 Markdown 文本。
使用 PaddleOCR (PP-OCRv4) 进行文字识别，PyMuPDF 处理 PDF。
"""

import sys
import os

try:
    import pymupdf
except ImportError:
    print("ERROR: pymupdf not installed. Run: pip install pymupdf", file=sys.stderr)
    sys.exit(1)

try:
    from paddleocr import PaddleOCR
except ImportError:
    print("ERROR: paddleocr not installed. Run: pip install paddleocr", file=sys.stderr)
    sys.exit(1)


def pdf_to_images(pdf_path, dpi=200):
    """将 PDF 每页转为 PIL Image"""
    doc = pymupdf.open(pdf_path)
    images = []
    for i in range(doc.page_count):
        page = doc[i]
        # 使用矩阵提高分辨率
        mat = pymupdf.Matrix(dpi/72, dpi/72)
        pix = page.get_pixmap(matrix=mat)
        from PIL import Image
        img = Image.frombytes("RGB", [pix.width, pix.height], pix.samples)
        images.append(img)
    doc.close()
    return images


def ocr_page(ocr, image, page_num):
    """对单页图片进行 OCR，返回文本行列表"""
    import numpy as np
    # PaddleOCR.predict() 只接受 numpy.ndarray 或 str
    if hasattr(image, 'convert'):
        img_array = np.array(image)
    else:
        img_array = image
    result = ocr.predict(img_array)
    lines = []
    if result:
        for line in result:
            # line format: {"text": "...", "score": 0.99, "bbox": [...]}
            text = line.get("text", "")
            score = line.get("score", 0)
            bbox = line.get("bbox", [])
            lines.append((bbox, text, score))
    return lines


def sort_lines_by_reading_order(lines):
    """按阅读顺序排序：先从上到下，再从左到右"""
    # bbox format: [[x1,y1], [x2,y1], [x2,y2], [x1,y2]]
    def get_y(line):
        bbox = line[0]
        return sum(p[1] for p in bbox) / 4  # 中心点 y 坐标
    def get_x(line):
        bbox = line[0]
        return sum(p[0] for p in bbox) / 4  # 中心点 x 坐标

    # 按 y 坐标分组（同一行的文本）
    lines = sorted(lines, key=get_y)

    # 简单处理：y 坐标接近的行，按 x 坐标排序
    grouped = []
    current_group = []
    current_y = None
    y_threshold = 20  # 同一行的阈值

    for line in lines:
        y = get_y(line)
        if current_y is None or abs(y - current_y) < y_threshold:
            current_group.append(line)
            if current_y is None:
                current_y = y
            else:
                current_y = (current_y * len(current_group) + y) / (len(current_group) + 1)
        else:
            # 排序当前组（从左到右），然后添加到结果
            current_group.sort(key=get_x)
            grouped.extend(current_group)
            current_group = [line]
            current_y = y

    if current_group:
        current_group.sort(key=get_x)
        grouped.extend(current_group)

    return grouped


def format_page_text(lines, page_num):
    """将 OCR 结果格式化为 Markdown 文本"""
    if not lines:
        return f"\n\n## 第 {page_num} 页\n\n（本页无识别到文字）\n"

    sorted_lines = sort_lines_by_reading_order(lines)
    texts = [line[1] for line in sorted_lines]

    # 合并为一个段落（简单处理）
    paragraph = "".join(texts)

    return f"\n\n## 第 {page_num} 页\n\n{paragraph}\n"


def main():
    if len(sys.argv) < 2:
        print("Usage: python paddle_ocr_pdf.py <input.pdf> [output.md]", file=sys.stderr)
        sys.exit(1)

    pdf_path = sys.argv[1]
    output_path = sys.argv[2] if len(sys.argv) > 2 else None

    if not os.path.exists(pdf_path):
        print(f"ERROR: File not found: {pdf_path}", file=sys.stderr)
        sys.exit(1)

    # 初始化 OCR（首次运行会下载模型，如果还没下载）
    print(f"Initializing PaddleOCR...", file=sys.stderr)
    ocr = PaddleOCR(
        lang='ch',
        text_det_limit_side_len=2000,  # 降低检测尺寸限制，避免内存溢出
    )

    print(f"Converting PDF to images: {pdf_path}", file=sys.stderr)
    images = pdf_to_images(pdf_path, dpi=150)  # 降低 DPI 避免内存溢出
    total = len(images)
    print(f"Total pages: {total}", file=sys.stderr)

    # 处理每一页
    all_text = ""
    for i, img in enumerate(images):
        page_num = i + 1
        print(f"OCR page {page_num}/{total}...", file=sys.stderr)
        lines = ocr_page(ocr, img, page_num)
        page_text = format_page_text(lines, page_num)
        all_text += page_text

    # 输出
    if output_path:
        with open(output_path, 'w', encoding='utf-8') as f:
            f.write(all_text)
        print(f"Saved to: {output_path}", file=sys.stderr)
    else:
        print(all_text)


if __name__ == "__main__":
    main()
