"""JSONL conformance worker; this is a protocol example, not a ported kernel."""
import json
import sys


def send(message):
    print(json.dumps(message, ensure_ascii=True), flush=True)


send({"type": "hello", "format": "roam-worker/1", "capabilities": []})
message = json.loads(sys.stdin.readline())
case = message["case"]
if message["type"] != "run" or case["contract"] != "portable-counter/1":
    raise ValueError("unsupported command or contract")
value = case.get("initial", 0)
result = {"status": "pass", "steps": 0, "issues": []}
for index, action in enumerate(case["steps"]):
    send({"type": "before", "step": index, "action": action})
    ack = json.loads(sys.stdin.readline())
    if ack != {"type": "ack", "step": index}:
        raise ValueError("checkpoint acknowledgement mismatch")
    if action["op"] == "add":
        value += action["value"]
    elif action["op"] == "check":
        if value != action["expected"]:
            result["status"] = "fail"
            result["failure"] = {"propertyId": "counter.value", "message": "counter differs", "expected": action["expected"], "actual": value}
    else:
        raise ValueError("unknown action")
    result["steps"] += 1
    send({"type": "after", "step": index})
    if result["status"] == "fail":
        break
send({"type": "result", "result": result})
