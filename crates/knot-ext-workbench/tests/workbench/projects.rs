//! `@knot/projects` registry behavior in the launched product.

use std::path::Path;

use gpui::TestAppContext;
use serde_json::Value;

use super::bundled::PACKAGES;
use knot::test_support::{CommandOutcome, TestProduct, TestWindow, wait_until};

pub(super) const TREE: &str = "builtin.projects";

pub(super) fn file_uri(path: &Path) -> String {
    url::Url::from_file_path(path).unwrap().to_string()
}

pub(super) fn wait_for_prompt(cx: &mut TestAppContext) {
    wait_until(cx, |cx| cx.has_pending_prompt());
}

/// The product launched with the registry shown in its window.
pub(super) struct Projects {
    pub(super) product: TestProduct,
    /// The window the last switch focused.
    pub(super) window: TestWindow,
}

impl Projects {
    pub(super) fn start(cx: &mut TestAppContext) -> Self {
        let product = TestProduct::launch(PACKAGES, cx);
        let window = product.active_window(cx);
        // Showing the tool through a command is an explicit user arrangement.
        assert_eq!(
            window.run_command("projects.show", cx),
            CommandOutcome::Completed
        );
        Self { product, window }
    }

    /// Follow the window a switch focused.
    pub(super) fn follow(&mut self, cx: &mut TestAppContext) {
        self.window = self.product.active_window(cx);
    }

    /// Create the project `name` over `directory` from the focused window,
    /// then follow the window it switched to.
    pub(super) fn create(&mut self, directory: &Path, name: &str, cx: &mut TestAppContext) {
        self.window = create_project(&self.product, &self.window, directory, name, cx);
    }

    /// The registry's projects and sessions, as Copy Projects reports them.
    pub(super) fn list(&self, cx: &mut TestAppContext) -> Value {
        list_projects(&self.window, cx)
    }

    /// The id of the project `name`.
    pub(super) fn id(&self, name: &str, cx: &mut TestAppContext) -> String {
        project_id(&self.list(cx), name)
    }
}

pub(super) fn list_projects(window: &TestWindow, cx: &mut TestAppContext) -> Value {
    assert_eq!(
        window.run_command("projects.list", cx),
        CommandOutcome::Completed
    );
    cx.read(|cx| serde_json::from_str(&cx.read_from_clipboard().unwrap().text().unwrap()).unwrap())
}

pub(super) fn project_id(listed: &Value, name: &str) -> String {
    listed["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["name"] == name)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Create the project `name` over `directory` from `window`; the focused
/// window afterwards.
pub(super) fn create_project(
    product: &TestProduct,
    window: &TestWindow,
    directory: &Path,
    name: &str,
    cx: &mut TestAppContext,
) -> TestWindow {
    product.choose_in_open_dialog(directory, cx);
    let create = window.start_command("projects.create", cx);
    window.wait_for_input(cx);
    window.press("cmd-a", cx);
    window.type_input(name, cx);
    window.press("enter", cx);
    // Creating a project switches the window to it; when the command was
    // captured on a document tab that the switch replaced, its completion
    // reports the stale capture rather than a failure.
    assert!(matches!(
        create.finish(cx),
        CommandOutcome::Completed | CommandOutcome::InvalidTarget
    ));
    product.active_window(cx)
}

fn shows(labels: Vec<String>, label: &str) -> bool {
    labels.iter().any(|shown| shown == label)
}

#[gpui::test]
fn bundled_projects_registry_manages_roots_and_persists(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);

    let project_dir = tempfile::tempdir().unwrap();
    let uri = file_uri(project_dir.path());
    projects.create(project_dir.path(), "Knot Editor", cx);
    // Creating the project replaced the blank window with the project's.
    let tree = projects.window.tree(TREE, cx);
    wait_until(cx, |cx| shows(tree.labels(cx), "Knot Editor"));
    assert_eq!(projects.window.roots(cx), std::slice::from_ref(&uri));

    let listed = projects.list(cx);
    let id = listed["projects"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(listed["projects"][0]["name"], "Knot Editor");
    assert_eq!(listed["projects"][0]["roots"][0], uri);
    assert_eq!(listed["recentProjectId"], id);
    wait_until(cx, |cx| tree.status(&id, cx).as_deref() == Some("active"));

    tree.invoke(&id, "projects.rename", cx);
    projects.window.wait_for_input(cx);
    projects.window.press("cmd-a", cx);
    projects.window.type_input("Renamed", cx);
    projects.window.press("enter", cx);
    wait_until(cx, |cx| shows(tree.labels(cx), "Renamed"));

    // A reload keeps the record and re-registers the contributed commands.
    projects.product.reload_packages(cx);
    let tree = projects.window.tree(TREE, cx);
    wait_until(cx, |cx| {
        tree.is_attached(cx) && shows(tree.labels(cx), "Renamed")
    });
    let listed = projects.list(cx);
    assert_eq!(listed["projects"][0]["id"], id);
    assert_eq!(listed["projects"][0]["name"], "Renamed");

    tree.invoke(&id, "projects.remove", cx);
    wait_for_prompt(cx);
    cx.simulate_prompt_answer("Remove");
    wait_until(cx, |cx| !shows(tree.labels(cx), "Renamed"));
    assert_eq!(projects.list(cx)["projects"].as_array().unwrap().len(), 0);
}

#[gpui::test]
fn switching_to_a_project_with_a_missing_root_is_actionable_and_keeps_the_source(
    cx: &mut TestAppContext,
) {
    let mut projects = Projects::start(cx);

    let gone_dir = tempfile::tempdir().unwrap();
    let path = gone_dir.path().to_path_buf();
    projects.create(&path, "Gone", cx);
    let other_dir = tempfile::tempdir().unwrap();
    let other_uri = file_uri(other_dir.path());
    projects.create(other_dir.path(), "Other", cx);
    let listed = projects.list(cx);
    let (gone, other) = (project_id(&listed, "Gone"), project_id(&listed, "Other"));
    let tree = projects.window.tree(TREE, cx);
    wait_until(cx, |cx| tree.labels(cx).len() == 2);

    // Frame actions are exposed beside the rows.
    assert_eq!(
        tree.frame_commands(cx),
        ["projects.create", "projects.new-window", "projects.refresh"]
    );

    // Arrow navigation follows the rendered rows.
    tree.focus(cx);
    for expected in tree.labels(cx) {
        projects.window.press("down", cx);
        assert_eq!(tree.selected(cx), Some(expected));
    }

    assert_eq!(listed["sessions"][&gone]["state"], "suspended");
    assert_eq!(listed["sessions"][&other]["state"], "active");

    std::fs::remove_dir_all(&path).unwrap();
    tree.activate_row(&gone, cx);
    wait_for_prompt(cx);
    assert!(
        cx.pending_prompt()
            .is_some_and(|(_, message)| message.contains("ResourceNotFound"))
    );
    cx.simulate_prompt_answer("OK");

    // The record is retained and the source window stays on its project.
    let listed = projects.list(cx);
    assert_eq!(
        listed["projects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|project| project["id"] == gone.as_str())
            .unwrap()["roots"][0],
        file_uri(&path)
    );
    assert_eq!(listed["sessions"][&other]["state"], "active");
    assert_eq!(projects.window.roots(cx), [other_uri]);
}

#[gpui::test]
fn project_selection_stays_in_the_capturing_window(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let first_dir = tempfile::tempdir().unwrap();
    let first_uri = file_uri(first_dir.path());
    projects.create(first_dir.path(), "First", cx);
    let first = projects.window;

    let second = projects.product.new_window(cx);
    assert_eq!(
        second.run_command("projects.show", cx),
        CommandOutcome::Completed
    );
    let second_dir = tempfile::tempdir().unwrap();
    let second_uri = file_uri(second_dir.path());
    // The blank second window is replaced by the new project's window.
    let second = create_project(&projects.product, &second, second_dir.path(), "Second", cx);
    let second_tree = second.tree(TREE, cx);
    let second_id = project_id(&list_projects(&second, cx), "Second");

    // Row activation in one window opens that project's root there only.
    second_tree.activate_row(&second_id, cx);
    assert_eq!(second.roots(cx), [second_uri]);
    assert_eq!(first.roots(cx), [first_uri]);
    wait_until(cx, |cx| shows(second_tree.labels(cx), "Second"));
}

#[gpui::test]
fn command_invoked_project_tool_changes_publish_the_window_default(cx: &mut TestAppContext) {
    // Showing the tool through a command is an explicit user arrangement and
    // therefore becomes the fresh-window default.
    let projects = Projects::start(cx);
    let tree = projects.window.tree(TREE, cx);
    wait_until(cx, |cx| tree.is_focused(cx));
    assert!(projects.product.fresh_window_shows(TREE, cx));
    assert_eq!(
        projects.product.fresh_window_left_dock(cx).as_deref(),
        Some(TREE)
    );

    // A late native explorer registration must not steal the stored selection.
    projects.window.restore_fresh_layout(cx);
    wait_until(cx, |cx| tree.is_focused(cx));

    projects.window.focus_document(cx);
    assert_eq!(
        projects.window.run_command("projects.hide", cx),
        CommandOutcome::Completed
    );
    cx.run_until_parked();
    assert!(!tree.is_attached(cx));
    assert!(!projects.product.fresh_window_shows(TREE, cx));
}
