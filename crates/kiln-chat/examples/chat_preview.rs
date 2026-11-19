//! Opens the chat panel on a scripted turn, to look at the design without
//! spending agent usage: `cargo run -p kiln-chat --example chat_preview`.
//! `KILN_PREVIEW_THEME=dark` picks the dark theme; `KILN_PREVIEW_PROMPT=1`
//! ends the turn on a permission prompt.

use std::{path::PathBuf, rc::Rc};

use gpui_kit::{
    AppContext as _, Bounds, WindowBounds, WindowOptions,
    component::{Theme, ThemeMode, ThemeRegistry},
    px, size,
};
use kiln_agent::{
    AgentBackend, AgentEvent, BlockKey, PermissionRequest, Remember, Session, SessionCommand,
    SessionSpec, ToolCall,
};
use kiln_chat::ChatView;
use serde_json::json;

const THEMES: &str = include_str!("../../../assets/themes/kiln.json");
const ROOT: &str = "/work/claude-cafe";

struct ScriptedBackend {
    prompt: bool,
}

impl AgentBackend for ScriptedBackend {
    fn spawn(&self, _: SessionSpec) -> anyhow::Result<Session> {
        let (events_tx, events) = async_channel::unbounded();
        let (commands, commands_rx) = async_channel::unbounded::<SessionCommand>();
        let script = script(self.prompt);
        Ok(Session {
            events,
            commands,
            // Plays the turn once the first message arrives; keeps the
            // command channel open so the turn stays "running".
            driver: Box::pin(play(script, events_tx, commands_rx)),
        })
    }
}

async fn play(
    script: Vec<AgentEvent>,
    events: async_channel::Sender<AgentEvent>,
    commands: async_channel::Receiver<SessionCommand>,
) {
    if commands.recv().await.is_err() {
        return;
    }
    for event in script {
        let _ = events.send(event).await;
    }
    while commands.recv().await.is_ok() {}
}

fn block(index: u64) -> BlockKey {
    BlockKey {
        message_id: "m1".into(),
        index,
    }
}

fn tool(id: &str, name: &str, input: serde_json::Value) -> AgentEvent {
    AgentEvent::ToolCall(ToolCall {
        id: id.into(),
        parent: None,
        name: name.into(),
        input,
    })
}

fn result(id: &str, output: &str) -> AgentEvent {
    AgentEvent::ToolResult {
        id: id.into(),
        output: output.into(),
        is_error: false,
    }
}

fn script(prompt: bool) -> Vec<AgentEvent> {
    let file = format!("{ROOT}/src/utils/localization.ts");
    let test = format!("{ROOT}/src/test/localization.test.ts");
    let read_output = "line\n".repeat(256);
    let mut events = vec![
        AgentEvent::Started {
            session_id: "preview".into(),
            model: "claude-opus-5-5".into(),
        },
        AgentEvent::ThinkingDelta {
            block: block(0),
            text: "The user wants tests for the localization helpers.".into(),
        },
        AgentEvent::TextDelta {
            block: block(1),
            text: "Sure, let me take a look at the file:".into(),
        },
        tool("t1", "Read", json!({ "file_path": file })),
        result("t1", &read_output),
        tool(
            "t2",
            "Write",
            json!({ "file_path": test, "content": "import { formatDate, translateKey } from '../utils/localization';\n\ndescribe('LocalizationUtil', () => {\n  describe('formatDate', () => {\n    it('formats a date', () => {\n      expect(formatDate(new Date(0), 'en')).toBe('1/1/1970');\n    });\n  });\n});" }),
        ),
        result("t2", "File created successfully"),
        tool(
            "t3",
            "Bash",
            json!({ "command": "npm test -- localization" }),
        ),
        result(
            "t3",
            "> claude-cafe@1.0.0 test\n> vitest run localization\n\n ✓ src/test/localization.test.ts (1 test) 4ms\n\n Test Files  1 passed (1)\n      Tests  1 passed (1)",
        ),
        AgentEvent::TextDelta {
            block: block(2),
            text: "The tests pass. I'll also tighten the **date format** in `formatDate`:".into(),
        },
        tool(
            "t4",
            "Edit",
            json!({ "file_path": file, "old_string": "  return date.toLocaleDateString();", "new_string": "  return date.toLocaleDateString(locale, {\n    timeZone: 'UTC',\n  });" }),
        ),
    ];
    if prompt {
        events.push(AgentEvent::PermissionRequest(PermissionRequest {
            id: "p1".into(),
            tool: "Edit".into(),
            input: json!({ "file_path": file }),
            description: None,
            remember: Some(Remember::AcceptEdits),
        }));
    }
    events
}

fn main() {
    let dark = std::env::var("KILN_PREVIEW_THEME").is_ok_and(|theme| theme == "dark");
    let prompt = std::env::var("KILN_PREVIEW_PROMPT").is_ok();
    gpui_kit::application()
        .with_assets(kiln_chat::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Err(error) = ThemeRegistry::global_mut(cx).load_themes_from_str(THEMES) {
                log_error(&format!("themes: {error:#}"));
            }
            let themes = ThemeRegistry::global(cx).themes().clone();
            Theme::update(cx, |theme| {
                if let Some(light) = themes.get("Kiln Light") {
                    theme.light_theme = light.clone();
                }
                if let Some(dark) = themes.get("Kiln Dark") {
                    theme.dark_theme = dark.clone();
                }
            });
            Theme::change(
                if dark {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                },
                None,
                cx,
            );
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(560.), px(860.)),
                    cx,
                ))),
                ..Default::default()
            };
            let opened = gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| {
                    let backend: Rc<dyn AgentBackend> = Rc::new(ScriptedBackend { prompt });
                    let mut view = ChatView::new(backend, PathBuf::from(ROOT), window, cx);
                    view.submit(
                        "can you add tests for the localization utils?".into(),
                        window,
                        cx,
                    );
                    view
                })
            });
            if let Err(error) = opened {
                log_error(&format!("cannot open the preview window: {error:#}"));
                cx.quit();
            }
        });
}

fn log_error(message: &str) {
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr(), "{message}");
}
