#!/usr/bin/env python3
"""Drive `claude -p` over stream-json and record both directions as JSONL.

usage: capture.py <case> <out.jsonl>   case: allow | deny | interrupt | controls | plan | usage | fallback

"controls" sends no prompt, so it spends no usage: it changes the model and
mode, asks for context and plan usage, and sends one invalid mode to record
an error.
"plan" starts in plan mode and answers every tool call with a plain allow, to
record how approving the plan (ExitPlanMode) leaves plan mode.
"usage" runs one tiny turn, then asks for plan usage, so the turn's
rate_limit_event can be compared with the get_usage reply.
"fallback" switches to a model the plan may not include (Fable), sends one
message, then a second, to record what the CLI reports when it refuses.
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
    "usage": "Reply with the single word ok.",
    "fallback": "Reply with the single word ok.",
    "plan": "Plan how to create the file kiln-plan.txt containing the word hi. Keep the plan to one sentence and present it right away without exploring, then carry it out.",
}

case, out_path = sys.argv[1], sys.argv[2]
env = {k: v for k, v in os.environ.items() if k not in STRIP}
proc = subprocess.Popen(
    ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
     "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio",
     "--model", "haiku", "--no-session-persistence"]
    + (["--permission-mode", "plan"] if case == "plan" else []),
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


send({"type": "control_request", "request_id": "initialize" if case == "controls" else "init-1",
      "request": {"subtype": "initialize", "hooks": None}})
# Ids follow Kiln's own scheme (`Translator::request_id`), so a replay can
# issue the same commands and match the replies.
CONTROLS = [
    ("kiln-model-1", {"subtype": "set_model", "model": "sonnet"}),
    ("kiln-mode-2", {"subtype": "set_permission_mode", "mode": "acceptEdits"}),
    ("kiln-mode-3", {"subtype": "set_permission_mode", "mode": "not-a-mode"}),
    ("kiln-context-4", {"subtype": "get_context_usage"}),
    ("kiln-usage-5", {"subtype": "get_usage"}),
]
if case == "controls":
    for request_id, request in CONTROLS:
        send({"type": "control_request", "request_id": request_id, "request": request})
elif case == "fallback":
    send({"type": "control_request", "request_id": "kiln-model-1",
          "request": {"subtype": "set_model", "model": "fable"}})
else:
    send({"type": "user", "message": {"role": "user", "content": PROMPTS[case]},
          "parent_tool_use_id": None, "session_id": ""})
awaiting = {request_id for request_id, _ in CONTROLS} if case == "controls" else set()

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
        if case in ("allow", "plan"):
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
    if kind == "control_response":
        awaiting.discard(msg["response"].get("request_id"))
        if case == "controls" and not awaiting:
            break
        if case == "usage" and msg["response"].get("request_id") == "kiln-usage-1":
            break
    if kind == "control_response" and case == "fallback" \
            and msg["response"].get("request_id") == "kiln-model-1":
        send({"type": "user", "message": {"role": "user", "content": PROMPTS[case]},
              "parent_tool_use_id": None, "session_id": ""})
    if kind == "result" and case == "fallback" and not interrupted:
        interrupted = True
        send({"type": "user", "message": {"role": "user", "content": PROMPTS[case]},
              "parent_tool_use_id": None, "session_id": ""})
        continue
    if kind == "result" and case == "usage":
        send({"type": "control_request", "request_id": "kiln-usage-1",
              "request": {"subtype": "get_usage"}})
        continue
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
