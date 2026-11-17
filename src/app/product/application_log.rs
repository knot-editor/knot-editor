//! Native Knot: Open Log and Knot: Open Personal Configuration: open the
//! application log file or the personal `post-init.js` as a document in the
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

/// Open the personal configuration's `post-init.js` where the captured target
/// opens documents; it is a new document when the file does not exist yet.
pub(crate) fn dispatch_open_personal_config(
    target: &ProductCommandTarget,
    completion: CommandCompletion,
    cx: &mut App,
) -> Result<(), CommandOutcome> {
    let window = target.window(cx).ok_or(CommandOutcome::InvalidTarget)?;
    let path = crate::app::personal_config::user_config_root()
        .map(|root| root.join(crate::app::personal_config::ConfigPhase::PostInit.entry()))
        .map_err(|message| CommandOutcome::HandlerFailure { message })?;
    let request = OpenRequest::from_path(&path, Path::new("/"))
        .map_err(|message| CommandOutcome::HandlerFailure { message })?;
    let open_target = OpenTarget::from_command(target);
    window.update(cx, |window, cx| {
        window.start_open_request(request, open_target, false, Some(completion), cx);
    });
    Ok(())
}
