#!/usr/bin/env python3
"""
CardNote Compiler Mock Provider
模拟 OpenAI-compatible API，将请求保存到文件，等待响应文件后返回。
"""

import json
import os
import sys
import time
from http.server import HTTPServer, BaseHTTPRequestHandler

REQ_DIR = "/tmp/cardnote_requests"
RESP_DIR = "/tmp/cardnote_responses"
os.makedirs(REQ_DIR, exist_ok=True)
os.makedirs(RESP_DIR, exist_ok=True)

class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        # 静默日志，避免干扰 cardc 输出
        pass

    def do_POST(self):
        if self.path == "/chat/completions":
            content_len = int(self.headers.get('Content-Length', 0))
            body = self.rfile.read(content_len).decode('utf-8')
            req_data = json.loads(body)

            # 生成请求 ID
            req_id = f"req_{int(time.time() * 1000000)}"
            req_file = os.path.join(REQ_DIR, f"{req_id}.json")

            # 保存请求
            with open(req_file, 'w') as f:
                json.dump(req_data, f, ensure_ascii=False, indent=2)

            # 打印提示到 stderr（stdout 会被 cardc 消费）
            print(f"\n[MOCK] 收到请求: {req_id}", file=sys.stderr)
            # 提取最后一条用户消息的前200字用于提示
            msgs = req_data.get('messages', [])
            last_msg = msgs[-1]['content'][:200] if msgs else ""
            print(f"[MOCK] 最后消息: {last_msg}...", file=sys.stderr)
            print(f"[MOCK] 等待响应文件: {RESP_DIR}/{req_id}.json", file=sys.stderr)

            # 轮询等待响应文件
            resp_file = os.path.join(RESP_DIR, f"{req_id}.json")
            waited = 0
            while not os.path.exists(resp_file):
                time.sleep(0.5)
                waited += 0.5
                if waited > 3600:  # 1小时超时
                    self.send_error(504, "Timeout waiting for response")
                    return

            # 读取响应
            with open(resp_file, 'r') as f:
                resp_data = json.load(f)

            # 删除已处理的文件
            os.remove(req_file)
            os.remove(resp_file)

            # 返回 OpenAI 格式响应
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(resp_data, ensure_ascii=False).encode())
        else:
            self.send_error(404)

    def do_GET(self):
        # 健康检查
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.end_headers()
        self.wfile.write(b'{"status":"ok"}')

if __name__ == "__main__":
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8787
    server = HTTPServer(('127.0.0.1', port), Handler)
    print(f"[MOCK] Mock Provider 启动于 http://127.0.0.1:{port}", file=sys.stderr)
    print(f"[MOCK] 请求保存到: {REQ_DIR}", file=sys.stderr)
    print(f"[MOCK] 响应读取自: {RESP_DIR}", file=sys.stderr)
    server.serve_forever()
