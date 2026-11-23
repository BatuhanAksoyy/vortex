#!/usr/bin/env python3
"""Drive `claude -p` over stream-json and record both directions as JSONL.

usage: capture.py <case> <out.jsonl>
  case: allow | deny | interrupt | controls | plan | usage | fallback | commands | ask
        | questions | skills | rules | resume | login | history | running

"controls" sends no prompt, so it spends no usage: it changes the model and
mode, asks for context and plan usage, and sends one invalid mode to record
an error.
"plan" starts in plan mode and answers every tool call with a plain allow, to
record how approving the plan (ExitPlanMode) leaves plan mode.
"usage" runs one tiny turn, then asks for plan usage, so the turn's
rate_limit_event can be compared with the get_usage reply.
"fallback" switches to a model the plan may not include (Fable), sends one
message, then a second, to record what the CLI reports when it refuses.
"commands" sends two slash commands the CLI runs itself, so it spends no
usage: `/effort low`, whose output is text, then `/clear`, which resets the
conversation.
"ask" has the model ask a question with AskUserQuestion and answers it with
the first option, to record how a question arrives and how its answer goes
back.
"questions" has the model ask two questions at once, one allowing several
answers; it is answered with two options joined by ", " and with free text
that matches no option, as the "Other" field would send.
"skills" sends no prompt, so it spends no usage: it writes two project skills,
one turned off in the local settings, and asks for the /skills menu's rows.
"rules" sends no prompt, so it spends no usage: it writes two rules to the
local settings and lists the session's rules, then removes one from the file,
as Kiln does, and lists them again to record whether the CLI sees the change.
"resume" runs two tiny turns: one in a first process, which saves its
session, then one in a second process started with `--resume`, which is the
one recorded. A "note" record holds the first session's id, to compare with
the id the resumed process reports.
"login" sends no prompt: it starts a claude.ai sign-in in a credential store
of its own (`./credentials`, so the usual login is not touched), prints the
sign-in URL and waits until the sign-in in the browser completes. Run it from
a plain terminal, not from inside Claude Code.
"history" runs two tiny turns in a process that saves its session, the first
reading a file with the Read tool, then copies the session file the CLI
wrote. Its lines are the stored conversation, not stream-json records.
"running" sends no prompt, so it spends no usage: it starts a process and
copies the record the CLI keeps of it under `<config>/sessions/` while it
runs, which tells other clients the conversation is open.
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
    "commands": "/effort low",
    "questions": "Call the AskUserQuestion tool once with two questions. First: 'Which fruits do you like?' with header 'Fruits', multiSelect true, options Apple, Banana, Cherry. Second: 'Which city do you prefer?' with header 'City', multiSelect false, options Paris, Rome. After I answer, reply with only my answers, separated by a semicolon.",
    "ask": "Use the AskUserQuestion tool to ask me which color I prefer, with exactly two options: Red and Blue. After I answer, reply with only the color I chose.",
    "resume": "What word did I ask you to remember? Reply with that word only.",
    "plan": "Plan how to create the file kiln-plan.txt containing the word hi. Keep the plan to one sentence and present it right away without exploring, then carry it out.",
}

case, out_path = sys.argv[1], sys.argv[2]
if case == "skills":
    for name in ("kiln-fixture", "kiln-hidden"):
        os.makedirs(f".claude/skills/{name}", exist_ok=True)
        with open(f".claude/skills/{name}/SKILL.md", "w") as skill:
            skill.write(f"---\nname: {name}\ndescription: A skill the {name} fixture lists.\n---\nSay hi.\n")
    with open(".claude/settings.local.json", "w") as settings:
        json.dump({"skillOverrides": {"kiln-hidden": "off"}}, settings)
RULES = {"allow": ["Bash(npm test:*)", "WebFetch"], "deny": ["Read(./.env)"]}
if case == "rules":
    os.makedirs(".claude", exist_ok=True)
    with open(".claude/settings.local.json", "w") as settings:
        json.dump({"permissions": RULES}, settings)
env = {k: v for k, v in os.environ.items() if k not in STRIP}
if case == "login":
    env["CLAUDE_SECURESTORAGE_CONFIG_DIR"] = os.path.abspath("credentials")
BASE = ["claude", "-p", "--input-format", "stream-json", "--output-format", "stream-json",
        "--verbose", "--include-partial-messages", "--permission-prompt-tool", "stdio",
        "--model", "haiku"]


def first_session():
    """Runs one turn in a process that saves its session; returns its id."""
    lines = [
        {"type": "control_request", "request_id": "init-1",
         "request": {"subtype": "initialize", "hooks": None}},
        {"type": "user", "message": {"role": "user",
                                     "content": "Remember the word marigold. Reply with the single word ok."},
         "parent_tool_use_id": None, "session_id": ""},
    ]
    first = subprocess.Popen(BASE, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, env=env, text=True, bufsize=1)
    for line in lines:
        first.stdin.write(json.dumps(line) + "\n")
    first.stdin.flush()
    session_id = None
    for line in first.stdout:
        msg = json.loads(line)
        if msg.get("type") == "system" and msg.get("subtype") == "init":
            session_id = msg["session_id"]
        if msg.get("type") == "result":
            break
    first.stdin.close()
    first.wait(timeout=30)
    return session_id


def history():
    """Runs two turns that save their session; copies the session file."""
    with open("note.txt", "w") as note:
        note.write("lantern\n")
    turns = [
        "Read the file note.txt with the Read tool, then reply with its first word only.",
        "Reply with the single word done.",
    ]
    proc = subprocess.Popen(BASE, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, env=env, text=True, bufsize=1)

    def write(obj):
        proc.stdin.write(json.dumps(obj) + "\n")
        proc.stdin.flush()

    write({"type": "control_request", "request_id": "init-1",
           "request": {"subtype": "initialize", "hooks": None}})
    session_id = None
    for turn in turns:
        write({"type": "user", "message": {"role": "user", "content": turn},
               "parent_tool_use_id": None, "session_id": ""})
        for line in proc.stdout:
            msg = json.loads(line)
            if msg.get("type") == "system" and msg.get("subtype") == "init":
                session_id = msg["session_id"]
            if msg.get("type") == "control_request":
                request = msg["request"]
                write({"type": "control_response", "response": {
                    "subtype": "success", "request_id": msg["request_id"],
                    "response": {"behavior": "allow", "updatedInput": request.get("input", {})}}})
            if msg.get("type") == "result":
                break
    proc.stdin.close()
    proc.wait(timeout=30)
    config = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
    projects = os.path.join(config, "projects")
    for project in os.listdir(projects):
        path = os.path.join(projects, project, f"{session_id}.jsonl")
        if os.path.exists(path):
            with open(path) as stored, open(out_path, "w") as out:
                out.write(stored.read())
            print(f"Copied {path}")
            return
    sys.exit(f"no session file for {session_id}")


def running():
    """Copies the CLI's record of a running process."""
    proc = subprocess.Popen(BASE, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, env=env, text=True, bufsize=1)
    proc.stdin.write(json.dumps({"type": "control_request", "request_id": "initialize",
                                 "request": {"subtype": "initialize", "hooks": None}}) + "\n")
    proc.stdin.flush()
    for line in proc.stdout:
        if json.loads(line).get("type") == "control_response":
            break
    config = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
    record = os.path.join(config, "sessions", f"{proc.pid}.json")
    with open(record) as kept, open(out_path, "w") as out:
        out.write(kept.read().strip() + "\n")
    proc.stdin.close()
    proc.wait(timeout=30)


if case == "history":
    history()
    sys.exit(0)
if case == "running":
    running()
    sys.exit(0)
resumed = first_session() if case == "resume" else None
proc = subprocess.Popen(
    BASE + ["--no-session-persistence"]
    + (["--permission-mode", "plan"] if case == "plan" else [])
    + (["--resume", resumed] if resumed else []),
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    env=env, text=True, bufsize=1,
)
log = open(out_path, "w")
lock = threading.Lock()
if resumed:
    log.write(json.dumps({"dir": "note", "msg": {"resumed": resumed}}) + "\n")


def record(direction, obj):
    with lock:
        log.write(json.dumps({"dir": direction, "msg": obj}) + "\n")
        log.flush()


def send(obj):
    record("in", obj)
    proc.stdin.write(json.dumps(obj) + "\n")
    proc.stdin.flush()


send({"type": "control_request", "request_id": "initialize" if case in ("controls", "commands", "skills", "rules", "login") else "init-1",
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
elif case == "skills":
    send({"type": "control_request", "request_id": "kiln-skills-1",
          "request": {"subtype": "get_skills_dialog"}})
elif case == "rules":
    send({"type": "control_request", "request_id": "kiln-rules-1",
          "request": {"subtype": "list_permission_rules"}})
elif case == "login":
    send({"type": "control_request", "request_id": "kiln-login-1",
          "request": {"subtype": "claude_authenticate", "loginWithClaudeAi": True}})
elif case == "fallback":
    send({"type": "control_request", "request_id": "kiln-model-1",
          "request": {"subtype": "set_model", "model": "fable"}})
else:
    send({"type": "user", "message": {"role": "user", "content": PROMPTS[case]},
          "parent_tool_use_id": None, "session_id": ""})
awaiting = {request_id for request_id, _ in CONTROLS} if case == "controls" else set()

deltas = 0
interrupted = False
deadline = time.time() + (600 if case == "login" else 120)
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
        elif case == "questions":
            answers = {}
            for q in req["input"].get("questions", []):
                labels = [o["label"] for o in q["options"]]
                answers[q["question"]] = ", ".join(labels[:2]) if q.get("multiSelect") else "Istanbul"
            response = {"behavior": "allow",
                        "updatedInput": {**req["input"], "answers": answers}}
        elif case == "ask":
            questions = req["input"].get("questions", [])
            answers = {q["question"]: q["options"][0]["label"] for q in questions}
            response = {"behavior": "allow",
                        "updatedInput": {**req["input"], "answers": answers}}
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
        if case == "skills" and msg["response"].get("request_id") == "kiln-skills-1":
            break
        if case == "rules" and msg["response"].get("request_id") == "kiln-rules-1":
            with open(".claude/settings.local.json", "w") as settings:
                json.dump({"permissions": {**RULES, "allow": RULES["allow"][1:]}}, settings)
            # The CLI notices settings changes by watching the files.
            time.sleep(10)
            send({"type": "control_request", "request_id": "kiln-rules-2",
                  "request": {"subtype": "list_permission_rules"}})
        if case == "rules" and msg["response"].get("request_id") == "kiln-rules-2":
            break
        if case == "login" and msg["response"].get("request_id") == "kiln-login-1":
            urls = msg["response"].get("response") or {}
            print("Sign in at:", urls.get("automaticUrl") or msg["response"], flush=True)
            send({"type": "control_request", "request_id": "kiln-login-2",
                  "request": {"subtype": "claude_oauth_wait_for_completion"}})
        if case == "login" and msg["response"].get("request_id") == "kiln-login-2":
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
    if kind == "result" and case == "commands" and not interrupted:
        interrupted = True
        send({"type": "user", "message": {"role": "user", "content": "/clear"},
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
