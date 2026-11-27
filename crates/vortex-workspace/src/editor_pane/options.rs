//! The editor settings an open file's editor follows.

use gpui_kit::{
    Context, Window,
    component::input::{EditorState, TabSize},
};
use vortex_settings::{EditorSettings, ScrollBeyond, Whitespace};

/// `editor` built as the settings say.
pub(super) fn built(editor: EditorState, settings: &EditorSettings) -> EditorState {
    editor
        .line_number(settings.line_numbers)
        .indent_guides(settings.indent_guides)
        .soft_wrap(settings.soft_wrap)
        .tab_size(tab_size(settings))
        .folding(settings.gutter.folds)
        .auto_close(settings.typing.auto_close_brackets)
        .smart_indent(settings.typing.auto_indent)
        .show_whitespaces(settings.show_whitespace == Whitespace::All)
        .scroll_beyond_last_line(beyond(settings))
}

/// Changes `editor` to what the settings say now.
pub(super) fn apply(
    editor: &mut EditorState,
    settings: &EditorSettings,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) {
    editor.set_line_number(settings.line_numbers, window, cx);
    editor.set_soft_wrap(settings.soft_wrap, window, cx);
    editor.set_indent_guides(settings.indent_guides, window, cx);
    editor.set_tab_size(tab_size(settings), cx);
    editor.set_folding(settings.gutter.folds, window, cx);
    editor.set_auto_close(settings.typing.auto_close_brackets, window, cx);
    editor.set_smart_indent(settings.typing.auto_indent, window, cx);
    let whitespace = settings.show_whitespace == Whitespace::All;
    editor.set_show_whitespaces(whitespace, window, cx);
    editor.set_scroll_beyond_last_line(beyond(settings), window, cx);
}

fn tab_size(settings: &EditorSettings) -> TabSize {
    TabSize {
        tab_size: usize::from(settings.tab_size),
        hard_tabs: false,
    }
}

/// `None` is the editor's own half a page.
fn beyond(settings: &EditorSettings) -> Option<usize> {
    (settings.scroll_beyond_last_line == ScrollBeyond::Off).then_some(0)
}
