//! Native editor command declarations.
use super::*;
use crate::app::commands::declarations::{CommandArguments, CommandContext, command};
use crate::app::commands::dispatch::ProductCommandTarget;
pub(crate) use knot_model::editor::commands::*;
use knot_protocol::{Command, CommandArgumentValue, CommandOutcome};

fn claim_editor(command: &Command, target: &ProductCommandTarget, cx: &App) -> bool {
    if command.name.as_ref() != COPY_COMMAND || command.arguments != CommandArgumentValue::Null {
        return true;
    }
    matches!(target.command_view(cx), Some(crate::app::workbench::CommandView::Editor(view)) if view.read(cx).editor().read(cx).has_selection())
}

#[command(knot_model::editor::commands::MOVE_LEFT_COMMAND, title = "Move left", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_RIGHT_COMMAND, title = "Move right", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_UP_COMMAND, title = "Move up", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_UP_LOGICAL_COMMAND, title = "Move up ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_DOWN_COMMAND, title = "Move down", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_DOWN_LOGICAL_COMMAND, title = "Move down ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_WORD_LEFT_COMMAND, title = "Move to previous word", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_WORD_RIGHT_COMMAND, title = "Move to next word", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_LINE_START_COMMAND, title = "Move to line start", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_LINE_START_LOGICAL_COMMAND, title = "Move to line start ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_SMART_HOME_COMMAND, title = "Move to first character or line start", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_SMART_HOME_LOGICAL_COMMAND, title = "Move to first character or line start ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_LINE_END_COMMAND, title = "Move to line end", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_LINE_END_LOGICAL_COMMAND, title = "Move to line end ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_PAGE_UP_COMMAND, title = "Move up one page", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_PAGE_UP_LOGICAL_COMMAND, title = "Move up one page ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_PAGE_DOWN_COMMAND, title = "Move down one page", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_PAGE_DOWN_LOGICAL_COMMAND, title = "Move down one page ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_DOCUMENT_START_COMMAND, title = "Move to document start", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::MOVE_DOCUMENT_END_COMMAND, title = "Move to document end", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_LEFT_COMMAND, title = "Select left", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_RIGHT_COMMAND, title = "Select right", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_UP_COMMAND, title = "Select up", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_UP_LOGICAL_COMMAND, title = "Select up ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_DOWN_COMMAND, title = "Select down", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_DOWN_LOGICAL_COMMAND, title = "Select down ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_WORD_LEFT_COMMAND, title = "Select to previous word", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_WORD_RIGHT_COMMAND, title = "Select to next word", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_LINE_START_COMMAND, title = "Select to line start", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_LINE_START_LOGICAL_COMMAND, title = "Select to line start ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_SMART_HOME_COMMAND, title = "Select to first character or line start", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_SMART_HOME_LOGICAL_COMMAND, title = "Select to first character or line start ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_LINE_END_COMMAND, title = "Select to line end", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_LINE_END_LOGICAL_COMMAND, title = "Select to line end ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_PAGE_UP_COMMAND, title = "Select up one page", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_PAGE_UP_LOGICAL_COMMAND, title = "Select up one page ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_PAGE_DOWN_COMMAND, title = "Select down one page", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_PAGE_DOWN_LOGICAL_COMMAND, title = "Select down one page ignoring soft wrap", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_DOCUMENT_START_COMMAND, title = "Select to document start", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_DOCUMENT_END_COMMAND, title = "Select to document end", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::INSERT_NEWLINE_COMMAND, title = "Insert Newline", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::INSERT_TAB_COMMAND, title = "Insert Tab", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::INDENT_LINES_COMMAND, title = "Indent Lines", description = "Indent the selected lines by one level.", participant = Editor)]
#[command(knot_model::editor::commands::REINDENT_BUFFER_COMMAND, title = "Reindent Buffer", description = "Indent every line of the document from the language's syntax.", participant = Editor)]
#[command(knot_model::editor::commands::REINDENT_SELECTED_LINES_COMMAND, title = "Reindent Selected Lines", description = "Indent the selected lines from the language's syntax.", participant = Editor)]
#[command(knot_model::editor::commands::DETECT_INDENTATION_COMMAND, title = "Detect Indentation from Content", description = "Indent with the style the document's text uses, or the default when it shows none.", participant = Editor)]
#[command(knot_model::editor::commands::CONVERT_INDENTATION_TO_SPACES_COMMAND, title = "Convert Indentation to Spaces", description = "Rewrite the indentation of every line with spaces, and indent with spaces from now on.", participant = Editor)]
#[command(knot_model::editor::commands::CONVERT_INDENTATION_TO_TABS_COMMAND, title = "Convert Indentation to Tabs", description = "Rewrite the indentation of every line with tabs, and indent with tabs from now on.", participant = Editor)]
#[command(knot_model::editor::commands::TRIM_TRAILING_WHITESPACE_COMMAND, title = "Trim Trailing Whitespace", description = "Remove the spaces and tabs that end each line.", participant = Editor)]
#[command(knot_model::editor::commands::OUTDENT_LINES_COMMAND, title = "Outdent Lines", description = "Outdent the selected lines by one level.", participant = Editor)]
#[command(knot_model::editor::commands::COPY_LINES_UP_COMMAND, title = "Copy Lines Up", description = "Duplicate the selected lines above them.", participant = Editor)]
#[command(knot_model::editor::commands::COPY_LINES_DOWN_COMMAND, title = "Copy Lines Down", description = "Duplicate the selected lines below them.", participant = Editor)]
#[command(knot_model::editor::commands::MOVE_LINES_UP_COMMAND, title = "Move Lines Up", description = "Move the selected lines above the previous line.", participant = Editor)]
#[command(knot_model::editor::commands::MOVE_LINES_DOWN_COMMAND, title = "Move Lines Down", description = "Move the selected lines below the next line.", participant = Editor)]
#[command(knot_model::editor::commands::DELETE_LINES_COMMAND, title = "Delete Lines", description = "Delete the selected lines.", participant = Editor)]
#[command(knot_model::editor::commands::JOIN_LINES_COMMAND, title = "Join Lines", description = "Join the selected lines, or the current line and the next.", participant = Editor)]
#[command(knot_model::editor::commands::JUMP_TO_MATCHING_BRACKET_COMMAND, title = "Jump to Matching Bracket", description = "Move to the bracket matching the one at the cursor, or to the closing bracket around it.", participant = Editor)]
#[command(knot_model::editor::commands::TOGGLE_LINE_COMMENT_COMMAND, title = "Toggle Line Comment", description = "Comment or uncomment the selected lines.", participant = Editor)]
#[command(knot_model::editor::commands::TOGGLE_BLOCK_COMMENT_COMMAND, title = "Toggle Block Comment", description = "Wrap the selection in a block comment, or unwrap it.", participant = Editor)]
#[command(knot_model::editor::commands::TOGGLE_SOFT_WRAP_COMMAND, title = "Toggle Soft Wrap", description = "Wrap the document's long lines at the window's width, or stop wrapping them.", participant = Editor)]
#[command(knot_model::editor::commands::FOLD_COMMAND, title = "Fold", description = "Collapse the region at the cursor's line.", participant = Editor)]
#[command(knot_model::editor::commands::UNFOLD_COMMAND, title = "Unfold", description = "Expand the fold on the cursor's line.", participant = Editor)]
#[command(knot_model::editor::commands::TOGGLE_FOLD_COMMAND, title = "Toggle Fold", description = "Collapse the region at the cursor's line, or expand its fold.", participant = Editor)]
#[command(knot_model::editor::commands::FOLD_ALL_COMMAND, title = "Fold All", description = "Collapse every foldable region.", participant = Editor)]
#[command(knot_model::editor::commands::UNFOLD_ALL_COMMAND, title = "Unfold All", description = "Expand every fold.", participant = Editor)]
#[command(knot_model::editor::commands::DELETE_BACKWARD_COMMAND, title = "Delete Backward", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::DELETE_FORWARD_COMMAND, title = "Delete Forward", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::DELETE_TO_LINE_END_COMMAND, title = "Delete to Line End", palette = false, participant = Editor)]
#[command(knot_model::editor::commands::SELECT_ALL_COMMAND, title = "Select All", description = "Select the whole document.", participant = Editor)]
#[command(knot_model::editor::commands::UNDO_COMMAND, title = "Undo", description = "Undo the last edit.", participant = Editor)]
#[command(knot_model::editor::commands::REDO_COMMAND, title = "Redo", description = "Redo the last undone edit.", participant = Editor)]
#[command(knot_model::editor::commands::CUT_COMMAND, title = "Cut", description = "Cut the selection to the clipboard.", participant = Editor)]
#[command(knot_model::editor::commands::COPY_COMMAND, title = "Copy", description = "Copy the selection to the clipboard.", participant = Editor, claim = claim_editor)]
#[command(knot_model::editor::commands::PASTE_COMMAND, title = "Paste", description = "Paste from the clipboard.", participant = Editor)]
pub(crate) async fn editing(context: CommandContext, (): ()) -> Result<(), CommandOutcome> {
    let name = context.name;
    let pending = context
        .foreground(move |target, cx| {
            let Some(crate::app::workbench::CommandView::Editor(view)) = target.command_view(cx)
            else {
                return Err(CommandOutcome::InvalidTarget);
            };
            let editor = view.read(cx).editor().clone();
            // A focused bar field takes the clipboard and select-all commands.
            if let Some(input) = editor.read(cx).focused_bar_input().cloned() {
                let handled = input.update(cx, |input, cx| {
                    match name {
                        COPY_COMMAND => input.copy(cx),
                        CUT_COMMAND => input.cut(cx),
                        PASTE_COMMAND => input.paste(cx),
                        SELECT_ALL_COMMAND => input.select_all(),
                        _ => return false,
                    }
                    cx.notify();
                    true
                });
                if handled {
                    return Ok(None);
                }
            }
            if name == PASTE_COMMAND {
                // A language's paste provider shapes the text, so its answer
                // is awaited; without one the paste is synchronous.
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                let call = text.as_deref().and_then(|text| {
                    let request = editor.read(cx).paste_request(text, cx)?;
                    let execution =
                        crate::app::extensions::host::call_paste_provider(target, &request, cx)?;
                    Some((request, execution))
                });
                return Ok(match call {
                    Some((request, execution)) => Some((editor, request, execution)),
                    None => {
                        editor.update(cx, |editor, cx| editor.execute_editing_command(name, cx));
                        None
                    }
                });
            }
            if editor.update(cx, |editor, cx| editor.execute_editing_command(name, cx)) {
                Ok(None)
            } else {
                Err(CommandOutcome::Unavailable)
            }
        })
        .await?;
    let Some((editor, request, execution)) = pending else {
        return Ok(());
    };
    let answer = knot_model::editor::PasteRequest::answer(
        execution
            .await
            .unwrap_or_else(|error| Err(error.to_string())),
    );
    context
        .foreground(move |_, cx| {
            editor.update(cx, |editor, cx| editor.paste_answered(&request, answer, cx));
            Ok(())
        })
        .await
}

/// The arguments of [`TYPE_COMMAND`].
pub(crate) struct TypeArguments {
    text: String,
    replace: usize,
}

impl CommandArguments for TypeArguments {
    fn decode(value: &CommandArgumentValue) -> Result<Self, CommandOutcome> {
        let invalid = |message: &str| CommandOutcome::InvalidArgument {
            message: message.into(),
        };
        let CommandArgumentValue::Object(fields) = value else {
            return Err(invalid("editor.type takes an object with a text field"));
        };
        let Some(CommandArgumentValue::String(text)) = fields.get("text") else {
            return Err(invalid("editor.type needs a string text"));
        };
        let replace = match fields.get("replace") {
            None | Some(CommandArgumentValue::Null) => 0,
            Some(CommandArgumentValue::Number(count))
                if count.fract() == 0. && *count >= 0. && *count <= u32::MAX as f64 =>
            {
                *count as usize
            }
            Some(_) => return Err(invalid("editor.type replace must be a count of characters")),
        };
        if fields.keys().any(|key| key != "text" && key != "replace") {
            return Err(invalid("editor.type takes only text and replace"));
        }
        Ok(Self {
            text: text.clone(),
            replace,
        })
    }
}

/// Type text into the captured editor as the keyboard does, over the
/// characters before the caret when asked.
#[command(knot_model::editor::commands::TYPE_COMMAND, title = "Type Text", palette = false, participant = Editor)]
pub(crate) async fn type_text(
    context: CommandContext,
    arguments: TypeArguments,
) -> Result<(), CommandOutcome> {
    context
        .foreground(move |target, cx| {
            let Some(crate::app::workbench::CommandView::Editor(view)) = target.command_view(cx)
            else {
                return Err(CommandOutcome::InvalidTarget);
            };
            let editor = view.read(cx).editor().clone();
            if let Some(input) = editor.read(cx).focused_bar_input().cloned() {
                input.update(cx, |input, cx| input.insert_text(None, &arguments.text, cx));
                return Ok(());
            }
            editor.update(cx, |editor, cx| {
                editor.type_replacing(arguments.replace, &arguments.text, cx)
            });
            Ok(())
        })
        .await
}
