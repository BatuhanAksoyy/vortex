//! The agent reading and answering what the page asks the user: a dialog,
//! a device chooser, a permission request. An action of the agent's
//! that sets one off returns once it is asked, since the page waits.

use std::pin::pin;

use futures::future::{Either, select};
use gpui_kit::{AsyncWindowContext, Context, WeakEntity, Window};
use vortex_bridge::{Answer, Content, Question, Reply};

use super::{
    Browser,
    agent::{CAUTION, POLL},
};

impl Browser {
    /// What the page waits on the user for, in words; `None` when nothing.
    pub(super) fn questions(&self) -> Option<String> {
        let asked: Vec<String> = [
            self.dialog_question(),
            self.device_question(),
            self.permission_question(),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!asked.is_empty()).then(|| format!("{}\n{CAUTION}", asked.join("\n\n")))
    }

    pub(super) fn answer_for_agent(
        &mut self,
        answer: &Answer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Reply {
        let Answer {
            question,
            accept,
            text,
            device,
        } = answer;
        let done = match question {
            Question::Dialog => {
                (self.answer_dialog_for_agent(*accept, text.as_deref(), window, cx))
                    .map(str::to_owned)
            }
            Question::Device => self.pick_device_for_agent(*accept, device.as_deref(), window, cx),
            Question::Permission => self
                .answer_permission_for_agent(*accept, cx)
                .map(str::to_owned),
        };
        done.map_or_else(Reply::failed, Reply::text)
    }
}

/// Does `acting`, unless the page asks the user something first; the page
/// would then wait, and the action with it.
pub(super) async fn unless_asked(
    this: &WeakEntity<Browser>,
    acting: impl Future<Output = anyhow::Result<Reply>>,
    cx: &mut AsyncWindowContext,
) -> anyhow::Result<Reply> {
    let before = this.read_with(cx, |this, _| this.agent.asked)?;
    let done = {
        let mut watching = cx.clone();
        let asked = pin!(asked_since(this, before, &mut watching));
        match select(pin!(acting), asked).await {
            Either::Left((done, _)) => Some(done?),
            Either::Right(((), _)) => None,
        }
    };
    let questions = this.read_with(cx, |this, _| this.questions())?;
    Ok(match (done, questions) {
        (Some(done), None) => done,
        (Some(mut done), Some(questions)) => {
            let asked = format!("The page now asks:\n{questions}\nAnswer with browser_answer.");
            done.content.push(Content::Text(asked));
            done
        }
        (None, questions) => Reply::text(format!(
            "The page asks this and waits for the answer:\n{}\nAnswer with browser_answer.",
            questions.unwrap_or_default()
        )),
    })
}

async fn asked_since(this: &WeakEntity<Browser>, before: u64, cx: &mut AsyncWindowContext) {
    loop {
        cx.background_executor().timer(POLL).await;
        // Gone, the action fails by itself.
        let asked = this.read_with(cx, |this, _| this.agent.asked);
        if asked.is_ok_and(|asked| asked != before) {
            return;
        }
    }
}
