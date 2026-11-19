#!/usr/bin/env python3
"""Drive `claude -p` over stream-json and record both directions as JSONL.

usage: capture.py <case> <out.jsonl>   case: allow | deny | interrupt
"""
import json, os, subprocess, sys, threading, time

STRIP = {
    "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET", "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH", "CLAUDE_PID", "CLAUDE_EFFORT", "AI_AGENT",
}
PROMPTS = {
    "allow": "Run the shell command `touch kiln-fixture.txt` with the Bash tool, then reply with the single word done.",
    "deny": "Run the shell command `touch kiln-fixture.txt` with the Bash tool, then reply with the single word done.",
    "interrupt": "Write the numbers from 1 to 300, one per line, with no other text.",
}

case, out_path = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if k not in STRIP}
proc = subprocess.Popen(
    ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
     "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio",
     "--model", "haiku", "--no-session-persistence"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    env=env, text=True, bufsize=1,
)
log = open(out_path, "w")
lock = threading.Lock()


def record(direction, obj):
    with lock:
        log.write(json.dumps({"dir": direction, "msg": obj}) + "\n")
        log.flush()


def send(obj):
    record("in", obj)
    proc.stdin.write(json.dumps(obj) + "\n")
    proc.stdin.flush()


send({"type": "control_request", "request_id": "init-1",
      "request": {"subtype": "initialize", "hooks": None}})
send({"type": "user", "message": {"role": "user", "content": PROMPTS[case]},
      "parent_tool_use_id": None, "session_id": ""})

deltas = 0
interrupted = False
deadline = time.time() + 120
for line in proc.stdout:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except json.JSONDecodeError:
        record("out-raw", line)
        continue
    record("out", msg)
    kind = msg.get("type")
    if kind == "control_request" and msg["request"].get("subtype") == "can_use_tool":
        req = msg["request"]
        if case == "allow":
            response = {"behavior": "allow", "updatedInput": req["input"]}
        else:
            response = {"behavior": "deny", "message": "The user denied this tool call."}
        send({"type": "control_response", "response": {
            "subtype": "success", "request_id": msg["request_id"], "response": response}})
    if case == "interrupt" and kind == "stream_event":
        deltas += 1
        if deltas == 15 and not interrupted:
            interrupted = True
            send({"type": "control_request", "request_id": "interrupt-1",
                  "request": {"subtype": "interrupt"}})
    if kind == "result" or time.time() > deadline:
        break

proc.stdin.close()
try:
    proc.wait(timeout=15)
except subprocess.TimeoutExpired:
    proc.kill()
err = proc.stderr.read()
if err.strip():
    record("stderr", err)
print(case, "exit", proc.returncode, "lines", sum(1 for _ in open(out_path)))
