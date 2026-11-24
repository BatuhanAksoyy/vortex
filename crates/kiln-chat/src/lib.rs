//! The agent chat panel: transcript, permission prompts and the composer.

mod agent_state;
mod assets;
mod chat_view;
mod code_block;
mod entry_view;
mod hook_list;
mod literal;
mod permission_prompt;
mod pictures;
mod processes;
mod prompt_cache;
mod question_form;
mod question_prompt;
mod skill_menu;
mod slash;
mod spinner;
mod subagent_view;
mod tool_format;
mod transcript;

pub use assets::Assets;
pub use chat_view::{ChatEvent, ChatView, attachments::Attachment};
pub use processes::{
    AgentProcesses, ProcessMeter, ProcessMeterEvent, Processes, ProcessesDialog,
    ProcessesDialogEvent,
};
pub use transcript::Status;
