#!/usr/bin/env python3
"""Drive `claude -p` over stream-json and record both directions as JSONL.

usage: capture.py <case> <out.jsonl>
  case: allow | edit | deny | interrupt | controls | plan | usage | fallback | commands | ask
        | questions | skills | hooks | effort | subagent | resume | login | history
        | running | attach | followup | models | suggestion | thinking | recap
        | nested | stop | background | compact | advisor

"background" has the model start a short shell command in the background and
end its turn while it runs, to record how the CLI reports a backgrounded
shell starting and, after the turn, ending.
"edit" has the model read a ten-line file and change two lines next to each
other with one Edit, to record the patch the CLI reports with the result.
It keeps its session and copies it beside the output as `<out>-session.jsonl`,
where the stored result holds the patch too.
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
"hooks" sends no prompt, so it spends no usage: it writes a hook to the
local settings and asks for the /hooks listing, then adds a hook, edits the
first and removes the added one with `claude edit-hook`, as Vortex does, and
lists them again. "note" records hold each edit and what the command answered.
"effort" sends no prompt, so it spends no usage: it reads the session's
settings, sets the effort to max and then to low through the flag settings
layer, and reads the settings after each change.
"subagent" runs one turn whose model hands a small task to a subagent with
the Agent tool, with subagent text forwarded, to record the task events and
the subagent's own messages. Every tool call is allowed. The subagent runs
in the background, so recording goes on past the first turn until the
subagent has reported and the turn its report starts has ended.
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
"attach" runs one turn whose message holds a picture (a red square) next to
its text, which also mentions a file as `@note.txt`. The model reads a second
picture with the Read tool. It records how a picture goes in, whether the
CLI expands a mention, and how a tool's picture comes back. Every tool call
is allowed.
"followup" runs the "subagent" turn, then, once the subagent has reported,
has the model send that subagent a second task with SendMessage, and records
until it has reported again. It keeps its session and copies it, with the
files the CLI stores for each subagent, beside the output as
`<out>-session.jsonl` and `<out>-agent-<id>.jsonl`/`.meta.json`. Every tool
call is allowed.
"models" sends no prompt, so it spends no usage: it asks for the model
catalogue with `list_models`, to compare it with the models `initialize`
lists.
"suggestion" runs two tiny turns with `--prompt-suggestions`, and records
on past the second's result until the CLI sends the next prompt it
predicts. The CLI predicts none after a turn that writes much to the prompt
cache, as a session's first turn does.
"nested" runs one turn whose subagent starts two subagents of its own, and
copies its session as "followup" does. Every tool call is allowed.
"stop" runs one turn whose subagent runs a long shell command, stops that
subagent with `stop_task`, then tries to stop it again, to record both
replies and how a stopped subagent is reported. Every tool call is allowed.
"thinking" runs one turn that needs some reasoning, with thinking summaries
asked for (`--thinking-display summarized`), to record how a summary, its
token estimates and the turn's thinking tokens arrive.
"recap" runs one tiny turn, then the `/recap` command, whose output is a
summary of the conversation the model writes. It keeps its session and
copies it beside the output as `<out>-session.jsonl`.
"compact" runs one tiny turn, then the `/compact` command, to record how the
CLI reports compacting the conversation and the boundary it leaves.
"advisor" runs one turn on Sonnet with Opus as the advisor model (the
`advisorModel` setting), whose model is asked to consult the advisor once, to
record how the server-side advisor tool's call and answer arrive.
"running" sends no prompt, so it spends no usage: it starts a process and
copies the record the CLI keeps of it under `<config>/sessions/` while it
runs, which tells other clients the conversation is open.
"""
import base64, json, os, struct, subprocess, sys, threading, time, zlib

STRIP = {
    "CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION", "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET", "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH", "CLAUDE_PID", "CLAUDE_EFFORT", "AI_AGENT",
}
PROMPTS = {
    "allow": "Run the shell command `touch vortex-fixture.txt` with the Bash tool, then reply with the single word done.",
    "deny": "Run the shell command `touch vortex-fixture.txt` with the Bash tool, then reply with the single word done.",
    "interrupt": "Write the numbers from 1 to 300, one per line, with no other text.",
    "usage": "Reply with the single word ok.",
    "fallback": "Reply with the single word ok.",
    "commands": "/effort low",
    "questions": "Call the AskUserQuestion tool once with two questions. First: 'Which fruits do you like?' with header 'Fruits', multiSelect true, options Apple, Banana, Cherry. Second: 'Which city do you prefer?' with header 'City', multiSelect false, options Paris, Rome. After I answer, reply with only my answers, separated by a semicolon.",
    "ask": "Use the AskUserQuestion tool to ask me which color I prefer, with exactly two options: Red and Blue. After I answer, reply with only the color I chose.",
    "resume": "What word did I ask you to remember? Reply with that word only.",
    "followup": "Use the Agent tool once, with subagent_type general-purpose, description 'Read the note' and the prompt 'Read the file note.txt with the Read tool and reply with its first word only.' Then reply with only the word the agent reported.",
    "subagent": "Use the Agent tool once, with subagent_type general-purpose, description 'Read the note' and the prompt 'Read the file note.txt with the Read tool and reply with its first word only.' Then reply with only the word the agent reported.",
    "nested": "Use the Agent tool once, with subagent_type general-purpose, run_in_background false, description 'Outer' and this prompt: 'Use the Agent tool twice in one message, both with subagent_type general-purpose and run_in_background false, with descriptions Inner A and Inner B, each with the prompt: Read note.txt with the Read tool and reply with its word only. Then reply with both words.' Then reply with only what the agent reported.",
    "stop": "Use the Agent tool once, with subagent_type general-purpose, run_in_background false, description 'Wait' and the prompt 'Run the shell command `perl -e \"sleep 60\"` with the Bash tool in the foreground and reply with the word done.' Then reply with the single word ok.",
    "suggestion": "Write a Python function that returns the sum of a list of numbers. Reply with only the code.",
    "thinking": "Think step by step: how many times does the letter r appear in 'strawberry raspberry'? Reply with only the number.",
    "recap": "Write a haiku about rust. Reply with only the haiku.",
    "edit": "Read lines.txt with the Read tool, then use the Edit tool once to replace the line `four` with `FOUR` and the line `five` with `5`, then reply with the single word done.",
    "background": "Run the shell command `perl -e \"sleep 5\"` with the Bash tool and run_in_background true, then reply with the single word started without waiting for it.",
    "compact": "Reply with the single word ok.",
    "advisor": "Consult the advisor tool once about whether a function named add should take two numbers or a list. Then reply with one sentence saying what it advised.",
    "plan": "Plan how to create the file vortex-plan.txt containing the word hi. Keep the plan to one sentence and present it right away without exploring, then carry it out.",
}


SECOND_SUGGESTION = "Now make it ignore values that are not numbers. Reply with only the code."


def square_png(rgb, size=16):
    """A plain square of colour `rgb`, as PNG bytes."""
    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))
    rows = b"".join(b"\x00" + bytes(rgb) * size for _ in range(size))
    header = struct.pack(">IIBBBBB", size, size, 8, 2, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))


case, out_path = sys.argv[1], sys.argv[2]
if case == "attach":
    with open("note.txt", "w") as note:
        note.write("lantern\n")
    with open("picture.png", "wb") as picture:
        picture.write(square_png((0, 0, 255)))
if case == "edit":
    with open("lines.txt", "w") as lines:
        lines.write("".join(f"{word}\n" for word in
                            "one two three four five six seven eight nine ten".split()))
if case in ("subagent", "nested"):
    with open("note.txt", "w") as note:
        note.write("lantern\n")
if case == "followup":
    with open("note.txt", "w") as note:
        note.write("lantern moth\n")
FOLLOWUP = ("Use the SendMessage tool to send the agent you started the message "
            "'Reply with the second word of note.txt.' Then reply with only the word it reports.")
if case == "skills":
    for name in ("vortex-fixture", "vortex-hidden"):
        os.makedirs(f".claude/skills/{name}", exist_ok=True)
        with open(f".claude/skills/{name}/SKILL.md", "w") as skill:
            skill.write(f"---\nname: {name}\ndescription: A skill the {name} fixture lists.\n---\nSay hi.\n")
    with open(".claude/settings.local.json", "w") as settings:
        json.dump({"skillOverrides": {"vortex-hidden": "off"}}, settings)
if case == "hooks":
    os.makedirs(".claude", exist_ok=True)
    with open(".claude/settings.local.json", "w") as settings:
        json.dump({"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [
            {"type": "command", "command": "echo vortex-pre", "timeout": 30}]}]}}, settings)
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
    BASE + ([] if case in ("edit", "followup", "recap", "nested") else ["--no-session-persistence"])
    + (["--permission-mode", "plan"] if case == "plan" else [])
    + (["--forward-subagent-text"] if case in ("subagent", "followup", "nested", "stop") else [])
    + (["--prompt-suggestions"] if case == "suggestion" else [])
    + (["--thinking-display", "summarized"] if case == "thinking" else [])
    + (["--model", "sonnet", "--settings", json.dumps({"advisorModel": "opus"})]
       if case == "advisor" else [])
    + (["--resume", resumed] if resumed else []),
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    env=env, text=True, bufsize=1,
)
notified = False
# For "followup": how many subagent reports have come, and whether the
# follow-up was sent; and the session to copy.
reports = 0
followed_up = False
session_id = None
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


send({"type": "control_request", "request_id": "initialize" if case in ("controls", "commands", "skills", "hooks", "effort", "login", "models") else "init-1",
      "request": {"subtype": "initialize", "hooks": None}})
# Ids follow Vortex's own scheme (`Translator::request_id`), so a replay can
# issue the same commands and match the replies.
CONTROLS = [
    ("vortex-model-1", {"subtype": "set_model", "model": "sonnet"}),
    ("vortex-mode-2", {"subtype": "set_permission_mode", "mode": "acceptEdits"}),
    ("vortex-mode-3", {"subtype": "set_permission_mode", "mode": "not-a-mode"}),
    ("vortex-context-4", {"subtype": "get_context_usage"}),
    ("vortex-usage-5", {"subtype": "get_usage"}),
]
# Sent one at a time, each once the previous one is answered.
EFFORT = [
    ("vortex-settings-1", {"subtype": "get_settings"}),
    ("vortex-effort-2", {"subtype": "apply_flag_settings", "settings": {"effortLevel": "max"}}),
    ("vortex-settings-3", {"subtype": "get_settings"}),
    ("vortex-effort-4", {"subtype": "apply_flag_settings", "settings": {"effortLevel": "low"}}),
    ("vortex-settings-5", {"subtype": "get_settings"}),
]
if case == "controls":
    for request_id, request in CONTROLS:
        send({"type": "control_request", "request_id": request_id, "request": request})
elif case == "skills":
    send({"type": "control_request", "request_id": "vortex-skills-1",
          "request": {"subtype": "get_skills_dialog"}})
elif case == "models":
    send({"type": "control_request", "request_id": "vortex-models-1",
          "request": {"subtype": "list_models"}})
elif case == "hooks":
    send({"type": "control_request", "request_id": "vortex-hooks-1",
          "request": {"subtype": "get_hooks_listing"}})
elif case == "effort":
    request_id, request = EFFORT[0]
    send({"type": "control_request", "request_id": request_id, "request": request})
elif case == "login":
    send({"type": "control_request", "request_id": "vortex-login-1",
          "request": {"subtype": "claude_authenticate", "loginWithClaudeAi": True}})
elif case == "fallback":
    send({"type": "control_request", "request_id": "vortex-model-1",
          "request": {"subtype": "set_model", "model": "fable"}})
elif case == "attach":
    image = {"type": "image", "source": {"type": "base64", "media_type": "image/png",
             "data": base64.b64encode(square_png((255, 0, 0))).decode()}}
    text = {"type": "text", "text": "Name the colour of the attached image in one word. "
            "Then read picture.png with the Read tool. Then reply on one line with: the "
            "attached image's colour, picture.png's colour, and the first word of @note.txt."}
    send({"type": "user", "message": {"role": "user", "content": [image, text]},
          "parent_tool_use_id": None, "session_id": ""})
else:
    send({"type": "user", "message": {"role": "user", "content": PROMPTS[case]},
          "parent_tool_use_id": None, "session_id": ""})
awaiting = {request_id for request_id, _ in CONTROLS} if case == "controls" else set()

deltas = 0
interrupted = False
deadline = time.time() + {"login": 600, "followup": 240}.get(case, 120)
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
        if case in ("allow", "edit", "plan", "subagent", "attach", "followup", "nested", "stop", "background"):
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
    if case == "stop":
        if msg.get("subtype") == "task_started" and msg.get("task_type") == "local_agent":
            stopped_task = msg["task_id"]
        if msg.get("subtype") == "task_started" and msg.get("task_type") == "local_bash":
            send({"type": "control_request", "request_id": "vortex-stop-1",
                  "request": {"subtype": "stop_task", "task_id": stopped_task}})
        if kind == "control_response" and msg["response"].get("request_id") == "vortex-stop-1":
            send({"type": "control_request", "request_id": "vortex-stop-2",
                  "request": {"subtype": "stop_task", "task_id": stopped_task}})
    if kind == "control_response":
        awaiting.discard(msg["response"].get("request_id"))
        if case == "controls" and not awaiting:
            break
        if case == "usage" and msg["response"].get("request_id") == "vortex-usage-1":
            break
        if case == "skills" and msg["response"].get("request_id") == "vortex-skills-1":
            break
        if case == "models" and msg["response"].get("request_id") == "vortex-models-1":
            break
        ids = [request_id for request_id, _ in EFFORT]
        if case == "effort" and msg["response"].get("request_id") in ids:
            done = ids.index(msg["response"]["request_id"])
            if done + 1 == len(EFFORT):
                break
            request_id, request = EFFORT[done + 1]
            send({"type": "control_request", "request_id": request_id, "request": request})
        if case == "hooks" and msg["response"].get("request_id") == "vortex-hooks-1":
            listed = msg["response"]["response"]["hooks"][0]
            stop = {"type": "command", "command": "echo vortex-stop"}
            for edit in (
                {"op": "add", "source": "localSettings", "event": "Stop", "matcher": "",
                 "hook": stop},
                {"op": "replace", "source": "localSettings", "event": "PreToolUse",
                 "matcher": "Edit|Write",
                 "hook": {**listed["editable"]["config"], "command": "echo vortex-edited"},
                 "target": {"event": "PreToolUse", "matcher": listed["editable"]["matcher"],
                            "hook": listed["editable"]["config"]}},
                {"op": "remove", "source": "localSettings", "event": "Stop", "matcher": "",
                 "target": {"event": "Stop", "matcher": "", "hook": stop}},
            ):
                done = subprocess.run(["claude", "edit-hook", "--json"], input=json.dumps(edit),
                                      capture_output=True, text=True, env=env, timeout=60)
                record("note", {"edit": edit, "status": done.returncode,
                                "stdout": done.stdout, "stderr": done.stderr})
            # The CLI notices settings changes by watching the files.
            time.sleep(10)
            send({"type": "control_request", "request_id": "vortex-hooks-2",
                  "request": {"subtype": "get_hooks_listing"}})
        if case == "hooks" and msg["response"].get("request_id") == "vortex-hooks-2":
            break
        if case == "login" and msg["response"].get("request_id") == "vortex-login-1":
            urls = msg["response"].get("response") or {}
            print("Sign in at:", urls.get("automaticUrl") or msg["response"], flush=True)
            send({"type": "control_request", "request_id": "vortex-login-2",
                  "request": {"subtype": "claude_oauth_wait_for_completion"}})
        if case == "login" and msg["response"].get("request_id") == "vortex-login-2":
            break
    if kind == "control_response" and case == "fallback" \
            and msg["response"].get("request_id") == "vortex-model-1":
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
    # The subagent runs in the background: the turn that started it may end
    # first, and its report starts another turn once it finishes.
    if kind == "system" and msg.get("subtype") == "task_notification":
        notified = True
        reports += 1
    if kind == "system" and msg.get("subtype") == "init":
        session_id = msg.get("session_id")
    if kind == "result" and case == "subagent" and not notified:
        continue
    if case == "background" and kind == "system" and msg.get("subtype") == "task_notification":
        deadline = min(deadline, time.time() + 30)
    if kind == "result" and case == "background" and not notified:
        continue
    if kind == "result" and case == "followup":
        if reports == 0 or (followed_up and reports < 2):
            continue
        if not followed_up:
            followed_up = True
            send({"type": "user", "message": {"role": "user", "content": FOLLOWUP},
                  "parent_tool_use_id": None, "session_id": ""})
            continue
    if kind == "result" and case in ("recap", "compact") and not interrupted:
        interrupted = True
        send({"type": "user", "message": {"role": "user", "content": f"/{case}"},
              "parent_tool_use_id": None, "session_id": ""})
        continue
    # The prediction comes after the turn's result.
    if kind == "result" and case == "suggestion" and not interrupted:
        interrupted = True
        send({"type": "user", "message": {"role": "user", "content": SECOND_SUGGESTION},
              "parent_tool_use_id": None, "session_id": ""})
        continue
    if kind == "result" and case == "suggestion":
        deadline = min(deadline, time.time() + 60)
        continue
    if kind == "prompt_suggestion" and case == "suggestion":
        break
    if kind == "result" and case == "usage":
        send({"type": "control_request", "request_id": "vortex-usage-1",
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


def copy_session():
    """Copies the session file and each subagent's files beside the output."""
    config = os.environ.get("CLAUDE_CONFIG_DIR") or os.path.expanduser("~/.claude")
    projects = os.path.join(config, "projects")
    stem = out_path[:-len(".jsonl")] if out_path.endswith(".jsonl") else out_path
    for project in os.listdir(projects):
        path = os.path.join(projects, project, f"{session_id}.jsonl")
        if not os.path.exists(path):
            continue
        with open(path) as stored, open(f"{stem}-session.jsonl", "w") as out:
            out.write(stored.read())
        agents = os.path.join(projects, project, session_id, "subagents")
        for name in sorted(os.listdir(agents)) if os.path.isdir(agents) else []:
            with open(os.path.join(agents, name)) as stored, open(f"{stem}-{name}", "w") as out:
                out.write(stored.read())
        print(f"Copied {path} and {agents}")
        return
    print(f"no session file for {session_id}")


if case in ("edit", "followup", "recap", "nested"):
    copy_session()
print(case, "exit", proc.returncode, "lines", sum(1 for _ in open(out_path)))
