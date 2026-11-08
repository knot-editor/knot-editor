//! Native editor command declarations.
use super::*;
use crate::app::commands::declarations::{CommandContext, command};
use crate::app::commands::dispatch::ProductCommandTarget;
use knot_protocol::{Command, CommandArgumentValue, CommandOutcome};

fn claim_editor(command: &Command, target: &ProductCommandTarget, cx: &App) -> bool {
    if command.name.as_ref() != COPY_COMMAND || command.arguments != CommandArgumentValue::Null {
        return true;
    }
    matches!(target.command_view(cx), Some(crate::app::workbench::CommandView::Editor(view)) if view.read(cx).editor().read(cx).has_selection())
}

#[command("editor.move-left", title = "Move left", palette = false, id = MOVE_LEFT_COMMAND, participant = Editor)]
#[command("editor.move-right", title = "Move right", palette = false, id = MOVE_RIGHT_COMMAND, participant = Editor)]
#[command("editor.move-up", title = "Move up", palette = false, id = MOVE_UP_COMMAND, participant = Editor)]
#[command("editor.move-up-logical", title = "Move up ignoring soft wrap", palette = false, id = MOVE_UP_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-down", title = "Move down", palette = false, id = MOVE_DOWN_COMMAND, participant = Editor)]
#[command("editor.move-down-logical", title = "Move down ignoring soft wrap", palette = false, id = MOVE_DOWN_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-word-left", title = "Move to previous word", palette = false, id = MOVE_WORD_LEFT_COMMAND, participant = Editor)]
#[command("editor.move-word-right", title = "Move to next word", palette = false, id = MOVE_WORD_RIGHT_COMMAND, participant = Editor)]
#[command("editor.move-line-start", title = "Move to line start", palette = false, id = MOVE_LINE_START_COMMAND, participant = Editor)]
#[command("editor.move-line-start-logical", title = "Move to line start ignoring soft wrap", palette = false, id = MOVE_LINE_START_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-smart-home", title = "Move to first character or line start", palette = false, id = MOVE_SMART_HOME_COMMAND, participant = Editor)]
#[command("editor.move-smart-home-logical", title = "Move to first character or line start ignoring soft wrap", palette = false, id = MOVE_SMART_HOME_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-line-end", title = "Move to line end", palette = false, id = MOVE_LINE_END_COMMAND, participant = Editor)]
#[command("editor.move-line-end-logical", title = "Move to line end ignoring soft wrap", palette = false, id = MOVE_LINE_END_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-page-up", title = "Move up one page", palette = false, id = MOVE_PAGE_UP_COMMAND, participant = Editor)]
#[command("editor.move-page-up-logical", title = "Move up one page ignoring soft wrap", palette = false, id = MOVE_PAGE_UP_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-page-down", title = "Move down one page", palette = false, id = MOVE_PAGE_DOWN_COMMAND, participant = Editor)]
#[command("editor.move-page-down-logical", title = "Move down one page ignoring soft wrap", palette = false, id = MOVE_PAGE_DOWN_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.move-document-start", title = "Move to document start", palette = false, id = MOVE_DOCUMENT_START_COMMAND, participant = Editor)]
#[command("editor.move-document-end", title = "Move to document end", palette = false, id = MOVE_DOCUMENT_END_COMMAND, participant = Editor)]
#[command("editor.select-left", title = "Select left", palette = false, id = SELECT_LEFT_COMMAND, participant = Editor)]
#[command("editor.select-right", title = "Select right", palette = false, id = SELECT_RIGHT_COMMAND, participant = Editor)]
#[command("editor.select-up", title = "Select up", palette = false, id = SELECT_UP_COMMAND, participant = Editor)]
#[command("editor.select-up-logical", title = "Select up ignoring soft wrap", palette = false, id = SELECT_UP_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-down", title = "Select down", palette = false, id = SELECT_DOWN_COMMAND, participant = Editor)]
#[command("editor.select-down-logical", title = "Select down ignoring soft wrap", palette = false, id = SELECT_DOWN_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-word-left", title = "Select to previous word", palette = false, id = SELECT_WORD_LEFT_COMMAND, participant = Editor)]
#[command("editor.select-word-right", title = "Select to next word", palette = false, id = SELECT_WORD_RIGHT_COMMAND, participant = Editor)]
#[command("editor.select-line-start", title = "Select to line start", palette = false, id = SELECT_LINE_START_COMMAND, participant = Editor)]
#[command("editor.select-line-start-logical", title = "Select to line start ignoring soft wrap", palette = false, id = SELECT_LINE_START_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-smart-home", title = "Select to first character or line start", palette = false, id = SELECT_SMART_HOME_COMMAND, participant = Editor)]
#[command("editor.select-smart-home-logical", title = "Select to first character or line start ignoring soft wrap", palette = false, id = SELECT_SMART_HOME_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-line-end", title = "Select to line end", palette = false, id = SELECT_LINE_END_COMMAND, participant = Editor)]
#[command("editor.select-line-end-logical", title = "Select to line end ignoring soft wrap", palette = false, id = SELECT_LINE_END_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-page-up", title = "Select up one page", palette = false, id = SELECT_PAGE_UP_COMMAND, participant = Editor)]
#[command("editor.select-page-up-logical", title = "Select up one page ignoring soft wrap", palette = false, id = SELECT_PAGE_UP_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-page-down", title = "Select down one page", palette = false, id = SELECT_PAGE_DOWN_COMMAND, participant = Editor)]
#[command("editor.select-page-down-logical", title = "Select down one page ignoring soft wrap", palette = false, id = SELECT_PAGE_DOWN_LOGICAL_COMMAND, participant = Editor)]
#[command("editor.select-document-start", title = "Select to document start", palette = false, id = SELECT_DOCUMENT_START_COMMAND, participant = Editor)]
#[command("editor.select-document-end", title = "Select to document end", palette = false, id = SELECT_DOCUMENT_END_COMMAND, participant = Editor)]
#[command("editor.insert-newline", title = "Insert Newline", palette = false, id = INSERT_NEWLINE_COMMAND, participant = Editor)]
#[command("editor.insert-tab", title = "Insert Tab", palette = false, id = INSERT_TAB_COMMAND, participant = Editor)]
#[command("editor.indent-lines", title = "Indent Lines", description = "Indent the selected lines by one level.", id = INDENT_LINES_COMMAND, participant = Editor)]
#[command("editor.reindent-buffer", title = "Reindent Buffer", description = "Indent every line of the document from the language's syntax.", id = REINDENT_BUFFER_COMMAND, participant = Editor)]
#[command("editor.reindent-selected-lines", title = "Reindent Selected Lines", description = "Indent the selected lines from the language's syntax.", id = REINDENT_SELECTED_LINES_COMMAND, participant = Editor)]
#[command("editor.detect-indentation", title = "Detect Indentation from Content", description = "Indent with the style the document's text uses, or the default when it shows none.", id = DETECT_INDENTATION_COMMAND, participant = Editor)]
#[command("editor.convert-indentation-to-spaces", title = "Convert Indentation to Spaces", description = "Rewrite the indentation of every line with spaces, and indent with spaces from now on.", id = CONVERT_INDENTATION_TO_SPACES_COMMAND, participant = Editor)]
#[command("editor.convert-indentation-to-tabs", title = "Convert Indentation to Tabs", description = "Rewrite the indentation of every line with tabs, and indent with tabs from now on.", id = CONVERT_INDENTATION_TO_TABS_COMMAND, participant = Editor)]
#[command("editor.trim-trailing-whitespace", title = "Trim Trailing Whitespace", description = "Remove the spaces and tabs that end each line.", id = TRIM_TRAILING_WHITESPACE_COMMAND, participant = Editor)]
#[command("editor.outdent-lines", title = "Outdent Lines", description = "Outdent the selected lines by one level.", id = OUTDENT_LINES_COMMAND, participant = Editor)]
#[command("editor.copy-lines-up", title = "Copy Lines Up", description = "Duplicate the selected lines above them.", id = COPY_LINES_UP_COMMAND, participant = Editor)]
#[command("editor.copy-lines-down", title = "Copy Lines Down", description = "Duplicate the selected lines below them.", id = COPY_LINES_DOWN_COMMAND, participant = Editor)]
#[command("editor.move-lines-up", title = "Move Lines Up", description = "Move the selected lines above the previous line.", id = MOVE_LINES_UP_COMMAND, participant = Editor)]
#[command("editor.move-lines-down", title = "Move Lines Down", description = "Move the selected lines below the next line.", id = MOVE_LINES_DOWN_COMMAND, participant = Editor)]
#[command("editor.delete-lines", title = "Delete Lines", description = "Delete the selected lines.", id = DELETE_LINES_COMMAND, participant = Editor)]
#[command("editor.join-lines", title = "Join Lines", description = "Join the selected lines, or the current line and the next.", id = JOIN_LINES_COMMAND, participant = Editor)]
#[command("editor.jump-to-matching-bracket", title = "Jump to Matching Bracket", description = "Move to the bracket matching the one at the cursor, or to the closing bracket around it.", id = JUMP_TO_MATCHING_BRACKET_COMMAND, participant = Editor)]
#[command("editor.toggle-line-comment", title = "Toggle Line Comment", description = "Comment or uncomment the selected lines.", id = TOGGLE_LINE_COMMENT_COMMAND, participant = Editor)]
#[command("editor.toggle-block-comment", title = "Toggle Block Comment", description = "Wrap the selection in a block comment, or unwrap it.", id = TOGGLE_BLOCK_COMMENT_COMMAND, participant = Editor)]
#[command("editor.toggle-soft-wrap", title = "Toggle Soft Wrap", description = "Wrap the document's long lines at the window's width, or stop wrapping them.", id = TOGGLE_SOFT_WRAP_COMMAND, participant = Editor)]
#[command("editor.fold", title = "Fold", description = "Collapse the region at the cursor's line.", id = FOLD_COMMAND, participant = Editor)]
#[command("editor.unfold", title = "Unfold", description = "Expand the fold on the cursor's line.", id = UNFOLD_COMMAND, participant = Editor)]
#[command("editor.toggle-fold", title = "Toggle Fold", description = "Collapse the region at the cursor's line, or expand its fold.", id = TOGGLE_FOLD_COMMAND, participant = Editor)]
#[command("editor.fold-all", title = "Fold All", description = "Collapse every foldable region.", id = FOLD_ALL_COMMAND, participant = Editor)]
#[command("editor.unfold-all", title = "Unfold All", description = "Expand every fold.", id = UNFOLD_ALL_COMMAND, participant = Editor)]
#[command("editor.delete-backward", title = "Delete Backward", palette = false, id = DELETE_BACKWARD_COMMAND, participant = Editor)]
#[command("editor.delete-forward", title = "Delete Forward", palette = false, id = DELETE_FORWARD_COMMAND, participant = Editor)]
#[command("editor.delete-to-line-end", title = "Delete to Line End", palette = false, id = DELETE_TO_LINE_END_COMMAND, participant = Editor)]
#[command("editor.select-all", title = "Select All", description = "Select the whole document.", id = SELECT_ALL_COMMAND, participant = Editor)]
#[command("editor.undo", title = "Undo", description = "Undo the last edit.", id = UNDO_COMMAND, participant = Editor)]
#[command("editor.redo", title = "Redo", description = "Redo the last undone edit.", id = REDO_COMMAND, participant = Editor)]
#[command("editor.cut", title = "Cut", description = "Cut the selection to the clipboard.", id = CUT_COMMAND, participant = Editor)]
#[command("copy", title = "Copy", description = "Copy the selection to the clipboard.", id = COPY_COMMAND, participant = Editor, claim = claim_editor)]
#[command("editor.paste", title = "Paste", description = "Paste from the clipboard.", id = PASTE_COMMAND, participant = Editor)]
pub(crate) async fn editing(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    let name = context.name;
    context
        .foreground(move |target, cx| {
            let Some(crate::app::workbench::CommandView::Editor(view)) = target.command_view(cx)
            else {
                return Err(CommandOutcome::InvalidTarget);
            };
            let editor = view.read(cx).editor().clone();
            if editor.update(cx, |editor, cx| editor.execute_editing_command(name, cx)) {
                Ok(())
            } else {
                Err(CommandOutcome::Unavailable)
            }
        })
        .await
}
