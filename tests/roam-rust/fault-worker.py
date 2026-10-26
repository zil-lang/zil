"""Deliberately malformed single-process worker for native host rejection tests."""
import json
import os
import sys
import time

mode = sys.argv[1]

def send(value):
    print(json.dumps(value), flush=True)

if mode == "missing":
    sys.exit(0)
if mode == "malformed":
    print("{bad", flush=True)
    sys.exit(0)
send({"type": "hello", "format": "roam-worker/1", "capabilities": []})
if mode == "not-reading":
    # A valid large run frame fills stdin's pipe; timeout must remain supervised.
    time.sleep(1.2)
    sys.exit(0)
case = json.loads(sys.stdin.readline())["case"]
if mode == "timeout":
    time.sleep(5)
elif mode == "after-before":
    send({"type": "after", "step": 0})
elif mode == "wrong-action":
    send({"type": "before", "step": 0, "action": {"op": "edit", "value": "one"}})
elif mode == "forged-pass":
    send({"type": "result", "result": {"status": "pass", "steps": 0, "issues": []}})
elif mode == "wrong-count":
    send({"type": "result", "result": {"status": "error", "steps": 1, "issues": []}})
elif mode == "duplicate-result":
    message = {"type": "result", "result": {"status": "invalid", "steps": 0, "issues": []}}
    send(message)
    send(message)
elif mode == "output-budget":
    print("x" * 10000, flush=True)
elif mode == "crash":
    send({"type": "before", "step": 0, "action": case["steps"][0]})
    json.loads(sys.stdin.readline())
    os._exit(7)
