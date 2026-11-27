//! What the user has answered so far to the agent's questions, without any
//! UI: the question on screen, the highlighted row, the options picked and
//! the user's own words. Each question lists its options and then an
//! "Other" row that takes free text.

use std::collections::BTreeSet;

use vortex_agent::Question;

/// What picking a row did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picked {
    /// An option of a single-choice question: the form may move on.
    Single,
    /// An option of a multiple-choice question was toggled.
    Toggled,
    /// The "Other" row: its text field wants focus.
    Other,
}

pub struct QuestionForm {
    questions: Vec<Question>,
    current: usize,
    highlighted: usize,
    answers: Vec<Answer>,
}

#[derive(Clone, Default)]
struct Answer {
    picked: BTreeSet<usize>,
    other: bool,
    other_text: String,
}

impl QuestionForm {
    pub fn new(questions: Vec<Question>) -> Self {
        let answers = vec![Answer::default(); questions.len()];
        Self {
            questions,
            current: 0,
            highlighted: 0,
            answers,
        }
    }

    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn current_question(&self) -> Option<&Question> {
        self.questions.get(self.current)
    }

    /// The row the keyboard is on; the option count means "Other".
    pub fn highlighted(&self) -> usize {
        self.highlighted
    }

    pub fn other_row(&self) -> usize {
        self.current_question()
            .map_or(0, |question| question.options.len())
    }

    pub fn show(&mut self, index: usize) {
        if index < self.questions.len() && index != self.current {
            self.current = index;
            self.highlighted = 0;
        }
    }

    /// Moves to the next question; `false` on the last one.
    pub fn next(&mut self) -> bool {
        let has_next = self.current + 1 < self.questions.len();
        if has_next {
            self.show(self.current + 1);
        }
        has_next
    }

    pub fn previous(&mut self) {
        if let Some(index) = self.current.checked_sub(1) {
            self.show(index);
        }
    }

    /// Moves the highlight by `step` rows, stopping at the first and last.
    pub fn move_highlight(&mut self, step: isize) {
        self.highlighted = self
            .highlighted
            .saturating_add_signed(step)
            .min(self.other_row());
    }

    /// Picks `row` of the current question: selects a single choice,
    /// toggles a multiple one, or picks "Other".
    pub fn pick(&mut self, row: usize) -> Option<Picked> {
        let question = self.questions.get(self.current)?;
        let multi = question.multi_select;
        let options = question.options.len();
        let answer = &mut self.answers[self.current];
        self.highlighted = row.min(options);
        if row >= options {
            answer.other = !(multi && answer.other);
            if !multi {
                answer.picked.clear();
            }
            return Some(Picked::Other);
        }
        if multi {
            if !answer.picked.remove(&row) {
                answer.picked.insert(row);
            }
            return Some(Picked::Toggled);
        }
        answer.picked = BTreeSet::from([row]);
        answer.other = false;
        Some(Picked::Single)
    }

    pub fn is_picked(&self, question: usize, row: usize) -> bool {
        let Some(answer) = self.answers.get(question) else {
            return false;
        };
        if row == self.questions[question].options.len() {
            answer.other
        } else {
            answer.picked.contains(&row)
        }
    }

    pub fn set_other_text(&mut self, question: usize, text: &str) {
        if let Some(answer) = self.answers.get_mut(question) {
            text.clone_into(&mut answer.other_text);
        }
    }

    /// The answer to `question`, joined with ", "; empty while unanswered.
    /// "Other" counts only once something is typed into it.
    pub fn answer(&self, question: usize) -> String {
        let (Some(answer), Some(asked)) =
            (self.answers.get(question), self.questions.get(question))
        else {
            return String::new();
        };
        let own = answer.other_text.trim();
        answer
            .picked
            .iter()
            .filter_map(|&row| asked.options.get(row))
            .map(|option| option.label.as_str())
            .chain((answer.other && !own.is_empty()).then_some(own))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn is_answered(&self, question: usize) -> bool {
        !self.answer(question).is_empty()
    }

    pub fn is_complete(&self) -> bool {
        (0..self.questions.len()).all(|question| self.is_answered(question))
    }

    pub fn answers(&self) -> Vec<String> {
        (0..self.questions.len())
            .map(|question| self.answer(question))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use vortex_agent::QuestionOption;

    use super::*;

    fn question(multi_select: bool) -> Question {
        Question {
            text: "Which?".into(),
            header: "Pick".into(),
            options: ["A", "B", "C"]
                .iter()
                .map(|label| QuestionOption {
                    label: (*label).into(),
                    description: String::new(),
                })
                .collect(),
            multi_select,
        }
    }

    #[test]
    fn a_single_choice_replaces_the_last_one() {
        let mut form = QuestionForm::new(vec![question(false)]);
        assert_eq!(form.pick(0), Some(Picked::Single));
        form.pick(2);
        assert_eq!(form.answers(), ["C"]);
        assert!(form.is_complete());
    }

    #[test]
    fn multiple_choices_toggle_and_keep_option_order() {
        let mut form = QuestionForm::new(vec![question(true)]);
        form.pick(2);
        form.pick(0);
        form.pick(1);
        form.pick(1);
        assert_eq!(form.answers(), ["A, C"]);
    }

    #[test]
    fn other_counts_once_something_is_typed() {
        let mut form = QuestionForm::new(vec![question(false)]);
        form.pick(0);
        assert_eq!(form.pick(3), Some(Picked::Other));
        assert!(!form.is_complete(), "Other is still empty");
        form.set_other_text(0, "  Navy ");
        assert_eq!(form.answers(), ["Navy"]);
        form.pick(1);
        assert_eq!(form.answers(), ["B"], "an option replaces Other");
    }

    #[test]
    fn other_adds_to_multiple_choices() {
        let mut form = QuestionForm::new(vec![question(true)]);
        form.pick(1);
        form.pick(3);
        form.set_other_text(0, "Z");
        assert_eq!(form.answers(), ["B, Z"]);
    }

    #[test]
    fn every_question_needs_an_answer() {
        let mut form = QuestionForm::new(vec![question(false), question(true)]);
        form.pick(0);
        assert!(!form.is_complete());
        assert!(form.next());
        assert!(!form.next(), "the last question");
        form.pick(2);
        assert!(form.is_complete());
        assert_eq!(form.answers(), ["A", "C"]);
    }

    #[test]
    fn the_highlight_stays_within_the_rows() {
        let mut form = QuestionForm::new(vec![question(false)]);
        form.move_highlight(-1);
        assert_eq!(form.highlighted(), 0);
        form.move_highlight(10);
        assert_eq!(form.highlighted(), form.other_row());
    }
}
