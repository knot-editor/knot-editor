//! Native Knot: Open Log: open the application log file as a document in the
//! captured target's window.

use super::*;
use knot_protocol::CommandOutcome;

/// Open the current log file where the captured target opens documents.
pub(crate) fn dispatch_open_log(
    target: &ProductCommandTarget,
    completion: CommandCompletion,
    cx: &mut App,
) -> Result<(), CommandOutcome> {
    let window = target.window(cx).ok_or(CommandOutcome::InvalidTarget)?;
    let path = crate::app::logging::log_directory()
        .map(|directory| crate::app::logging::log_file(&directory))
        .ok_or_else(|| CommandOutcome::HandlerFailure {
            message: "cannot determine the log directory".into(),
        })?;
    let request = OpenRequest::from_path(&path, Path::new("/"))
        .map_err(|message| CommandOutcome::HandlerFailure { message })?;
    let open_target = OpenTarget::from_command(target);
    window.update(cx, |window, cx| {
        window.start_open_request(request, open_target, false, Some(completion), cx);
    });
    Ok(())
}
