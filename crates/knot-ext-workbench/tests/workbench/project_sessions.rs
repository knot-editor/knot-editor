//! `@knot/projects` sessions: switching, tracking, suspension, and removal.

use gpui::TestAppContext;
use serde_json::Value;

use super::projects::{
    Projects, TREE, create_project, file_uri, list_projects, project_id, wait_for_prompt,
};
use knot::test_support::{CommandOutcome, wait_until};

const OWNER: &str = "@knot/projects";

/// A project directory with one file.
fn project_dir(file: &str, text: &str) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join(file), text).unwrap();
    directory
}

/// Stored JSON numbers are doubles; compare records independently of that.
fn normalized(value: Value) -> Value {
    match value {
        Value::Number(number) => serde_json::json!(number.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.into_iter().map(normalized).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .into_iter()
                .map(|(key, value)| (key, normalized(value)))
                .collect(),
        ),
        other => other,
    }
}

fn captures(session: &Value) -> Vec<String> {
    session["windows"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|window| window["entries"].as_array().unwrap())
        .filter_map(|entry| entry["capture"].as_str().map(str::to_owned))
        .collect()
}

fn project_roots(listed: &Value, id: &str) -> Vec<String> {
    listed["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["id"] == id)
        .unwrap()["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|root| root.as_str().unwrap().to_owned())
        .collect()
}

impl Projects {
    /// Wait until the focused window shows exactly these roots, then follow it.
    fn follow_roots(&mut self, expected: &[&str], cx: &mut TestAppContext) {
        wait_until(cx, |cx| {
            self.product.has_active_window(cx)
                && self.product.active_window(cx).roots(cx) == expected
        });
        self.follow(cx);
    }

    /// Select the project `id` in the focused window's registry.
    fn select(&self, id: &str, cx: &mut TestAppContext) {
        let tree = self.window.tree(TREE, cx);
        wait_until(cx, |cx| tree.has_row(id, cx));
        tree.activate_row(id, cx);
    }

    fn session_windows(&self, id: &str, cx: &mut TestAppContext) -> usize {
        self.list(cx)["sessions"][id]["windows"]
            .as_array()
            .unwrap()
            .len()
    }

    fn retained_texts(&self, cx: &mut TestAppContext) -> Vec<String> {
        let mut texts: Vec<_> = self
            .product
            .retained(OWNER, cx)
            .into_iter()
            .map(|retained| retained.text)
            .collect();
        texts.sort();
        texts
    }
}

#[gpui::test]
fn switching_suspends_and_restores_unsaved_documents_across_windows(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    std::fs::write(a.path().join("two.txt"), "two").unwrap();
    let b = project_dir("b.txt", "bee");
    projects.create(a.path(), "A", cx);
    assert_eq!(projects.window.roots(cx), [file_uri(a.path())]);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    projects.window.append(" edited", cx);
    projects.window.new_document(cx);
    projects.window.append("scratch", cx);
    // A second window on the same root joins the active project.
    let second = projects.product.new_window(cx);
    second.set_roots(&[&file_uri(a.path())], cx);
    second.open_path(&a.path().join("two.txt"), cx);

    projects.create(b.path(), "B", cx);
    assert_eq!(projects.window.roots(cx), [file_uri(b.path())]);
    assert!(!second.is_open(cx));
    // A project without a session opens with an empty center.
    assert!(projects.window.document_texts(cx).is_empty());
    assert!(!projects.window.has_center_pane(cx));
    let listed = projects.list(cx);
    let (a_id, b_id) = (project_id(&listed, "A"), project_id(&listed, "B"));
    let suspended = &listed["sessions"][&a_id];
    assert_eq!(suspended["state"], "suspended");
    assert_eq!(suspended["windows"].as_array().unwrap().len(), 2);
    assert_eq!(
        suspended["windows"][1]["entries"],
        serde_json::json!([{ "uri": file_uri(&a.path().join("two.txt")) }])
    );
    assert_eq!(captures(suspended).len(), 2);
    assert_eq!(projects.retained_texts(cx), ["one edited", "scratch"]);
    assert_eq!(listed["sessions"][&b_id]["state"], "active");

    // Back to A: both windows return with their documents and dirty text.
    projects.select(&a_id, cx);
    projects.follow_roots(&[&file_uri(a.path())], cx);
    assert_eq!(
        projects.window.document_texts(cx),
        ["one edited", "scratch"]
    );
    let windows = projects.product.windows(cx);
    assert_eq!(windows.len(), 2);
    let created = windows
        .iter()
        .find(|window| **window != projects.window)
        .unwrap();
    assert_eq!(created.roots(cx), [file_uri(a.path())]);
    assert_eq!(created.document_texts(cx), ["two"]);
    let listed = projects.list(cx);
    assert_eq!(listed["sessions"][&a_id]["state"], "active");
    assert!(captures(&listed["sessions"][&a_id]).is_empty());
    assert_eq!(listed["sessions"][&b_id]["state"], "suspended");
    // Superseded captures are released once A's active record is durable.
    assert!(projects.product.retained(OWNER, cx).is_empty());
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn a_switch_racing_an_edit_retries_with_fresh_captures(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    let b = project_dir("b.txt", "bee");
    projects.create(a.path(), "A", cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    projects.window.append(" first", cx);
    projects.create(b.path(), "B", cx);
    projects.window.open_path(&b.path().join("b.txt"), cx);
    projects.window.append(" typed", cx);
    let a_id = projects.id("A", cx);

    // Hold the destination's file read so an edit lands between retain and commit.
    let held = projects.product.hold_file_reads(cx);
    projects.select(&a_id, cx);
    wait_until(cx, |cx| {
        projects
            .product
            .retained(OWNER, cx)
            .iter()
            .any(|retained| retained.text == "bee typed")
    });
    projects.window.append(" racing", cx);
    held.release();
    projects.follow_roots(&[&file_uri(a.path())], cx);
    assert!(!cx.has_pending_prompt());
    assert_eq!(projects.window.document_texts(cx), ["one first"]);
    let listed = projects.list(cx);
    let b_id = project_id(&listed, "B");
    let kept = captures(&listed["sessions"][&b_id]);
    assert_eq!(kept.len(), 1);
    // The racing text is retained; the first attempt's capture was released.
    let retained = projects.product.retained(OWNER, cx);
    assert_eq!(retained.len(), 1);
    assert_eq!(retained[0].capture_key, kept[0]);
    assert_eq!(retained[0].text, "bee typed racing");
}

#[gpui::test]
fn closing_the_last_window_suspends_without_captures(cx: &mut TestAppContext) {
    let mut projects = Projects::start_windowed(cx);
    let a = project_dir("one.txt", "one");
    std::fs::write(a.path().join("two.txt"), "two").unwrap();
    projects.create(a.path(), "A", cx);
    let second = projects.product.new_window(cx);
    second.set_roots(&[&file_uri(a.path())], cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    second.open_path(&a.path().join("two.txt"), cx);
    let a_id = projects.id("A", cx);
    assert_eq!(projects.session_windows(&a_id, cx), 2);

    // Closing a window that is not the last removes it from the session.
    assert_eq!(
        second.run_command("window.close", cx),
        CommandOutcome::Completed
    );
    let listed = projects.list(cx);
    assert_eq!(listed["sessions"][&a_id]["state"], "active");
    let windows = &listed["sessions"][&a_id]["windows"];
    assert_eq!(windows.as_array().unwrap().len(), 1);
    assert_eq!(
        windows[0]["entries"],
        serde_json::json!([{ "uri": file_uri(&a.path().join("one.txt")) }])
    );
    assert_eq!(windows[0]["selected"], 0);
    // Each session window keeps its captured layout.
    assert_eq!(windows[0]["layout"]["version"], 2);

    // Closing the last one suspends the project with its last membership,
    // which records the dirty document's identity once tracking sees it.
    let other = projects.product.new_window(cx);
    projects.window.append(" dirty", cx);
    let mut dirty = Value::Null;
    wait_until(cx, |cx| {
        dirty = list_projects(&other, cx)["sessions"][&a_id]["windows"][0]["entries"][0].clone();
        dirty["document"].is_string()
    });
    assert_eq!(dirty["uri"], file_uri(&a.path().join("one.txt")));
    let close = projects.window.start_command("window.close", cx);
    wait_for_prompt(cx);
    cx.simulate_prompt_answer("Don't Save");
    assert_eq!(close.finish(cx), CommandOutcome::Completed);
    let listed = list_projects(&other, cx);
    let session = &listed["sessions"][&a_id];
    assert_eq!(session["state"], "suspended");
    assert_eq!(session["windows"][0]["entries"], serde_json::json!([dirty]));
    assert!(captures(session).is_empty());
    assert!(projects.product.retained(OWNER, cx).is_empty());
}

#[gpui::test]
fn startup_suspends_active_sessions_without_a_window(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    projects.create(a.path(), "A", cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    let a_id = projects.id("A", cx);
    assert_eq!(projects.list(cx)["sessions"][&a_id]["state"], "active");

    // The root changes while the package is not running, as across a restart.
    projects.product.stop_package(OWNER, cx);
    projects.window.set_roots(&[], cx);
    projects.product.reload_packages(cx);
    let listed = projects.list(cx);
    let session = &listed["sessions"][&a_id];
    assert_eq!(session["state"], "suspended");
    assert_eq!(
        session["windows"][0]["entries"],
        serde_json::json!([{ "uri": file_uri(&a.path().join("one.txt")) }])
    );
}

#[gpui::test]
fn quit_records_no_window_closure(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    projects.create(a.path(), "A", cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    let a_id = projects.id("A", cx);
    let before = projects.list(cx)["sessions"][&a_id].clone();
    assert_eq!(before["state"], "active");

    projects.product.take_application_events(cx);
    assert_eq!(
        projects.window.run_command("application.quit", cx),
        CommandOutcome::Completed
    );
    cx.run_until_parked();
    assert!(
        !projects
            .product
            .take_application_events(cx)
            .iter()
            .any(|change| change["type"] == "windowClosed")
    );
    // The durable record still describes the window as it was at quit.
    let stored = projects
        .product
        .stored_record(OWNER, &format!("session:{a_id}"));
    assert_eq!(stored.map(normalized), Some(normalized(before)));
}

#[gpui::test]
fn a_partial_restore_commits_and_reports_what_was_skipped(cx: &mut TestAppContext) {
    let mut projects = Projects::start_windowed(cx);
    let a = project_dir("one.txt", "one");
    std::fs::write(a.path().join("gone.txt"), "soon gone").unwrap();
    let b = project_dir("b.txt", "bee");
    projects.create(a.path(), "A", cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    projects.window.open_path(&a.path().join("gone.txt"), cx);
    projects.create(b.path(), "B", cx);
    let a_id = projects.id("A", cx);
    std::fs::remove_file(a.path().join("gone.txt")).unwrap();

    projects.select(&a_id, cx);
    wait_for_prompt(cx);
    let (_, message) = cx.pending_prompt().unwrap();
    assert_eq!(message, "Opened A. gone.txt no longer exists.");
    cx.simulate_prompt_answer("OK");
    projects.follow_roots(&[&file_uri(a.path())], cx);
    assert_eq!(projects.window.document_texts(cx), ["one"]);
}

#[gpui::test]
fn restarting_the_registry_keeps_bound_sessions_active(cx: &mut TestAppContext) {
    let mut projects = Projects::start_windowed(cx);
    let a = project_dir("one.txt", "one");
    projects.create(a.path(), "A", cx);
    let a_id = projects.id("A", cx);

    let restart = projects.window.start_command("extensions.restart", cx);
    assert_eq!(restart.finish(cx), CommandOutcome::Completed);
    wait_until(cx, |cx| projects.window.transient_open(cx));
    projects.window.type_input(OWNER, cx);
    projects.window.press("enter", cx);
    wait_for_prompt(cx);
    assert_eq!(cx.pending_prompt().unwrap().0, "Restarted @knot/projects");
    cx.simulate_prompt_answer("OK");
    cx.run_until_parked();

    assert_eq!(projects.list(cx)["sessions"][&a_id]["state"], "active");
    assert_eq!(projects.window.roots(cx), [file_uri(a.path())]);
}

#[gpui::test]
fn root_edits_in_a_bound_window_move_the_project_and_its_other_windows(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    let c = project_dir("c.txt", "see");
    let (a_uri, c_uri) = (file_uri(a.path()), file_uri(c.path()));
    projects.create(a.path(), "A", cx);
    let a_id = projects.id("A", cx);
    let second = projects.product.new_window(cx);
    second.set_roots(&[&a_uri], cx);
    assert_eq!(projects.session_windows(&a_id, cx), 2);

    // Adding a folder in one window updates the record and the other window.
    projects.window.set_roots(&[&a_uri, &c_uri], cx);
    second.wait_for_roots(&[&a_uri, &c_uri], cx);
    assert_eq!(
        project_roots(&projects.list(cx), &a_id),
        [a_uri.clone(), c_uri.clone()]
    );
    let tree = projects.window.tree(TREE, cx);
    let detail = project_roots(&projects.list(cx), &a_id)
        .iter()
        .map(|root| root.trim_start_matches("file://").trim_end_matches('/'))
        .collect::<Vec<_>>()
        .join(", ");
    wait_until(cx, |cx| tree.detail(&a_id, cx).as_deref() == Some(&*detail));

    // Membership ignores order: a window with the roots reversed joins.
    let third = projects.product.new_window(cx);
    third.set_roots(&[&c_uri, &a_uri], cx);
    assert_eq!(projects.session_windows(&a_id, cx), 3);
    assert_eq!(
        project_roots(&projects.list(cx), &a_id),
        [a_uri.clone(), c_uri.clone()]
    );

    // Removing a folder in another window follows back the same way.
    second.set_roots(&[&c_uri], cx);
    projects.follow_roots(&[&c_uri], cx);
    third.wait_for_roots(&[&c_uri], cx);
    let listed = projects.list(cx);
    assert_eq!(project_roots(&listed, &a_id), std::slice::from_ref(&c_uri));
    assert_eq!(listed["sessions"][&a_id]["state"], "active");
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn a_multi_root_project_restores_every_root_across_switches_and_restarts(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    let c = project_dir("c.txt", "see");
    let b = project_dir("b.txt", "bee");
    let (a_uri, c_uri) = (file_uri(a.path()), file_uri(c.path()));
    projects.create(a.path(), "A", cx);
    projects.window.set_roots(&[&a_uri, &c_uri], cx);
    projects.window.open_path(&c.path().join("c.txt"), cx);
    projects.window.append(" dirty", cx);
    let a_id = projects.id("A", cx);

    projects.create(b.path(), "B", cx);
    projects.select(&a_id, cx);
    projects.follow_roots(&[&a_uri, &c_uri], cx);
    assert_eq!(projects.window.document_texts(cx), ["see dirty"]);

    // Suspended with its text, A survives a restart and restores both roots.
    let b_id = projects.id("B", cx);
    projects.select(&b_id, cx);
    projects.follow_roots(&[&file_uri(b.path())], cx);
    // Listing runs after the switch finishes recording, so the restart cannot
    // interrupt it.
    assert_eq!(projects.list(cx)["sessions"][&b_id]["state"], "active");
    projects.product.reload_packages(cx);
    let listed = projects.list(cx);
    assert_eq!(listed["sessions"][&a_id]["state"], "suspended");
    assert_eq!(captures(&listed["sessions"][&a_id]).len(), 1);
    assert_eq!(projects.product.retained(OWNER, cx).len(), 1);
    projects.select(&a_id, cx);
    projects.follow_roots(&[&a_uri, &c_uri], cx);
    assert_eq!(projects.window.document_texts(cx), ["see dirty"]);
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn a_new_project_window_opens_with_the_project_roots_and_joins_it(cx: &mut TestAppContext) {
    let mut projects = Projects::start_windowed(cx);
    let a = project_dir("one.txt", "one");
    let c = project_dir("c.txt", "see");
    let (a_uri, c_uri) = (file_uri(a.path()), file_uri(c.path()));
    projects.create(a.path(), "A", cx);
    projects.window.set_roots(&[&a_uri, &c_uri], cx);
    let a_id = projects.id("A", cx);

    assert_eq!(
        projects.window.run_command("projects.new-window", cx),
        CommandOutcome::Completed
    );
    let windows = projects.product.windows(cx);
    assert_eq!(windows.len(), 2);
    let created = windows
        .iter()
        .find(|window| **window != projects.window)
        .unwrap();
    created.wait_for_roots(&[&a_uri, &c_uri], cx);
    assert_eq!(projects.session_windows(&a_id, cx), 2);

    // A window outside every project has no project to open.
    let blank = projects.product.new_window(cx);
    let refused = blank.start_command("projects.new-window", cx);
    wait_for_prompt(cx);
    cx.simulate_prompt_answer("OK");
    assert_eq!(refused.finish(cx), CommandOutcome::Completed);
    assert_eq!(projects.product.windows(cx).len(), 3);
}

/// A window's arrangement without document identities, which restored
/// captures and reopened files replace, nor its focus and the dock's
/// selection, which selecting through the tree moves to the Projects tool
/// before each switch.
fn arrangement(layout: Value) -> Value {
    fn forget(node: &mut Value) {
        for tab in node["tabs"].as_array_mut().into_iter().flatten() {
            tab["document"] = Value::Null;
        }
        for child in ["first", "second"] {
            if node.get(child).is_some() {
                forget(&mut node[child]);
            }
        }
    }
    let mut center = layout["center"].clone();
    forget(&mut center);
    let mut docks = layout["docks"].clone();
    docks["left"]["root"]["selected"] = Value::Null;
    serde_json::json!({ "center": center, "docks": docks })
}

#[gpui::test]
fn a_multi_root_layout_with_shared_views_survives_repeated_switches(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", &"line\n".repeat(200));
    let c = project_dir("c.txt", "see");
    let b = project_dir("b.txt", "bee");
    let (a_uri, c_uri) = (file_uri(a.path()), file_uri(c.path()));
    projects.create(a.path(), "A", cx);
    projects.window.set_roots(&[&a_uri, &c_uri], cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    projects.window.append("dirty", cx);
    projects.window.split(cx);
    projects.window.set_dock_size("left", 300., cx);
    projects.window.set_view(10, 25, 20, cx);
    let a_id = projects.id("A", cx);
    projects.create(b.path(), "B", cx);
    let b_id = projects.id("B", cx);

    // The empty placeholder tab is pruned on the first return; every later
    // return reproduces that arrangement exactly.
    let mut before = None;
    for round in 0..2 {
        projects.select(&a_id, cx);
        projects.follow_roots(&[&a_uri, &c_uri], cx);
        let current = arrangement(projects.window.layout(cx));
        assert_eq!(current["center"]["kind"], "split");
        assert_eq!(current["docks"]["left"]["size"], 300.);
        assert_eq!(
            before.get_or_insert(current.clone()),
            &current,
            "round {round}"
        );
        let editors = projects.window.editors(cx);
        let [.., split_from, split] = &editors[..] else {
            panic!("a split editor");
        };
        assert_eq!(split_from.buffer, split.buffer);
        assert_eq!((split.anchor, split.caret, split.top), (10, 25, 20));
        assert!(split.text.ends_with("dirty"));
        projects.select(&b_id, cx);
        projects.follow_roots(&[&file_uri(b.path())], cx);
        // Switching never duplicates windows or documents.
        assert_eq!(projects.product.windows(cx).len(), 1);
        assert!(projects.product.document_count(cx) <= 2);
    }
}

#[gpui::test]
fn schema_one_records_switch_without_a_layout(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    let b = project_dir("b.txt", "bee");
    projects.create(a.path(), "A", cx);
    projects.create(b.path(), "B", cx);
    let a_id = projects.id("A", cx);
    projects.product.write_record(
        OWNER,
        &format!("session:{a_id}"),
        serde_json::json!({
            "schema": 1, "state": "suspended",
            "windows": [{ "entries": [{ "uri": file_uri(&a.path().join("one.txt")) }], "selected": 0 }],
        }),
    );
    projects.product.reload_packages(cx);
    projects.select(&a_id, cx);
    projects.follow_roots(&[&file_uri(a.path())], cx);
    assert_eq!(projects.window.document_texts(cx), ["one"]);
    // The record is upgraded when it is next written.
    let listed = projects.list(cx);
    assert_eq!(listed["sessions"][&a_id]["schema"], 2);
    assert!(listed["sessions"][&a_id]["windows"][0]["layout"].is_object());
}

#[gpui::test]
fn restored_windows_rebind_after_launch_and_unrestored_sessions_suspend(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    let c = project_dir("c.txt", "see");
    projects.create(a.path(), "A", cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    projects.window.append(" dirty", cx);
    let second = projects.product.new_window(cx);
    create_project(&projects.product, &second, c.path(), "C", cx);
    let listed = projects.list(cx);
    let (a_id, c_id) = (project_id(&listed, "A"), project_id(&listed, "C"));
    assert_eq!(listed["sessions"][&c_id]["state"], "active");

    // The record holds only A's window, as if C's window had not been
    // recorded, so C's session never returns.
    projects.product.relaunch_restoring(&[&projects.window], cx);

    // The restored window rebinds to A with its text; no window or document
    // was added, and C, whose window did not return, is suspended.
    let windows = projects.product.windows(cx);
    assert_eq!(windows.len(), 1);
    projects.follow(cx);
    assert_eq!(projects.window.document_texts(cx), ["one dirty"]);
    let listed = projects.list(cx);
    assert_eq!(listed["sessions"][&a_id]["state"], "active");
    assert_eq!(listed["sessions"][&c_id]["state"], "suspended");
    let tree = projects.window.tree(TREE, cx);
    wait_until(cx, |cx| tree.status(&a_id, cx).as_deref() == Some("active"));
    assert_eq!(projects.product.windows(cx).len(), 1);
    assert_eq!(projects.product.document_count(cx), 1);
}

#[gpui::test]
fn opening_a_suspended_projects_folder_restores_it(cx: &mut TestAppContext) {
    let mut projects = Projects::start(cx);
    let a = project_dir("one.txt", "one");
    let b = project_dir("b.txt", "bee");
    projects.create(a.path(), "A", cx);
    projects.window.open_path(&a.path().join("one.txt"), cx);
    projects.window.append(" edited", cx);
    projects.create(b.path(), "B", cx);
    let a_id = projects.id("A", cx);
    assert_eq!(projects.list(cx)["sessions"][&a_id]["state"], "suspended");

    // A new window on A's folder is replaced by A's restored windows.
    let hand_opened = projects.product.new_window(cx);
    hand_opened.set_roots(&[&file_uri(a.path())], cx);
    wait_until(cx, |cx| !hand_opened.is_open(cx));
    let restored = projects.product.active_window(cx);
    assert_eq!(restored.roots(cx), [file_uri(a.path())]);
    assert_eq!(restored.document_texts(cx), ["one edited"]);
    let listed = projects.list(cx);
    assert_eq!(listed["sessions"][&a_id]["state"], "active");
    assert_eq!(listed["projects"].as_array().unwrap().len(), 2);
}
